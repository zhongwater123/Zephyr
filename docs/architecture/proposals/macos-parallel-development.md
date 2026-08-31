---
{
  "documentType": "architecture-proposal",
  "viewStatus": "proposed",
  "owner": "product-maintainers",
  "createdAt": "2026-08-31",
  "revisitWhen": "维护者确认 macOS 首发范围、最低系统与 CPU 架构、直接分发路线和平台交付安全契约，或任一 macOS 技术探针推翻目标捕获、快捷键、非激活浮层或原子粘贴方案时",
  "relatedFeatures": ["FEAT-SMART-DICTATION", "FEAT-SHORTCUT-BINDING", "FEAT-SHORTCUT-TRIGGER-MODES", "FEAT-VOICE-INPUT-CONTROL-PLANE", "FEAT-WINDOWS-DISTRIBUTION"]
}
---

# macOS 单仓并行开发与安全交付边界

> 本文是 Proposed architecture，不描述当前实现，也不把讨论结论自动升级为 Accepted ADR 或已确认 Feature 契约。当前产品仍是 Windows-only Tauri 应用；源码、Current C4、Runtime View 和各 Feature Dossier 的现有验证状态保持权威。

## 1. 目的与当前结论

Zephyr 可以继续使用同一个 Tauri、Rust 和 Preact 仓库并行开发 Windows 与 macOS，不需要复制一套独立 Mac App。共享层保留语音会话仲裁、录音/ASR 编排、智能成稿、文本校验、Pending、History、Hotword 和 IncidentVault；操作系统相关能力通过显式平台端口分别实现。

macOS MVP 的候选分发路线是经过 Developer ID 签名、Hardened Runtime 和 Apple Notarization 的直接 DMG，而不是 Mac App Store。该方向仍需由维护者通过 ADR 正式接受；本 Proposal 不改变 ADR-0001 的 Accepted 状态。

加入 macOS 不应被理解为“给现有 Windows 函数增加若干 `cfg` 分支”。当前共享模型中仍存在 HWND、Windows 扫描码、`.exe`、Ctrl+V、OLE 和 Windows Credential Manager 泄漏；若不先拆除这些泄漏，代码虽然位于同一仓库，实际上仍会形成两套相互牵制的应用。

## 2. 已核对的当前实现事实

以下内容是本 Proposal 创建时的源码事实，不代表目标设计：

- `TargetWindowIdentity` 直接保存 `hwnd`、PID、Windows 进程创建时间和 EXE；非 Windows 的捕获、存在性验证、前台验证和激活全部返回 Unsupported。
- `ShortcutManager::initialize` 直接创建 `WindowsKeyboardEngine`；`ShortcutRuntimePort` 的错误和诊断类型仍来自 `windows_keyboard`。
- `PhysicalKeyId` 使用 Windows scan code 与 `extended` 位；前端录入表包含 Windows 扫描码、Win 修饰键和 Windows 保留组合。
- `AppServices::production` 固定装配 `WindowsCredentialStore` 和 `WindowsNativeConfirmation`；`keyring` 仅启用 `windows-native` feature。
- `UnicodeTextInjector` 的 Unicode、剪贴板和 AtomicPaste 实现只存在于 Windows；非 Windows 返回 Unsupported。
- Tauri bundle 只启用 NSIS；CI 只运行 Windows job；尚无 macOS Info.plist、entitlements、签名、公证或 DMG 发布脚本。
- preinput 窗口请求透明、置顶和初始不聚焦，但没有 macOS non-activating panel 的实现与目标环境证据。
- Current C4 和 Runtime View 正确地将当前部署描述为 Windows-only；在 macOS 实现完成并复核前不得提前修改为跨平台 Current 事实。

## 3. 必须先修复的现存安全问题

### 3.1 Delivery 提交结果必须是三态

当前 `AtomicPasteReceipt` 只用 `paste_submitted: bool` 表达是否提交，而 Windows `SendInput` 可以返回部分事件已写入。部分写入时，目标可能已经收到 Ctrl+V，但上层会把所有注入错误映射为 `injection_rejected_before_submit` 并创建普通 Pending，后续重投存在重复输入风险。

在 macOS 平台工作开始前，候选共享契约应改为：

```text
SubmissionState
├── NotSubmitted   可以进入可重投 Pending
├── Submitted      视为不可逆提交，不得再次交付
└── Unknown        可能已经产生目标副作用，严禁自动重试
```

`Unknown` 只能显示“文字可能已经输入，请先检查目标应用”，并提供复制或另一个需要用户明确确认的恢复动作；不能伪装成普通失败，也不能自动进入一键重投路径。

安全关键的 `TextInjector::inject_atomic_paste` 不应提供会自动宣称 `Restored` 的默认实现。每个平台必须显式实现提交状态和剪贴板恢复状态。

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

该修复是现有 Windows 产品的安全工作，不应被归入“Mac 特有任务”而延期。

## 4. 候选目标架构

### 4.1 共享业务与平台服务

