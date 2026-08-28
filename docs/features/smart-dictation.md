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
    "status": "partial",
    "sourceRevision": "c4c3cac5a6680084c607b360eb794987a1e4c831",
    "worktreeState": "dirty",
    "changedPaths": ["src-tauri/src/text_processing/", "src-tauri/resources/prompts/smart_dictation/", "src-tauri/src/voice_controller/", "src-tauri/src/delivery.rs", "src-tauri/src/inject.rs", "src-tauri/src/history.rs", "src-tauri/src/hotwords.rs", "src-tauri/src/config.rs", "src/features/settings/", "src/app/AppShellV2.tsx"],
    "reviewedAt": "2026-08-28",
    "summary": "核心 MVP 源码已切换为统一的应用感知 Prompt 与全局三档强度：ASR final 冻结、应用上下文数据边界、DeepSeek JSON Processing、取消/失败兜底、AtomicPaste、History provenance、热词学习栅栏和强度设置已有源码与自动化复核。内部安装凭据预置、真实模型联调和目标环境仍未闭环。",
    "knownDeviations": ["仓库尚未提供内部管理员使用的非交互凭据预置工具；当前运行时读取兼容保留的共享 Windows Credential Manager 槽，安装部署仍需在应用外完成预置。"]
  },
  "validationStatus": "partial",
  "components": ["system.zephyr", "frontend.features", "backend.services", "backend.repositories", "backend.voice-controller", "backend.streaming", "backend.delivery", "backend.shortcut", "backend.incident-vault", "platform.windows"],
  "decisions": ["ADR-0002", "ADR-0003", "ADR-0004", "ADR-0008", "ADR-0012", "ADR-0013", "ADR-0014", "ADR-0017"],
  "validationSlices": [
    { "id": "AC-SD-01", "components": ["backend.voice-controller", "backend.streaming"], "requiredEvidence": ["automated", "fault_injection"] },
    { "id": "AC-SD-02", "components": ["backend.services", "backend.voice-controller", "backend.delivery"], "requiredEvidence": ["automated", "fault_injection"] },
    { "id": "AC-SD-03", "components": ["backend.services", "backend.delivery"], "requiredEvidence": ["automated", "external_app_interop"] },
    { "id": "AC-SD-04", "components": ["backend.services", "backend.delivery"], "requiredEvidence": ["automated", "external_app_interop"] },
    { "id": "AC-SD-05", "components": ["backend.services", "backend.delivery"], "requiredEvidence": ["automated", "external_app_interop"] },
    { "id": "AC-SD-06", "components": ["backend.shortcut", "backend.voice-controller", "backend.services"], "requiredEvidence": ["automated", "runtime_hook"] },
    { "id": "AC-SD-07", "components": ["backend.services", "backend.delivery"], "requiredEvidence": ["automated", "fault_injection"] },
    { "id": "AC-SD-08", "components": ["backend.services", "backend.voice-controller", "backend.delivery"], "requiredEvidence": ["automated", "fault_injection"] },
    { "id": "AC-SD-09", "components": ["backend.services", "backend.incident-vault"], "requiredEvidence": ["automated", "fault_injection"] },
    { "id": "AC-SD-10", "components": ["backend.voice-controller", "backend.delivery"], "requiredEvidence": ["automated", "fault_injection", "external_app_interop"] },
    { "id": "AC-SD-11", "components": ["backend.services"], "requiredEvidence": ["automated"] },
    { "id": "AC-SD-12", "components": ["frontend.features", "backend.services", "backend.repositories", "platform.windows"], "requiredEvidence": ["automated", "windows_webview2", "restart_persistence"] },
    { "id": "AC-SD-13", "components": ["backend.shortcut", "backend.services", "backend.voice-controller"], "requiredEvidence": ["automated", "runtime_hook"] }
  ],
  "evidence": [{"id":"EV-SD-MVP-AUTOMATED-20260828","acceptanceIds":["AC-SD-01","AC-SD-02","AC-SD-03","AC-SD-04","AC-SD-05","AC-SD-06","AC-SD-07","AC-SD-08","AC-SD-09","AC-SD-10","AC-SD-11","AC-SD-12","AC-SD-13"],"acceptanceCoverage":[{"acceptanceId":"AC-SD-01","coverage":"partial"},{"acceptanceId":"AC-SD-02","coverage":"partial"},{"acceptanceId":"AC-SD-03","coverage":"partial"},{"acceptanceId":"AC-SD-04","coverage":"partial"},{"acceptanceId":"AC-SD-05","coverage":"partial"},{"acceptanceId":"AC-SD-06","coverage":"partial"},{"acceptanceId":"AC-SD-07","coverage":"partial"},{"acceptanceId":"AC-SD-08","coverage":"partial"},{"acceptanceId":"AC-SD-09","coverage":"partial"},{"acceptanceId":"AC-SD-10","coverage":"partial"},{"acceptanceId":"AC-SD-11","coverage":"partial"},{"acceptanceId":"AC-SD-12","coverage":"partial"},{"acceptanceId":"AC-SD-13","coverage":"partial"}],"method":"automated","result":"pass","freshness":"current","capabilities":["automated","fault_injection"],"scope":"Rust 181 项库测试、前端 48 项测试、production build、秘密扫描、架构检查和 ASR 边界检查通过；覆盖统一 Prompt、应用上下文数据边界、三档强度、配置迁移、失败兜底、AtomicPaste、History provenance 与热词学习栅栏","testRefs":["cargo test --manifest-path src-tauri/Cargo.toml --lib","src-tauri/src/text_processing/model.rs","src-tauri/src/text_processing/adapter.rs","src-tauri/src/text_processing/unified_prompt_repository.rs","src-tauri/src/config.rs","src-tauri/src/voice_controller/workflow/finalize.rs","src-tauri/src/delivery.rs","src-tauri/src/inject.rs","src-tauri/src/history.rs","src-tauri/src/hotwords.rs","src/features/settings/MoreSettingsPanel.test.tsx","npm test","npm run build","npm run security:secrets","npm run architecture:check","npm run architecture:asr"],"limitations":["未启动真实 Tauri/WebView2 与 Windows 全局快捷键","未使用真实 DeepSeek 凭据进行网络联调","未验证聊天、Office、编码助手和终端输入框互操作","未验证 Windows Credential Manager 重启持久化与内部安装包预置","缺少完整 finalize 竞争与 IncidentVault 拥塞集成故障注入","缺少三档跨应用语料质量评测","dirty worktree evidence 没有不可变 build identity"],"sourceRevision":"c4c3cac5a6680084c607b360eb794987a1e4c831","worktreeState":"dirty","changedPaths":["src-tauri/src/text_processing","src-tauri/resources/prompts/smart_dictation","src-tauri/src/voice_controller","src-tauri/src/delivery.rs","src-tauri/src/inject.rs","src-tauri/src/history.rs","src-tauri/src/hotwords.rs","src-tauri/src/config.rs","src/features/settings","src/app/AppShellV2.tsx"],"environment":"Windows development workspace; Rust tests and happy-dom frontend tests; no immutable package identity","validatedAt":"2026-08-28"}],
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

