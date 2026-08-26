---
{"documentType":"c4-view","viewStatus":"current"}
---

# C4 L3：Rust 后端组件

[component:backend.bootstrap] [component:backend.commands] [component:backend.services] [component:backend.voice-controller] [component:backend.streaming] [component:backend.delivery] [component:backend.shortcut] [component:backend.repositories] [component:backend.incident-vault]

```mermaid
flowchart TB
    web["Main / Preinput WebViews"]

    subgraph rust["Container: Native Core (Rust)"]
        bootstrap["Component<br/>Bootstrap + Platform Startup<br/><small>lib.rs, platform/tray.rs</small>"]
        commands["Component<br/>Thin IPC Commands<br/><small>commands/*, CommandError</small>"]
        services["Component<br/>Application Services<br/><small>VoiceInputService, ConfigService, ProviderService</small>"]
        shortcut["Component<br/>Shortcut Manager<br/><small>binding transaction + runtime adapter</small>"]
        controller["Component<br/>VoiceSessionController<br/><small>bounded event loop, state, session</small>"]
        streaming["Component<br/>StreamingPipeline<br/><small>audio, provider, preview, overflow</small>"]
        delivery["Component<br/>DeliveryService<br/><small>target, text, inject, Pending, commit</small>"]
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
    shortcut -->|"Pressed / Released"| controller
    controller -->|"start / observe / finalize"| streaming
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

`VoiceInputService` 是语音输入启停和通用配置保存的应用层协调器。它按固定顺序协调配置 CAS、会话取消、运行状态、provider 重建、快捷键运行时和状态事件；对应 command 只保留窗口权限校验、参数转发和错误 DTO 映射。

### VoiceSessionController

容量 16 的控制通道顺序处理 Pressed、Released、DeadlineReached、AudioOverflow、CancelRequested 和 ProviderFinished。通道不可用时失败关闭：取消当前会话并禁止注入。

`SessionCoordinator` 记录当前 session ID、取消令牌、Pending 队列和最新 metrics。异步完成路径必须验证仍拥有当前 session。

### ShortcutManager

`ShortcutManager` 串行化快捷键 edit、启停、系统恢复和关闭，负责权威校验、配置事务、运行时绑定切换及失败回滚。有焦点的设置录入由前端 feature 负责；Manager 不生成逐键候选。

`WindowsKeyboardEngine` 只负责应用运行期间的全局物理快捷键监听，并把已提交绑定的匹配结果转换为 `SessionEvent::Pressed/Released`。稳定职责边界见 [ADR-0010](adr/0010-separate-focused-shortcut-editing.md)。

Manager 事务、Windows Worker、generation 回执、失败回滚和诊断字段的完整说明见 [热键录入、换绑事务与 Windows 运行时链路](shortcut-editing.md)。

### StreamingPipeline

CPAL 录音以约 200ms chunk 写入容量 32 的有界队列。队列 Full 触发一次性 overflow 并取消，不静默丢帧；Closed 不误报 overflow。partial 通过 watch/latest-value 中继，final 通过 provider task 的 oneshot 结果返回。

### DeliveryService

交付顺序固定为：文本验证 → 目标身份/前台复验 → 注入 → 成功提交历史 → 触发热词整理。验证或注入失败进入最多 5 条、TTL 10 分钟的内存 Pending 队列。成功注入是不可回滚的提交点。

### Repository Ports + Adapters

`repositories.rs` 定义 ConfigRepository、CredentialStore、HistoryRepository、HotwordRepository 和 HotwordAgentClient。生产适配器继续复用原子 JSON、Windows Credential Manager 和同一个 SQLite 文件。

## IncidentVault

`IncidentSink` exposes only `try_emit`, health, and bounded shutdown; production initialization degrades to `NoopIncidentSink` on failure. The voice path pushes into capacity 64 control and capacity 64 audio `ArrayQueue` instances. Dropped audio uses an independent capacity 64 lock-free gap-marker queue, so completeness does not depend on spare control-queue capacity. `Bytes` and `Arc<str>` clones share PCM and attempt identity.

One OS thread owns the write-path `incident.db` SQLite WAL connection and artifact handles. It catches writer panic, drains accepted events during the 500ms bounded shutdown window, checkpoints PCM, and performs retention/capacity maintenance. User-initiated list/copy/play/export/delete commands open short-lived isolated connections; they never participate in capture, ASR, delivery, or formal-history decisions.

Consent is a persisted per-attempt snapshot with separate content, audio, and text authorization bits. Restart recovery indexes only audio-authorized `.pcm.part` files as `truncated`; orphan/unauthorized files are removed. Artifact paths are single-component, reads verify SHA-256, deletion failures keep indexes retryable, and all persisted diagnostic messages use the shared redactor.

The normal history repository and schema remain separate. Successful delivery discards recovery material after a formal-history commit or when history is disabled; a history write failure keeps recovery data. Query/export access is main-window-only, audio is binary IPC, and ZIP fields are allowlisted so choosing transcript text does not implicitly export target-app context.
