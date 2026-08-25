# ADR-0001：采用 Tauri 本地桌面边界

- Status: Accepted
- Date: 2026-08-24
- Deciders: Project maintainers
- Supersedes: None
- Superseded by: None

## Context

产品需要全局热键、低延迟麦克风、前台窗口身份、Unicode 输入、托盘、原生确认框和 Windows Credential Manager。设置与历史界面适合 Web 技术，但敏感能力不能由任意网页或本地 HTTP 调用者获得。

## Decision

以 Tauri 2 打包单机 Windows 应用。Preact WebViews 仅通过 Tauri IPC 调用 Rust；不建立 localhost HTTP 控制面。主窗口与 preinput 使用分离 capability，敏感 commands 在 Rust 再验证调用窗口 label。overlay 事件只定向发送到 preinput。

## Consequences

### Positive

- 本机能力留在 Rust/Windows 适配层，UI 可使用 Web 工程体系。
- 避免端口监听、Host/Origin/CSRF 和进程级 HTTP 凭据边界。
- 主窗口和悬浮窗可采用最小能力集。

### Negative

- Windows/WebView2/Tauri 生命周期耦合，需要 Windows 集成验收。
- IPC DTO 和 command 名称成为兼容契约。
- 不能把前端当普通浏览器页面独立运行全部功能。

## Alternatives considered

- 本地 FastAPI/HTTP 控制面：新增进程、端口和请求身份边界，当前收益不足以抵消风险。
- 全原生 UI：减少 WebView，但显著增加界面开发和维护成本。
- Electron：可实现 UI，但本机实时链路与分发体积不符合当前偏好。

## Revisit when

需要跨平台、远程控制、多进程隔离，或 Tauri 无法提供所需 Windows 能力时重新评估。
