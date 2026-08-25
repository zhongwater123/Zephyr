# ADR-0002：采用单所有者、有界、失败关闭的语音会话

- Status: Accepted
- Date: 2026-08-24
- Deciders: Project maintainers
- Supersedes: None
- Superseded by: None

## Context

热键 Pressed/Released、120 秒 deadline、音频 overflow、用户取消和 provider 完成可能并发到达。无界队列会积累音频或控制事件；多个异步任务直接修改状态会让旧会话覆盖新会话或产生错误注入。

## Decision

由 `VoiceSessionController` 独占会话状态修改，通过容量 16 的控制通道串行处理 SessionEvent。音频通道容量 32，满时显式 overflow 并取消；partial 使用 watch/latest-value；provider final 通过任务结果返回。每个会话携带不可变 session ID 和取消令牌。deadline、Release 与取消进入幂等终止路径，旧 session 完成不得产生副作用。

控制通道满、receiver 退出或音频 overflow 时失败关闭：取消录音并禁止注入。

## Consequences

### Positive

- 竞态收敛到一个事件循环，可测试 finalize 和取消不变量。
- 内存与延迟有界，不静默提交丢帧音频。
- 旧任务无法改变当前会话。

### Negative

- 控制器仍需管理 recorder、provider task、overlay 和 metrics 的生命周期。
- 突发控制事件会主动取消，而不是排队等待。
- 队列容量与超时需要结合真实设备持续校准。

## Alternatives considered

- 每个回调直接持锁修改运行时：实现短，但竞态和锁域难以推理。
- 无界 mpsc：避免发送失败，却把过载转为内存和延迟问题。
- Actor/runtime 新依赖：当前 Tokio + Tauri 已足够，不增加运行时。

## Revisit when

支持并行会话、长会议录音，或需要跨进程音频流水线时重新评估。
