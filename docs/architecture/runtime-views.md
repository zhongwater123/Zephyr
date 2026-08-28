---
{"documentType":"runtime-view","viewStatus":"current","sourceRevision":"b62667deab18f740c83bab2f1bcebae2fd0a59e2","worktreeState":"dirty","changedPaths":["src-tauri/src/voice_controller","src-tauri/src/voice_trigger.rs","src-tauri/src/voice_input_service.rs","src-tauri/src/streaming_pipeline.rs","src-tauri/src/lib.rs","docs/architecture/runtime-views.md"],"reviewStatus":"partial","reviewedAt":"2026-08-28","knownDeviations":["Starting 阶段的匹配 Release 只记录 finish_requested，不取消尚未完成的音频启动；慢设备或快速按放时，Recorder 可能在用户松开后才完成启动并随后 Stop。"]}
---

# 运行时与部署视图

## 语音输入主链路

[component:backend.shortcut] [component:backend.voice-controller] [component:backend.streaming] [component:frontend.overlay] [component:backend.delivery]

```mermaid
sequenceDiagram
    actor User as 用户
    participant HK as Shortcut Adapter
    participant VC as Voice Actor
    participant RT as Actor-owned VoiceRuntime
    participant START as Start Workflow
    participant AUD as AudioSessionActor
    participant WIN as Windows Target Adapter
    participant ASR as Streaming ASR
    participant PRE as Presenter / Overlay
    participant FIN as Finalize Workflow
    participant DEL as DeliveryService
    participant APP as 目标应用
    participant DB as History / Hotwords

    User->>HK: Pressed
    HK->>HK: 创建并持有 ActivationId
    HK->>VC: VoiceTriggerPort.begin(activation)
    VC->>RT: reducer 接受 Activation / Starting / 固定 config revision
    VC-->>HK: BeginReceipt -> Accepted/Rejected（快捷键不等待）
    VC->>PRE: begin + Starting
    VC->>START: StartJob(sessionId, config snapshot)
    START->>WIN: 捕获 HWND/PID/创建时间/EXE
    START->>START: 组合热词 + 构造 Provider
    START->>AUD: Start(sessionId, 200ms, data capacity=32)
    Note over AUD: AudioSessionActor mailbox 容量 4；独占 Recorder
    AUD-->>START: AudioStreamInfo / error
    START-->>VC: StartFinished(sessionId, PreparedSession)
    VC->>RT: reducer -> Recording 或立即 Stopping
    VC->>ASR: 启动 Provider task
    loop 录音期间
        AUD->>ASR: PCM chunk（有界背压）
        ASR-->>PRE: PresentationEvent（非权威字符进度）
    end
    User->>HK: Released
    HK->>VC: VoiceTriggerPort.finish(same ActivationId)
    alt 仍在 Starting
        VC->>RT: finish_requested=true
        Note over VC,RT: 当前实现等待 StartFinished 后再 Stop；Release 不丢失，但可能发生松开后才完成麦克风启动
    else Recording
        VC->>RT: reducer -> Stopping
    end
    VC->>AUD: Stop(sessionId)
    AUD-->>VC: AudioStopped(sessionId, duration)
    VC->>RT: reducer -> Transcribing
    VC->>FIN: move FinalizationJob（不含 Runtime）
    FIN->>ASR: 等待 final（有超时与取消）
    ASR-->>FIN: final text
    FIN->>DEL: validate(text, captured target)
    DEL->>WIN: 复验身份 + 当前前台 HWND
    alt 目标和文本有效
        FIN->>VC: ReadyToInject(sessionId)
        VC->>RT: 校验当前 session / enabled / cancellation
        VC-->>FIN: authorize + Pasting
        DEL->>APP: Unicode SendInput 或显式兼容模式
        APP-->>DEL: 注入成功
        DEL->>DB: 写历史；随后触发热词整理
        FIN->>VC: FinalizationFinished(Delivered)
        VC->>RT: metrics + complete
    else 目标变化、文本无效或注入失败
        DEL-->>FIN: delivery failure
        FIN->>VC: FinalizationFinished(Pending/Failed)
        VC->>RT: Pending（内存，5 条，TTL 10 分钟）+ complete/error
        Note over APP,DB: 不写目标应用、不写历史、不学习热词
    end
    VC->>PRE: hide current session
    Note over VC,FIN: reducer 是唯一控制状态迁移入口；过期结果无副作用
```

该图描述当前实现。`VoiceRuntime` 只保存 desired state、availability、阶段、当前 Session/Activation、revision、取消状态和指标，不保存 Recorder、Injector、Provider task 或 SessionResources。Runtime 由 Voice Actor mailbox task 按值持有，纯 reducer 产生 Effects；start/finalize/pending 和 Streaming worker 均不能访问 Runtime。Presenter 是唯一 Tauri 状态事件与 Overlay 出口，流式字符数只是展示进度，权威 `VoiceStatusSnapshot` 只由 Actor 发布。

### 快捷键录入与提交

