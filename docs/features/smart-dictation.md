---
{
  "schemaVersion": 3,
  "featureId": "FEAT-SMART-DICTATION",
  "authority": "mvp_contract",
  "confirmation": {
    "confirmedBy": "user",
    "confirmedAt": "2026-08-28",
    "sourceRef": "Codex task: user described shortcut A as context-aware smart dictation for chat, office writing, and vibe coding on 2026-08-28"
  },
  "specStatus": "confirmed",
  "implementationStatus": "in_progress",
  "implementationReview": {
    "status": "deviating",
    "sourceRevision": "a69242240d7da4e3d4f086b61548bfa019f93bdf",
    "worktreeState": "dirty",
    "changedPaths": ["src-tauri/crates/zephyr-paste-helper/src/platform.rs", "src-tauri/src/clipboard_transaction.rs", "src-tauri/src/delivery.rs", "src-tauri/src/inject.rs", "docs/features/smart-dictation.md", "docs/architecture/adr/0018-owned-clipboard-transaction-and-isolated-paste.md"],
    "reviewedAt": "2026-09-01",
    "summary": "已提交 revision f93bc4d、3ea6d9f 与 ce04cfb 分别移除 OLE 活对象路径、建立隔离剪贴板事务并完成 sidecar 打包门禁。基于 a692422 的当前脏工作树进一步修复动态注册格式被名称白名单误拒绝的问题，并只为 SmartDictation 单行开放覆盖前失败后的 Unicode 安全降级；Windows 打包前检查和自动化通过，用户随后报告暂未再遇到阻塞。该反馈缺少 clean revision、精确应用/格式矩阵和重复次数，不升级验证状态。",
    "knownDeviations": ["当前动态注册格式修复仍在脏工作树，尚无可追溯安装包或 clean revision；2026-08-28 与 2026-08-31 的既有 0xc000041d 失败证据尚未被同等级目标环境矩阵关闭。", "可以锁定并复制的注册格式已按不透明字节保存，但尚未用受控真实剪贴板 owner 验证其恢复后的语义等价；Chromium、Office、图片、文件和延迟渲染格式仍需矩阵验证。", "自动化尚未在真实 helper 进程上完成每阶段强杀、剪贴板占用、并发复制、部分提交和单次 recover。", "helper 缺失或自检失败时仍统一进入 Pending；当前没有独立于 sidecar 的 Phase 0 进程内 Unicode 回退。", "真实运行曾出现 PendingFull 后用户无法找到可操作的待处理入口；重启可清空内存队列，但这不是已验证的产品恢复路径。"]
  },
  "validationStatus": "invalidated",
  "components": ["system.zephyr", "frontend.features", "backend.services", "backend.repositories", "backend.voice-controller", "backend.streaming", "backend.delivery", "backend.shortcut", "backend.incident-vault", "platform.windows"],
  "decisions": ["ADR-0002", "ADR-0003", "ADR-0004", "ADR-0008", "ADR-0012", "ADR-0013", "ADR-0014", "ADR-0017", "ADR-0018"],
  "validationSlices": [
    { "id": "AC-SD-01", "components": ["backend.voice-controller", "backend.streaming"], "requiredEvidence": ["automated", "fault_injection"] },
    { "id": "AC-SD-02", "components": ["backend.services", "backend.voice-controller", "backend.delivery"], "requiredEvidence": ["automated", "fault_injection"] },
    { "id": "AC-SD-03", "components": ["backend.services", "backend.delivery"], "requiredEvidence": ["automated", "human_quality_eval", "external_app_interop"] },
    { "id": "AC-SD-04", "components": ["backend.services", "backend.delivery"], "requiredEvidence": ["automated", "human_quality_eval", "external_app_interop"] },
    { "id": "AC-SD-05", "components": ["backend.services", "backend.delivery"], "requiredEvidence": ["automated", "human_quality_eval", "external_app_interop"] },
    { "id": "AC-SD-06", "components": ["backend.shortcut", "backend.voice-controller", "backend.services"], "requiredEvidence": ["automated", "runtime_hook"] },
    { "id": "AC-SD-07", "components": ["backend.services", "backend.delivery"], "requiredEvidence": ["automated", "fault_injection"] },
    { "id": "AC-SD-08", "components": ["backend.services", "backend.voice-controller", "backend.delivery"], "requiredEvidence": ["automated", "fault_injection"] },
    { "id": "AC-SD-09", "components": ["backend.services", "backend.incident-vault"], "requiredEvidence": ["automated", "fault_injection"] },
    { "id": "AC-SD-10", "components": ["backend.voice-controller", "backend.delivery"], "requiredEvidence": ["automated", "fault_injection", "external_app_interop"] },
    { "id": "AC-SD-11", "components": ["backend.services"], "requiredEvidence": ["automated"] },
    { "id": "AC-SD-12", "components": ["frontend.features", "backend.services", "backend.repositories", "platform.windows"], "requiredEvidence": ["automated", "windows_webview2", "restart_persistence"] },
    { "id": "AC-SD-13", "components": ["backend.shortcut", "backend.services", "backend.voice-controller"], "requiredEvidence": ["automated", "runtime_hook"] },
    { "id": "AC-SD-14", "components": ["frontend.features", "backend.repositories"], "requiredEvidence": ["automated", "usability_observation", "windows_webview2", "restart_persistence"] },
    { "id": "AC-SD-15", "components": ["frontend.features", "backend.voice-controller", "backend.delivery", "backend.repositories"], "requiredEvidence": ["automated", "runtime_hook", "external_app_interop", "restart_persistence"] },
    { "id": "AC-SD-16", "components": ["backend.delivery", "platform.windows"], "requiredEvidence": ["automated", "fault_injection", "external_app_interop"] }
  ],
  "evidence": [{"id":"EV-SD-MVP-AUTOMATED-20260828","acceptanceIds":["AC-SD-01","AC-SD-02","AC-SD-03","AC-SD-04","AC-SD-05","AC-SD-06","AC-SD-07","AC-SD-08","AC-SD-09","AC-SD-10","AC-SD-11","AC-SD-12","AC-SD-13"],"acceptanceCoverage":[{"acceptanceId":"AC-SD-01","coverage":"partial"},{"acceptanceId":"AC-SD-02","coverage":"partial"},{"acceptanceId":"AC-SD-03","coverage":"partial"},{"acceptanceId":"AC-SD-04","coverage":"partial"},{"acceptanceId":"AC-SD-05","coverage":"partial"},{"acceptanceId":"AC-SD-06","coverage":"partial"},{"acceptanceId":"AC-SD-07","coverage":"partial"},{"acceptanceId":"AC-SD-08","coverage":"partial"},{"acceptanceId":"AC-SD-09","coverage":"partial"},{"acceptanceId":"AC-SD-10","coverage":"partial"},{"acceptanceId":"AC-SD-11","coverage":"partial"},{"acceptanceId":"AC-SD-12","coverage":"partial"},{"acceptanceId":"AC-SD-13","coverage":"partial"}],"method":"automated","result":"pass","freshness":"current","capabilities":["automated","fault_injection"],"scope":"Rust 181 项库测试、前端 48 项测试、production build、秘密扫描、架构检查和 ASR 边界检查通过；覆盖统一 Prompt、应用上下文数据边界、三档强度、配置迁移、失败兜底、AtomicPaste、History provenance 与热词学习栅栏","testRefs":["cargo test --manifest-path src-tauri/Cargo.toml --lib","src-tauri/src/text_processing/model.rs","src-tauri/src/text_processing/adapter.rs","src-tauri/src/text_processing/unified_prompt_repository.rs","src-tauri/src/config.rs","src-tauri/src/voice_controller/workflow/finalize.rs","src-tauri/src/delivery.rs","src-tauri/src/inject.rs","src-tauri/src/history.rs","src-tauri/src/hotwords.rs","src/features/settings/MoreSettingsPanel.test.tsx","npm test","npm run build","npm run security:secrets","npm run architecture:check","npm run architecture:asr"],"limitations":["未启动真实 Tauri/WebView2 与 Windows 全局快捷键","未使用真实 DeepSeek 凭据进行网络联调","未验证聊天、Office、编码助手和终端输入框互操作","未验证 Windows Credential Manager 重启持久化与内部安装包预置","缺少完整 finalize 竞争与 IncidentVault 拥塞集成故障注入","缺少三档跨应用语料质量评测","dirty worktree evidence 没有不可变 build identity"],"sourceRevision":"c4c3cac5a6680084c607b360eb794987a1e4c831","worktreeState":"dirty","changedPaths":["src-tauri/src/text_processing","src-tauri/resources/prompts/smart_dictation","src-tauri/src/voice_controller","src-tauri/src/delivery.rs","src-tauri/src/inject.rs","src-tauri/src/history.rs","src-tauri/src/hotwords.rs","src-tauri/src/config.rs","src/features/settings","src/app/AppShellV2.tsx"],"environment":"Windows development workspace; Rust tests and happy-dom frontend tests; no immutable package identity","validatedAt":"2026-08-28"},{"id":"EV-SD-WINDOWS-ATOMIC-PASTE-CRASH-20260831","acceptanceIds":["AC-SD-10","AC-SD-15","AC-SD-16"],"acceptanceCoverage":[{"acceptanceId":"AC-SD-10","coverage":"partial"},{"acceptanceId":"AC-SD-15","coverage":"partial"},{"acceptanceId":"AC-SD-16","coverage":"partial"}],"method":"manual","result":"fail","freshness":"current","capabilities":["runtime_hook","external_app_interop"],"scope":"真实 Windows Tauri 开发运行中，13 字/39 bytes 的 provider final、relay、aggregate 与 delivery payload 哈希完全一致；进入 delivery_inject 后 tokio-rt-worker 栈溢出，进程以 0xc000041d 退出。2026-08-28 的较长文本运行也在相同边界发生同类崩溃，说明故障不依赖长文本、ASR 聚合或 LLM 润色。","limitations":["现有日志只定位到 Delivery/AtomicPaste 边界，尚无 native crash dump 或 OleGetClipboard、SendInput、OleSetClipboard、OleFlushClipboard 的逐步轨迹，不能仅凭该证据断言具体栈溢出指令。"],"sourceRevision":"41b8702177692aae88cdf434f22e5c6b26577faa","worktreeState":"clean","environment":"Windows development machine; npm run tauri dev; real global trigger and external editable target; revision observed immediately after the run","validatedAt":"2026-08-31"}],
  "impactAssessments": []
}
---

