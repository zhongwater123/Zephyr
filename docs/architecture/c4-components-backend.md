---
{"documentType":"c4-view","viewStatus":"current","sourceRevision":"b62667deab18f740c83bab2f1bcebae2fd0a59e2","worktreeState":"dirty","changedPaths":["src-tauri/src/voice_controller","src-tauri/src/voice_trigger.rs","src-tauri/src/voice_input_service.rs","src-tauri/src/streaming_pipeline.rs","src-tauri/src/lib.rs","docs/architecture/c4-components-backend.md"],"reviewStatus":"reviewed","reviewedAt":"2026-08-28","knownDeviations":[]}
---

# C4 L3：Rust 后端组件

[component:backend.bootstrap] [component:backend.commands] [component:backend.services] [component:backend.voice-controller] [component:backend.streaming] [component:backend.delivery] [component:backend.shortcut] [component:backend.repositories] [component:backend.incident-vault]

```mermaid
flowchart TB
    web["Main / Preinput WebViews"]

    subgraph rust["Container: Native Core (Rust)"]
        bootstrap["Component<br/>Bootstrap + Platform Startup<br/><small>lib.rs, platform/tray.rs</small>"]
        commands["Component<br/>Thin IPC Commands<br/><small>commands/*, CommandError</small>"]
        services["Component<br/>Application Services<br/><small>VoiceControlService, ConfigService, ProviderService</small>"]
        shortcut["Component<br/>Shortcut Manager<br/><small>binding transaction + runtime adapter</small>"]
        controller["Component<br/>VoiceSessionActor + AudioSessionActor<br/><small>pure runtime/reducer, effects, typed outcomes</small>"]
        streaming["Component<br/>Voice Workflows + StreamingPipeline<br/><small>audio, provider, preview, delivery workers</small>"]
        delivery["Component<br/>Delivery + Pending Services<br/><small>target, text, lease, inject, commit</small>"]
        repos["Component<br/>Repository Ports + Adapters<br/><small>JSON, SQLite, Keyring, Agent</small>"]
        incident["Component<br/>IncidentVault<br/><small>lock-free ingress, isolated writer, recovery queries</small>"]
        win["Component<br/>Windows Adapters<br/><small>overlay, target identity, SendInput/OLE, confirmation</small>"]
    end

    web -->|"invoke"| commands
    bootstrap -->|"construct/manage"| services
    bootstrap -->|"register/manage"| shortcut
    bootstrap -->|"construct/manage"| controller
    commands -->|"snapshot / mutation / CRUD"| services
    commands -->|"incident list / recovery / export"| incident
    commands -->|"pending delivery request"| controller
    shortcut -->|"VoiceActivation begin / finish / cancel"| controller
    controller -->|"immutable jobs / internal outcomes"| streaming
    controller -.->|"try_emit only"| incident
    controller -->|"validate / inject / commit"| delivery
    controller -->|"immutable config snapshot"| services
    delivery -->|"history + hotword side effects after injection"| services
    services -->|"repository traits"| repos
    services -->|"native confirmation"| win
    delivery -->|"target and injection adapters"| win
    streaming -->|"ASR provider built by ProviderService"| services
```

## 组件责任

### Bootstrap + Commands

`lib.rs` 负责装配、Tauri managed state、handler 注册、窗口/托盘启动和退出清理。业务 command 位于 `commands/`，只处理窗口 label、DTO、服务调用和 `CommandError` 映射。

### AppServices

`ConfigService` 是配置的唯一服务层内存所有者，使用互斥 mutation 和 revision compare-and-swap。它通过 Repository/Credential 接口保存配置与秘密；写配置失败时恢复凭据快照。

`ProviderService` 在构建 ASR provider 时先检查 endpoint trust，再读取 CredentialStore。热词 Agent adapter 遵循相同顺序。

`VoiceControlService` 是语音输入启停和通用配置保存的应用层协调器。它先以 revision CAS 提交 `desired_enabled`，再等待 Actor 返回同一 committed revision 的 availability acknowledgment，最后协调快捷键运行时。Actor 未确认时返回包含 `committedRevision` 的 reconciliation error，不回滚已经提交的配置；每次后续配置操作都会按最新 revision 重试协调。Hook 安装失败只写入独立的 shortcut health 并保留 Actor 可用性，成功响应仍返回已提交 revision。对应 command 只保留窗口权限校验、参数转发和错误 DTO 映射。Provider 不保存在共享运行时中，由 `ProviderService` 从 begin 固定的配置快照构造会话 Provider。

### VoiceSessionActor + VoiceSessionHandle

容量 16 的控制通道顺序处理带 `ActivationId` 的 Begin、Finish、Cancel、可用性和 Pending 交付命令；DeadlineReached、AudioOverflow、ProviderFinished、ReadyToInject 和类型化 Workflow Outcome 是内部事件。Begin 入队后通过异步 `BeginReceipt` 返回 Actor 的 Accepted/Rejected 决策；快捷键回调不等待 receipt。通道满时通过独立失败关闭信号取消当前会话。外部组件只持有 Handle 和 watch 状态快照。

