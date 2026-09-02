---
{
  "documentType": "architecture-proposal",
  "viewStatus": "proposed",
  "owner": "product-maintainers",
  "createdAt": "2026-08-31",
  "revisitWhen": "macOS Runnable Slice 的实机结果推翻单仓复用边界，或维护者开始确认原生输入、自动写回、最低系统、CPU 架构和外部分发契约时",
  "relatedFeatures": ["FEAT-MACOS-RUNNABLE-SLICE", "FEAT-SMART-DICTATION", "FEAT-SHORTCUT-BINDING", "FEAT-SHORTCUT-TRIGGER-MODES", "FEAT-VOICE-INPUT-CONTROL-PLANE", "FEAT-WINDOWS-DISTRIBUTION"]
}
---

# macOS 单仓能力切片开发与安全交付边界

> 本文是 Proposed architecture，不描述当前实现，也不把讨论结论自动升级为 Accepted ADR。当前要交付的范围以 [macOS Runnable Slice](../../features/macos-runnable-slice.md) 为准：先在真实 Mac 上跑通启动、麦克风、共享 ASR/处理和结果查看/复制；本文后半部分的原生输入、自动写回和分发设计属于后续里程碑。

## 1. 目的与当前结论

Zephyr 可以继续使用同一个 Tauri、Rust 和 Preact 仓库开发 Windows 与 macOS，不需要复制一套独立 Mac App。当前先复用语音会话仲裁、录音/ASR 编排、文本处理和主界面，补齐真实 Mac 构建、启动、麦克风和结果查看/复制。快捷键、目标捕获、权限、浮层、自动文本交付和凭据等原生能力在其进入实际里程碑时再按纵向能力单元分别实现。

“Mac 上真实跑通”与“可对外安装分发”是两个验收闭环。当前 Runnable Slice 不要求 Developer ID、Hardened Runtime、Apple Notarization 或 DMG；进入外部测试或发布里程碑时，候选路线仍是站外直接分发，而不是 Mac App Store。该长期边界仍需在真正实施前形成 ADR；本 Proposal 不改写 ADR-0001 的历史，也不把尚未实现的 macOS 能力描述成当前事实。

加入 macOS 也不要求先建设一个覆盖所有 OS 能力的统一大平台层。简单的窗口生命周期、菜单、默认值和构建差异可以保留局部平台分支；只有需要跨越共享业务边界的语义才形成窄契约。当前共享模型中的 HWND、Windows 扫描码、`.exe`、Ctrl+V、OLE 和 Windows Credential Manager 泄漏应在对应 macOS 能力接入时拆除，不能继续穿过共享 Voice、Processing 或 Delivery 边界。

## 2. 已核对的历史实现基线

以下内容是截至 clean `main` revision `3d71d91` 的历史基线，用来解释本 Proposal 形成时的问题，不是持续更新的当前状态。实际实现进展与验证边界只在 [macOS Runnable Slice](../../features/macos-runnable-slice.md) 维护，避免 Proposal 同时充当计划、状态页和验收记录：

- 共享 Voice、Delivery 和 Pending 已改持有 `CapturedTarget`；平台中立 `TargetContext` 只暴露应用 key、窗口标题、PID 和多行风险，opaque payload 不序列化、不持久化。`WindowsTargetAdapter` 私有持有 HWND、PID、进程创建时间、EXE 路径与标题，并通过 `TargetPort` 提供捕获、存在性检查、前台复验和激活；非 Windows adapter 明确返回 Unsupported。
- 自动交付执行继续位于现有 `DeliveryExecutor`/`ClipboardTransactionService` 边界，但共享 Actor 仍把 `InjectionStrategy` 映射为 `DeliveryMode::Unicode` 或 `ClipboardPaste`。这是未来自动写回平台化需要处理的策略耦合，不是只展示并复制应用内结果的 Runnable Slice 前置条件。
- `ShortcutManager::initialize` 直接创建 `WindowsKeyboardEngine`；`ShortcutRuntimePort` 的错误和诊断类型仍来自 `windows_keyboard`。
- `PhysicalKeyId` 使用 Windows scan code 与 `extended` 位；前端录入表包含 Windows 扫描码、Win 修饰键和 Windows 保留组合。
- `AppServices::production` 固定装配 `WindowsCredentialStore` 和 `WindowsNativeConfirmation`；`keyring` 仅启用 `windows-native` feature。
- `UnicodeTextInjector` 的 Unicode、剪贴板和 AtomicPaste 实现只存在于 Windows；非 Windows 返回 Unsupported。
- Tauri bundle 只启用 NSIS；CI 只运行 Windows job；尚无 macOS Info.plist、entitlements、签名、公证或 DMG 发布脚本。
- preinput 窗口请求透明、置顶和初始不聚焦，但没有 macOS non-activating panel 的实现与目标环境证据。
- Current C4 和 Runtime View 正确地将当前部署描述为 Windows-only；在 macOS 实现完成并复核前不得提前修改为跨平台 Current 事实。
- revision `3d71d91` 的 Windows PR fast 与 Full engineering checks 已通过；真实 Windows 录音、目标切换、Pending 和自动输入运行时回归仍没有绑定该 revision 的目标环境证据。

