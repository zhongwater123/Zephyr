---
{
  "schemaVersion": 3,
  "featureId": "FEAT-SHORTCUT-TRIGGER-MODES",
  "authority": "mvp_contract",
  "confirmation": {
    "confirmedBy": "user",
    "confirmedAt": "2026-08-28",
    "sourceRef": "Codex task: user requested selectable hold-to-talk and press-to-toggle shortcut modes, then explicitly asked to record the refined user-side acceptance contract in the documentation system on 2026-08-28"
  },
  "specStatus": "confirmed",
  "implementationStatus": "not_started",
  "validationStatus": "unverified",
  "implementationReview": {
    "status": "unreviewed",
    "sourceRevision": "c4c3cac5a6680084c607b360eb794987a1e4c831",
    "worktreeState": "dirty",
    "changedPaths": ["src-tauri/src/config.rs", "src-tauri/src/voice_controller", "src-tauri/src/voice_trigger.rs", "src-tauri/src/streaming_pipeline.rs", "src/app/AppShellV2.tsx", "src/domain.ts"],
    "reviewedAt": "2026-08-28",
    "summary": "现有生产路径只实现按住说话；可选择的点击切换模式、配置持久化、模式化提示与目标环境验收尚未实现。现有统一 begin/finish/cancel 控制面可以复用，但不能据此声称触发模式功能已经存在。",
    "knownDeviations": [
      "VoiceActivation.TriggerBehavior 当前只有 PushToTalk，且除构造和测试外没有运行时策略消费者；真正的 Pressed/Released 映射仍硬编码在 ShortcutManager。",
      "ShortcutManager 当前只确认 Begin 已成功入队，不消费 BeginReceipt 的 Accepted/Rejected 决策；若直接增加 Toggle，本地状态可能在 Begin 被拒绝后错误锁存。",
      "Starting 阶段收到匹配 Finish 时当前只记录 finish_requested，可能在用户已要求停止后才完成麦克风启动；该行为不满足本功能的停止后不得迟到启麦验收。"
    ]
  },
  "components": ["frontend.features", "frontend.ipc", "backend.commands", "backend.services", "backend.repositories", "backend.voice-controller", "backend.streaming", "backend.delivery", "backend.shortcut", "platform.windows"],
  "decisions": ["ADR-0002", "ADR-0005", "ADR-0010", "ADR-0011", "ADR-0012", "ADR-0013"],
  "validationSlices": [
    { "id": "AC-STM-01", "components": ["frontend.features", "backend.repositories", "backend.shortcut"], "requiredEvidence": ["automated", "windows_webview2", "restart_persistence"] },
    { "id": "AC-STM-02", "components": ["frontend.features", "frontend.ipc", "backend.commands", "backend.services", "backend.repositories", "backend.shortcut"], "requiredEvidence": ["automated", "fault_injection"] },
    { "id": "AC-STM-03", "components": ["frontend.features", "backend.shortcut", "backend.voice-controller"], "requiredEvidence": ["automated", "fault_injection", "windows_webview2"] },
    { "id": "AC-STM-04", "components": ["backend.shortcut", "backend.voice-controller", "platform.windows"], "requiredEvidence": ["automated", "runtime_hook"] },
    { "id": "AC-STM-05", "components": ["backend.shortcut", "backend.voice-controller", "platform.windows"], "requiredEvidence": ["automated", "runtime_hook"] },
    { "id": "AC-STM-06", "components": ["backend.shortcut", "backend.voice-controller", "platform.windows"], "requiredEvidence": ["automated", "fault_injection", "runtime_hook"] },
    { "id": "AC-STM-07", "components": ["frontend.features", "backend.shortcut", "backend.voice-controller"], "requiredEvidence": ["automated", "fault_injection", "windows_webview2"] },
    { "id": "AC-STM-08", "components": ["frontend.features", "backend.voice-controller", "platform.windows"], "requiredEvidence": ["automated", "windows_webview2", "runtime_hook"] },
    { "id": "AC-STM-09", "components": ["backend.shortcut", "backend.voice-controller", "backend.streaming", "platform.windows"], "requiredEvidence": ["automated", "fault_injection", "runtime_hook"] },
    { "id": "AC-STM-10", "components": ["frontend.features", "backend.shortcut", "backend.voice-controller", "platform.windows"], "requiredEvidence": ["automated", "fault_injection", "runtime_hook"] },
    { "id": "AC-STM-11", "components": ["frontend.features", "backend.voice-controller", "backend.streaming"], "requiredEvidence": ["automated", "windows_webview2"] },
    { "id": "AC-STM-12", "components": ["backend.voice-controller", "backend.streaming", "backend.delivery"], "requiredEvidence": ["automated", "fault_injection", "external_app_interop"] },
    { "id": "AC-STM-13", "components": ["frontend.features", "backend.shortcut", "platform.windows"], "requiredEvidence": ["windows_webview2", "runtime_hook", "external_app_interop"] }
  ],
  "evidence": [],
  "impactAssessments": []
}
---