# 场景感知智能成稿

## 用户目标

用户通过快捷键 A 开始一次自然语音表达时，不必预先组织句式，也不必逐次选择“去语气词”“正式润色”或“编程提示词”等工具。系统应在本次语音结束、最后一个 ASR 确认结果返回后，基于用户的完整表达和目标使用场景生成可直接使用的文本，再写入原目标输入位置。

智能成稿必须服务于用户原本想表达的内容，而不是替用户回答问题或擅自执行话语中的指令：

- 在聊天输入场景，删除无意义语气词和口头重复，修复语病，并在有充分上下文时纠正明显的识别错误，保留用户原有口吻和事实。
- 在办公写作场景，形成更正式、清楚、有条理的表达，但不凭空增加事实、承诺或结论。
- 在面向编码助手的输入场景，把发散口述整理为目标、背景、约束和待办清晰的请求；可以显式化用户已经表达出的关系，不得捏造未表达的业务需求或技术决策。

Chatbot 可以根据冻结原文的完整上下文推测并替换无法确定是否为 ASR 错词的词语。Chatbot 的成功响应必须是一份无需再次对话、确认或解释即可交给 Delivery 的最终文本。Chatbot 处理失败时，系统使用未被润色覆盖的 ASR 确认原文作为兜底交付文本。

