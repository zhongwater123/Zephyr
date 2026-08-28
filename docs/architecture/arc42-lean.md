---
{"documentType":"arc42-view","viewStatus":"current","sourceRevision":"b62667deab18f740c83bab2f1bcebae2fd0a59e2","worktreeState":"dirty","changedPaths":["src-tauri/src/voice_controller","src-tauri/src/voice_trigger.rs","src-tauri/src/voice_input_service.rs","src-tauri/src/streaming_pipeline.rs","docs/architecture/arc42-lean.md"],"reviewStatus":"reviewed","reviewedAt":"2026-08-28","knownDeviations":[]}
---

# arc42-Lean：GY Typing 架构叙事

本文采用 arc42 的 12 个主题，但保持 Lean：只记录理解、演进和评审当前系统所需的内容。源码与运行配置承载实现事实；Current C4、Runtime View、arc42 和代码地图是对事实的解释；Feature Dossier 规定用户行为，ADR 记录长期决策。

## 1. 简介与目标

[component:system.zephyr]

GY Typing（Zephyr）是 Windows 语音输入助手。用户按住全局热键录音，应用将流式预览显示在非抢焦点悬浮窗中；释放后取得最终识别结果，复验原目标窗口，并在安全条件满足时输入文本。

首要架构目标：

1. **文本副作用可控**：窗口焦点或进程身份变化时不误写；只有成功注入后才提交历史和学习副作用。
2. **实时链路有界**：音频、控制事件和预览都具有明确容量或 latest-value 语义。
3. **凭据不越界**：自定义 endpoint 未经原生授权不得读取或发送凭据。
4. **本地优先**：配置、历史和秘密分别保存在本机合适的存储中，不引入本地 HTTP 控制面。
5. **兼容演进**：Tauri command、DTO、配置 schema 和 SQLite schema 的变化需显式管理。

## 2. 架构约束

- 产品定位为 Windows-only；目标身份、输入、热键、确认框和凭据均依赖 Windows API。
- UI 使用 Tauri 2 + WebView2 + Preact；实时采音和副作用链路保留在 Rust。
- 当前外部识别为 Volcengine-compatible WSS；热词整理为可选 DeepSeek-compatible HTTPS。
- 自定义 ASR 与 Agent endpoint 按 origin 和 purpose 独立授权。
- Pending 文本仅驻留内存，最多 5 条，TTL 10 分钟；退出后不可恢复。
- 单次录音硬上限 120 秒。
- 不引入微服务、新异步 runtime、离线 ASR、云同步或远程脚本。

## 3. 上下文与范围

业务和技术上下文见 [C4 L1](c4-context.md)。系统接收用户热键与麦克风输入，调用 Windows 和受信外部服务，输出到目标桌面应用，并把非秘密状态保存在本机。

关键数据流：

- 原始 PCM：麦克风 → Rust → 已授权 ASR；不进入历史库和 Hotword Agent。
- 识别文本：ASR → preview/final → 目标复验 → 注入或 Pending。
- 历史/上下文：成功注入后可写 SQLite；可选发送给已授权 Hotword Agent 进行整理。
- 凭据：主窗口配置 → Rust command → Windows Credential Manager；WebView 不持久化秘密。

## 4. 解决方案策略

- 使用 Tauri 把本机能力与 WebView UI 放在一个可部署桌面应用中。
- 使用薄 commands 和 `AppServices` 隔离 IPC、业务编排与存储适配器。
- 使用单所有者 `VoiceSessionActor` 和 `VoiceSessionHandle` 串行化带 Activation 身份的会话命令与内部完成事件。Actor 按值持有纯 Runtime，由 reducer 生成 Effects；独立 AudioSessionActor 独占 Recorder，启动、finalize、Pending 与展示按职责分层，异步 Workflow 只返回类型化 Outcome。
- 使用有界 `mpsc`、`watch` 和取消令牌表达背压、最新预览和终止。
- 使用 `DeliveryService` 集中目标复验、文本验证、注入、Pending 和提交顺序。
- 使用 revision CAS、原子 JSON 和凭据快照回滚保护配置事务。
- 默认以 Win32 Unicode `SendInput` 输入；剪贴板只作为按应用显式启用的兼容模式。
- 使用代码地图和 ADR 将组件演进与决策历史纳入 CI。

## 5. 构建块视图