# 快捷键触发模式

## 用户目标

用户可以在设置中明确选择“按住说话”或“点击切换”两种全局快捷键触发方式。按住模式下，按下开始、松开结束；点击切换模式下，第一次按下开始、第二次按下结束。无论选择哪种方式，用户都能始终判断系统是否正在取得麦克风输入，并确信结束操作生效后不会继续采音或迟到启动麦克风。

两种模式只改变用户表达 begin/finish 的方式，必须复用同一套会话仲裁、录音、ASR、智能成稿、目标复验、Delivery、Pending 和失败恢复语义。

## 验收场景

| ID | 用户可观察结果 | 当前验证要求 |
| --- | --- | --- |
| `AC-STM-01` | 设置中清楚显示互斥的“按住说话”和“点击切换”；新安装和旧版本升级默认保持按住说话；成功保存后无需重启即可影响下一次输入，应用重启后仍保留；功能禁用时也可保存且不会意外启用 | 前后端自动化 + Windows/WebView2 + 打包程序重启持久化 |
| `AC-STM-02` | 模式保存、运行态应用或 revision 协调失败时，界面显示明确原因并恢复权威模式，不出现“界面显示 Toggle、实际仍是 Hold”的分裂；成功配置提交不能因后续协调失败被前端静默回滚为旧意图 | CAS/协调自动化 + 故障注入 |
| `AC-STM-03` | 当前会话从启动到结束始终使用其开始时确定的模式；录音、识别或交付期间，用户不能通过模式切换或换绑把会话留在无法结束的状态，界面应阻止修改或明确告知何时生效 | 前端状态测试 + 会话/配置竞争故障注入 + Windows/WebView2 |
| `AC-STM-04` | 按住模式下，首次物理按下只开始一次，持续按住时保持录音，松开只结束一次并进入识别；键盘自动重复、重复 Release 或迟到事件不能重复启停或影响下一次会话 | 适配器状态机自动化 + 真实 Windows Hook |
| `AC-STM-05` | 点击切换模式下，第一次物理按下只开始一次，第一次松开不结束；第二次物理按下只结束一次并进入识别，第二次松开无副作用；持续按住不能因键盘自动重复而反复开关 | 适配器状态机自动化 + 真实 Windows Hook |
| `AC-STM-06` | 快速按放、快速双击、Begin 回执迟到或拒绝、控制面繁忙/拥塞时，系统在有限时间内恢复为含义明确的可重试状态；用户不需要第三次点击来修复一次失败的 Toggle，也不会得到并行录音 | 并发/回执自动化 + 故障注入 + 真实 Windows Hook |
| `AC-STM-07` | Begin 因禁用、繁忙、Pending 满或关闭而拒绝时，界面显示失败或繁忙原因并恢复为“下一次有效操作将开始”；上一轮仍在识别时再次触发不会创建并行会话，处理完成后下一次操作可正常开始 | 前后端自动化 + 故障注入 + Windows/WebView2 |
| `AC-STM-08` | 就绪、正在启动、正在聆听、正在识别/成稿和失败具有可区分的反馈；按住模式提示“松开结束”，点击切换模式提示“再次按下结束”；全局触发与反馈不抢走原目标应用焦点 | UI 自动化 + Windows/WebView2 + 真实 Hook |
| `AC-STM-09` | 用户发出本模式定义的结束操作后，系统立即离开“正在聆听”；停止之后不再接收新的音频，且设备尚未就绪时不能在用户已停止后才持续启动麦克风。极短输入可以无文本结束，但不能卡住或制造迟到采音 | Starting/AudioReady 竞争自动化 + 故障注入 + 真实 Hook/麦克风 |
| `AC-STM-10` | Hook 中断、禁用功能、应用退出或运行时关闭会取消当前会话并停止采音，不交付不完整文本；禁用功能始终可作为 Toggle 误触后的安全取消出口。旧 Activation 的迟到结束/取消事件不能影响当前会话 | 生命周期自动化 + 故障注入 + 真实 Hook |
| `AC-STM-11` | 用户忘记结束 Toggle 录音时，既有 120 秒上限仍会自动终止并给出明确反馈；设备启动失败时自动恢复到可重试状态，不要求用户再按一次来关闭一个从未成功开始的会话 | Deadline/设备错误自动化 + Windows/WebView2 |
| `AC-STM-12` | 相同音频通过两种模式结束后进入相同的配置 revision、Provider、ASR、智能成稿、目标复验、Delivery、历史、热词和 Pending 规则；两种模式不会建立不同质量或不同安全边界的音频/识别管线，并且同时最多只有一个活动会话 | 管线契约自动化 + 故障注入 + 外部目标应用互操作 |
| `AC-STM-13` | 在真实 Windows Hook、真实麦克风和至少一个目标应用中完整验证两种模式；若第三方 Hook 拦截导致 Zephyr 未收到按键，界面必须反映快捷键运行不可用，不能伪装成正在聆听，也不能宣称系统级独占 | Windows/WebView2 + 真实 Hook/麦克风 + 外部应用互操作 |