用户可选择 1–3 档全局润色强度，默认 2 档。1 档以去冗余和明显语病为主，尽量保持原句式；2 档改善清晰度、连贯性和场景表达，并在表达中存在明确或自然形成的并列意图时自动提取要点；3 档允许更深的重组和归纳。档位表示允许的最大介入程度，原文已经清晰有序时应少改，任何档位都不得虚构事实、承诺、结论、需求或技术决策。

智能成稿和兜底选择必须在目标输入框外全部完成。普通可编辑文本目标接收的是一份完整、已校验的最终纯文本，系统应像用户日常粘贴大段内容一样一次性整体写入并保留段落，而不是边生成边输入、逐段注入或为换行模拟 Enter。已知终端、shell 等粘贴换行可能执行命令的目标不属于普通文本框，必须失败关闭或转为用户主动交付。

MVP 面向线下分发的公司内部员工。员工不负责提供、输入、测试或轮换 DeepSeek API Key；内部部署流程预置一份由 HotwordAgent 与 TextProcessing 共同引用的凭据，两项用途仍分别授权和审计。智能润色使用一份独立、版本化、带哈希校验的统一语义 Prompt；三档强度作为明确输入，不复制三份 Prompt。TextProcessing 默认使用关闭思考模式的 `deepseek-v4-flash`。