## 3. 自动写回的安全门禁，不是 Runnable Slice 前置条件

### 3.1 Delivery 提交结果必须是三态

当前 main 已将共享 Delivery 回执改为 `NotSubmitted / Submitted / Unknown` 三态，并把不确定提交与普通可重投失败分开。该修复降低了现有 Windows 与未来 macOS 自动写回的重复输入风险，但对应 ADR 和目标环境验证仍未收口，不能把源码存在三态等同于安全能力已验证。

未来实现 macOS 自动文本写回时必须沿用并实机验证这一语义：

```text
SubmissionState
├── NotSubmitted   可以进入可重投 Pending
├── Submitted      视为不可逆提交，不得再次交付
└── Unknown        可能已经产生目标副作用，严禁自动重试
```

`Unknown` 只能显示“文字可能已经输入，请先检查目标应用”，并提供复制或另一个需要用户明确确认的恢复动作；不能伪装成普通失败，也不能自动进入一键重投路径。

安全关键的 injector 不应提供会自动宣称成功或 `Restored` 的默认实现。每个平台必须显式实现提交状态和剪贴板恢复状态。

### 3.2 多行目标分类必须识别输入表面

当前终端防护只按 EXE 黑名单判断，并把 `Code.exe`、`Cursor.exe` 视为非终端。该行为无法区分 IDE 编辑器和集成终端，与 ADR-0014“不得仅凭窗口是 IDE 就把集成终端误判为普通编辑器”的约束冲突。

在 Windows 或 macOS 无法确认当前焦点控件时，多行生成文本必须失败关闭：

```text
普通可编辑控件且已确认     → 允许一次性多行粘贴
终端或命令执行表面已确认   → 禁止自动粘贴
IDE 但内部控件未知          → 禁止多行自动粘贴
密码框 / Secure Input       → 禁止自动粘贴
无法分类                    → Pending / 用户主动复制
```

该修复是现有 Windows 产品与未来自动写回的安全工作，不应被归入“Mac 特有任务”而延期；但它不阻塞只在 Zephyr 内展示并复制结果的 Runnable Slice。

## 4. 候选目标架构

### 4.1 共享产品链路与纵向能力单元

```text
Main / Preinput WebViews
          │
VoiceSessionActor + shared workflows
          │
ASR / Processing / Delivery / History / IncidentVault
          ║
Narrow capability contracts（只表达共享业务需要的输入与结果）
          ║
          ├── mac-hotkey
          ├── mac-target-and-permission
          ├── mac-overlay
          ├── mac-paste
          ├── mac-microphone
          └── mac-credential-and-confirmation
```

每个 macOS 能力单元端到端拥有其原生实现、Rust/Tauri adapter、窄协议、启动与停止、权限失败、测试、构建装配和实机证据。能力单元可以直接使用 Rust 调用公开 macOS API，也可以使用一个职责单一的 Swift helper；选择由该能力的 API 可用性、签名、生命周期和错误可观察性决定，不设置全项目 helper 数量上限。

OpenWhispr 的可借鉴部分是这种能力级拆分：多个 `macos-*.swift` helper 分别拥有构建脚本、管理器、CPU 架构检查和最终包校验。Zephyr 不复制其 Electron 主进程结构、巨型平台分支文件、`osascript` 兜底或粘贴失败后再次提交的错误语义。

共享 Actor 只接收平台无关的 `Pressed`、`Released`、`Interrupted`、目标快照、交付结果和权限健康状态。Win32 Hook、CGEvent、HWND、AXUIElement、NSPanel、OLE 和 NSPasteboard 类型不得穿过共享能力契约。若只有一个调用点且不形成业务语义，允许在 Tauri 壳层保留小型平台条件分支，不为形式统一提前创建 trait 或 service registry。