- [C4 L2 容器](c4-container.md)
- [Rust 后端组件](c4-components-backend.md)
- [WebView 前端组件](c4-components-frontend.md)
- [代码地图](code-map.md)

主依赖方向为 WebView → typed IPC → thin commands → services/controller → repository/platform adapters。command 不应直接拼装存储事务，组件不应绕过 Delivery 产生文本副作用。

## 6. 运行时视图

详见 [运行时与部署视图](runtime-views.md)，覆盖：

- Activation begin → 流式识别 → finish/deadline → final → Delivery；
- 失败进入 Pending 与手动发送；
- revision 配置与凭据回滚事务；
- Windows 本机部署和外部 TLS 连接。

## 7. 部署视图

应用以单个 Windows Tauri 包部署，依赖 WebView2 和 Windows 用户会话。本机文件与 Credential Manager 不随 WebView bundle 暴露。开发环境额外启动 Vite localhost 服务；`scripts/tauri.mjs` 使用项目本地 Cargo target 并用 PID lock 防止重复 dev 实例。

## 8. 横切概念

### 安全与信任

- main/preinput 使用分离 capability；Rust command 再验证窗口 label。
- overlay 事件定向发送到 `preinput`。
- 自定义 endpoint 先原生确认并检查 trust，后读取 Keyring。
- 生产 CSP 禁止远程脚本、frame、object 和 base 重定向。
- 日志只记录请求 ID、状态码、字符数和错误分类，不记录完整 ASR JSON、服务正文或秘密。

### 一致性与恢复

[component:backend.services] [component:backend.repositories] [component:storage.local]

- `ConfigService` 串行化 mutation，以 expected revision 做 CAS。
- 配置采用同目录临时文件、flush、`sync_all` 和 Windows 原子替换，并保留最后一份有效备份。
- 主配置和备份都损坏时，以 `enabled=false` 默认配置启动。
- 凭据更新先快照；配置保存失败则恢复。
- SQLite 使用 WAL、NORMAL synchronous 和 3 秒 busy timeout。

### 并发与资源

[component:backend.voice-controller] [component:backend.streaming]

- 控制通道容量 16；AudioSessionActor 控制邮箱容量 4；音频数据通道容量 32；WebSocket 原始帧通道容量 4；partial 使用 latest-value。
- 公共控制通道满会触发失败关闭并令 Actor 进入 Faulted；音频数据队列溢出取消当前会话。两条路径均有自动化故障测试，真实设备行为仍待目标环境验证。
- SessionId、ActivationId 和取消令牌由 Actor 仲裁；Worker 在注入前请求 Actor 授权并再次检查取消，过期结果不能修改当前状态。
- 录音 120 秒自动 finalize；真实 Release 随后被幂等忽略。

### 文本交付

[component:backend.delivery]

- final 文本最多 8000 个 Unicode 字符；拒绝 NUL、换行、控制字符和双向覆盖/隔离字符。
- 自动注入要求原 HWND 仍为前台，且 PID、创建时间和 EXE 未变化。
- 默认 Unicode `SendInput`；UIPI/部分失败不自动降级到剪贴板。
- 剪贴板兼容模式按 EXE 显式授权，需完整 OLE 快照和 sequence 保护。
- 注入成功是提交点；提交前取消不写历史或学习热词。

### 错误与可观测性

commands 返回 `CommandError { code, message, details }`。session metrics 记录音频包数、队列高水位、overflow、时长、取消原因和最终状态。UI 将配置冲突、授权拒绝和 Pending 操作分别建模。

## 9. 架构决策

| ADR | 决策 |
| --- | --- |
| [ADR-0001](adr/0001-tauri-local-desktop-boundary.md) | Tauri 本地桌面边界，不建立本地 HTTP 控制面 |
| [ADR-0002](adr/0002-single-owner-bounded-voice-session.md) | 单所有者、有界、失败关闭的语音会话 |
| [ADR-0003](adr/0003-delivery-commit-point-and-pending-output.md) | 注入成功作为提交点，失败进入内存 Pending |
| [ADR-0004](adr/0004-trust-before-credentials.md) | 自定义 origin 授权必须先于凭据读取 |
| [ADR-0005](adr/0005-revisioned-atomic-local-storage.md) | revision CAS、原子 JSON、SQLite 与 Credential Manager |
| [ADR-0006](adr/0006-unicode-injection-default.md) | Unicode SendInput 默认，剪贴板按应用显式兼容 |
| [ADR-0007](adr/0007-architecture-docs-as-code.md) | C4 + arc42-Lean + ADR + 机器可读代码地图 |
| [ADR-0009](adr/0009-evidence-aware-document-governance.md) | 区分材料角色、实现与验证状态，并隔离 Proposed/Current |
| [ADR-0010](adr/0010-separate-focused-shortcut-editing.md) | 分离有焦点的设置录入与全局运行时监听 |
| [ADR-0012](adr/0012-unified-voice-input-control-plane.md) | 统一语音输入控制面所有权与触发端口 |
| [ADR-0013](adr/0013-strict-mailbox-owned-voice-runtime.md) | 严格 mailbox-owned Runtime 与控制/执行分层 |

