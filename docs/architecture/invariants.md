# 架构不变量

本页集中列出会直接影响安全、背压和用户数据副作用的实现常量。机器可读事实位于 [architecture-facts.json](architecture-facts.json)；CI 会从 Rust 常量读取实际值，并检查下面的 fact marker 与指定叙事仍包含相同值。

| Fact ID | 当前值 | 代码事实源 | 约束意义 |
| --- | --- | --- | --- |
| `voice.control-queue-capacity` [fact:voice.control-queue-capacity] | 16 events | `voice_controller/mod.rs::CONTROL_QUEUE_CAPACITY` | 控制事件积压超过上限时失败关闭 |
| `voice.audio-control-queue-capacity` [fact:voice.audio-control-queue-capacity] | 4 commands | `voice_controller/audio_actor.rs::AUDIO_CONTROL_CAPACITY` | 设备启停由独立单所有者执行器串行处理 |
| `voice.audio-queue-capacity` [fact:voice.audio-queue-capacity] | 32 chunks | `voice_controller/workflow/start.rs::AUDIO_QUEUE_CAPACITY` | 不允许用无界内存掩盖 provider 过慢 |
| `voice.recording-deadline-seconds` [fact:voice.recording-deadline-seconds] | 120 秒 | `voice_controller/actor.rs::MAX_RECORDING_SECS` | 单次录音必须自动 finalize |
| `voice.stream-chunk-milliseconds` [fact:voice.stream-chunk-milliseconds] | 200ms | `voice_controller/workflow/start.rs::STREAM_CHUNK_MS` | 决定音频队列可缓冲的时间尺度 |
| `voice.final-transcript-timeout-seconds` [fact:voice.final-transcript-timeout-seconds] | 25 秒 | `voice_controller/workflow/finalize.rs::FINAL_TRANSCRIPT_TIMEOUT_SECS` | 有 preview 时等待最终文本的上限 |
| `voice.empty-transcript-timeout-milliseconds` [fact:voice.empty-transcript-timeout-milliseconds] | 800ms | `voice_controller/workflow/finalize.rs::EMPTY_TRANSCRIPT_TIMEOUT_MS` | 无 preview 时快速结束空输入 |
| `delivery.max-output-characters` [fact:delivery.max-output-characters] | 8000 Unicode characters | `target.rs::MAX_OUTPUT_CHARACTERS` | 限制一次自动文本副作用规模 |
| `delivery.max-pending-outputs` [fact:delivery.max-pending-outputs] | 5 outputs | `target.rs::MAX_PENDING_OUTPUTS` | 满时拒绝新录音，绝不覆盖旧结果 |
| `delivery.pending-output-ttl-seconds` [fact:delivery.pending-output-ttl-seconds] | 600 秒（10 分钟） | `target.rs::PENDING_OUTPUT_TTL` | Pending 只做短期内存恢复 |
| `provider.max-websocket-frame-bytes` [fact:provider.max-websocket-frame-bytes] | 1 MiB | `provider/volcengine.rs::MAX_WS_FRAME_PAYLOAD` | 拒绝异常大的服务端 frame |
| `provider.raw-frame-queue-capacity` [fact:provider.raw-frame-queue-capacity] | 4 frames | `provider/volcengine.rs::RAW_WS_FRAME_QUEUE_CAPACITY` | reader 等待形成 TCP 背压 |
| `incident.control-queue-capacity` [fact:incident.control-queue-capacity] | 64 events | `incident/mod.rs::CONTROL_QUEUE_CAPACITY` | 语义事件只做有界无锁投递 |
| `incident.audio-queue-capacity` [fact:incident.audio-queue-capacity] | 64 chunks | `incident/mod.rs::AUDIO_QUEUE_CAPACITY` | Vault 变慢时不得把背压传给 ASR |
| `incident.audio-gap-queue-capacity` [fact:incident.audio-gap-queue-capacity] | 64 markers | `incident/mod.rs::AUDIO_GAP_QUEUE_CAPACITY` | 控制队列饱和时仍保留音频缺口完整性标记 |

## 行为边界

- `INV-SC-01`：设置页一旦在本地输入边界观察到按键，必须立即向用户反馈；该反馈不得依赖正式全局监听器的健康状态或异步后端往返。对应产品行为见 [FEAT-SHORTCUT-BINDING](../features/shortcut-binding.md)，决策依据见 [ADR-0010](adr/0010-separate-focused-shortcut-editing.md)。

## 更新规则

1. 先修改代码与测试，再修改 `architecture-facts.json`。
2. 修改所有 `mentions` 指向的叙事；校验器会验证模板展开后的值。
3. 不要在普通文档中新建未登记的关键容量或时限；新增不变量时同时增加 fact。
4. 运行 `npm run architecture:check`。