### 4.2 通用目标引用

候选共享引用：

```text
TargetRef
├── platform
├── opaqueTargetToken
├── applicationId
├── displayName
└── capturedAt
```

`opaqueTargetToken` 只在当前进程和会话中用于把共享 Delivery 请求交回原捕获能力，不持久化、不进入 Prompt，也不允许共享层解析。Windows 能力单元可以私有持有 HWND、PID、进程创建时间和 EXE；macOS 能力单元可以私有持有 PID、进程实例、bundle identifier、应用 URL、窗口号和可选的 Accessibility 表面分类。共享 Processing 只接收明确允许的低敏感应用身份，不接收平台句柄或平台表面快照。

窗口标题、页面、屏幕、文档正文、聊天历史、源代码、选区和剪贴板内容继续不属于 SmartDictation Prompt 上下文。未来若要使用必须形成独立知情授权和数据边界。

### 4.3 平台化快捷键绑定

快捷键持久化不能继续把 Windows scan code 当成通用物理键 ID。候选格式必须带平台和 schema version：

```json
{
  "platform": "windows",
  "schemaVersion": 1,
  "binding": {}
}
```

```json
{
  "platform": "macos",
  "schemaVersion": 1,
  "binding": {}
}
```

Pressed/Released/Interrupted 和 Hold/Toggle 状态机继续共享。默认绑定、保留组合、修饰键名称、左右键能力和物理码由各平台定义；Windows 的右 Alt 绑定不得自动迁移为 macOS 的右 Option。

### 4.4 平台化交付策略

共享 Delivery 决定：文本是否合法、目标是否允许、何时进入 Pending、提交状态如何影响副作用。平台 Injector 只负责执行平台动作和返回准确 receipt。

Windows 可继续使用 Unicode SendInput 或 OLE AtomicPaste；macOS 候选实现使用 NSPasteboard、目标复验和单次 CGEvent Cmd+V。两者必须遵守相同的三态提交结果、剪贴板并发保护和禁止歧义重试规则，不要求使用相同底层机制。

## 5. 分层目标，禁止把后续能力塞回当前 MVP

### 5.1 当前：Mac Runnable Slice

当前唯一必须完成的闭环是：

- 真实 Mac 从 clean revision 构建并启动主应用；
- 主窗口和核心设置可用，Windows-only 能力不会拖垮整个应用；
- 应用内按钮或简单可靠的触发方式开始/结束真实麦克风录音；
- 复用现有 Voice/ASR/Processing，至少跑通 Fast final transcript；
- 最终文字在 Zephyr 内可见并可复制，用户手动粘贴到目标应用；
- Windows 构建和既有自动化不回归。

以上范围由 `FEAT-MACOS-RUNNABLE-SLICE` 规定。全局快捷键、浮层、目标捕获、自动粘贴和签名 DMG 即使尚未完成，也不能据此判定当前切片失败。

### 5.2 后续：Mac Native Input Alpha

只有 Runnable Slice 已在真实 Mac 上闭环后，才按产品优先级逐项加入：

- 普通全局快捷键的 Pressed/Released，再评估按住说话和特殊物理键；
- frontmost application 捕获、写回前复验和 Accessibility 权限；
- 不抢焦点的非激活浮层，透明视觉不是第一优先级；
- 对已确认普通输入面的单次 Cmd+V；
- 无权限、目标变化、Secure Input 或输入面未知时降级为复制/Pending；
- 三态提交和剪贴板恢复的 macOS 实机故障验证。

这些能力不要求一次性打包成一个大 PR，也不要求为了接口形式统一提前重构全部 Windows 适配层。

### 5.3 后续：Mac Distribution Alpha

当团队要把应用交给开发机之外的用户安装时，再建立独立的分发闭环：Developer ID、Hardened Runtime、notarization、staple、DMG、干净机器安装、覆盖升级和发布清单。Mac App Store、MAS sandbox、自动更新、Intel 与 Universal Binary 仍不默认进入该里程碑。

### 5.4 不作为默认目标：Windows 功能对等

以下能力只能由新的明确产品优先级拉入，不从“支持 macOS”自动推导：Fn/Globe、单独右 Option、鼠标侧键、全局吞键、系统音频/会议录音、所有第三方应用自动输入、所有 Windows 设置与视觉行为的逐项复刻。