MVP 的快捷键 A 固定表达 SmartDictation 意图。系统把冻结的完整原文、目标 EXE、可用时的应用名称和全局润色强度一起交给统一 Chatbot；不再先按固定规则选择 `general`、`chat`、`office` 或 `coding_request` 画像。Chatbot 依靠原文意图与应用上下文自主决定清理、纠错、语气和结构，应用身份只是提示而不是硬分类。

用户可选择四档输出方式：`Fast`、轻微整理、自然表达和理清重点，默认自然表达。`Fast` 表示快速响应、仅识别原话：等待本次最后一个 ASR final 并冻结确认文本后，明确跳过 LLM Chatbot，直接进入后续本地转换与 Delivery。其余三档继续表示允许 LLM 介入的最大程度：轻微整理以去冗余和明显语病为主，尽量保持原句式；自然表达改善清晰度、连贯性和场景表达，并在表达中存在明确或自然形成的并列意图时自动提取要点；理清重点允许更深的重组和归纳。原文已经清晰有序时应少改，任何 LLM 档都不得虚构事实、承诺、结论、需求或技术决策。

润色设置位于首页设置栏“输入效果”的最下方，只保留一处入口。用户通过四档控件选择 `Fast`、轻微整理、自然表达或理清重点；`Fast` 的用户说明固定为“快速响应，仅识别原话”。界面用最终可观察结果解释能力，不向 C 端用户暴露 Prompt、模型、画像、路由或“介入强度”等实现术语。

智能成稿和兜底选择必须在目标输入框外全部完成。普通可编辑文本目标接收的是一份完整、已校验的最终纯文本，系统应像用户日常粘贴大段内容一样一次性整体写入并保留段落，而不是边生成边输入、逐段注入或为换行模拟 Enter。已知终端、shell 等粘贴换行可能执行命令的目标不属于普通文本框，必须失败关闭或转为用户主动交付。自动交付不得因为保存或恢复剪贴板而终止主进程，也不得在无法安全保存原剪贴板数据时静默覆盖它；无法证明安全时必须在不可逆写入前降级或进入 Pending。正常输入可用性高于对固定格式名单的机械服从：只要当前剪贴板内容可以由 Zephyr 独立保存和恢复，就不得仅因格式身份未知而阻断输入；确实无法安全保存时，单行文本应先尝试不触碰剪贴板的安全交付，再进入 Pending。

MVP 面向线下分发的公司内部员工。员工不负责提供、输入、测试或轮换 DeepSeek API Key；内部部署流程预置一份由 HotwordAgent 与 TextProcessing 共同引用的凭据，两项用途仍分别授权和审计。智能润色使用一份独立、版本化、带哈希校验的统一语义 Prompt；三个 LLM 档的整理程度作为明确输入，不复制三份 Prompt。`Fast` 不加载或调用该 Prompt。TextProcessing 默认使用关闭思考模式的 `deepseek-v4-flash`。

## 验收场景

