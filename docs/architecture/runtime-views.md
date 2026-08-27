---
{"documentType":"runtime-view","viewStatus":"current"}
---

# 运行时与部署视图

## 语音输入主链路

[component:backend.shortcut] [component:backend.voice-controller] [component:backend.streaming] [component:frontend.overlay] [component:backend.delivery]

```mermaid
sequenceDiagram
    actor User as 用户
    participant HK as Shortcut Adapter
    participant VC as VoiceSessionController
    participant WIN as Windows Target Adapter
    participant MIC as CPAL / Audio Queue
    participant ASR as Streaming ASR
    participant OVL as Preinput Overlay
    participant DEL as DeliveryService
    participant APP as 目标应用
    participant DB as History / Hotwords

    User->>HK: Pressed
    HK->>VC: SessionEvent::Pressed
    VC->>WIN: 捕获 HWND/PID/创建时间/EXE
    VC->>MIC: start_streaming(200ms, capacity=32)
    VC->>ASR: 建立已授权 WSS 会话
    VC->>OVL: begin + Recording
    loop 录音期间
        MIC->>ASR: PCM chunk（有界背压）
        ASR-->>VC: latest partial
        VC-->>OVL: emit_to(preinput, session_id + seq)
    end
    User->>HK: Released
    HK->>VC: SessionEvent::Released
    VC->>MIC: stop_streaming / final chunk
    VC->>ASR: 等待 final（有超时与取消）
    ASR-->>VC: final text
    VC->>DEL: validate(text, captured target)
    DEL->>WIN: 复验身份 + 当前前台 HWND
    alt 目标和文本有效
        DEL->>APP: Unicode SendInput 或显式兼容模式
        APP-->>DEL: 注入成功
        DEL->>DB: 写历史；随后触发热词整理
        DEL-->>VC: Delivered
    else 目标变化、文本无效或注入失败
        DEL-->>VC: PendingOutput（内存，5 条，TTL 10 分钟）
        Note over APP,DB: 不写目标应用、不写历史、不学习热词
    end
    VC-->>OVL: hide current session
```

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

- 120 秒 deadline 和真实 Released 进入相同的幂等 finalize 路径。
- 音频队列 Full、用户取消、provider 拒绝或控制通道失败都会取消当前会话并禁止交付。
- 旧 session 的 provider 完成只能记录，不能修改当前状态或产生文本副作用。
- provider final 最长等待路径由 preview 是否存在决定；取消令牌可提前终止等待。

## Pending 手动交付

```mermaid
sequenceDiagram
    actor User as 用户
    participant UI as Pending Panel
    participant CMD as Session Command
    participant DEL as DeliveryService
    participant WIN as Windows Target Adapter
    participant APP as 原目标应用

    User->>UI: 发送到原窗口
    UI->>CMD: deliver_pending_output(id)
    CMD->>DEL: deliver_pending(id, activate=true)
    DEL->>WIN: 重新验证目标身份
    alt 目标仍有效
        WIN->>APP: 激活原窗口
        DEL->>APP: 按应用策略注入
        DEL-->>UI: 成功并移除 Pending
    else 目标失效或注入失败
        DEL-->>UI: 结构化错误；Pending 保留
    end
```

“复制文本”是用户主动替换剪贴板，不自动恢复，也不自动写历史；“丢弃”只移除内存项。

## 配置与凭据事务

[component:backend.commands] [component:backend.services] [component:backend.repositories] [component:storage.local]

```mermaid
sequenceDiagram
    participant UI as Main WebView
    participant CMD as Config Command
    participant NATIVE as NativeConfirmation
    participant CFG as ConfigService
    participant KEY as CredentialStore
    participant JSON as ConfigRepository

    UI->>CMD: mutation(expectedRevision)
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
            CFG-->>UI: 新配置与新 revision
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