## 6. 权限与生命周期

当前 Runnable Slice 只必须管理 Microphone。后续原生输入阶段还需要分别管理 Accessibility，以及快捷键方案实际需要时的 Input Monitoring。任何进入实现的权限状态都不能压缩成一个持久化 boolean；候选状态包括 `Unknown`、`NotDetermined`、`Granted`、`Denied`、`Restricted`、`Revoked` 和 `Unavailable`。

权限交互应遵循：

1. 不在首次启动时无解释地连续请求多个权限；
2. 用户首次启用相应能力时说明原因，再触发系统动作；
3. 拒绝权限后仍可打开设置、查看 History 和复制 Pending；
4. 从系统设置返回、应用恢复、睡眠唤醒和升级后重新检测；
5. Debug、未签名测试包和正式签名包的授权证据互不替代；
6. App 更新保持稳定 bundle identifier、Team 和 designated requirement，避免无必要地使 TCC 授权失效；
7. `NSMicrophoneUsageDescription` 和本地化说明必须进入 Info.plist；缺失时禁止发布。

Accessibility 权限属于核心自动写回能力，但不能成为应用启动前置条件。未授权时功能降级为转写后 Pending/复制，不得报告为自动输入成功。

## 7. 浮层与透明窗口

当前 `.focused(false)` 不能单独证明窗口每次 `show()` 都不会激活或抢走 first responder。macOS 目标实现需要真实验证：

- non-activating panel；
- 不成为 key window；
- 不抢目标应用的 focused element；
- Spaces、全屏窗口和多显示器；
- 点击穿透或明确的交互策略；
- 浮层出现前已经捕获目标，写回前再次复验目标。

Tauri 的 macOS 透明 WebView需要开启 `app.macOSPrivateApi`。如果后续产品保留透明浮层，配置只能位于 `tauri.macos.conf.json` 等平台特定配置。候选分发方向是直接 DMG，不同时维护 Mac App Store 兼容目标；透明能力仍须服从不抢焦点、公开可支持 API、签名和实机验证要求。

## 8. 分发与凭据

### 8.1 后续候选的直接分发链路

进入 Mac Distribution Alpha 后，候选方案是站外 DMG 分发，不建立 `mas` target、App Store provisioning profile 或 Mac App Store 提交流程。Developer ID 签名和 Apple Notarization 用于 Gatekeeper 信任，不等同于 App Store 上架。这不是当前 Runnable Slice 的完成条件。

```text
macOS CI / 受控 Mac 构建机
→ build + tests
→ Developer ID Application signing
→ Hardened Runtime
→ notarization
→ staple
→ codesign / spctl / ticket verification
→ DMG + release manifest + SHA-256
```

发布清单至少记录版本、Git revision、dirty 状态、CPU 架构、bundle identifier、签名身份摘要、公证状态、构件名称、大小和 SHA-256。签名、公证和构件生成不构成功能目标环境验收。

### 8.2 共享秘密边界

当前编译期内置 ASR/DeepSeek 共享凭据只在受控公司内部、短期、限额、监控和可吊销的 MVP 信任模型下成立。macOS 签名、公证、Hardened Runtime 和 Keychain 都不能防止本机用户从客户端提取编译期秘密。

以下行为完全禁止：

- 把含长期共享密钥的客户端公开分发；
- 把公证或代码签名描述为凭据保密措施；
- 把密钥写入前端、普通配置、Prompt、日志、Incident 或发布清单；
- 在扩大分发范围前跳过凭据签发、额度、吊销和服务端滥用控制的重新设计。

## 9. 完全禁止的产品与实现行为

1. SubmissionState 为 Unknown 后自动重试 Cmd+V 或自动重投 Pending。
2. 对已知终端、shell、Secure Input 或未知 IDE 内部控件自动粘贴多行文本。
3. 为了取得快捷键优先级而周期性抢占 Hook、修改用户 Fn/Globe 系统设置或破坏其他应用输入监听。
4. 要求用户关闭 SIP、Gatekeeper、TCC 或全局降低 macOS 安全策略。
5. 未经独立授权把窗口标题、屏幕、页面、选区、剪贴板或文档内容送入 LLM。
6. 用单元测试、CI 构建、签名或公证代替真实 Mac 权限、快捷键、麦克风和外部应用互操作证据。
7. 将 macOS 尚未实现的 adapter 以 no-op 成功返回；未实现能力必须显式 Unsupported 或 Unavailable 并失败关闭。