- View status: `current`
- Feature: [FEAT-SHORTCUT-BINDING](../features/shortcut-binding.md)
- Decisions: [ADR-0010](adr/0010-separate-focused-shortcut-editing.md), [ADR-0011](adr/0011-capability-aware-effective-validation.md)
- Validation: `partial`

```mermaid
sequenceDiagram
    actor U as 用户
    participant FE as 有焦点的设置录入
    participant M as ShortcutManager
    participant RT as 全局运行时监听
    participant CFG as 配置存储

    U->>FE: 点击快捷键字段
    FE-->>U: 立即进入录入并显示本地观察到的按键
    FE->>M: 开始编辑（异步）
    M->>RT: 暂停旧绑定触发
    U->>FE: 输入、取消或完成候选
    FE-->>U: 持续显示完整候选或恢复旧显示
    alt 用户取消
        FE->>M: 取消编辑
        M->>RT: 恢复旧绑定
        M-->>FE: 原绑定未变化
    else 提交候选
        FE->>M: 提交候选与预期配置 revision
        M->>M: 校验候选
        M->>RT: 应用候选绑定
        M->>CFG: 以预期 revision 持久化
        alt 运行时与持久化均成功
            M-->>FE: 新绑定已生效或已保存待启用
            FE-->>U: 保留新值并反馈成功
        else 任一步失败
            M->>RT: 恢复权威旧绑定
            M-->>FE: 返回失败与恢复结果
            FE-->>U: 保留失败候选信息并恢复权威显示
        end
    end
```

设置录入负责即时反馈；后端负责校验、运行时应用、持久化和回滚；全局监听器只触发已提交绑定。具体 DOM 状态机、IPC 字段、Hook 恢复和日志排查见[非规范性 Implementation Guide](shortcut-editing.md)。

### 并发与终止条件

- 120 秒 deadline 和匹配当前 Activation 的真实 Released 进入相同的幂等 finalize 路径；迟到或其他 Activation 的 finish/cancel 被忽略。
- Starting 阶段的匹配 Release 只设置 `finish_requested`，不会取消 Start Workflow 或启动取消令牌；StartFinished 后才向 AudioSessionActor 发 Stop。这是当前实现事实，不是已验证的目标语义：慢设备或快速按放时可能在用户松开后才完成麦克风启动。
- 音频队列 Full、用户取消、provider 拒绝或控制通道失败都会取消当前会话并禁止交付。
- 旧 session 的 provider 完成只能记录，不能修改当前状态或产生文本副作用。
- provider final 最长等待路径由 preview 是否存在决定；取消令牌可提前终止等待。
- 每个已接受会话固定开始时的配置 revision、Provider 和注入策略快照；配置变更只影响后续会话。
- `desired_enabled`、Actor availability 与 shortcut health 是三个独立投影；Hook 错误不能把 Actor 改为 Disabled。
- 最后一个 Handle 释放后强 sender 消失，Worker 的 WeakSender 不维持 mailbox；应用退出另外显式 Shutdown Voice Actor，再由其 Shutdown Audio Actor。

## Pending 手动交付

```mermaid
sequenceDiagram
    actor User as 用户
    participant UI as Pending Panel
    participant CMD as Session Command
    participant ACT as VoiceSessionActor
    participant PEND as PendingOutputService
    participant WF as Pending Delivery Workflow
    participant DEL as DeliveryService
    participant WIN as Windows Target Adapter
    participant APP as 原目标应用

    User->>UI: 发送到原窗口
    UI->>CMD: deliver_pending_output(id)
    CMD->>ACT: DeliverPending(id)
    ACT->>ACT: 与 Begin 串行，确认无活动会话
    ACT->>PEND: reserve lease(id)
    ACT->>WF: move lease + immutable config
    WF->>DEL: validate + inject(activate=true)
    DEL->>WIN: 重新验证目标身份
    alt 目标仍有效
        WIN->>APP: 激活原窗口
        DEL->>APP: 按应用策略注入
        WF->>ACT: PendingDeliveryFinished(Delivered, lease)
        ACT->>PEND: complete(lease)
        ACT-->>UI: 成功并移除 Pending
    else 目标失效或注入失败
        WF->>ACT: PendingDeliveryFinished(Retained, lease)
        ACT->>PEND: drop lease / release(id)
        ACT-->>UI: 结构化错误；Pending 保留
    end
```

“复制文本”是用户主动替换剪贴板，不自动恢复，也不自动写历史；“丢弃”只移除内存项。复制、丢弃和重新交付共享同一租约，不能重复消费同一 Pending。

## 配置与凭据事务

[component:backend.commands] [component:backend.services] [component:backend.repositories] [component:storage.local]

