# ADR-0010：分离有焦点的设置录入与全局运行时监听

- Status: Accepted
- Date: 2026-08-26
- Deciders: Project maintainers
- Drivers: 快捷键换绑必须即时显示候选，同时保持运行时应用、持久化和失败回滚的后端权威
- Related features: FEAT-SHORTCUT-BINDING
- Assumptions: 当前有焦点输入边界能够提供产品要求的左右物理键粒度
- Evidence: 702676870f2c4289c79cef24f2fe5d96777505b7; shortcut capture misalignment postmortem
- Supersedes: None
- Superseded by: None

## Context

旧方案让正式 Windows 全局 Hook 同时承担设置录入，候选必须经过后端生命周期事件才能显示。真实使用中，UI 可以进入 `capturing`，但没有按键候选；事件重试、operation ID 和轮询只能加固错误边界，不能提供本地即时反馈。

## Decision

- 有焦点的设置输入边界负责观察用户按键、形成完整候选并立即反馈。
- 后端负责权威校验、edit 事务、运行时绑定切换、持久化和失败回滚。
- 全局运行时监听器只匹配已经提交的物理绑定，并产生运行时 Pressed/Released；它不生成设置候选。
- 设置反馈不得等待异步后端往返，也不得依赖正式全局监听器的健康状态。
- 具体 DOM、WebView2、Tauri 或 Win32 API 属于当前实现，不是本决策的永久技术约束。

## Consequences

### Positive

- 点击和逐键反馈不再跨越异步后端链路。
- 编辑面和运行面可以独立失败、诊断和测试。
- 后端继续掌握实际运行绑定和配置事务，不牺牲回滚一致性。

### Negative

- 前后端都需要验证候选：前端用于即时反馈，后端用于权威提交。
- 有焦点输入边界与 Windows 物理模型之间需要维护明确映射。
- 真实 WebView2 焦点和键盘布局行为仍需要目标环境验收。

## Alternatives considered

- Hook 同时承担设置录入和运行时触发：反馈路径过长，Hook 健康、队列、事件桥和轮询会污染本地编辑体验。
- 等 begin IPC 成功后才进入录入：把后端可用性和延迟变成用户看到按键的前置条件。
- 增加二次保存按钮：不能解决候选来源错误，也改变了已经确认的自动提交体验。

## Revisit when

- 当前有焦点输入边界无法区分产品要求的物理键或左右修饰键；
- WebView 平台更换后不能在不依赖正式全局监听器的情况下可靠观察输入；
- 产品明确要求无焦点设置录入，并接受相应的全局输入与安全边界。