| ID | 用户可观察结果 | 当前验证要求 |
| --- | --- | --- |
| `AC-SD-01` | 快捷键 A 的处理中，流式 preview 只用于展示；只有用户结束输入且本次会话最后一个 ASR final 已被接收后，才以有序冻结的完整文本启动一次智能成稿 | 自动化 + ASR 迟到/超时故障注入 |
| `AC-SD-02` | ASR 确认原文与智能成稿结果是两个有来源标记的值；润色结果不能回写、覆盖或污染本次 ASR 原文，失败或取消也不能交付半成品 | 自动化 + 故障注入 |
| `AC-SD-03` | 面向聊天输入时，最终文本去除无意义语气词、口头重复和明显语病，同时保持原意、事实、称谓和表达立场 | 自动化回归 + 成对人工质量评测 + 真实聊天输入框互操作 |
| `AC-SD-04` | 面向办公写作时，最终文本比口述更正式、结构更清楚，同时不新增原文没有的事实、承诺或确定性 | 自动化回归 + 成对人工质量评测 + 真实办公软件互操作 |
| `AC-SD-05` | 面向编码助手时，最终文本把发散口述整理成可执行的请求结构，同时不代替下游编码助手回答或执行该请求，也不虚构需求 | 自动化回归 + 成对人工质量评测 + 真实编码助手输入框互操作 |
| `AC-SD-06` | 快捷键或其他触发入口携带的显式处理意图与会话生命周期分离；新增处理模式不复制录音、ASR、取消或 Delivery 控制链路 | 契约测试 + 真实快捷键运行时 |
| `AC-SD-07` | Chatbot 成功响应是单一、非空、不超过 8000 个 Unicode 字符且可直接交付的最终文本；Processing 与 Delivery 使用同一 8000 字符上限，超过上限时判定处理失败并使用 ASR 原文兜底，不得静默截断；系统不在成功响应后插入澄清对话或人工确认步骤 | 自动化 + 7999/8000/8001 字符边界 + Delivery 故障注入 |
| `AC-SD-08` | Chatbot 从 adapter 接收请求开始，到完整响应完成 JSON 解析并通过应用层结果校验为止具有 20 秒硬截止；20 秒内没有产生有效最终文本，或发生网络/服务错误、空响应、非法 JSON、字段缺失、空文本、截断等无效响应时，系统选择冻结的 ASR 确认原文继续进入既有 Delivery；用户主动取消仍然禁止任何文本交付，目标复验或注入失败仍按 Delivery/Pending 语义处理 | 自动化 + 20 秒边界/取消/无效响应/注入故障注入 |
| `AC-SD-09` | Chatbot 超时必须向 IncidentVault 非阻塞地提交带会话关联、处理阶段、耗时、超时预算、润色强度和稳定原因码的异常轨迹；文本等用户内容仍服从 IncidentVault 已有授权快照，Vault 拥塞或故障不得阻断原文兜底 | 自动化 + IncidentVault 拥塞/故障注入 |
| `AC-SD-10` | 智能成稿或 ASR 兜底在进入 Delivery 前已经完整确定；对普通可编辑文本目标，单行和多行结果都作为一个纯文本载荷一次性粘贴，不要求用户预先按 EXE 启用 `clipboard_compatibility`，不为 LF 生成 Enter；目标复验或 Ctrl+V 提交前失败进入 Pending，提交后剪贴板并发变化只跳过恢复并记录异常，不能触发可能重复的再次交付；已知终端或命令执行表面不自动接收含 LF 的生成文本 | 自动化 + 剪贴板竞争故障注入 + 聊天/办公/编码输入框互操作 |
| `AC-SD-11` | TextProcessing 只加载一份版本化统一语义 Prompt；缺失、篡改或哈希不符时失败关闭并使用冻结原文。Prompt 明确定义三档强度、应用上下文只是提示、事实保持、自动提取要点和“清晰时少改”，adapter 继续独立拥有 JSON Output envelope 与输出校验 | Prompt manifest/hash/缺失/篡改自动化 + 三档跨应用语料回归 |
| `AC-SD-12` | 内部员工安装和使用时看不到也不需要处理 API Key 配置；HotwordAgent 与 TextProcessing 读取同一个预置 DeepSeek credential reference，但保留独立 purpose 授权和使用轨迹。Key 缺失时智能成稿直接使用 ASR 原文兜底并提供非秘密诊断，不把 Key 写入前端、普通配置、Prompt、日志或 Incident | 凭据零序列化/零前端暴露/用途隔离自动化 + Windows Credential Manager 重启持久化 + 内部安装包验收 |
| `AC-SD-13` | 快捷键 A 固定请求 SmartDictation；非 `Fast` 处理冻结并传递完整 ASR 原文、目标 EXE、可选应用名称和三种 LLM 整理程度的快照，默认自然表达。请求不得包含窗口标题、页面、屏幕、聊天历史、文档正文、源代码或剪贴板内容；未覆盖部署配置时使用关闭思考模式的 `deepseek-v4-flash` | 请求数据边界/三种 LLM 程度配置迁移与快照/默认模型自动化 + 真实快捷键运行时 |
| `AC-SD-14` | 首页设置栏的“输入效果”末尾显示唯一的智能润色入口；用户可选择 `Fast`、轻微整理、自然表达或理清重点，默认自然表达。`Fast` 显示“快速响应，仅识别原话”；其余文案说明允许的整理结果，不暴露 Prompt、模型、画像、路由或“介入强度”等专业术语。选择后自动保存并在重启后保持 | 前端自动化 + 目标用户可用性观察 + Windows WebView2 视觉/键盘可访问性 + 重启持久化 |
| `AC-SD-15` | 用户选择 `Fast` 后，系统仍等待并冻结有序的完整 ASR final，但不创建 TextProcessing/Chatbot 请求，不等待 LLM、不加载语义 Prompt，也不把该路径记录为超时或失败兜底；确认原文经允许的本地转换后直接进入既有 Delivery。History 必须把它标记为用户主动选择的 ASR 直出来源，而不是 LLM 失败；因此该档响应时间不包含 LLM 往返 | 自动化旁路与 provenance 契约 + 真实快捷键运行时 + 真实输入框互操作 + 重启持久化 |
| `AC-SD-16` | 自动交付保存和恢复剪贴板时不得让主进程崩溃；只有已经成为 Zephyr 自有数据且可以独立重建的原格式才能自动恢复。不得仅因格式身份不在内置名单而拒绝可安全保存的剪贴板；无法证明可重建时必须在覆盖前失败，并对单行文本先尝试不触碰剪贴板的安全交付，再进入 Pending。任何交付方式只提交了部分输入事件、在不可逆提交附近失联或不能证明尚未提交时，都必须标记为状态不确定并禁止自动重试。剪贴板清空、事务标记写入、逐格式写入和恢复之间的任一辅助进程故障不得被误判为可安全重放，也不得把内容送往未经复验的当前窗口 | 自动化 + 原生辅助进程/剪贴板格式/部分 SendInput 故障注入 + 真实 Windows 文本/HTML/RTF/图片/文件/自定义格式互操作 |

