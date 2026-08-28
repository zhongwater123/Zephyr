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
  "implementationStatus": "not_started",
  "implementationReview": {
    "status": "unreviewed",
    "sourceRevision": "b62667deab18f740c83bab2f1bcebae2fd0a59e2",
    "worktreeState": "dirty",
    "changedPaths": ["src-tauri/src/voice_controller/", "src-tauri/src/voice_trigger.rs", "src-tauri/src/streaming_pipeline.rs"],
    "reviewedAt": "2026-08-28",
    "summary": "当前实现把 ASR final transcript 直接交给 Delivery；尚无独立 Router、完整会话文本池或智能成稿处理层。",
    "knownDeviations": []
  },
  "validationStatus": "unverified",
  "components": ["system.zephyr", "frontend.features", "backend.services", "backend.voice-controller", "backend.streaming", "backend.delivery", "backend.shortcut", "backend.incident-vault"],
  "decisions": ["ADR-0002", "ADR-0003", "ADR-0004", "ADR-0008", "ADR-0012", "ADR-0013", "ADR-0014"],
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
    { "id": "AC-SD-10", "components": ["backend.voice-controller", "backend.delivery"], "requiredEvidence": ["automated", "fault_injection", "external_app_interop"] }
  ],
  "evidence": [],
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

智能成稿和兜底选择必须在目标输入框外全部完成。普通可编辑文本目标接收的是一份完整、已校验的最终纯文本，系统应像用户日常粘贴大段内容一样一次性整体写入并保留段落，而不是边生成边输入、逐段注入或为换行模拟 Enter。已知终端、shell 等粘贴换行可能执行命令的目标不属于普通文本框，必须失败关闭或转为用户主动交付。

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
| `AC-SD-09` | Chatbot 超时必须向 IncidentVault 非阻塞地提交带会话关联、处理阶段、耗时、超时预算、画像和稳定原因码的异常轨迹；文本等用户内容仍服从 IncidentVault 已有授权快照，Vault 拥塞或故障不得阻断原文兜底 | 自动化 + IncidentVault 拥塞/故障注入 |
| `AC-SD-10` | 智能成稿或 ASR 兜底在进入 Delivery 前已经完整确定；对普通可编辑文本目标，单行和多行结果都作为一个纯文本载荷一次性粘贴，不要求用户预先按 EXE 启用 `clipboard_compatibility`，不为 LF 生成 Enter；目标复验或 Ctrl+V 提交前失败进入 Pending，提交后剪贴板并发变化只跳过恢复并记录异常，不能触发可能重复的再次交付；已知终端或命令执行表面不自动接收含 LF 的生成文本 | 自动化 + 剪贴板竞争故障注入 + 聊天/办公/编码输入框互操作 |

## 明确不规定的实现

- 不规定 Router、文本池和智能成稿器的具体文件名、类名、线程模型或进程形态；拟议边界不得被当成当前实现事实。
- 不要求把一次成稿拆成多次模型调用；逻辑上的纠错、去冗余、结构化和风格适配可以由一次受约束调用完成。
- 不规定目标场景必须通过 EXE 名称、窗口标题、辅助功能树、用户配置或模型分类中的哪一种方式判断。
- “使用目标场景”不授权把屏幕、文档正文、聊天历史、源代码或既有剪贴板内容作为 Router/Prompt 上下文；若未来需要这些上下文，必须形成单独的知情授权和数据边界。AtomicPaste 为本地恢复而持有的不透明 OLE 快照不得进入 Router、Prompt、History 或日志。
- 快捷键 A 的正常成功路径不增加原文确认步骤；冻结的 ASR 确认原文作为 Chatbot 失败后的自动兜底值存在。
- MVP 不要求用户为普通可编辑目标预先理解或配置 `clipboard_compatibility`。实现可以演进具体剪贴板或目标控件技术，但必须保持“完整定稿后一次性交付、不模拟 Enter、终端失败关闭”的可观察结果。
- DeepSeek 请求仍需按供应商协议设置有限的 `max_tokens`；token 上限与产品的 8000 Unicode 字符上限是不同单位。应用在 JSON 解析后按与 Delivery 相同的字符定义校验，不得通过截断来满足上限。

## 局部假设

