---
{
  "schemaVersion": 3,
  "featureId": "FEAT-VOICE-INPUT-CONTROL-PLANE",
  "authority": "mvp_contract",
  "confirmation": {
    "confirmedBy": "user",
    "confirmedAt": "2026-08-27",
    "sourceRef": "Codex task: voice input ownership unification plan explicitly approved for implementation on 2026-08-27"
  },
  "specStatus": "confirmed",
  "implementationStatus": "in_progress",
  "validationStatus": "partial",
  "implementationReview": {
    "status": "partial",
    "sourceRevision": "b5929f21cee3329d80e732a3fa2ed86ff6035f5c",
    "worktreeState": "dirty",
    "changedPaths": ["src-tauri/src/voice_controller/", "src-tauri/src/voice_trigger.rs", "src-tauri/src/voice_input_service.rs", "src-tauri/src/shortcut_manager/", "src-tauri/src/state.rs", "src-tauri/src/overlay.rs", "src-tauri/src/lib.rs", "src/app/AppShellV2.tsx", "src/preinput/"],
    "reviewedAt": "2026-08-28",
    "summary": "Runtime 单写入者和分层迁移继续成立；Starting 阶段的匹配 Finish 现会立即清除当前会话、取消 Start Workflow 与 AudioSessionActor，并将迟到启动结果作为 stale 资源丢弃。真实 Hook、麦克风和目标应用验证仍未完成。",
    "knownDeviations": []
  },
  "components": ["backend.bootstrap", "backend.commands", "backend.services", "backend.voice-controller", "backend.delivery", "backend.shortcut", "platform.windows"],
  "decisions": ["ADR-0002", "ADR-0003", "ADR-0005", "ADR-0012", "ADR-0013"],
  "validationSlices": [
    { "id": "AC-VI-01", "components": ["backend.shortcut", "backend.voice-controller", "platform.windows"], "requiredEvidence": ["automated", "runtime_hook"] },
    { "id": "AC-VI-02", "components": ["backend.voice-controller"], "requiredEvidence": ["automated", "fault_injection"] },
    { "id": "AC-VI-03", "components": ["backend.services", "backend.voice-controller"], "requiredEvidence": ["automated"] },
    { "id": "AC-VI-04", "components": ["backend.bootstrap", "backend.commands", "backend.services", "backend.voice-controller", "backend.delivery"], "requiredEvidence": ["automated"] },
    { "id": "AC-VI-05", "components": ["backend.voice-controller", "backend.delivery"], "requiredEvidence": ["automated", "fault_injection"] },
    { "id": "AC-VI-06", "components": ["backend.shortcut", "backend.voice-controller"], "requiredEvidence": ["automated"] },
    { "id": "AC-VI-07", "components": ["backend.voice-controller", "backend.streaming", "backend.delivery"], "requiredEvidence": ["automated"] }
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
      "result": "pass",
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
    },
    {
      "id": "EV-VI-STRICT-ACTOR-20260828",
      "acceptanceIds": ["AC-VI-02", "AC-VI-03", "AC-VI-04", "AC-VI-06", "AC-VI-07"],
      "acceptanceCoverage": [
        { "acceptanceId": "AC-VI-02", "coverage": "full" },
        { "acceptanceId": "AC-VI-03", "coverage": "partial" },
        { "acceptanceId": "AC-VI-04", "coverage": "full" },
        { "acceptanceId": "AC-VI-06", "coverage": "full" },
        { "acceptanceId": "AC-VI-07", "coverage": "full" }
      ],
      "method": "automated",
      "result": "pass",
      "freshness": "revalidated",
      "capabilities": ["automated", "fault_injection"],
      "scope": "验证 VoiceRuntime 只含纯控制状态，Actor reducer/Effects、AudioSessionActor、Starting 快速释放、异步 BeginReceipt、控制邮箱满/关闭、最后 Handle 释放、Presenter 单一 UI 出口以及外部层/Workflow 边界；Rust 153 项、前端 46 项通过，架构检查覆盖 106/106 个生产源码文件及 15 条代码不变量。",
      "testRefs": ["voice_controller::actor::reducer::tests::quick_finish_during_starting_is_deferred_until_start_succeeds", "voice_controller::actor::reducer::tests::stale_activation_cannot_finish_or_cancel_current_session", "voice_controller::tests::full_public_mailbox_fails_closed", "voice_controller::tests::closed_public_mailbox_is_reported", "voice_controller::tests::dropping_last_handle_closes_mailbox", "voice_trigger::tests::begin_receipt_reports_actor_decision_without_blocking_submission", "tests::external_voice_layers_cannot_reach_mutable_runtime", "tests::voice_runtime_has_exactly_one_source_writer", "tests::spawned_voice_workers_do_not_capture_runtime", "tests::presenter_is_the_only_voice_ui_gateway"],
      "limitations": ["Starting 快速释放测试只证明当前 finish_requested 延迟结束行为，没有证明 Release 后不会再取得麦克风，也不能证明该行为符合 Push-to-Talk 的产品与隐私语义", "配置 revision 固定目前主要由类型与状态边界证明，尚缺真实 Provider 双 revision 集成测试", "Pending 与真实 Actor Begin 的调度级竞争及进程崩溃 exactly-once 不在现有证据中", "脏工作树证据没有不可变 build identity", "未在真实 Windows Hook、WebView2 和外部目标应用上验证"],
      "sourceRevision": "b62667deab18f740c83bab2f1bcebae2fd0a59e2",
      "worktreeState": "dirty",
      "changedPaths": ["src-tauri/src/voice_controller/", "src-tauri/src/voice_trigger.rs", "src-tauri/src/voice_input_service.rs", "src-tauri/src/streaming_pipeline.rs", "src-tauri/src/lib.rs", "src/app/AppShellV2.tsx", "src/security-model.ts"],
      "environment": "Windows development workspace; cargo test; npm test; npm run build; npm run architecture:test; npm run architecture:check; npm run architecture:asr",
      "validatedAt": "2026-08-28"
    }
  ],
  "impactAssessments": []
}
---