以下任一用户可观察结果出现时，本功能不得验收通过：第一次 Toggle Begin 被拒绝后需要第三次点击才能开始；第一次松开错误结束 Toggle；自动重复导致反复启停；用户结束后仍继续采音或随后才启动麦克风；界面显示未录音但后台仍在录音；设置显示模式与实际模式不一致；中断、禁用或退出后仍交付文本；只通过模拟测试而没有目标环境证据。

## 明确不规定的实现

- 不要求某个内部枚举、状态机类型、IPC 名称、线程模型或定时器实现；用户可观察结果是契约。
- 不要求同一模式自动区分“短按 Toggle、长按 Push-to-Talk”。用户先选择模式，因此“点击”指一次完整物理按下/松开，不定义隐藏的毫秒阈值。
- 不增加按应用触发模式、一个动作多个快捷键、自定义长短按阈值或并行录音会话。
- 不持久化“Toggle 当前是否开启”；每次进程启动都从未录音状态开始。
- 不要求新建第二套录音、ASR、智能成稿或 Delivery 管线。
- 不规定未来专用“取消本次输入”按钮或取消快捷键的具体形态；MVP 至少保证禁用功能能可靠取消且不交付。
- 不承诺 Zephyr 在所有第三方低级键盘 Hook 前获得事件或拥有系统级独占。

## 局部假设

除已确认的用户目标、验收场景和明确不规定的实现外，下面内容是可被实现探针或用户反馈挑战的当前 MVP 假设：

- `ASM-STM-01`（Open）：点击切换在第一次 `Pressed` 开始、第二次 `Pressed` 结束，以减少开始和结束延迟；若真实 Hook/用户研究证明以完整 click 或 Release 为边界更可靠，可以调整内部边界，但不得破坏一次点击一次含义和停止后的隐私结果。
- `ASM-STM-02`（Open）：录音或处理中最简单的设置体验是暂时禁止模式切换和换绑；也可以采用“仅影响下一会话”，但必须显示生效时机并保证当前会话仍能按启动时模式结束。
- `ASM-STM-03`（Challenged）：`VoiceActivation.TriggerBehavior` 的存在不证明多模式策略已经实现。当前源码只有 `PushToTalk`，且运行时不消费该字段；模式语义应由真实调用链和目标环境证据证明。
- `ASM-STM-04`（Challenged）：统一 `begin/finish/cancel` 端口证明下游可以复用，不证明只改一个中间层文件即可交付用户选择；配置、UI、持久化、回执恢复、状态反馈和目标环境验证均属于功能范围。
- `ASM-STM-05`（Open）：Starting 阶段的 Finish 应优先满足“停止后不迟到启麦”的结果；采用立即取消启动还是更精确的 AudioReady 仲裁属于实现选择，但现有延迟 Stop 行为不能作为验收依据。
- `ASM-STM-06`（Open）：运行时匹配快捷键后是否必须吞掉原按键行为继续沿用 `FEAT-SHORTCUT-BINDING` 的开放假设，不因增加 Toggle 模式而被默认为已确认。

