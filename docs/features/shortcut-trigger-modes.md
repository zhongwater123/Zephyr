---
{
  "schemaVersion": 3,
  "featureId": "FEAT-SHORTCUT-TRIGGER-MODES",
  "authority": "mvp_contract",
  "confirmation": {
    "confirmedBy": "user",
    "confirmedAt": "2026-09-01",
    "sourceRef": "Codex tasks: user requested selectable hold-to-talk and press-to-toggle shortcut modes on 2026-08-28, then explicitly refined and requested documentation of the current frontend presentation and feedback contract on 2026-09-01"
  },
  "specStatus": "confirmed",
  "implementationStatus": "implemented",
  "validationStatus": "partial",
  "implementationReview": {
    "status": "conformant",
    "sourceRevision": "b5929f21cee3329d80e732a3fa2ed86ff6035f5c",
    "worktreeState": "dirty",
    "changedPaths": ["src-tauri/src/config.rs", "src-tauri/src/commands/config.rs", "src-tauri/src/shortcut_manager", "src-tauri/src/voice_controller", "src-tauri/src/voice_trigger.rs", "src-tauri/src/state.rs", "src-tauri/src/overlay.rs", "src/app/AppShellV2.tsx", "src/features/shortcut", "src/features/settings", "src/preinput", "src/domain.ts", "src/ipc/client.ts"],
    "reviewedAt": "2026-08-28",
    "summary": "源码符合性复核确认 Hold/Toggle 配置、专用 CAS 命令、模式快照、适配器状态机、Begin 拒绝与 completion 复位、Starting 即时取消、模式化 UI 和活动期设置保护均已落地；真实 Windows Hook、麦克风、WebView2、重启和目标应用证据仍待补充。",
    "knownDeviations": []
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
  "evidence": [
    {
      "id": "EV-STM-AUTOMATED-20260828",
      "acceptanceIds": ["AC-STM-01", "AC-STM-02", "AC-STM-03", "AC-STM-04", "AC-STM-05", "AC-STM-06", "AC-STM-07", "AC-STM-08", "AC-STM-09", "AC-STM-10", "AC-STM-11", "AC-STM-12"],
      "acceptanceCoverage": [
        { "acceptanceId": "AC-STM-01", "coverage": "partial" },
        { "acceptanceId": "AC-STM-02", "coverage": "partial" },
        { "acceptanceId": "AC-STM-03", "coverage": "partial" },
        { "acceptanceId": "AC-STM-04", "coverage": "partial" },
        { "acceptanceId": "AC-STM-05", "coverage": "partial" },
        { "acceptanceId": "AC-STM-06", "coverage": "partial" },
        { "acceptanceId": "AC-STM-07", "coverage": "partial" },
        { "acceptanceId": "AC-STM-08", "coverage": "partial" },
        { "acceptanceId": "AC-STM-09", "coverage": "partial" },
        { "acceptanceId": "AC-STM-10", "coverage": "partial" },
        { "acceptanceId": "AC-STM-11", "coverage": "partial" },
        { "acceptanceId": "AC-STM-12", "coverage": "partial" }
      ],
      "method": "automated",
      "result": "pass",
      "freshness": "current",
      "capabilities": ["automated"],
      "scope": "Rust 191 项覆盖配置迁移与持久化、模式 provenance、Hold/Toggle 适配器序列、ActivationId 安全复位、Starting 即时取消、迟到启动丢弃、completion 收束、浮层 session 隔离和单写入者边界；前端 54 项覆盖互斥 radio、活动期锁定、模式化文案和 Starting 可访问反馈；production build、安全扫描、架构工具测试与 ASR 边界检查通过。",
      "testRefs": ["config::tests::legacy_config_without_shortcut_trigger_mode_defaults_to_hold", "config::tests::toggle_shortcut_trigger_mode_round_trips", "commands::config::tests::trigger_mode_change_preserves_disabled_state_and_advances_revision", "shortcut_manager::trigger_mode_tests::hold_finishes_on_release_and_ignores_repeated_press", "shortcut_manager::trigger_mode_tests::toggle_ignores_release_and_finishes_on_second_press", "shortcut_manager::trigger_mode_tests::active_activation_keeps_its_original_mode", "shortcut_manager::trigger_mode_tests::interruption_cancels_and_stale_completion_cannot_clear_new_activation", "voice_controller::actor::reducer::tests::quick_finish_during_starting_cancels_immediately", "voice_controller::actor::reducer::tests::stale_start_result_is_cancelled_after_disable", "voice_controller::actor::tests::activation_completion_fires_only_after_runtime_releases_the_activation", "overlay::tests::stale_hide_cannot_close_a_newer_preinput_session", "src/features/settings/SettingsSidebar.test.tsx", "src/features/shortcut/ShortcutCaptureField.test.tsx", "src/preinput/PreInputOverlay.test.tsx"],
      "limitations": ["未启动 Tauri 或 Windows WebView2", "未使用真实 Windows Hook 和真实麦克风", "未验证打包程序重启持久化", "未验证 120 秒真实等待、外部目标应用注入或第三方 Hook 冲突", "脏工作树包含并行 agent 的设置页与智能成稿改动，没有不可变 build identity"],
      "sourceRevision": "b5929f21cee3329d80e732a3fa2ed86ff6035f5c",
      "worktreeState": "dirty",
      "changedPaths": ["src-tauri/src/config.rs", "src-tauri/src/commands/config.rs", "src-tauri/src/shortcut_manager", "src-tauri/src/voice_controller", "src-tauri/src/voice_trigger.rs", "src-tauri/src/state.rs", "src-tauri/src/overlay.rs", "src/app/AppShellV2.tsx", "src/features/shortcut", "src/features/settings", "src/preinput", "src/domain.ts", "src/ipc/client.ts", "docs/features/shortcut-trigger-modes.md", "docs/architecture/runtime-views.md"],
      "environment": "Windows development workspace; cargo test --lib; Vitest happy-dom; TypeScript + Vite production build; architecture and security scripts",
      "validatedAt": "2026-08-28"
    }
  ],
  "impactAssessments": []
}
---

# 快捷键触发模式

## 用户目标

用户可以在设置中明确选择“按住说话”或“点击切换”两种全局快捷键触发方式。按住模式下，按下开始、松开结束；点击切换模式下，第一次按下开始、第二次按下结束。无论选择哪种方式，用户都能始终判断系统是否正在取得麦克风输入，并确信结束操作生效后不会继续采音或迟到启动麦克风。

两种模式只改变用户表达 begin/finish 的方式，必须复用同一套会话仲裁、录音、ASR、智能成稿、目标复验、Delivery、Pending 和失败恢复语义。

主界面与语音输入侧边栏应以简洁、稳定且不抖动的方式表达当前模式和运行状态。主界面底部操作说明保持单行；快捷键设置分别使用“按住开始，松开结束”和“按下开始，再按结束”的短说明。成功切换模式时不插入临时保存文案、不改变模块高度或整体明暗；保存失败仍必须显示原因并恢复权威模式。

侧边栏顶部以强调的“Zephyr / 语音输入”和主标题“只说话，别打字”建立信息层级。首个语音状态模块在常态下一行显示名称、状态、动态说明与开关，开关不带额外外层容器框；仅就绪和工作中状态使用呼吸动效，暂停、不可用和错误状态保持静止。

## 验收场景

| ID | 用户可观察结果 | 当前验证要求 |
| --- | --- | --- |
| `AC-STM-01` | 设置中清楚显示互斥、文字清晰且在按钮内均衡排布的“按住说话”和“点击切换”；对应短说明分别为“按住开始，松开结束”和“按下开始，再按结束”；新安装和旧版本升级默认保持按住说话；成功保存后无需重启即可影响下一次输入，应用重启后仍保留；功能禁用时也可保存且不会意外启用 | 前后端自动化 + Windows/WebView2 + 打包程序重启持久化 |
| `AC-STM-02` | 模式保存、运行态应用或 revision 协调失败时，界面显示明确原因并恢复权威模式，不出现“界面显示 Toggle、实际仍是 Hold”的分裂；正常保存期间不显示临时“正在切换/保存”文案、不让模式模块变灰或改变高度；成功配置提交不能因后续协调失败被前端静默回滚为旧意图 | CAS/协调自动化 + 故障注入 |
| `AC-STM-03` | 当前会话从启动到结束始终使用其开始时确定的模式；录音、识别或交付期间，用户不能通过模式切换或换绑把会话留在无法结束的状态，界面应阻止修改或明确告知何时生效 | 前端状态测试 + 会话/配置竞争故障注入 + Windows/WebView2 |
| `AC-STM-04` | 按住模式下，首次物理按下只开始一次，持续按住时保持录音，松开只结束一次并进入识别；键盘自动重复、重复 Release 或迟到事件不能重复启停或影响下一次会话 | 适配器状态机自动化 + 真实 Windows Hook |
| `AC-STM-05` | 点击切换模式下，第一次物理按下只开始一次，第一次松开不结束；第二次物理按下只结束一次并进入识别，第二次松开无副作用；持续按住不能因键盘自动重复而反复开关 | 适配器状态机自动化 + 真实 Windows Hook |
| `AC-STM-06` | 快速按放、快速双击、Begin 回执迟到或拒绝、控制面繁忙/拥塞时，系统在有限时间内恢复为含义明确的可重试状态；用户不需要第三次点击来修复一次失败的 Toggle，也不会得到并行录音 | 并发/回执自动化 + 故障注入 + 真实 Windows Hook |
| `AC-STM-07` | Begin 因禁用、繁忙、Pending 满或关闭而拒绝时，界面显示失败或繁忙原因并恢复为“下一次有效操作将开始”；上一轮仍在识别时再次触发不会创建并行会话，处理完成后下一次操作可正常开始 | 前后端自动化 + 故障注入 + Windows/WebView2 |
| `AC-STM-08` | 就绪、正在启动、正在聆听、正在识别/成稿和失败具有可区分的反馈；主界面底部操作说明保持单行；侧边栏顶部显示强调的“Zephyr / 语音输入”和主标题“只说话，别打字”；首个状态模块在常态下一行显示名称、状态、动态说明和无额外外层框的开关；仅就绪和工作中状态图标具有呼吸动效，暂停、不可用和错误状态保持静止；全局触发与反馈不抢走原目标应用焦点 | UI 自动化 + Windows/WebView2 + 真实 Hook |
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

- `ASM-STM-01`（Confirmed）：点击切换在第一次 `Pressed` 开始、第二次 `Pressed` 结束；真实 Hook 验证仍需确认物理去重边界。
- `ASM-STM-02`（Confirmed）：Starting、Recording、Transcribing 和 Pasting 期间暂时禁止模式切换和换绑。
- `ASM-STM-03`（Resolved）：`TriggerBehavior` 现在记录 PushToTalk 或 PressToToggle provenance；模式语义由 ShortcutManager 状态机、回执恢复和自动化共同证明，枚举本身仍不构成目标环境证据。
- `ASM-STM-04`（Challenged）：统一 `begin/finish/cancel` 端口证明下游可以复用，不证明只改一个中间层文件即可交付用户选择；配置、UI、持久化、回执恢复、状态反馈和目标环境验证均属于功能范围。
- `ASM-STM-05`（Confirmed）：Starting 阶段的匹配 Finish 立即清除会话并取消启动；迟到启动结果只能执行资源丢弃和再次取消。
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
- 主界面底部单行操作说明：`src/ZephyrAsciiField.tsx`、`src/styles.css`
- 前后端配置契约：`src/domain.ts`、`src/ipc/client.ts`、`src-tauri/src/config.rs`、`src-tauri/src/commands/config.rs`
- 快捷键语义适配器：`src-tauri/src/shortcut_manager/`
- Windows 物理按键边界：`src-tauri/src/windows_keyboard.rs`
- 统一触发端口：`src-tauri/src/voice_trigger.rs`
- 会话仲裁与 Starting/Finish：`src-tauri/src/voice_controller/`
- 音频与 ASR：`src-tauri/src/streaming_pipeline.rs`
- 现有主链路：[语音输入主链路](../architecture/runtime-views.md#语音输入主链路)

源码与运行配置是当前实现事实。当前实现由持久化模式、专用配置命令、ShortcutManager 映射层和统一 Voice Actor 共同完成；旧 `.orig` 文件中的 `shortcut_mode` 残留仍不是实现证据。

2026-09-01 的当前前端实现摘要：侧边栏主标题使用 28px 字号，顶栏底部内边距为 4px；首个状态模块使用单行布局并清除紧凑开关容器的边框、背景、内边距和阴影；就绪与工作中状态图标使用 2.2 秒呼吸动效；模式按钮文字使用 13px 字号与 `0.08em` 字间距；模式保存期间静默阻止重复操作，不渲染临时保存提示或视觉锁定态。这些数值是当前实现事实，不属于不可替换的 MVP 机制约束。

## 验证状态

当前实现状态为 `implemented`，验证状态为 `partial`。源码复核与自动化证明当前测试定义下的模式持久化、CAS、Hold/Toggle 序列、模式快照、Begin 回执恢复、activation completion、Starting 即时取消、模式化 UI、活动期设置保护和统一 ASR 边界可以工作。

这些证据没有启动 Tauri/WebView2，也没有真实 Hook、麦克风、打包程序重启、120 秒真实超时、第三方 Hook 或外部目标应用能力。因此 AC-STM-13 尚无证据，其他涉及目标环境的切片也只覆盖部分要求；达到全部目标环境要求前不得标记为 `validated`。

2026-09-01 的侧边栏与模式提示调整已在脏工作树中通过相关 happy-dom 组件测试和 TypeScript/Vite 生产构建，但尚无 clean revision、Windows/WebView2 截图验收或真实 Hook 证据，因此不新增结构化 evidence，也不提升实现复核或验证状态。

## 澄清历史

- 2026-08-28：用户确认需要让用户选择按住说话或短按开关式触发，并要求从用户侧细化验收。
- 2026-08-28：用户明确要求先写入文档系统，并允许持续挑战文档中的误导性表述。
- 2026-08-28：文档复核决定为该跨配置、Hook、会话和音频隐私边界的能力建立独立 Dossier；`FEAT-SHORTCUT-BINDING` 继续只负责物理绑定编辑，`FEAT-VOICE-INPUT-CONTROL-PLANE` 继续负责统一会话所有权。
- 2026-09-01：用户确认主界面与侧边栏的当前呈现契约，包括单行提示、精简模式文案、稳定的静默保存态、单行状态模块、无额外开关容器框、就绪/工作中呼吸反馈，以及“只说话，别打字”的标题层级，并要求沉淀到文档系统。