## 验收场景

| ID | 用户可观察结果 | 当前验证要求 |
| --- | --- | --- |
| `AC-SD-01` | 快捷键 A 的处理中，流式 preview 只用于展示；只有用户结束输入且本次会话最后一个 ASR final 已被接收后，才以有序冻结的完整文本启动一次智能成稿 | 自动化 + ASR 迟到/超时故障注入 |
| `AC-SD-02` | ASR 确认原文与智能成稿结果是两个有来源标记的值；润色结果不能回写、覆盖或污染本次 ASR 原文，失败或取消也不能交付半成品 | 自动化 + 故障注入 |
| `AC-SD-03` | 面向聊天输入时，最终文本去除无意义语气词、口头重复和明显语病，同时保持原意、事实、称谓和表达立场 | 自动化语料评测 + 真实聊天输入框互操作 |
| `AC-SD-04` | 面向办公写作时，最终文本比口述更正式、结构更清楚，同时不新增原文没有的事实、承诺或确定性 | 自动化语料评测 + 真实办公软件互操作 |
| `AC-SD-05` | 面向编码助手时，最终文本把发散口述整理成可执行的请求结构，同时不代替下游编码助手回答或执行该请求，也不虚构需求 | 自动化语料评测 + 真实编码助手输入框互操作 |
| `AC-SD-06` | 快捷键或其他触发入口携带的显式处理意图与会话生命周期分离；新增处理模式不复制录音、ASR、取消或 Delivery 控制链路 | 契约测试 + 真实快捷键运行时 |
| `AC-SD-07` | Chatbot 成功响应是单一、非空、不超过 8000 个 Unicode 字符且可直接交付的最终文本；Processing 与 Delivery 使用同一 8000 字符上限，超过上限时判定处理失败并使用 ASR 原文兜底，不得静默截断；系统不在成功响应后插入澄清对话或人工确认步骤 | 自动化 + 7999/8000/8001 字符边界 + Delivery 故障注入 |
| `AC-SD-08` | Chatbot 从 adapter 接收请求开始，到完整响应完成 JSON 解析并通过应用层结果校验为止具有 20 秒硬截止；20 秒内没有产生有效最终文本，或发生网络/服务错误、空响应、非法 JSON、字段缺失、空文本、截断等无效响应时，系统选择冻结的 ASR 确认原文继续进入既有 Delivery；用户主动取消仍然禁止任何文本交付，目标复验或注入失败仍按 Delivery/Pending 语义处理 | 自动化 + 20 秒边界/取消/无效响应/注入故障注入 |
| `AC-SD-09` | Chatbot 超时必须向 IncidentVault 非阻塞地提交带会话关联、处理阶段、耗时、超时预算、润色强度和稳定原因码的异常轨迹；文本等用户内容仍服从 IncidentVault 已有授权快照，Vault 拥塞或故障不得阻断原文兜底 | 自动化 + IncidentVault 拥塞/故障注入 |
| `AC-SD-10` | 智能成稿或 ASR 兜底在进入 Delivery 前已经完整确定；对普通可编辑文本目标，单行和多行结果都作为一个纯文本载荷一次性粘贴，不要求用户预先按 EXE 启用 `clipboard_compatibility`，不为 LF 生成 Enter；目标复验或 Ctrl+V 提交前失败进入 Pending，提交后剪贴板并发变化只跳过恢复并记录异常，不能触发可能重复的再次交付；已知终端或命令执行表面不自动接收含 LF 的生成文本 | 自动化 + 剪贴板竞争故障注入 + 聊天/办公/编码输入框互操作 |
| `AC-SD-11` | TextProcessing 只加载一份版本化统一语义 Prompt；缺失、篡改或哈希不符时失败关闭并使用冻结原文。Prompt 明确定义三档强度、应用上下文只是提示、事实保持、自动提取要点和“清晰时少改”，adapter 继续独立拥有 JSON Output envelope 与输出校验 | Prompt manifest/hash/缺失/篡改自动化 + 三档跨应用语料回归 |
| `AC-SD-12` | 内部员工安装和使用时看不到也不需要处理 API Key 配置；HotwordAgent 与 TextProcessing 读取同一个预置 DeepSeek credential reference，但保留独立 purpose 授权和使用轨迹。Key 缺失时智能成稿直接使用 ASR 原文兜底并提供非秘密诊断，不把 Key 写入前端、普通配置、Prompt、日志或 Incident | 凭据零序列化/零前端暴露/用途隔离自动化 + Windows Credential Manager 重启持久化 + 内部安装包验收 |
| `AC-SD-13` | 快捷键 A 固定请求 SmartDictation；一次处理冻结并传递完整 ASR 原文、目标 EXE、可选应用名称和 1–3 档强度快照，默认 2 档。请求不得包含窗口标题、页面、屏幕、聊天历史、文档正文、源代码或剪贴板内容；未覆盖部署配置时使用关闭思考模式的 `deepseek-v4-flash` | 请求数据边界/三档配置迁移与快照/默认模型自动化 + 真实快捷键运行时 |

