---
{
  "schemaVersion": 3,
  "featureId": "FEAT-VOICE-INPUT-CONTROL-PLANE",
  "confirmation": {
    "confirmedBy": "user",
    "confirmedAt": "2026-08-27",
    "sourceRef": "Codex task: voice input ownership unification plan explicitly approved for implementation on 2026-08-27"
  },
  "specStatus": "confirmed",
  "implementationStatus": "in_progress",
  "validationStatus": "partial",
  "implementationReview": {
    "status": "deviating",
    "sourceRevision": "38e54443bb4357771c9c789f83d5fc7e4ed3830c",
    "worktreeState": "dirty",
    "changedPaths": ["src-tauri/src/voice_controller.rs", "src-tauri/src/voice_input_service.rs", "src-tauri/src/shortcut_manager/mod.rs", "src-tauri/src/pending_output_service.rs", "src-tauri/src/voice_trigger.rs"],
    "reviewedAt": "2026-08-27",
    "summary": "外部 Runtime 访问边界已经收口，但 Actor 尚未独占全部会话 mutation，启停和触发接受语义仍有未闭环偏差。",
    "knownDeviations": [
      "release 后的 finalize task 持有 SharedRuntime 并直接修改会话状态，Actor 不是唯一写入者",
      "取消只在注入前检查一次，取消或禁用与不可逆文本注入之间仍有竞争窗口",
      "VoiceControlService 先提交配置再分别应用 Actor 与 ShortcutManager，失败时可能部分提交",
      "VoiceTriggerPort.begin 返回控制队列入队结果，而不是 Activation 接受结果"
    ]
  },
  "components": ["backend.bootstrap", "backend.commands", "backend.services", "backend.voice-controller", "backend.delivery", "backend.shortcut", "platform.windows"],
  "decisions": ["ADR-0002", "ADR-0003", "ADR-0005", "ADR-0012"],
  "validationSlices": [
    { "id": "AC-VI-01", "components": ["backend.shortcut", "backend.voice-controller", "platform.windows"], "requiredEvidence": ["automated", "runtime_hook"] },
    { "id": "AC-VI-02", "components": ["backend.voice-controller"], "requiredEvidence": ["automated", "fault_injection"] },
    { "id": "AC-VI-03", "components": ["backend.services", "backend.voice-controller"], "requiredEvidence": ["automated"] },
    { "id": "AC-VI-04", "components": ["backend.bootstrap", "backend.commands", "backend.services", "backend.voice-controller", "backend.delivery"], "requiredEvidence": ["automated"] },
    { "id": "AC-VI-05", "components": ["backend.voice-controller", "backend.delivery"], "requiredEvidence": ["automated", "fault_injection"] },
    { "id": "AC-VI-06", "components": ["backend.shortcut", "backend.voice-controller"], "requiredEvidence": ["automated"] }
  ],
  "evidence": [
    {
      "id": "EV-VI-BOUNDARY-20260827",
      "acceptanceIds": ["AC-VI-04"],
      "acceptanceCoverage": [{ "acceptanceId": "AC-VI-04", "coverage": "full" }],
      "method": "automated",
      "result": "pass",
      "freshness": "potentially_stale",
      "capabilities": ["automated"],
      "scope": "固定外部组件清单的源码扫描未发现 SharedRuntime、runtime.lock、runtime.provider、sessions.pending_outputs 或旧 SessionEvent 旁路。",
      "testRefs": ["tests::external_voice_layers_cannot_reach_mutable_runtime"],
      "limitations": ["字符串扫描不能证明 voice_controller 模块内部只有一个写入者", "固定文件清单不能自动覆盖未来新增适配器", "脏工作树证据没有不可变 build identity"],
      "sourceRevision": "38e54443bb4357771c9c789f83d5fc7e4ed3830c",
      "worktreeState": "dirty",
      "changedPaths": ["src-tauri/src/lib.rs", "src-tauri/src/commands", "src-tauri/src/platform/tray.rs", "src-tauri/src/shortcut_manager", "src-tauri/src/streaming_pipeline.rs", "src-tauri/src/delivery.rs"],
      "environment": "Windows development workspace; cargo test",
      "validatedAt": "2026-08-27"
    },
    {
      "id": "EV-VI-ACTIVATION-PRIMITIVES-20260827",
      "acceptanceIds": ["AC-VI-02", "AC-VI-06"],
      "acceptanceCoverage": [
        { "acceptanceId": "AC-VI-02", "coverage": "partial" },
        { "acceptanceId": "AC-VI-06", "coverage": "partial" }
      ],
      "method": "automated",
      "result": "partial",
      "freshness": "potentially_stale",
      "capabilities": ["automated"],
      "scope": "验证 ActivationId 唯一性、快捷键 PushToTalk 构造和纯 activation_matches 判断。",
      "testRefs": ["voice_trigger::tests::activation_ids_are_unique", "voice_trigger::tests::shortcut_activation_has_push_to_talk_semantics", "voice_controller::tests::only_the_active_activation_can_finish_a_session"],
      "limitations": ["没有启动真实 Actor", "没有覆盖 begin 接受回执、重复命令、迟到 finish/cancel 或并发故障注入", "没有第二个触发适配器契约测试"],
      "sourceRevision": "38e54443bb4357771c9c789f83d5fc7e4ed3830c",
      "worktreeState": "dirty",
      "changedPaths": ["src-tauri/src/voice_controller.rs", "src-tauri/src/voice_trigger.rs", "src-tauri/src/shortcut_manager/mod.rs"],
      "environment": "Windows development workspace; cargo test",
      "validatedAt": "2026-08-27"
    },
    {
      "id": "EV-VI-PENDING-LEASE-20260827",
      "acceptanceIds": ["AC-VI-05"],
      "acceptanceCoverage": [{ "acceptanceId": "AC-VI-05", "coverage": "partial" }],
      "method": "automated",
      "result": "partial",
      "freshness": "potentially_stale",
      "capabilities": ["automated"],
      "scope": "验证 PendingOutputService 的互斥 reserve、失败 release 和成功 complete。",
      "testRefs": ["pending_output_service::tests::reservation_prevents_duplicate_delivery", "pending_output_service::tests::failed_delivery_release_keeps_output", "pending_output_service::tests::committed_delivery_removes_output"],
      "limitations": ["没有覆盖 Pending 重投递与 Actor Begin 的真实串行", "没有注入提交点故障注入", "没有覆盖进程退出或 panic 后的租约恢复"],
      "sourceRevision": "38e54443bb4357771c9c789f83d5fc7e4ed3830c",
      "worktreeState": "dirty",
      "changedPaths": ["src-tauri/src/pending_output_service.rs", "src-tauri/src/voice_controller.rs", "src-tauri/src/commands/session.rs"],
      "environment": "Windows development workspace; cargo test",
      "validatedAt": "2026-08-27"
    }
  ],
  "impactAssessments": []
}
---