## 明确不规定的实现

- 不规定文本池和智能成稿器的具体文件名、类名、线程模型或进程形态；MVP 不要求存在独立 Router 组件，拟议边界不得被当成当前实现事实。
- 不要求把一次成稿拆成多次模型调用；逻辑上的纠错、去冗余、结构化和风格适配可以由一次受约束调用完成。
- MVP 只把目标 EXE 与可用时的应用名称作为应用上下文；不提供逐应用画像配置，也不把应用映射成固定写作模式。未来增加显式用户意图或更丰富上下文时需扩展输入契约。
- “使用目标场景”不授权把窗口标题、屏幕、文档正文、聊天历史、源代码或既有剪贴板内容作为 Prompt 上下文；若未来需要这些上下文，必须形成单独的知情授权和数据边界。交付为本地恢复而持有的剪贴板快照及格式元数据不得进入 Prompt、History 或普通日志。
- 快捷键 A 的正常成功路径不增加原文确认步骤；冻结的 ASR 确认原文作为 Chatbot 失败后的自动兜底值存在。
- MVP 不要求用户为普通可编辑目标预先理解或配置 `clipboard_compatibility`。实现可以演进具体剪贴板或目标控件技术，但必须保持“完整定稿后一次性交付、不模拟 Enter、终端失败关闭”的可观察结果。
- MVP 不提供员工可见的 API Key 配置或 Prompt 编辑器；凭据预置、轮换和 Prompt 发布属于内部部署职责。
- 不规定智能润色控件的具体组件文件、原生控件或视觉装饰；只规定首页位置、四个用户可理解的输出方式、唯一入口、自动保存和可访问的可观察结果。
- 三种 LLM 整理程度不各自维护 Prompt 文件；统一 Prompt 中的程度定义与 adapter 的 JSON schema、数据转义和通用安全 envelope 仍属于不同责任。
- 不规定 `Fast` 旁路的具体函数、枚举或文件名；但它必须是用户主动选择的正常路径，不能复用“LLM 调用后失败”的语义，也不能产生 LLM 超时/失败事件。
- DeepSeek 请求仍需按供应商协议设置有限的 `max_tokens`；token 上限与产品的 8000 Unicode 字符上限是不同单位。应用在 JSON 解析后按与 Delivery 相同的字符定义校验，不得通过截断来满足上限。

## 局部假设

- `ASM-SD-01`（Resolved for MVP）：目标 EXE 与应用名称只作为模型的场景提示，不能证明系统理解了用户意图。模型同时依据冻结原文判断合适表达；MVP 不采集窗口标题、页面、屏幕或输入表面内容。
- `ASM-SD-02`（Resolved for MVP）：快捷键 A 固定表达 SmartDictation；MVP 不设固定画像 Router 或逐应用覆盖。统一模型接收应用上下文和原文，三档全局强度限定允许的最大介入程度。
- `ASM-SD-03`（Resolved）：即使不能确定某个词是否为 ASR 错词，也允许 Chatbot 根据完整上下文自动推测并替换；原始 ASR 确认文本必须继续独立保留，不能被猜测结果覆盖。
- `ASM-SD-04`（Resolved）：Vibe coding 的结果必须是可直接交给 Delivery 的单一文本，不在本次链路中向用户发起澄清对话；模型仍不得补造用户未表达的需求。
- `ASM-SD-05`（Partially resolved）：Chatbot 处理硬截止已确认为 20 秒；处理失败统一使用 ASR 确认原文兜底，超时轨迹进入 IncidentVault。松开快捷键到请求开始前的本地处理预算，以及完整端到端延迟目标仍未确认。
- `ASM-SD-06`（Resolved）：办公和 vibe coding 成稿允许受控换行；普通可编辑文本目标使用完整定稿后的单次整体粘贴，不要求按 EXE 预先启用兼容模式，也不把 LF 转为 Enter。已知终端、shell 或命令执行表面保持失败关闭。除允许的 LF 外，NUL、双向覆盖/隔离符和其他控制字符仍不因此获得授权。
- `ASM-SD-07`（Resolved）：Processing 与 Delivery 统一使用 8000 Unicode 字符硬上限；模型结果超过上限视为处理失败并回退 ASR 原文，不截断模型结果。ASR 原文本身超过上限时仍由 Delivery 按既有失败/Pending 语义处理。
- `ASM-SD-08`（Resolved）：智能润色只使用一份独立、版本化、带哈希校验的统一语义 Prompt；三个 LLM 档的整理程度作为请求输入，`Fast` 不进入该请求。JSON Output、数据封装、输出 schema 和通用安全边界继续由 adapter 拥有。
- `ASM-SD-09`（Confirmed for internal MVP）：安装包只线下分发给公司内部员工，维护者负责同一 DeepSeek Key 的预置、监控、轮换和吊销，员工无需配置。该信任模型接受桌面端共享秘密可能被本机账户提取的剩余风险，不把流量监控表述为绝对防泄露保证；外部分发时必须重新评估。
- `ASM-SD-10`（Resolved for MVP）：TextProcessing 默认模型为 `deepseek-v4-flash` 并显式关闭思考模式。模型名由内部部署配置拥有，不向员工暴露；一次处理使用冻结的配置快照。
- `ASM-SD-11`（Resolved for MVP）：`Fast` 是明确的 ASR 直出模式，不是最低 LLM 润色强度。它保留 ASR provider 自身的确认文本、标点、热词和允许的本地简繁转换，因此“识别原话”不承诺逐音逐字转写。
- `ASM-SD-12`（Confirmed）：系统不能把外部应用仍然拥有的活剪贴板对象视为 Zephyr 自有的完整快照。自动恢复只覆盖已经按值复制并能安全重建的格式；数据完整性仍是硬边界，但“格式身份未知”本身不证明数据不可保存，系统应在不静默丢失原内容的前提下优先保持正常输入可用。