# 语音输入控制面

## 用户目标

快捷键只是语音输入的一种触发方式。用户未来通过界面按钮、硬件或其他入口触发语音输入时，应复用同一套会话仲裁、录音、识别、交付和失败恢复语义，不因触发来源不同产生并行会话或错误结束其他会话。

快捷键内部的“按住说话”和“点击切换”选择由 [FEAT-SHORTCUT-TRIGGER-MODES](shortcut-trigger-modes.md) 规定。统一 `begin/finish/cancel` 控制面只证明下游复用边界存在，不证明任一具体触发模式已经实现、符合用户语义或通过目标环境验证。

## 验收场景

| ID | 用户可观察结果 | 当前验证要求 |
| --- | --- | --- |
| `AC-VI-01` | 现有按住快捷键录音、松开识别和中断取消行为保持不变 | 自动化 + 真实 Windows Hook |
| `AC-VI-02` | 只有启动当前会话的 Activation 才能结束或取消它；迟到、重复或其他来源的结束事件无副作用 | 单元/并发测试 + 故障注入 |
| `AC-VI-03` | 单次会话从接受开始到提交始终使用同一配置 revision 和 Provider 快照；配置更新只影响后续会话 | 自动化 |
| `AC-VI-04` | Commands、托盘和平台适配器不能直接写会话运行时、Provider 或 Pending 容器 | 架构边界检查 |
| `AC-VI-05` | Pending 重新交付与新会话启动串行；未达到注入提交点时 Pending 保留且不会重复交付 | 自动化 + 故障注入 |
| `AC-VI-06` | 新触发适配器只需实现统一 begin/finish/cancel 契约，不依赖快捷键专属事件 | 契约测试 |
| `AC-VI-07` | 只有 VoiceSessionActor mailbox task 可以修改 VoiceRuntime；异步录音、ASR、交付与展示任务只能返回带 SessionId 的结果 | 单元测试 + 架构边界检查 |