## 10. 不宜激进的技术选择

- 不因 macOS 支持而把 Tauri/Rust 重写为 Electron；OpenWhispr 只提供平台边界参考，不是迁移框架的依据。
- 不同时进行 Tauri 大版本升级、数据库替换、前端框架迁移、Prompt 重构和 macOS port；除非存在明确阻塞，否则保持故障可归因。
- 每个能力先选择错误语义最清楚、可测试且可签名的实现方式；Rust 可以直接覆盖时不额外增加 helper，Swift 更适合公开 macOS API 时允许建立多个职责单一、签名、版本化、协议有界并进入最终包校验的 helper。禁止的是松散 sidecar，而不是 helper 数量。
- 简单的平台生命周期、菜单、窗口默认值或构建差异可以使用局部 `cfg`；高风险原生能力和共享业务语义不得通过散布平台分支混入 Voice、Processing 或 Delivery。
- 自动写回核心链路不以 `osascript` 为主实现，避免新增任意 Apple Events、Automation 权限和弱错误契约。
- 不复制 OpenWhispr 在 macOS 快速粘贴失败后重新发送 Cmd+V 的兜底；任何可能已经提交的失败都必须返回 `Unknown`，不得换另一种工具再次尝试。
- 首个分发 Alpha 不同时承担 Apple Silicon、Intel、Mac App Store和自动更新；支持矩阵由实际设备清单驱动。
- 不为了透明视觉延迟正确的目标捕获、非激活语义和 Delivery 安全；必要时先使用保守窗口形态。

## 11. 文档系统边界

当前执行范围已经拆成独立的 [macOS Runnable Slice Dossier](../../features/macos-runnable-slice.md)，避免让 Windows 原生输入、自动写回和分发要求污染首次 Mac bring-up。既有跨平台功能 Dossier 多数仍把共享用户行为和 Windows 证据能力写在同一个 validation slice；这些功能真正声明 macOS 支持时，不能复制整套 Dossier，也不能让 Windows 证据完成 macOS 验收。

候选调整：

1. Runnable Slice 只维护自己的真实 Mac 启动、麦克风、共享 ASR 和结果复制验收，不把尚未承诺的平台对等要求写入。
2. 某个既有共享功能真正声明 macOS 支持时，再为对应 acceptance 增加明确的平台证据切片；总体 validation status 不得由 Windows 证据替代 Mac 证据。
3. 按实际实现增加 `platform.macos.<capability>` 能力组件；只有多个能力确实共享稳定责任时才增加通用平台组件，不预设单体 `backend.platform-boundary`。
4. 自动跨应用写回开始实施前，新建或确认 hard-boundary Dossier，集中管理提交三态、目标身份、终端防护、剪贴板和 Pending。
5. 对外分发开始实施前，新建 macOS 分发 Dossier，独立记录签名、公证、安装、覆盖升级和权限连续性。
6. 长期单仓 macOS 能力边界和直接 DMG 分发进入实施时，再分别形成 Proposed ADR；不为了 Runnable Slice 预先冻结 helper 数量、目录形状或所有平台 trait。
7. 文档门禁应禁止 HWND、Win32、WindowsKeyboard、AXUIElement、CGEvent 和平台错误类型穿过共享业务契约，但不得禁止它们存在于对应平台能力单元。
8. 若引入原生 helper，构建与发布证据必须覆盖源码哈希/版本、目标 CPU 架构、可执行权限、签名、应用包内路径和启动失败语义。

除新增当前 Runnable Slice 契约外，本 Proposal 不提前修改既有 Feature 的 macOS 支持声明、ADR 状态、Current C4、Runtime View 或代码地图。它们只能在 clean revision 和真实实现证据出现后由收口者更新。

## 12. 分阶段实施与门禁

### Phase A：Mac Runnable Slice（当前）

- 当前已完成 Windows 目标能力的窄端口准备；不以重构完整自动交付策略作为继续实施的门禁；
- 先建立能在真实 Mac 编译、启动和运行的最小构建脊柱；
- 只处理会阻塞启动的 Windows-only 依赖、装配和配置，不预先抽象所有平台能力；
- 接通真实麦克风、VoiceSessionActor、现有 ASR/Processing 和应用内结果展示/复制；
- 应用内按钮优先，简单快捷键可选；未实现能力明确失败关闭；
- 在 Windows 上运行现有自动化，在真实 Mac 上记录绑定 clean revision 的构建和端到端结果。