## 概念迭代记录

| ID | 状态 | 当前判断与复核条件 |
| --- | --- | --- |
| `CI-SD-01` | Rejected | “目标 EXE 固定分类 + 四种画像 Prompt + 用户逐应用覆盖”把应用线索误当成用户意图，并让简单润色助手过早复杂化。当前统一模型方案已替代它；除非未来多快捷键或显式意图产生可观察需求，否则不要恢复固定画像 Router。 |
| `CI-SD-02` | Challenged | 四档中的 `Fast` 已确认为不调用 LLM 的硬边界；其余三个整理阶段是否采用连续滑轨、由浅到深的视觉反馈和柔性吸附仍处于讨论中，尚未升级为 MVP 验收。连续视觉不能在后端静默压回粗粒度档位而制造虚假精细控制。 |
| `CI-SD-03` | Rejected | “协议、构建和自动化通过即可说明智能润色有效”是错误判断。现有证据只证明工程链路和失败边界；聊天、办公和 coding 输出是否更有用必须通过 `human_quality_eval` 与真实应用互操作证明。 |
| `CI-SD-04` | Open | 若引入 `0–100` 连续值，前端位置必须对应真实处理差异，不能在后端静默压回 1/2/3 造成虚假精细控制。候选方案是把一个用户滑块映射为保留原句、句式改写、结构重排、场景语气和分点门槛等内部策略曲线；确认前需要跨场景语料对照评测。 |
| `CI-SD-05` | Confirmed | 用户已确认提供 `Fast` 正常模式：“快速响应，仅识别原话”。它主动跳过 LLM，但仍保留 ASR provider 与允许的本地转换；不再把最低 LLM 整理程度命名为“保留原话”。 |
| `CI-SD-06` | Rejected | “保存活 OLE `IDataObject`、粘贴后再 `OleSetClipboard`/`OleFlushClipboard` 就等于完整原子恢复”已被真实 Windows 两次进程崩溃推翻。不得通过增加线程栈、延长等待或重试继续包装该方案；后续采用自有数据快照、完整事务串行化和原生故障隔离。 |
| `CI-SD-07` | Rejected | “名称不在固定白名单就一定不安全”把格式识别能力误当成数据所有权证明，并曾导致普通 Chromium 文本永久进入 Pending。安全判断应基于当前数据能否成为 Zephyr 自有、受限且可重建的值；格式身份只决定是否需要专用解析或额外验证。 |

## 架构决策

- [ADR-0002：单所有者、有界、失败关闭的语音会话](../architecture/adr/0002-single-owner-bounded-voice-session.md)
- [ADR-0003：成功注入为提交点，失败进入内存 Pending](../architecture/adr/0003-delivery-commit-point-and-pending-output.md)
- [ADR-0004：自定义 origin 授权必须先于凭据读取](../architecture/adr/0004-trust-before-credentials.md)
- [ADR-0008：产品前端融合、后端隔离的本地异常恢复](../architecture/adr/0008-incident-vault-isolated-recovery.md)
- [ADR-0012：统一语音输入控制面所有权](../architecture/adr/0012-unified-voice-input-control-plane.md)
- [ADR-0013：严格 mailbox-owned 语音运行时](../architecture/adr/0013-strict-mailbox-owned-voice-runtime.md)
- [ADR-0014：智能成稿使用一次性整体粘贴交付（Accepted）](../architecture/adr/0014-atomic-smart-dictation-paste.md)
- [ADR-0015：内部分发共享 DeepSeek 凭据并隔离写作画像 Prompt（Superseded）](../architecture/adr/0015-internal-shared-deepseek-credential-and-isolated-prompts.md)
- [ADR-0016：MVP 确定性路由与 DeepSeek Flash 默认模型（Superseded）](../architecture/adr/0016-deterministic-mvp-routing-and-deepseek-flash.md)
- [ADR-0017：统一的应用感知智能润色与三档强度（Accepted）](../architecture/adr/0017-unified-app-aware-polishing-with-strength.md)
- [ADR-0018：以自有剪贴板快照和隔离粘贴进程替代 OLE 活对象恢复（Proposed）](../architecture/adr/0018-owned-clipboard-transaction-and-isolated-paste.md)
- [场景感知文本路由与智能成稿 Proposal](../architecture/proposals/context-aware-text-routing.md)

固定画像 Router 与四份隔离 Prompt 已由 ADR-0017 替代。MVP 当前采用单一应用感知 Prompt 和全局三档强度；未来多快捷键、多意图路由仍未成为 Accepted 决策。ADR-0018 记录替代 ADR-0014 中 OLE 活对象恢复的候选架构。当前分支已经按该候选边界实现 helper 隔离，但实现先行不等于决策已被接受，也不等于目标环境验证通过；ADR-0018 在指定收口者完成评审前继续保持 `Proposed`。

