# ADR-0013：严格 mailbox-owned 语音运行时

- Status: Accepted
- Date: 2026-08-27
- Deciders: Project maintainers
- Drivers: 关闭 ADR-0012 实现偏差，避免控制面与异步执行层共同写入会话状态
- Related features: FEAT-VOICE-INPUT-CONTROL-PLANE
- Assumptions: 当前只允许一个活动语音会话；设备调用必须隔离在音频执行器，不能阻塞 Voice Actor mailbox
- Evidence: `actor/runtime.rs` 的纯控制状态、`actor/reducer.rs` 与 Effect 子模块、容量 4 的 `audio_actor.rs`、拆分后的 start/finalize/pending workflows；153 项 Rust 测试、46 项前端测试、106/106 源码追踪与 15 条架构不变量检查
- Supersedes: None
- Superseded by: None

## Context

ADR-0012 已统一外部触发和服务边界，但当前实现仍把模块私有的 `Arc<Mutex<VoiceRuntime>>` 传给 finalize、故障复位和 Pending 交付任务。外部调用者不能写 Runtime，不等于 Actor mailbox task 是唯一写入者。异步任务在文本注入等不可逆副作用附近直接迁移状态，会使取消、禁用、过期结果和会话完成之间缺少统一仲裁点。

## Decision

- `VoiceSessionActor` 按值持有 `VoiceRuntime`，只有 Actor mailbox loop 可以修改 Runtime、状态机、当前 Activation 和会话指标。
- `VoiceSessionHandle` 只持有容量 16 的消息 sender、只读状态 receiver 和失败关闭信号；状态 sender 属于 Actor。
- 公共命令和内部事件使用不同类型，经私有 Actor message 进入同一有界邮箱。
- 录音、ASR finalization、文本交付和 Pending 重投递作为执行 Workflow，只接收不可变 Job 或从 Actor 移交的资源，并返回带 SessionId 的类型化 Outcome。
- Worker、Streaming Pipeline、Commands、Platform 和 Shortcut 层不得依赖 `VoiceRuntime`、`AppStateMachine`，不得捕获 Runtime 或直接发布权威状态。
- Actor 维护唯一权威 `VoiceStatusSnapshot`；流式字符数和 Overlay 更新属于展示事件。

本决策细化而不替代 ADR-0012；ADR-0012 继续规定统一控制面及外围所有权，本文规定控制面内部的严格单写入实现。

## Consequences

### Positive

- 会话状态修改可以通过类型可见性和递归架构测试定位到 Actor mailbox task 及其私有 reducer，而不依赖单文件行数判断。
- 过期 Activation、过期 Worker 结果、禁用与完成竞争在同一仲裁点处理。
- 控制面和执行面可以独立测试，长耗时 ASR/Delivery 不阻塞 Actor 邮箱。

### Negative

- 异步流水线需要显式 Job、Outcome 和内部事件类型。
- 会话资源必须在 Recording、Finalizing 与完成阶段之间移动。

## Alternatives considered

- 只机械拆文件：保留共享 Runtime 和多写入者，无法关闭根因。
- 继续使用 Mutex 并约定模块内部可写：异步竞争仍存在。
- 把所有 ASR 和 Delivery 工作放进 Actor loop：会阻塞控制命令和失败关闭。

## Revisit when

需要并行语音会话、Worker 跨进程执行，或 Actor 邮箱在目标环境形成可复现吞吐瓶颈时重新评估。