门禁：必须有真实 Mac 证据，只有 `cargo check`、前端构建或 Windows CI 不算跑通。反过来，缺少 Accessibility、浮层、自动 Cmd+V、Developer ID 或 DMG 不阻塞本阶段完成。

Phase A 的实现顺序固定为两个可独立归因的切片：先建立 target-specific 依赖、macOS credential backend、平台专用 Tauri 配置、Windows-only 启动装配隔离和 Apple Silicon `.app` CI；随后接入应用内 `OutputRoute`、录音入口与结果路线。每个切片的完成情况以 Dossier 为准；任何 CI 构建都不得提前声称真实 Mac 已完成窗口、授权、录音或结果链路验收。

### Phase B：Mac Native Input Alpha（后续、按需拆分）

- 普通全局快捷键与 Pressed/Released 生命周期；
- frontmost application 捕获、复验和权限状态；
- 非激活浮层；
- 已确认普通输入面的单次 Cmd+V；
- 无权限、目标变化和未知输入面时的复制/Pending 降级；
- 三态交付、剪贴板并发和退出/睡眠唤醒的实机验证。

门禁：Delivery 安全纠错只阻塞自动写回，不阻塞 Phase A。每个进入实现的原生能力单元应同时承担 adapter、生命周期、权限/失败语义、构建装配、测试和实机缺口，避免把孤立 API 封装扔给其他 Agent 收尾。

### Phase C：外部应用与安全矩阵（自动写回后）

只有 Phase B 真正加入自动写回后，才验证 TextEdit/Notes、浏览器、Office、IDE 编辑器与集成终端、Terminal、密码框/Secure Input、中文输入法、多显示器/全屏 Space、目标退出/切换和剪贴板并发。任何目标环境失败都否定对应验收，不能用模拟测试解释掉实机结果。

### Phase D：签名、公证与受控分发（需要交付他人时）

- Developer ID、Hardened Runtime、notarization、staple 和 DMG；
- 干净 Mac 安装、首次权限、覆盖升级、卸载和本地数据保持；
- 发布清单记录构件身份、SHA-256、源码 revision、工作树、CPU 架构、系统和签名身份。

门禁：本阶段完成前不得声称“可对外分发”，但不回头否定已经有证据的开发机 Runnable Slice。

## 13. 当前 Runnable Slice 的完成定义

只有同时满足以下条件，才可以说“Zephyr 已在 Mac 上首次跑通”：

- 同一仓库和共享主链路，没有复制第二套 Voice/ASR/Processing；
- clean revision 在一台记录了芯片和系统版本的真实 Mac 上构建并启动；
- 用户能明确开始/结束录音，真实麦克风数据进入现有会话链路；
- 至少一次 Fast final transcript 成功产生；在达成前，任何失败都必须准确定位为权限、设备、网络、凭据或服务问题，不能以“可解释但未跑通”宣称完成；
- 最终文本在 Zephyr 内可见且可复制，用户可手动粘贴；
- Windows 既有自动化与构建基线未回归；
- Dossier 仍诚实标记尚未覆盖的实机环境与后续能力，不把一次开发机成功升级为全面 macOS 支持。

全局快捷键、目标捕获、透明浮层、自动 Cmd+V、外部应用矩阵和公证 DMG 不属于这一定义。

## 14. 待确认问题，按里程碑处理

### 当前 Phase A 需要尽快确定

1. 第一台可用于实机联调的 Mac 芯片和 macOS 版本；
2. 首次跑通由应用内按钮触发，还是已有一个低成本、无需额外权限的可靠入口；
3. 现有内部 ASR 凭据在该 Mac 上如何安全注入和验证；
4. 最终文本先复用哪个现有界面区域展示和复制。

### 不应提前阻塞 Phase A

1. 默认全局快捷键、右 Option、Fn/Globe 和按住/切换的完整语义；
2. 透明浮层的最终形态；
3. IDE 编辑器与集成终端的输入面分类；
4. 自动写回目标应用矩阵；
5. 最低支持系统、Intel/Universal Binary、Developer ID、公证和 DMG；
6. 何时切换服务端短期令牌或按用户签发凭据。

后续问题只有在对应里程碑真正启动时才升级为 Feature 契约或 ADR，不得隐含进 Phase A 的默认值和完成门禁。