## 明确不规定的实现

- 不要求新增外部 HTTP 服务、本地端口或新的前端 IPC。
- 不规定未来第二种触发器的具体产品形态。
- 不改变历史、热词和 IncidentVault 的产品职责。
- 不承诺快捷键拥有 Windows Hook 链的系统级独占或永久优先级。

## 局部假设

除用户目标和已确认的用户可观察结果外，本 Dossier 对 Actor 内部形态、测试方法和渐进迁移顺序的描述均是当前 MVP 的技术假设。若有更简单的方案仍满足当前契约，可以调整这些描述，并记录调整原因与验证证据。

- 首个生产触发适配器仍为全局快捷键；其他触发方式接入前不得把快捷键健康状态重新合并为语音会话可用状态。
- 当前只支持一个活动语音会话；若未来需要并行会议录音，应重新评估 ADR-0002 和 ADR-0012。
- Starting 阶段收到匹配 Finish 时立即取消启动是当前已确认契约；实现通过取消令牌、AudioSessionActor cancel 和 stale StartFinished 丢弃共同收束，但真实慢设备与麦克风权限边界仍需目标环境验证。

## 架构决策

- [ADR-0002：单所有者、有界、失败关闭的语音会话](../architecture/adr/0002-single-owner-bounded-voice-session.md)
- [ADR-0003：成功注入为提交点，失败进入内存 Pending](../architecture/adr/0003-delivery-commit-point-and-pending-output.md)
- [ADR-0005：revision CAS 与原子本地存储](../architecture/adr/0005-revisioned-atomic-local-storage.md)
- [ADR-0012：统一语音输入控制面所有权](../architecture/adr/0012-unified-voice-input-control-plane.md)
- [ADR-0013：严格 mailbox-owned 语音运行时](../architecture/adr/0013-strict-mailbox-owned-voice-runtime.md)

## 当前实现入口

- 启动装配：`src-tauri/src/lib.rs`
- 会话控制面：`src-tauri/src/voice_controller/`
- 应用层启停编排：`src-tauri/src/voice_input_service.rs`
- 触发适配器：`src-tauri/src/shortcut_manager/`
- Pending 与交付：`src-tauri/src/pending_output_service.rs`、`src-tauri/src/delivery.rs`

源码与运行配置是实现事实；当前组件关系与时序已同步到 Current C4 与 Runtime View。

## 验证状态

当前实现状态为 `in_progress`，验证状态仍为 `partial`。最新自动化为 Rust 191 项、前端 54 项、production build、架构工具测试、安全扫描和 ASR 边界检查通过。新增证据证明当前测试定义下的 Starting 即时取消、迟到启动丢弃、Activation completion、Hold/Toggle 配对、浮层 session 隔离、控制邮箱故障关闭和 Runtime/Worker 单写入者边界可以工作。配置双 revision Provider 集成、Pending 与真实 Actor Begin 的调度级竞争仍缺专项证据。

Starting/Release 偏差已在源码和 Current Runtime View 中关闭。配置提交与运行态协调继续采用 reconciliation 语义；Hook 故障只降低 shortcut health，不禁用 Actor。由于真实 Windows Hook、快速按放、WebView2、目标窗口捕获/注入、设备错误和 Pending 重投递尚未形成目标环境证据，本功能不得宣称完整验证。

## 澄清历史

- 2026-08-27：用户确认先解决语音输入链路的所有权分裂，并批准以单一控制面、统一触发端口和渐进迁移方式实施。
- 2026-08-28：复核确认当前 Starting 阶段的 Release 只记录 `finish_requested`，可能在用户松开后才完成麦克风启动并随后 Stop。用户要求先登记该问题；最终采用立即取消还是 AudioReady 精确仲裁尚未决定。
- 2026-08-28：用户随后明确选择 Starting 中的 Finish 立即取消且不产生文本；实现删除 `finish_requested` 延迟路径，并以取消令牌、AudioSessionActor cancel、stale StartFinished 丢弃和 activation completion 收束。
