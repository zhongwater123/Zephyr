# ADR-0012：统一语音输入控制面所有权

- Status: Accepted
- Date: 2026-08-27
- Deciders: Project maintainers
- Drivers: 为快捷键之外的触发入口建立稳定边界，消除会话、Provider、启停和 Pending 的多写入者
- Related features: FEAT-VOICE-INPUT-CONTROL-PLANE
- Assumptions: 当前只允许一个活动语音会话；快捷键是首个但不是永久唯一的触发适配器
- Evidence: 现有源码所有权审计；cargo test（134 passed）；架构边界回归测试；ADR-0002、ADR-0003、ADR-0005
- Supersedes: None
- Superseded by: None

## Context

现有 `VoiceSessionController` 已通过有界通道串行处理核心会话事件，但 `SharedRuntime` 同时暴露给 Commands、托盘、快捷键观察器和 Pending 交付路径。Provider、启停状态、快捷键错误和 Pending 容器存在多个直接写入者；快捷键 Pressed/Released 也没有能够识别一次激活的稳定身份。

这种结构会让配置更新、禁用、Pending 重发和迟到释放事件绕过会话仲裁，也会把快捷键 Hook 健康状态错误地提升为整个语音输入能力的状态。

## Decision

建立统一语音输入控制面：

- `VoiceSessionActor` 独占可变会话运行时，通过容量 16 的控制通道处理公共命令和独立的内部完成事件；外部只持有 `VoiceSessionHandle` 与只读状态快照。
- 所有触发适配器依赖 `VoiceTriggerPort`，使用不可变 `ActivationId` 配对 begin、finish 和 cancel。非当前 Activation 的终止事件没有副作用。
- Bootstrap 创建 Actor、Handle 和各适配器；ShortcutManager 不再创建控制器。
- ConfigService 独占配置与 revision；VoiceControlService 独占配置提交后的启停编排；ProviderService 独占 Provider 构造。会话接受 begin 时固定配置 revision 与 Provider 快照。
- PendingOutputService 独占 Pending 容器并提供互斥租约。Pending 交付由 Actor 与新会话开始串行化，注入成功后才删除。
- 快捷键绑定/Hook 健康由 ShortcutManager 持有，并与语音会话可用性分离。Hook 故障不得自动禁用其他触发入口。
- Commands、托盘和平台适配器不得直接访问或写入会话运行时。

本决策扩展 ADR-0002、ADR-0003 和 ADR-0005，不改变其有界失败关闭、注入提交点和配置 CAS 决策。

## Implementation conformance

当前符合性为 `deviating`，不是本 ADR 已完整落地的证明。外部 Commands、托盘、快捷键和 Delivery 已不能直接取得 Runtime；Activation、Provider 会话快照和 Pending 服务边界也已经建立。但 `VoiceRuntime` 仍由 Actor 与 release 后的 finalize task 通过模块私有 `Arc<Mutex<_>>` 共同修改，取消与不可逆注入之间没有 Actor 串行化的授权点；`VoiceControlService` 也会在 Actor 与快捷键运行时确认前提交配置。对应功能档案必须保持 `implementationStatus=in_progress`，直到这些偏差关闭并重新复核。

## Consequences

### Positive

- 新触发器只需适配稳定端口，不接触 ASR 或运行时锁。
- 会话、Provider、启停和 Pending 的写入边界可由类型可见性和架构测试强制。
- 迟到释放、并发配置和 Pending 重发具有明确仲裁位置。

### Negative

- 同步命令需要通过快照或请求/响应消息读取状态。
- 迁移期间需要调整 Bootstrap、Commands、托盘、流式回调和现有测试。
- Pending 租约与失败恢复增加少量内部状态。

## Alternatives considered

- 保留公开 SharedRuntime，只约定调用者自律：无法通过编译边界阻止新的旁路写入。
- 在 ShortcutManager 中增加更多触发类型：继续把产品入口与快捷键生命周期耦合。
- 复制 OpenWhispr 的主进程/渲染进程双状态：会重新引入跨边界真相分裂。

## Revisit when

需要并行语音会话、跨进程音频流水线、持久化 Pending，或控制通道容量与目标环境负载不匹配时重新评估。