- `ASM-SD-01`（Challenged）：目标应用类别只能提供默认写作画像，不能单独代表用户意图。聊天工具可能承载正式沟通，办公软件可能承载私人草稿，IDE 也不能区分代码编辑区与编码助手输入框。重新评估条件是确认首版场景识别仅依赖应用身份，或确定可获得更精确且隐私可接受的输入表面信号。
- `ASM-SD-02`（Open）：当显式快捷键意图、用户配置、目标场景和文本推断冲突时，显式用户选择应优先，未知场景采用保守的通用成稿画像。
- `ASM-SD-03`（Resolved）：即使不能确定某个词是否为 ASR 错词，也允许 Chatbot 根据完整上下文自动推测并替换；原始 ASR 确认文本必须继续独立保留，不能被猜测结果覆盖。
- `ASM-SD-04`（Resolved）：Vibe coding 的结果必须是可直接交给 Delivery 的单一文本，不在本次链路中向用户发起澄清对话；模型仍不得补造用户未表达的需求。
- `ASM-SD-05`（Partially resolved）：Chatbot 处理硬截止已确认为 20 秒；处理失败统一使用 ASR 确认原文兜底，超时轨迹进入 IncidentVault。松开快捷键到请求开始前的本地处理预算，以及完整端到端延迟目标仍未确认。
- `ASM-SD-06`（Resolved）：办公和 vibe coding 成稿允许受控换行；普通可编辑文本目标使用完整定稿后的单次整体粘贴，不要求按 EXE 预先启用兼容模式，也不把 LF 转为 Enter。已知终端、shell 或命令执行表面保持失败关闭。除允许的 LF 外，NUL、双向覆盖/隔离符和其他控制字符仍不因此获得授权。
- `ASM-SD-07`（Resolved）：Processing 与 Delivery 统一使用 8000 Unicode 字符硬上限；模型结果超过上限视为处理失败并回退 ASR 原文，不截断模型结果。ASR 原文本身超过上限时仍由 Delivery 按既有失败/Pending 语义处理。

## 架构决策

- [ADR-0002：单所有者、有界、失败关闭的语音会话](../architecture/adr/0002-single-owner-bounded-voice-session.md)
- [ADR-0003：成功注入为提交点，失败进入内存 Pending](../architecture/adr/0003-delivery-commit-point-and-pending-output.md)
- [ADR-0004：自定义 origin 授权必须先于凭据读取](../architecture/adr/0004-trust-before-credentials.md)
- [ADR-0008：产品前端融合、后端隔离的本地异常恢复](../architecture/adr/0008-incident-vault-isolated-recovery.md)
- [ADR-0012：统一语音输入控制面所有权](../architecture/adr/0012-unified-voice-input-control-plane.md)
- [ADR-0013：严格 mailbox-owned 语音运行时](../architecture/adr/0013-strict-mailbox-owned-voice-runtime.md)
- [ADR-0014：智能成稿使用一次性整体粘贴交付（Proposed）](../architecture/adr/0014-atomic-smart-dictation-paste.md)
- [场景感知文本路由与智能成稿 Proposal](../architecture/proposals/context-aware-text-routing.md)

现有 ADR 尚未决定智能成稿 Router、处理画像或新增 Chatbot endpoint 的长期边界。方案确认后应新增 Proposed ADR，不得把本 Dossier 中的候选实现直接升级为 Accepted 决策。

## 当前实现入口

- 触发语义：`src-tauri/src/voice_trigger.rs`、`src-tauri/src/shortcut_manager/`
- 会话完成与 ASR final：`src-tauri/src/voice_controller/workflow/`
- ASR provider 与 preview：`src-tauri/src/provider.rs`、`src-tauri/src/streaming_pipeline.rs`、`src-tauri/src/preview.rs`
- 文本交付：`src-tauri/src/delivery.rs`
- 异常捕获：`src-tauri/src/incident/`

源码复核显示当前没有独立 Router、完整会话文本池或智能成稿处理层；现有 final transcript 仍直接进入 Delivery。上述 Proposal 只描述候选演进方向。

## 验证状态

当前为 `unverified`，实现状态为 `not_started`。本 Dossier 记录的是新增产品目标，不得引用现有语音控制面、ASR 或 Delivery 测试来宣称智能成稿已经实现或验证。

## 澄清历史

- 2026-08-28：用户明确快捷键 A 应允许自然、发散地口述，并根据聊天、办公写作和 vibe coding 等使用场景生成可直接输入的文本。
- 2026-08-28：文档评审挑战“仅凭应用类别即可确定用户意图”的假设，将其降为默认画像信号，并保留为待继续确认的产品问题。
- 2026-08-28：用户确认 ASR 确认原文是 Chatbot 失败后的自动兜底；模型可以猜测疑似错词；所有成功响应直接进入 Delivery；处理超时轨迹进入 IncidentVault。
- 2026-08-28：用户确认 Chatbot 超过 20 秒即超时，并指定 DeepSeek JSON Output 文档作为结构化响应依据；空 content、非法或不完整结果按处理失败进入原文兜底。
- 2026-08-28：用户确认首版 Processing 不限制 Chatbot 输出文本长度，并选择后续调整 Delivery 及注入边界以支持办公和 vibe coding 的受控多段文本；Delivery 现有 8000 字符上限是否移除仍作为独立安全问题待确认。
- 2026-08-28：用户进一步确认 Processing 与 Delivery 统一采用当前 8000 Unicode 字符上限，替代上一轮“Processing 不限长”的候选表述；超过上限使用原文兜底，不做静默截断。
- 2026-08-28：用户确认所有智能成稿处理应在写入目标输入框前完成，最终结果像日常粘贴大段格式化内容一样一次性整体进入输入框；按应用预先启用剪贴板兼容不再作为普通文本框的 MVP 前置条件。参考 OpenWhispr 后保留目标复验、剪贴板并发保护和终端换行执行防护。