## 当前实现入口

- 触发语义：`src-tauri/src/voice_trigger.rs`、`src-tauri/src/shortcut_manager/`
- 会话完成与 ASR final：`src-tauri/src/voice_controller/workflow/`
- ASR provider 与 preview：`src-tauri/src/provider.rs`、`src-tauri/src/streaming_pipeline.rs`、`src-tauri/src/preview.rs`
- 冻结原文、应用上下文计划、Prompt Repository 与 DeepSeek adapter：`src-tauri/src/text_processing/`
- 统一智能润色 Prompt：`src-tauri/resources/prompts/smart_dictation/v2/`
- 文本交付、事务仲裁与 Pending：`src-tauri/src/delivery.rs`、`src-tauri/src/clipboard_transaction.rs`、`src-tauri/src/inject.rs`、`src-tauri/src/pending_output_service.rs`
- 共享 helper 协议与隔离 Windows 实现：`src-tauri/crates/paste-protocol/`、`src-tauri/crates/zephyr-paste-helper/`
- History provenance 与热词学习栅栏：`src-tauri/src/history.rs`、`src-tauri/src/hotwords.rs`
- 首页智能润色为四档控件：`Fast`、轻微整理、自然表达和理清重点；相关入口与保存链路：`src/features/settings/PolishLevelSetting.tsx`、`src/features/settings/SettingsSidebar.tsx`、`src/app/AppShellV2.tsx`
- 异常捕获：`src-tauri/src/incident/`

当前 finalize 链路只在权威 ASR final 返回后创建不可变 `FrozenTranscript`。`Fast` 在此后把 Processing 标记为策略跳过，不加载 Prompt、不创建 TextProcessing 请求、不记录 LLM 超时或失败，并以 `asr_direct` provenance 交给 Delivery；其余三档才创建 ProcessingPlan、调用 Processor，任何非取消处理失败选择冻结原文并标记 `asr_fallback`。普通可编辑目标的 SmartDictation 单行和多行都请求一次 `clipboardPaste`，已知终端含 LF 仍在不可逆写入前进入 Pending。

Bootstrap 为首次交付和 Pending 重发注入同一个 `ClipboardTransactionService`。它用异步互斥锁覆盖 helper 自检、捕获、发布、提交、500ms 载荷保留、恢复及最多一次 recover；显式复制不取锁。主进程通过 stdin 单请求和 stdout NDJSON 驱动 `zephyr-paste-helper`，按最后可信阶段仲裁 `NotSubmitted | Submitted | Unknown`。helper 按值验证并保存已解析的已知格式，以及能够同步物化、锁定并通过大小边界的动态注册 `HGLOBAL`；格式名称仍随原始字节一起保存，以便恢复 helper 重新注册。它使用当前用户 DPAPI 和 UUID 事务文件，发布私有标记与 Unicode 文本，并在目标 HWND/PID/创建时间/规范化 EXE/前台身份复验后发送一次 Ctrl+V。恢复在同一次剪贴板锁内重验 sequence、标记和指纹；竞争变化只跳过恢复。捕获在覆盖前明确失败且文本为单行时，同一隔离 helper 改用 Unicode 输入；多行不模拟 Enter。生产自动交付主进程不再含 OLE、Win32 剪贴板写入或 `SendInput`。

## 验证状态

当前实现状态为 `in_progress`，验证状态为 `invalidated`。基于 revision `a69242240d7da4e3d4f086b61548bfa019f93bdf` 的脏工作树已修复动态注册格式被固定名称名单误拒绝的问题，并把覆盖前确认未提交的单行降级限制在 SmartDictation；Legacy 兼容路径不会被静默改回已知不适配的 Unicode 语义。当前 Windows 打包前检查通过，包含前端 58 项、Rust 主库 195 项、共享协议 1 项、helper 8 项，以及架构、ASR 边界和秘密扫描；2 项需要真实网络凭据的测试按预期忽略。用户在重新使用后报告“暂时没遇到问题”，这只能作为修复方向的冒烟反馈：没有绑定 clean revision、安装包、精确应用/格式组合、重复次数或剪贴板恢复比对，因此不新增正式验证证据，也不关闭既有失败。


现有自动化证据不具备 `human_quality_eval` 或 `usability_observation` 能力，因此不能回答“用户是否感到明显润色”“三档是否可预测”或“结果是否比原文更可用”。这些问题属于未完成的产品验证，不应再用链路测试通过来代替。

仍未完成：把当前修复形成 clean revision 和可追踪安装包；在受控 clipboard owner 上验证已知结构、动态注册值、图片、文件和无法物化格式的逐格式恢复等价；在真实 helper 进程上覆盖每个阶段强杀、超时、剪贴板占用、部分提交和单次 recover；验证记事本、浏览器/WebView2、Word/Outlook、VS Code/Cursor 和终端；确认 PendingFull 时用户始终能找到并处理队列而无需重启；执行 1–2 分钟长语音并复核 ASR/润色/Delivery 哈希、单次提交、恢复规则和主进程存活；完成真实 DeepSeek 成稿质量评测、Windows Credential Manager 重启持久化、覆盖安装以及 IncidentVault 拥塞故障注入。目标环境崩溃关闭前不得恢复为 `partial`，缺少完整证据时不得升级为 `validated` 或发布完成。