```mermaid
sequenceDiagram
    participant UI as Main WebView
    participant CMD as Config Command
    participant NATIVE as NativeConfirmation
    participant CFG as ConfigService
    participant VCS as VoiceControlService
    participant ACT as Voice Actor
    participant HK as ShortcutManager
    participant KEY as CredentialStore
    participant JSON as ConfigRepository

    UI->>CMD: save_config / set_enabled(expectedRevision)
    CMD->>CMD: 验证 window label == main
    opt 自定义 endpoint 首次授权
        CMD->>NATIVE: Windows 原生确认（带父窗口）
        NATIVE-->>CMD: allow / deny
    end
    CMD->>CFG: commit(expectedRevision, next, credentialUpdates)
    CFG->>CFG: 串行化 mutation + revision CAS
    alt revision 冲突
        CFG-->>UI: config_conflict + currentConfig
    else revision 匹配
        CFG->>KEY: 快照并事务式更新
        CFG->>JSON: 临时文件 + flush + sync_all + 原子替换
        alt JSON 保存失败
            CFG->>KEY: 恢复旧快照
            CFG-->>UI: structured storage error
        else 成功
            CFG-->>VCS: committed config + revision
            VCS->>ACT: SetAvailability(desired, committedRevision)
            opt 禁用且存在活动会话
                ACT->>ACT: reducer 拒绝新 Begin + 取消资源
                ACT->>ACT: 音频 Cancel 已进入 Audio Actor 邮箱
            end
            ACT-->>VCS: availability acknowledgment + desiredRevision
            alt Actor 未确认
                VCS-->>UI: voice_reconciliation_failed + committedRevision
                Note over UI,VCS: 配置不回滚；前端保留已提交意图，后续配置操作重试
            else Actor 已确认
                VCS->>HK: 协调 Hook enabled 状态
                alt Hook 安装失败
                    HK-->>ACT: shortcut health error
                    VCS-->>UI: 新配置与新 revision
                    Note over ACT,HK: Actor 仍 Available，其他触发入口不受影响
                else Hook 正常
                    VCS-->>UI: 新配置与新 revision
                end
            end
        end
    end
```

网络测试、录音和热词整理都必须在读取 Keyring 前重新确认 `scheme + host + effective port + purpose` 已授权。

## 部署视图

[component:system.zephyr] [component:platform.windows] [component:storage.local] [component:external.asr] [component:external.hotword-agent]

```mermaid
flowchart LR
    subgraph pc["Deployment Node: Windows 用户会话"]
        exe["gy-typing.exe<br/><small>Tauri + Rust + Tokio</small>"]
        webview["WebView2 Runtime<br/><small>Main + Preinput</small>"]
        cfg[("App config dir<br/>config.json / .bak")]
        db[("App config dir<br/>history.db + WAL")]
        incident[("LocalAppData/gy-typing/incidents<br/>incident.db + artifacts")]
        cred[("Windows Credential Manager")]
        apps["目标桌面应用"]
        exe <--> webview
        exe --> cfg
        exe --> db
        exe --> incident
        exe --> cred
        exe --> apps
    end
    exe -->|"TLS / WSS"| asr["Volcengine-compatible ASR"]
    exe -->|"TLS / HTTPS，可选"| agent["DeepSeek-compatible Agent"]
```

应用是 Windows-only 单机部署。开发时 Vite 运行在 localhost:1420；生产包只加载本地前端资源。

## 异常恢复旁路

[component:backend.incident-vault] [fact:incident.control-queue-capacity] [fact:incident.audio-queue-capacity] [fact:incident.audio-gap-queue-capacity]

```mermaid
sequenceDiagram
    participant Audio as Audio callback
    participant ASR as ASR queue
    participant Sink as IncidentSink
    participant Gap as Gap marker queue
    participant Vault as Vault OS thread
    participant DB as incident.db/artifacts
    Audio->>ASR: try_send(Bytes clone)
    Audio->>Sink: try_emit(AudioChunk, capacity=64)
    alt audio queue accepted
        Sink-->>Audio: Accepted
    else audio queue full
        Sink->>Gap: push Arc<str> attempt ID, capacity=64
        Sink-->>Audio: Dropped + atomic counter
    end
    Vault->>Gap: drain completeness markers
    Vault->>DB: SQLite WAL events + sequential PCM append
    Note over Audio,ASR: no file, SQLite, JSON, wait, or Vault mutex on the voice path
```

partial checkpoint 最多每 500ms 一次；provider canonical final 在成功终止事件之前单独保存。Writer 在处理 `AttemptEnded` 前先排空 gap 与已接收音频，正常退出最多等待 500ms；writer panic 被自身线程捕获并只更新 health。

成功交付时，正式历史提交成功或历史被策略关闭都会删除恢复材料；正式历史写入失败则保留材料并记录 `history_write_failed`。失败材料默认 7 天，内容无关聚合默认 30 天。启动时未结束会话转为 `interrupted`：只有持久化音频子授权的 `.pcm.part` 会封存为 `truncated`，孤儿和未授权文件被删除。panic emergency 合法行导入后移除，坏行或失败行留待重试。

前端 History Dialog 通过独立 `incidentApi` 聚合恢复项。音频以二进制 IPC 转 WAV；Blob URL 在替换和卸载时撤销。诊断 ZIP 默认不含文本、音频或普通日志；勾选文本只加入 partial/final，不隐式加入 target app。普通日志选项当前截取最近日志尾部、上限 256KB，并统一脱敏。
