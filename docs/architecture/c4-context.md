---
{"documentType":"c4-view","viewStatus":"current"}
---

# C4 L1：系统上下文

[component:system.zephyr] [component:external.asr] [component:external.hotword-agent] [component:platform.windows]

本图描述 GY Typing（界面品牌 Zephyr）的系统边界。它是用户 Windows 会话中的本地桌面应用，不提供本地 HTTP 控制面。

```mermaid
flowchart LR
    user["Person<br/>Windows 用户<br/><small>按住全局热键进行语音输入并管理设置</small>"]
    target["External Software System<br/>目标桌面应用<br/><small>接收最终 Unicode 输入或显式兼容粘贴</small>"]
    windows["External Platform<br/>Windows<br/><small>热键、窗口身份、SendInput、OLE、Credential Manager</small>"]
    asr["External Software System<br/>流式 ASR<br/><small>Volcengine-compatible WSS</small>"]
    agent["External Software System<br/>热词 Agent<br/><small>DeepSeek-compatible HTTPS；可选</small>"]
    zephyr["Software System<br/>GY Typing / Zephyr<br/><small>本地采音、预览、目标复验和文本交付</small>"]

    user -->|"按下/释放热键；操作主窗口"| zephyr
    zephyr -->|"最终文本；仅在目标身份验证通过后"| target
    zephyr -->|"调用本机 API；秘密存入系统凭据库"| windows
    zephyr -->|"授权 origin 后发送 PCM 音频流；接收 partial/final 文本"| asr
    zephyr -->|"授权 origin 后发送词语/上下文整理请求；不发送原始音频"| agent
```

## 信任与数据边界

- **本机信任边界**：主 WebView 通过 Tauri IPC 调用 Rust；敏感 command 复核调用窗口 label。preinput WebView 只有悬浮展示所需能力。
- **目标应用边界**：自动交付前复验 HWND、PID、进程创建时间、可执行文件名和前台窗口；目标变化时不写入任何窗口。
- **ASR 边界**：音频只发往被授权的 ASR origin。官方 origin 默认信任，自定义 origin 首次携带凭据前必须经过 Rust 发起的 Windows 原生确认。
- **Agent 边界**：热词 Agent 是独立 purpose 的授权 origin；它不接收原始音频。
- **本地数据边界**：非秘密配置、历史/热词和凭据分别进入 JSON、SQLite 和 Windows Credential Manager。

## 范围外

当前边界不包含离线 ASR、模型下载、OTA、会议录音、屏幕上下文、云同步或本地 HTTP API。
