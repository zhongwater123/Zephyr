# C4 L2：容器

[component:frontend.ipc] [component:backend.bootstrap] [component:storage.local] [component:external.asr] [component:external.hotword-agent] [component:platform.windows]

这里的 Container 是 C4 的可独立理解运行单元，不等同于操作系统进程。主 WebView、preinput WebView 和 Rust 核心随同一个 Tauri 桌面包部署；数据存储由本机设施提供。

```mermaid
flowchart TB
    user["Person<br/>Windows 用户"]
    target["External System<br/>目标桌面应用"]
    asr["External System<br/>Streaming ASR"]
    agent["External System<br/>Hotword Agent"]

    subgraph app["Software System: GY Typing / Zephyr"]
        main["Container<br/>Main WebView<br/><small>Preact + TypeScript<br/>设置、历史、热词、Pending、快捷键</small>"]
        overlay["Container<br/>Preinput WebView<br/><small>Preact + TypeScript<br/>轻量、定向语音预览</small>"]
        core["Container<br/>Native Core<br/><small>Rust + Tauri + Tokio<br/>IPC、会话、录音、provider、交付</small>"]
        config[("Container / Data Store<br/>config.json + .bak<br/><small>revision CAS；原子替换</small>")]
        sqlite[("Container / Data Store<br/>history.db<br/><small>SQLite WAL；历史与热词</small>")]
        incidents[("Container / Data Store<br/>incident.db + artifacts<br/><small>隔离恢复索引与短期材料</small>")]
        keyring[("External Local Store<br/>Windows Credential Manager<br/><small>service: gy-typing</small>")]
    end

    user -->|"UI 操作"| main
    user -->|"全局热键"| core
    main -->|"类型化 Tauri invoke；camelCase 参数"| core
    core -->|"emit_to(preinput)；session_id + seq"| overlay
    core -->|"Win32 SendInput / 显式 OLE 兼容模式"| target
    core -->|"原子读写"| config
    core -->|"Repository CRUD"| sqlite
    core -.->|"bounded try_emit + recovery commands"| incidents
    core -->|"授权检查后读取/事务式更新"| keyring
    core -->|"WSS PCM 流"| asr
    core -->|"HTTPS 整理请求"| agent
```

## 容器契约

| 容器 | 对外契约 | 不应承担 |
| --- | --- | --- |
| Main WebView | `src/ipc/client.ts` 中的类型化 commands；监听 voice/pending 事件 | 直接访问文件、数据库、Keyring 或远程服务 |
| Preinput WebView | 只读 overlay payload 与定向事件 | 配置、历史、凭据或外部请求 |
| Native Core | Tauri commands、全局热键、窗口/托盘、会话事件 | 暴露本地 HTTP 控制面 |
| config.json | schema version、revision、非秘密配置、信任和注入策略 | 保存 API key |
| history.db | 历史、热词、用户和应用上下文 | 保存原始音频 |
| incident.db + artifacts | 异常索引、授权后的短期音频/文本、内容无关指标 | 改变 ASR/注入/正式历史决策，或自动上传 |
| Credential Manager | 按凭据类型保存秘密 | 决定 endpoint 是否被授权 |

生产 CSP 禁止远程脚本、frame、object 和 base 重定向；开发 CSP 单独允许 Vite 本地服务。