## 明确不规定的实现

- 不规定文本池和智能成稿器的具体文件名、类名、线程模型或进程形态；MVP 不要求存在独立 Router 组件，拟议边界不得被当成当前实现事实。
- 不要求把一次成稿拆成多次模型调用；逻辑上的纠错、去冗余、结构化和风格适配可以由一次受约束调用完成。
- MVP 只把目标 EXE 与可用时的应用名称作为应用上下文；不提供逐应用画像配置，也不把应用映射成固定写作模式。未来增加显式用户意图或更丰富上下文时需扩展输入契约。
- “使用目标场景”不授权把窗口标题、屏幕、文档正文、聊天历史、源代码或既有剪贴板内容作为 Prompt 上下文；若未来需要这些上下文，必须形成单独的知情授权和数据边界。AtomicPaste 为本地恢复而持有的不透明 OLE 快照不得进入 Prompt、History 或日志。
- 快捷键 A 的正常成功路径不增加原文确认步骤；冻结的 ASR 确认原文作为 Chatbot 失败后的自动兜底值存在。
- MVP 不要求用户为普通可编辑目标预先理解或配置 `clipboard_compatibility`。实现可以演进具体剪贴板或目标控件技术，但必须保持“完整定稿后一次性交付、不模拟 Enter、终端失败关闭”的可观察结果。
- MVP 不提供员工可见的 API Key 配置或 Prompt 编辑器；凭据预置、轮换和 Prompt 发布属于内部部署职责。
- 三档强度不各自维护 Prompt 文件；统一 Prompt 中的强度定义与 adapter 的 JSON schema、数据转义和通用安全 envelope 仍属于不同责任。
- DeepSeek 请求仍需按供应商协议设置有限的 `max_tokens`；token 上限与产品的 8000 Unicode 字符上限是不同单位。应用在 JSON 解析后按与 Delivery 相同的字符定义校验，不得通过截断来满足上限。

## 局部假设