```text
Main / Preinput WebViews
          │
          ▼
VoiceSessionActor + shared workflows
          │
          ├── ASR / Processing / History / IncidentVault
          │
          ▼
Delivery policy and safety contract
          │
          ▼
PlatformServices
├── ShortcutRuntime
├── TargetApplicationService
├── TextInjector
├── OverlayWindowService
├── PermissionService
├── CredentialStore
├── NativeConfirmation
└── AudioPlatformService（仅在 CPAL 不能覆盖的边界出现）
          │
          ├── platform/windows/*
          └── platform/macos/*
```

共享 Actor 只接收平台无关的 `Pressed`、`Released`、`Interrupted`、目标快照、交付结果和权限健康状态。Win32 Hook、CGEvent、HWND、AXUIElement、NSPanel、OLE 和 NSPasteboard 类型不得穿过平台端口。

### 4.2 通用目标身份

候选共享身份：

```text
TargetIdentity
├── platform
├── processInstance
├── applicationId
├── displayName
├── capturedAt
└── platformSurface（只由同平台 adapter 解释）
```

Windows adapter 可以持有 HWND、PID、进程创建时间和 EXE；macOS adapter 可以持有 PID、进程实例、bundle identifier、应用 URL、窗口号和可选的 Accessibility 表面分类。共享 Processing 只接收明确允许的低敏感应用身份，不接收平台句柄。

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

## 5. macOS MVP 候选能力

### 5.1 可以纳入首个内部 Alpha

- 启动主窗口、托盘/菜单和本地设置；
- 麦克风权限与录音；
- 复用现有 ASR、Fast 和 LLM 成稿链路；
- 普通组合键的全局 Pressed/Released；
- 按快捷键时捕获 frontmost application；
- 不激活原目标的轻量浮层，或在原生非激活透明面板完成前使用保守的不透明面板；
- 对已确认的普通可编辑输入框执行单次 Cmd+V；
- 辅助功能未授权、目标变化或输入表面未知时进入 Pending/手动复制；
- Apple Silicon 内部测试构件；
- Developer ID 签名、公证和 DMG。

### 5.2 不应成为首个 Alpha 前置条件

- Mac App Store；
- Intel 与 Universal Binary，除非公司设备清单证明必要；
- Fn/Globe 键；
- 单独右 Option 等 modifier-only 快捷键；
- 鼠标侧键；
- 全局吞键或与其他 Hook 的系统级独占；
- 自动更新；
- 系统音频/会议录音；
- 屏幕内容和窗口标题采集；
- 所有第三方应用都能自动输入的承诺。

## 6. 权限与生命周期

macOS 至少需要分别管理 Microphone、Accessibility，以及快捷键方案实际需要时的 Input Monitoring。权限状态不能压缩成一个持久化 boolean；候选状态包括 `Unknown`、`NotDetermined`、`Granted`、`Denied`、`Restricted`、`Revoked` 和 `Unavailable`。

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

Tauri 的 macOS 透明 WebView需要开启 `app.macOSPrivateApi`。如果产品保留透明浮层，配置只能位于 `tauri.macos.conf.json` 等平台特定配置，并明确绑定到直接分发路线；不能同时把 Mac App Store 兼容声明为 MVP 目标。

## 8. 分发与凭据

### 8.1 候选直接分发链路

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
- 优先在 Rust macOS adapter 中调用公开 AppKit、ApplicationServices 和 CoreGraphics API；确有必要时最多先引入一个小型、签名、版本化、协议有界的 Swift helper，不复制多个松散 sidecar。
- 自动写回核心链路不以 `osascript` 为主实现，避免新增任意 Apple Events、Automation 权限和弱错误契约。
- 首个 Alpha 不同时承担 Apple Silicon、Intel、Mac App Store和自动更新；支持矩阵由实际公司设备清单驱动。
- 不为了透明视觉延迟正确的目标捕获、非激活语义和 Delivery 安全；必要时先使用保守窗口形态。

## 11. 文档系统候选调整

当前 Dossier 多数把共享用户行为和 Windows 证据能力写在同一个 validation slice。macOS 接入后，不能复制整套 Dossier，也不能让 Windows 证据完成 macOS 验收。

候选调整：

1. Dossier 保留共享用户目标和平台无关的可观察行为；
2. validation slice 增加明确 `platform` 或等价维度；
3. 同一 acceptance 可以拥有 Windows 与 macOS 两个证据切片；
4. 对外声明支持的平台集合必须版本化；总体 validation status 取所声明支持平台中的最低有效状态；
5. 增加 `platform.macos` 和通用 `backend.platform-boundary` 组件；
6. 新建跨应用文本交付 hard-boundary Dossier，集中管理提交三态、目标身份、终端防护、剪贴板和 Pending；
7. 新建 macOS 分发 Dossier，独立记录签名、公证、安装、覆盖升级和权限连续性；
8. 创建新的 Proposed ADR 决定单仓跨平台和直接分发，并在接受时替代 ADR-0001 的 Windows-only 决策表述；
9. 创建新的 Proposed ADR 替代 AtomicPaste 的布尔提交回执，不改写 ADR-0003/0014 的历史；
10. `implementationStatus=implemented` 应要求 `implementationReview.status=conformant` 且无已知偏差；
11. 文档门禁应禁止 HWND、Win32、WindowsKeyboard 和平台错误类型穿过共享端口，但不能把平台实现细节升级为永久技术禁令。