## 澄清历史

- 2026-08-28：内部测试打包改为在编译期注入共享 DeepSeek 凭据，员工安装后无需配置 Key；构建门禁在凭据缺失时失败，客户端可提取凭据的边界仅适用于当前受控小范围测试。
- 2026-08-28：用户明确快捷键 A 应允许自然、发散地口述，并根据聊天、办公写作和 vibe coding 等使用场景生成可直接输入的文本。
- 2026-08-28：文档评审挑战“仅凭应用类别即可确定用户意图”的假设，将其降为默认画像信号，并保留为待继续确认的产品问题。
- 2026-08-28：用户确认 ASR 确认原文是 Chatbot 失败后的自动兜底；模型可以猜测疑似错词；所有成功响应直接进入 Delivery；处理超时轨迹进入 IncidentVault。
- 2026-08-28：用户确认 Chatbot 超过 20 秒即超时，并指定 DeepSeek JSON Output 文档作为结构化响应依据；空 content、非法或不完整结果按处理失败进入原文兜底。
- 2026-08-28：用户确认首版 Processing 不限制 Chatbot 输出文本长度，并选择后续调整 Delivery 及注入边界以支持办公和 vibe coding 的受控多段文本；Delivery 现有 8000 字符上限是否移除仍作为独立安全问题待确认。
- 2026-08-28：用户进一步确认 Processing 与 Delivery 统一采用当前 8000 Unicode 字符上限，替代上一轮“Processing 不限长”的候选表述；超过上限使用原文兜底，不做静默截断。
- 2026-08-28：用户确认所有智能成稿处理应在写入目标输入框前完成，最终结果像日常粘贴大段格式化内容一样一次性整体进入输入框；按应用预先启用剪贴板兼容不再作为普通文本框的 MVP 前置条件。参考 OpenWhispr 后保留目标复验、剪贴板并发保护和终端换行执行防护。
- 2026-08-28：用户确认 HotwordAgent 与 TextProcessing 共用同一 DeepSeek Key，但用途继续隔离；MVP 仅供公司内部员工线下安装，Key 由内部部署预置和维护，员工不接触配置。四种润色画像的语义 Prompt 必须使用相互隔离的独立文件，以支持快速并行迭代。
- 2026-08-28：用户确认 Router 首版使用目标 EXE 与用户逐应用覆盖，优先级为“用户覆盖 > 内置 EXE 分类 > general”，浏览器默认 `general`；TextProcessing 默认模型为关闭思考模式的 `deepseek-v4-flash`。
- 2026-08-28：用户挑战固定画像方案，重新确认产品目标是与 Typeless 同类但更场景智能的统一 AI 润色助手；当前应用身份与完整原文直接交给模型自主判断，不再维护固定画像 Router 和逐应用覆盖。
- 2026-08-28：用户确认润色能力提供 1–3 档全局强度，档位越高允许介入越深，默认采用 2 档；表达存在明确或自然形成的分列意图时自动提取要点，原文清晰时不为展示能力而强改。
- 2026-08-28：用户要求把智能润色的唯一设置入口移到首页设置栏“输入效果”最下方，以三段滑块选择结果层级；界面从 Job to Be Done 解释“说完后得到怎样的可用文字”，面向 C 端隐藏 Prompt、模型、画像、路由和介入程度等内部术语。
- 2026-08-28：用户认为当前前端解释仍然过重，确认滑块表达的是“希望最终文字被整理到什么程度”，并挑战“保留原话”名称，因为一档仍会清理内容。
- 2026-08-28：用户提出更丝滑的连续渐变滑轨与三个阶段的范围吸附，并明确本轮先讨论参数化润色量化，不立即把候选的 `0–100` 方案写成已确认验收；同时授权直接挑战和改进仍处于 MVP 阶段的文档系统。
- 2026-08-31：用户确认智能润色改为四档，并将第一档命名为 `Fast`，面向用户说明“快速响应，仅识别原话”；该档主动跳过 LLM Chatbot，仅在完整 ASR final 冻结后进入本地转换和 Delivery。
- 2026-08-31：用户提供第二次真实 Windows 短文本崩溃日志；13 字文本在 provider、relay、aggregate 和 delivery 阶段哈希一致，排除长文本、ASR 聚合和 LLM 润色后仍在 AtomicPaste 边界栈溢出。用户确认把“自有剪贴板快照、单一事务所有者、隔离粘贴辅助进程、无法安全保存时失败关闭”的替代计划写入文档系统。
- 2026-08-31：基于 revision `b0c2faa1e116763a18cfeb318cb50bbbb238996a` 的源码复核确认，当前 SmartDictation 首次交付与 Pending 重新交付共用危险 AtomicPaste，Legacy `clipboard_compatibility` 仍可启用，现有自动化未执行原生 OLE 路径。用户进一步确认文档应区分当前已证实偏差与迁移方案风险，并把部分 `SendInput`、提交附近失联以及剪贴板清空后标记写入前崩溃纳入不可自动重试边界。
- 2026-09-01：真实运行在复制普通 Chromium 文本后重复进入 `clipboard_snapshot_unsupported`，确认可保存的注册元数据与 Unicode/HTML 文本并存。用户确认正常输入可用性优先于机械格式名单；能够成为应用自有值的格式不得仅因身份未知而被拒绝，剪贴板确实无法安全保存时单行文本先使用不触碰剪贴板的安全输入。