## 10. 质量要求与场景

| 属性 | 可验证场景 |
| --- | --- |
| 安全 | 自定义 endpoint 未授权时，测试、录音和 Agent 整理均不得读取 Keyring |
| 正确性 | 识别期间切换前台窗口，原窗口和新窗口都不应收到自动输入 |
| 可靠性 | 音频队列 Full 时会话取消并报告 overflow，不提交残缺 final |
| 可恢复性 | 主配置损坏可加载最后有效备份；主备都坏则禁用启动 |
| 有界资源 | 单次录音 120 秒自动结束；Pending 满 5 条时拒绝新录音 |
| 隐私 | 默认 Unicode 输入不改变任意剪贴板格式；日志不包含完整响应正文 |
| 可维护性 | `architecture:impact` 沿组件依赖传播复核范围；CI 校验 100% 生产源码映射、Schema、Rust [架构不变量](invariants.md)、链接与 Mermaid 语法 |
| 兼容性 | command 名称、参数、camelCase DTO、配置和 SQLite schema 通过契约测试保护 |

## 11. 风险与技术债

| 风险 / 技术债 | 当前控制 | 后续触发条件 |
| --- | --- | --- |
| 配置已提交后 Actor acknowledgment 仍可能失败 | 返回包含 committedRevision 的 reconciliation error；前端保留已提交意图，后续配置操作按最新 revision 重试 | 产品要求跨进程强一致或出现无法自动协调的持久故障时设计持久 outbox |
| Provider 与热词实现仍有较高领域复杂度 | 语音控制面已按 mailbox、reducer/Effects、Audio Actor、三类 workflow 与 Presenter 分层；内聚性以依赖和可见性测试约束 | 新增 provider 协议或学习链路前按领域继续拆分，不使用文件行数作为符合性证明 |
| 热词自动学习直接读取正式历史的单一 `text`，缺少 ASR 原文、交付文本和来源标记，可能把未来的模型润色结果反馈为 ASR hints | 目前仅在成功注入并写入历史后触发，手工热词仍可独立管理 | 接入路由/润色层前重构学习输入契约、provenance、批次游标和历史编辑后的重新学习语义 |
| `AppShellV2.tsx` 仍承担大量页面装配 | feature controllers 已分离，preinput 已动态拆包 | 新增主页面功能前继续拆 Settings 与 shell orchestration |
| Win32/OLE 行为受目标应用和 UIPI 影响 | 默认不回退、Pending 兜底、Windows CI 与手工验收 | 支持高完整性目标或更多兼容应用时增加隔离集成测试 |
| 外部 ASR/Agent 协议可能变化 | provider 错误分类、endpoint trust、可替换 adapter trait | API 版本或认证方式变化时新增 ADR 并更新 L1/L2 |
| 架构文档可能与实现漂移 | 代码地图路径、组件 marker、链接和 ADR 元数据进入 CI | 语义漂移仍需评审者结合 impact 输出人工复核 |

## 12. 术语表

| 术语 | 含义 |
| --- | --- |
| PendingOutput | 未自动交付的内存文本，保留原目标身份和失败原因 |
| commit point | 文本成功注入目标应用的时刻；之后才允许历史/热词副作用 |
| target identity | HWND、PID、进程创建时间、EXE 和可选窗口标题的组合 |
| origin | `scheme + host + effective port`；与 purpose 共同形成授权键 |
| purpose | endpoint 用途，目前为 ASR 或 Hotword Agent |
| latest-value | 只保留最新 partial preview，不为过时预览排队 |
| fail closed | 队列、授权、目标或注入出现异常时取消并禁止文本副作用 |
| revision CAS | mutation 携带 expected revision，只有与当前 revision 相等时提交 |