`VoiceSessionActor` 按值持有只含控制状态的 `VoiceRuntime`，私有纯 reducer 生成 Effects；mailbox task 提交状态并调度 Effects。Recorder 由 AudioSessionActor 独占，AudioSessionActor mailbox 容量 4；Start/Stop/Cancel 结果始终带 SessionId，设备启动和停止不阻塞 Voice Actor。SessionResources 与取消令牌只存在于 Actor 的私有执行资源区，不进入 Runtime。Start、Finalize 和 Pending workflow 只持有不可变 Job 或移交资源，Streaming Pipeline 只发送展示数据。Finalization 在注入前向 Actor 请求带 SessionId 的授权，Actor 再推进 Pasting；过期 Activation 或 Worker Outcome 不修改当前会话。

代码按职责拆为 façade/contract、Actor mailbox、Runtime/reducer、Effect executor、Audio Actor、start/finalize/pending workflows、Presenter 和 Incident；模块路径仍保持 `crate::voice_controller`，Tauri command 名称与主要响应保持兼容。Presenter 是语音模块唯一 Tauri 状态事件和 Overlay 出口。严格所有权边界见 [ADR-0013](adr/0013-strict-mailbox-owned-voice-runtime.md)。

### ShortcutManager

`ShortcutManager` 串行化快捷键 edit、启停、系统恢复和关闭，负责权威校验、配置事务、运行时绑定切换及失败回滚。有焦点的设置录入由前端 feature 负责；Manager 不生成逐键候选。

快捷键运行错误的权威副本位于 Manager 的协调状态；它还通过无确认的控制消息把错误投影到 Voice 状态快照用于 UI 展示。该投影不是语音输入可用性的权威，控制队列拥塞时可能暂时陈旧。

`WindowsKeyboardEngine` 只负责应用运行期间的全局物理快捷键监听。`ShortcutManager` 为一次按下创建 `ActivationId`，并通过 `VoiceTriggerPort` 配对 begin/finish；Hook 中断转换为同一 Activation 的 cancel。稳定职责边界见 [ADR-0010](adr/0010-separate-focused-shortcut-editing.md) 与 [ADR-0012](adr/0012-unified-voice-input-control-plane.md)。

Manager 事务、Windows Worker、generation 回执、失败回滚和诊断字段的完整说明见 [热键录入、换绑事务与 Windows 运行时链路](shortcut-editing.md)。

### StreamingPipeline

CPAL 录音以约 200ms chunk 写入容量 32 的有界队列。队列 Full 触发一次性 overflow 并取消，不静默丢帧；Closed 不误报 overflow。partial 通过 watch/latest-value 中继，final 通过 provider task 的 oneshot 结果返回。

### DeliveryService

交付顺序固定为：文本验证 → 目标身份/前台复验 → 注入 → 成功提交历史 → 触发热词整理。`PendingOutputService` 独占最多 5 条、TTL 10 分钟的内存队列，并用租约防止复制、丢弃和重新交付重复消费。Pending 重新交付由 Actor 与新会话开始串行化；成功注入仍是不可回滚的提交点。

### Repository Ports + Adapters

`repositories.rs` 定义 ConfigRepository、CredentialStore、HistoryRepository、HotwordRepository 和 HotwordAgentClient。生产适配器继续复用原子 JSON、Windows Credential Manager 和同一个 SQLite 文件。

## IncidentVault

`IncidentSink` exposes only `try_emit`, health, and bounded shutdown; production initialization degrades to `NoopIncidentSink` on failure. The voice path pushes into capacity 64 control and capacity 64 audio `ArrayQueue` instances. Dropped audio uses an independent capacity 64 lock-free gap-marker queue, so completeness does not depend on spare control-queue capacity. `Bytes` and `Arc<str>` clones share PCM and attempt identity.

One OS thread owns the write-path `incident.db` SQLite WAL connection and artifact handles. It catches writer panic, drains accepted events during the 500ms bounded shutdown window, checkpoints PCM, and performs retention/capacity maintenance. User-initiated list/copy/play/export/delete commands open short-lived isolated connections; they never participate in capture, ASR, delivery, or formal-history decisions.

Consent is a persisted per-attempt snapshot with separate content, audio, and text authorization bits. Restart recovery indexes only audio-authorized `.pcm.part` files as `truncated`; orphan/unauthorized files are removed. Artifact paths are single-component, reads verify SHA-256, deletion failures keep indexes retryable, and all persisted diagnostic messages use the shared redactor.

The normal history repository and schema remain separate. Successful delivery discards recovery material after a formal-history commit or when history is disabled; a history write failure keeps recovery data. Query/export access is main-window-only, audio is binary IPC, and ZIP fields are allowlisted so choosing transcript text does not implicitly export target-app context.