# 语音输入控制面

## 用户目标

快捷键只是语音输入的一种触发方式。用户未来通过界面按钮、硬件或其他入口触发语音输入时，应复用同一套会话仲裁、录音、识别、交付和失败恢复语义，不因触发来源不同产生并行会话或错误结束其他会话。

## 验收场景

| ID | 用户可观察结果 | 当前验证要求 |
| --- | --- | --- |
| `AC-VI-01` | 现有按住快捷键录音、松开识别和中断取消行为保持不变 | 自动化 + 真实 Windows Hook |
| `AC-VI-02` | 只有启动当前会话的 Activation 才能结束或取消它；迟到、重复或其他来源的结束事件无副作用 | 单元/并发测试 + 故障注入 |
| `AC-VI-03` | 单次会话从接受开始到提交始终使用同一配置 revision 和 Provider 快照；配置更新只影响后续会话 | 自动化 |
| `AC-VI-04` | Commands、托盘和平台适配器不能直接写会话运行时、Provider 或 Pending 容器 | 架构边界检查 |
| `AC-VI-05` | Pending 重新交付与新会话启动串行；未达到注入提交点时 Pending 保留且不会重复交付 | 自动化 + 故障注入 |
| `AC-VI-06` | 新触发适配器只需实现统一 begin/finish/cancel 契约，不依赖快捷键专属事件 | 契约测试 |

## 明确不规定的实现

- 不要求新增外部 HTTP 服务、本地端口或新的前端 IPC。
- 不规定未来第二种触发器的具体产品形态。
- 不改变历史、热词和 IncidentVault 的产品职责。
- 不承诺快捷键拥有 Windows Hook 链的系统级独占或永久优先级。

## 局部假设

- 首个生产触发适配器仍为全局快捷键；其他触发方式接入前不得把快捷键健康状态重新合并为语音会话可用状态。
- 当前只支持一个活动语音会话；若未来需要并行会议录音，应重新评估 ADR-0002 和 ADR-0012。

## 架构决策

- [ADR-0002：单所有者、有界、失败关闭的语音会话](../architecture/adr/0002-single-owner-bounded-voice-session.md)
- [ADR-0003：成功注入为提交点，失败进入内存 Pending](../architecture/adr/0003-delivery-commit-point-and-pending-output.md)
- [ADR-0005：revision CAS 与原子本地存储](../architecture/adr/0005-revisioned-atomic-local-storage.md)
- [ADR-0012：统一语音输入控制面所有权](../architecture/adr/0012-unified-voice-input-control-plane.md)

## 当前实现入口

- 启动装配：`src-tauri/src/lib.rs`
- 会话控制面：`src-tauri/src/voice_controller.rs`
- 应用层启停编排：`src-tauri/src/voice_input_service.rs`
- 触发适配器：`src-tauri/src/shortcut_manager/`
- Pending 与交付：`src-tauri/src/pending_output_service.rs`、`src-tauri/src/delivery.rs`

源码与运行配置是实现事实；当前组件关系与时序已同步到 Current C4 与 Runtime View。

## 验证状态

当前实现状态为 `in_progress`，验证状态为 `partial`。Rust 全量测试 134 项通过，但新增证据只完整覆盖已列外部组件不能直接访问 Runtime 的静态边界；Activation 与 Pending 测试只覆盖原语，没有证明真实 Actor 仲裁、配置快照一致性、第二触发适配器契约、取消与注入竞争或 Pending 与 Begin 串行。真实 Windows Hook、WebView2、外部目标应用和控制队列故障注入也仍未形成目标环境证据。

当前已知实现偏差是：release 后的 finalize task 仍持有 `SharedRuntime` 并直接写会话状态；取消与不可逆注入之间存在窗口；启停配置可能在运行时应用失败前已经提交；`begin` 的成功只表示控制消息入队，不表示 Activation 被 Actor 接受。在这些偏差关闭并形成对应自动化证据前，不得把本功能标记为 `implemented`。

## 澄清历史

- 2026-08-27：用户确认先解决语音输入链路的所有权分裂，并批准以单一控制面、统一触发端口和渐进迁移方式实施。