- `ASM-SD-01`（Resolved for MVP）：目标 EXE 与应用名称只作为模型的场景提示，不能证明系统理解了用户意图。模型同时依据冻结原文判断合适表达；MVP 不采集窗口标题、页面、屏幕或输入表面内容。
- `ASM-SD-02`（Resolved for MVP）：快捷键 A 固定表达 SmartDictation；MVP 不设固定画像 Router 或逐应用覆盖。统一模型接收应用上下文和原文，三档全局强度限定允许的最大介入程度。
- `ASM-SD-03`（Resolved）：即使不能确定某个词是否为 ASR 错词，也允许 Chatbot 根据完整上下文自动推测并替换；原始 ASR 确认文本必须继续独立保留，不能被猜测结果覆盖。
- `ASM-SD-04`（Resolved）：Vibe coding 的结果必须是可直接交给 Delivery 的单一文本，不在本次链路中向用户发起澄清对话；模型仍不得补造用户未表达的需求。
- `ASM-SD-05`（Partially resolved）：Chatbot 处理硬截止已确认为 20 秒；处理失败统一使用 ASR 确认原文兜底，超时轨迹进入 IncidentVault。松开快捷键到请求开始前的本地处理预算，以及完整端到端延迟目标仍未确认。
- `ASM-SD-06`（Resolved）：办公和 vibe coding 成稿允许受控换行；普通可编辑文本目标使用完整定稿后的单次整体粘贴，不要求按 EXE 预先启用兼容模式，也不把 LF 转为 Enter。已知终端、shell 或命令执行表面保持失败关闭。除允许的 LF 外，NUL、双向覆盖/隔离符和其他控制字符仍不因此获得授权。
- `ASM-SD-07`（Resolved）：Processing 与 Delivery 统一使用 8000 Unicode 字符硬上限；模型结果超过上限视为处理失败并回退 ASR 原文，不截断模型结果。ASR 原文本身超过上限时仍由 Delivery 按既有失败/Pending 语义处理。
- `ASM-SD-08`（Resolved）：智能润色只使用一份独立、版本化、带哈希校验的统一语义 Prompt；三档强度作为请求输入。JSON Output、数据封装、输出 schema 和通用安全边界继续由 adapter 拥有。
- `ASM-SD-09`（Confirmed for internal MVP）：安装包只线下分发给公司内部员工，维护者负责同一 DeepSeek Key 的预置、监控、轮换和吊销，员工无需配置。该信任模型接受桌面端共享秘密可能被本机账户提取的剩余风险，不把流量监控表述为绝对防泄露保证；外部分发时必须重新评估。
- `ASM-SD-10`（Resolved for MVP）：TextProcessing 默认模型为 `deepseek-v4-flash` 并显式关闭思考模式。模型名由内部部署配置拥有，不向员工暴露；一次处理使用冻结的配置快照。

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
- [场景感知文本路由与智能成稿 Proposal](../architecture/proposals/context-aware-text-routing.md)

固定画像 Router 与四份隔离 Prompt 已由 ADR-0017 替代。MVP 当前采用单一应用感知 Prompt 和全局三档强度；未来多快捷键、多意图路由仍未成为 Accepted 决策。源码仍是当前实现事实，Proposal 只描述未落地的后续演进。

## 当前实现入口

- 触发语义：`src-tauri/src/voice_trigger.rs`、`src-tauri/src/shortcut_manager/`
- 会话完成与 ASR final：`src-tauri/src/voice_controller/workflow/`
- ASR provider 与 preview：`src-tauri/src/provider.rs`、`src-tauri/src/streaming_pipeline.rs`、`src-tauri/src/preview.rs`
- 冻结原文、应用上下文计划、Prompt Repository 与 DeepSeek adapter：`src-tauri/src/text_processing/`
- 统一智能润色 Prompt：`src-tauri/resources/prompts/smart_dictation/v2/`
- 文本交付与 Pending：`src-tauri/src/delivery.rs`、`src-tauri/src/inject.rs`、`src-tauri/src/pending_output_service.rs`
- History provenance 与热词学习栅栏：`src-tauri/src/history.rs`、`src-tauri/src/hotwords.rs`
- 全局润色强度与零 Key 员工设置：`src/features/settings/`、`src/app/AppShellV2.tsx`
- 异常捕获：`src-tauri/src/incident/`

当前 finalize 链路只在权威 ASR final 返回后创建不可变 `FrozenTranscript`；ProcessingPlan 冻结目标应用身份、配置 revision 和润色强度，Processor 成功返回已校验最终文本，任何非取消处理失败选择冻结原文。两条路径随后共用一次 ReadyToInject、AtomicPaste、Pending 和 History 提交链路；模型结果标记为不可参与热词学习。

## 验证状态

当前实现状态为 `in_progress`，验证状态为 `partial`。2026-08-28 的 dirty worktree 中，Rust 181 个测试、前端 48 个测试、production build、秘密扫描、架构文档检查和 ASR 边界检查通过；这些证据覆盖统一 Prompt、应用上下文请求边界、三档强度、配置迁移、JSON 响应校验、文本边界、AtomicPaste receipt、History provenance、热词栅栏和员工零 Key UI，但不能替代真实模型质量与目标环境验收。

仍未完成：内部管理员非交互凭据预置工具；真实 DeepSeek 调用；Windows/WebView2 快捷键；聊天、Office、编码助手和终端输入框互操作；Windows Credential Manager 重启持久化；完整 finalize 竞争和 IncidentVault 拥塞故障注入。缺少这些证据时不得升级为 `validated` 或发布完成。

## 澄清历史

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