这些调整仍是候选设计；本次记录不修改现有 Dossier schema、Feature 契约、ADR、Current C4、Runtime View 或代码地图。

## 12. 分阶段实施与门禁

### Phase 0：共享安全纠错

- Delivery 回执改为 NotSubmitted / Submitted / Unknown；
- 部分 SendInput 和进程异常使用 Unknown；
- Unknown 禁止自动重试和普通 Pending 重投；
- 移除安全关键 injector 的伪成功默认实现；
- 无控件级证据的 IDE 多行交付失败关闭；
- 补充故障注入、ADR 和 hard-boundary Dossier。

门禁：在该阶段完成前不实现 macOS 自动 Cmd+V。

### Phase 1：双平台可编译骨架

- 建立 PlatformServices 和 windows/macos adapter 目录；
- 重构 TargetIdentity、ShortcutRuntime 和平台化 ShortcutBinding；
- 将 Windows、Apple Keychain 和 OS API 依赖改为 target-specific；
- 增加 `tauri.macos.conf.json`、Info.plist 和 macOS CI；
- 尚未实现的能力显式失败关闭。

门禁：Windows 自动化与当前行为不得回归；macOS runner 必须完成 compile/test，而不是只编译前端。

### Phase 2：macOS 内部 Alpha 纵向链路

- 权限状态机；
- 麦克风与普通组合快捷键；
- frontmost app 捕获与复验；
- 非激活浮层；
- 已确认普通输入框的单次 Cmd+V；
- 无权限、目标变化和未知表面的 Pending/复制降级；
- Apple Silicon 内部构件。

门禁：真实 Mac 上验证快速按放、权限拒绝/撤销、切应用、退出、睡眠唤醒和剪贴板并发。

### Phase 3：外部应用与安全矩阵

至少覆盖：

- TextEdit 或 Notes；
- 浏览器普通输入框；
- Office 输入面；
- VS Code/Cursor 编辑器与集成终端分别验证；
- Terminal、iTerm 或企业实际终端；
- 密码框与 Secure Input；
- 中文输入法组合状态；
- 多显示器和全屏 Space；
- 识别期间目标退出、切换或 PID 复用；
- 粘贴期间其他程序修改剪贴板。

门禁：任何目标环境失败都否定对应验收，不用模拟测试解释掉实机结果。

### Phase 4：签名、公证与受控分发

- Developer ID、Hardened Runtime、notarization 和 staple；
- DMG 与发布清单；
- 干净 Mac 安装、首次权限、覆盖升级、卸载和本地数据保持；
- 同 bundle identifier/Team 更新后的 TCC 权限连续性；
- 公证失败、下载损坏和签名不匹配的失败关闭。

门禁：未保存构件身份、SHA-256、源码 revision、工作树、目标机器和系统环境时，不得声称 macOS 分发已验证。

## 13. 建议的完成定义

macOS MVP 只有同时满足以下条件才可以从 Alpha 升级：

- 单仓共享链路没有复制第二套 Voice/ASR/Processing/Delivery；
- Windows 与 macOS 平台 adapter 均通过共享契约测试；
- SubmissionState Unknown 在所有路径上禁止自动重试；
- 多行终端、Secure Input 和未知 IDE 表面失败关闭；
- 权限拒绝/撤销后不继续采音或伪报自动输入；
- 浮层不抢走原目标焦点，并有真实 Mac 证据；
- 外部应用矩阵记录版本、硬件、系统、签名身份与结果；
- DMG 已签名、公证、staple 并在干净目标机验证；
- 对应 Dossier、ADR、Current C4、Runtime View 和代码地图已在实现复核后更新；
- validation status 与实际证据能力一致。

## 14. 待维护者确认的问题

1. 首个内部 Alpha 是否只支持 Apple Silicon，以及公司设备清单中的最低 macOS 版本；
2. 是否正式接受“直接 DMG、非 Mac App Store”为 MVP 外部分发边界；
3. macOS 首版默认快捷键与允许的组合范围；
4. 透明浮层是否是 Alpha 必须项，还是可以先使用保守的不透明非激活面板；
5. 无法区分 IDE 编辑器与集成终端时，多行结果统一 Pending 的产品取舍；
6. 何时停止编译期共享凭据并切换到服务端短期令牌/按用户签发；
7. macOS Alpha 的目标应用矩阵和内部测试人员范围。

这些问题确认后，应创建对应 Proposed ADR 和 Dossier 变更；在此之前不应把候选答案隐含进源码默认值。