## 架构决策

- [ADR-0002：单所有者、有界、失败关闭的语音会话](../architecture/adr/0002-single-owner-bounded-voice-session.md)
- [ADR-0005：revision CAS 与原子本地存储](../architecture/adr/0005-revisioned-atomic-local-storage.md)
- [ADR-0010：分离有焦点的设置录入与全局运行时监听](../architecture/adr/0010-separate-focused-shortcut-editing.md)
- [ADR-0011：能力感知的有效验证](../architecture/adr/0011-capability-aware-effective-validation.md)
- [ADR-0012：统一语音输入控制面所有权](../architecture/adr/0012-unified-voice-input-control-plane.md)
- [ADR-0013：严格 mailbox-owned 语音运行时](../architecture/adr/0013-strict-mailbox-owned-voice-runtime.md)

本功能不改变上述 Accepted ADR 的长期边界，因此当前不新增 ADR。若实现要求 Voice Actor 接收原始键盘事件、复制第二套录音/ASR 管线，或改变单活动会话边界，应先提出替代设计并复核相关 ADR。

## 当前实现入口

- 设置与模式化提示：`src/features/shortcut/`、`src/features/settings/`、`src/app/AppShellV2.tsx`
- 前后端配置契约：`src/domain.ts`、`src/ipc/client.ts`、`src-tauri/src/config.rs`、`src-tauri/src/commands/config.rs`
- 快捷键语义适配器：`src-tauri/src/shortcut_manager/`
- Windows 物理按键边界：`src-tauri/src/windows_keyboard.rs`
- 统一触发端口：`src-tauri/src/voice_trigger.rs`
- 会话仲裁与 Starting/Finish：`src-tauri/src/voice_controller/`
- 音频与 ASR：`src-tauri/src/streaming_pipeline.rs`
- 现有主链路：[语音输入主链路](../architecture/runtime-views.md#语音输入主链路)

源码与运行配置是当前实现事实。现有实现只有按住说话；旧 `.orig` 文件中的 `shortcut_mode` 残留不是当前配置能力或实现证据。

## 验证状态

当前为 `unverified`，实现状态为 `not_started`。现有按住说话、ActivationId、统一控制面和 Actor 边界证据只能作为可复用基础，不能覆盖本功能的模式选择、持久化、Toggle 序列、BeginReceipt 拒绝恢复、模式化 UI、Starting 后停止隐私结果或真实 Hook/麦克风验收。

不得仅因为 `VoiceTriggerPort` 已存在、`TriggerBehavior` 类型已存在、单元测试能构造 Toggle 序列，或原按住模式曾在开发机工作，就把本功能升级状态。实际源码实现开始后应及时改为 `in_progress` 并做源码符合性复核；实现复核确认契约已经落地且没有已知偏差后才可标记 `implemented`。只有对应证据存在时才能把验证状态升级为 `partial`，达到全部目标环境要求前不得标记 `validated`。

## 澄清历史

- 2026-08-28：用户确认需要让用户选择按住说话或短按开关式触发，并要求从用户侧细化验收。
- 2026-08-28：用户明确要求先写入文档系统，并允许持续挑战文档中的误导性表述。
- 2026-08-28：文档复核决定为该跨配置、Hook、会话和音频隐私边界的能力建立独立 Dossier；`FEAT-SHORTCUT-BINDING` 继续只负责物理绑定编辑，`FEAT-VOICE-INPUT-CONTROL-PLANE` 继续负责统一会话所有权。
