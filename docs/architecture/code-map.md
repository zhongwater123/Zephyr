# 代码地图

本页是人工导航视图；机器可读关系位于 [code-map.json](code-map.json)。稳定组件 ID 不随文件拆分轻易改变。

| 组件 ID | 责任与边界 | 主要源码入口 | 主视图 / 决策 |
| --- | --- | --- | --- |
| `system.zephyr` [component:system.zephyr] | Windows 语音输入助手整体，公开边界是本机 UI、全局热键和受控外部服务访问 | [README](../../README.md), [启动入口](../../src-tauri/src/lib.rs) | [L1](c4-context.md), [ADR-0001](adr/0001-tauri-local-desktop-boundary.md) |
| `frontend.entry` [component:frontend.entry] | 按窗口查询参数动态装载主界面或轻量悬浮窗 | [main.tsx](../../src/main.tsx) | [前端 L3](c4-components-frontend.md) |
| `frontend.shell` [component:frontend.shell] | 主窗口布局、配置快照、全局语音状态和通知 | [AppShell](../../src/app/AppShellV2.tsx), [revision hook](../../src/app/useRevisionedConfigMutation.ts) | [前端 L3](c4-components-frontend.md) |
| `frontend.features` [component:frontend.features] | History、Hotwords、Pending、Settings 的局部交互，以及有焦点的快捷键编辑意图与本地反馈状态 | [features](../../src/features) | [前端 L3](c4-components-frontend.md), [非规范性实现指南](shortcut-editing.md), [ADR-0010](adr/0010-separate-focused-shortcut-editing.md) |
| `frontend.presentation` [component:frontend.presentation] | 共享样式和按需加载的 Three.js ASCII 视觉 bundle | [styles](../../src/styles.css), [ZephyrAsciiField](../../src/ZephyrAsciiField.tsx) | [前端 L3](c4-components-frontend.md) |
| `frontend.overlay` [component:frontend.overlay] | 仅展示定向的预输入 payload，不持有敏感配置能力 | [PreInputOverlay](../../src/preinput/PreInputOverlay.tsx) | [前端 L3](c4-components-frontend.md), [运行时](runtime-views.md) |
| `frontend.ipc` [component:frontend.ipc] | Tauri command 的类型化调用面、DTO 和前端安全模型 | [IPC client](../../src/ipc/client.ts), [domain](../../src/domain.ts), [security model](../../src/security-model.ts) | [容器图](c4-container.md) |
| `backend.bootstrap` [component:backend.bootstrap] | 组装 Tauri、托盘、窗口、managed state、handler 和退出清理 | [lib.rs](../../src-tauri/src/lib.rs), [tray](../../src-tauri/src/platform/tray.rs) | [后端 L3](c4-components-backend.md) |
| `backend.commands` [component:backend.commands] | 校验窗口 label、解析 IPC 参数、调用服务并映射结构化错误 | [commands](../../src-tauri/src/commands), [CommandError](../../src-tauri/src/command_error.rs) | [后端 L3](c4-components-backend.md) |
| `backend.services` [component:backend.services] | `AppServices`、语音输入应用协调器、配置单所有者和凭据安全的 provider 构造 | [services.rs](../../src-tauri/src/services.rs), [voice input service](../../src-tauri/src/voice_input_service.rs) | [后端 L3](c4-components-backend.md), [ADR-0004](adr/0004-trust-before-credentials.md) |
| `backend.voice-controller` [component:backend.voice-controller] | 容量 16 的 Actor 单写入控制面、纯 reducer/Effects、容量 4 的音频执行器、类型化 start/finalize/pending workflow 与统一 Presenter | [voice_controller](../../src-tauri/src/voice_controller), [state](../../src-tauri/src/state.rs) | [运行时](runtime-views.md), [ADR-0002](adr/0002-single-owner-bounded-voice-session.md), [ADR-0013](adr/0013-strict-mailbox-owned-voice-runtime.md) |
| `backend.streaming` [component:backend.streaming] | 采音、有界音频队列、ASR WebSocket、latest preview 与 overflow | [pipeline](../../src-tauri/src/streaming_pipeline.rs), [audio](../../src-tauri/src/audio.rs), [provider core](../../src-tauri/src/provider.rs), [provider model](../../src-tauri/src/provider_model.rs), [Volcengine adapter](../../src-tauri/src/provider/volcengine.rs), [preview](../../src-tauri/src/preview.rs) | [后端 L3](c4-components-backend.md), [ADR-0002](adr/0002-single-owner-bounded-voice-session.md) |
| `backend.delivery` [component:backend.delivery] | 目标与文本复验、单一自动事务仲裁、三态回执、Pending 降级及成功后的副作用提交 | [delivery](../../src-tauri/src/delivery.rs), [transaction service](../../src-tauri/src/clipboard_transaction.rs), [protocol](../../src-tauri/crates/paste-protocol), [target model](../../src-tauri/src/target.rs), [target port](../../src-tauri/src/target_port.rs) | [运行时](runtime-views.md), [ADR-0003](adr/0003-delivery-commit-point-and-pending-output.md), [ADR-0018](adr/0018-owned-clipboard-transaction-and-isolated-paste.md) |
| `backend.shortcut` [component:backend.shortcut] | 快捷键校验与可回滚提交事务，以及已提交绑定的 Windows 全局物理键监听 | [manager](../../src-tauri/src/shortcut_manager), [physical model](../../src-tauri/src/physical_shortcut.rs), [Windows engine](../../src-tauri/src/windows_keyboard.rs) | [后端 L3](c4-components-backend.md), [运行时](runtime-views.md), [非规范性实现指南](shortcut-editing.md), [ADR-0010](adr/0010-separate-focused-shortcut-editing.md) |
| `backend.repositories` [component:backend.repositories] | Repository/Credential/Agent 接口及 JSON、SQLite、Keyring 生产适配器 | [repositories](../../src-tauri/src/repositories.rs), [config](../../src-tauri/src/config.rs), [history](../../src-tauri/src/history.rs), [hotwords](../../src-tauri/src/hotwords.rs) | [后端 L3](c4-components-backend.md), [ADR-0005](adr/0005-revisioned-atomic-local-storage.md) |
| `backend.incident-vault` [component:backend.incident-vault] | 与正式历史隔离的无锁异常事件入口、SQLite writer、恢复材料和本地诊断导出 | [incident module](../../src-tauri/src/incident), [incident commands](../../src-tauri/src/commands/incident.rs) | [后端 L3](c4-components-backend.md), [运行时](runtime-views.md), [事件字典](incident-event-dictionary.md), [ADR-0008](adr/0008-incident-vault-isolated-recovery.md) |
| `platform.windows` [component:platform.windows] | 原生确认框、托盘、窗口身份、悬浮窗定位，以及隔离 sidecar 内的 Win32 剪贴板事务与按键提交 | [platform](../../src-tauri/src/platform.rs), [helper](../../src-tauri/crates/zephyr-paste-helper), [Windows target](../../src-tauri/src/windows_target.rs), [keyboard engine](../../src-tauri/src/windows_keyboard.rs) | [部署视图](runtime-views.md), [ADR-0018](adr/0018-owned-clipboard-transaction-and-isolated-paste.md) |
| `storage.local` [component:storage.local] | 原子 JSON + 备份、SQLite WAL、Windows Credential Manager | [config](../../src-tauri/src/config.rs), [history](../../src-tauri/src/history.rs), [hotwords](../../src-tauri/src/hotwords.rs), [IncidentVault](../../src-tauri/src/incident) | [容器图](c4-container.md), [ADR-0005](adr/0005-revisioned-atomic-local-storage.md) |
| `external.asr` [component:external.asr] | Volcengine-compatible WSS 流式识别边界，只接收被授权 origin 的音频请求 | [provider core](../../src-tauri/src/provider.rs), [provider model](../../src-tauri/src/provider_model.rs), [Volcengine adapter](../../src-tauri/src/provider/volcengine.rs) | [L1](c4-context.md), [ADR-0004](adr/0004-trust-before-credentials.md) |
| `external.hotword-agent` [component:external.hotword-agent] | DeepSeek-compatible HTTPS 热词整理边界，不接收原始音频 | [agent adapter](../../src-tauri/src/repositories.rs), [hotword domain](../../src-tauri/src/hotwords.rs) | [L1](c4-context.md), [ADR-0004](adr/0004-trust-before-credentials.md) |

## 机器可读元数据

Schema v2 为每个组件记录 `status`、`owner`、`dependsOn`、`publicContracts` 和 `changeTriggers`。这些字段不在人工表格中重复展开；请直接查看 [code-map.json](code-map.json) 或运行 `npm run architecture:impact`。

影响分析先匹配源码，再沿反向依赖传播。例如 `backend.streaming` 变化会提示复核依赖它的 `backend.voice-controller` 和更上层装配边界。

## 依赖方向

允许的主依赖方向是：

```text
WebView -> typed IPC -> commands -> AppServices/controller
                                -> repository traits -> production adapters
                                -> Windows adapters
```

`commands` 不直接读写 SQLite、JSON 或 Keyring。网络路径在读取凭据前必须完成 endpoint 授权检查。文本副作用只能通过 Delivery 边界提交。
