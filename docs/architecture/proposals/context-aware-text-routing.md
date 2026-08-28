---
{
  "documentType": "architecture-proposal",
  "viewStatus": "proposed",
  "owner": "product-maintainers",
  "createdAt": "2026-08-28",
  "revisitWhen": "用户确认场景判定信号和端到端延迟目标，并准备进入实现设计时",
  "relatedFeatures": ["FEAT-SMART-DICTATION"]
}
---

# 场景感知文本路由与智能成稿

> 本文是 Proposed architecture，不描述当前实现。当前链路仍是 ASR final transcript 直接进入 Delivery。

## 1. 设计目标

快捷键 A 表达的是稳定的用户能力“自然口述并智能成稿”，不是固定的一段 Prompt。系统在完整会话文本冻结后，根据显式触发意图、目标输入场景、用户偏好和完整语义选择写作画像，再调用受约束的文本处理能力生成最终可交付文本。

候选链路：

```text
Activation + captured target context
                ↓
ASR confirmed segments → FrozenTranscript
                ↓
Router → ProcessingPlan + RouteReason
                ↓
TextProcessor ── success ──→ ProcessedText
       │                         │
       └── failure ──→ FrozenTranscript fallback
                                 ↓
                         Target Delivery
                ↓
DeliveryReceipt → history commit → learning event
```

## 2. 需要分开的三个问题

### 2.1 用户要做什么

由显式触发意图表达，例如普通原文输入、智能成稿、翻译或 Agent 指令。触发来源和处理意图是两个维度：快捷键、界面按钮和硬件入口可以请求同一种意图，同一个入口也可以通过不同绑定请求不同意图。

### 2.2 这段文字应怎样写

由写作画像表达，例如 `general_conversation`、`office_formal`、`coding_request`。画像约束语气、结构、压缩程度和允许的改写强度，但不拥有会话控制权，也不决定交付目标。

### 2.3 结果送到哪里

快捷键 A 的 Chatbot 成功结果和失败时的 ASR 原文兜底都直接进入既有 Target Delivery；该链路不增加澄清对话或人工确认。未来其他触发意图可以选择复制或打开助手等输出端，但不改变快捷键 A 的当前产品契约。

## 3. 两阶段路由

### 3.1 Activation 阶段

录音开始时固定不可变的会话事实：显式处理意图、触发绑定身份、配置 revision、目标窗口身份，以及当时可合法获得的低敏感上下文。该阶段可以决定是否需要捕获某类上下文，但不能在 ASR 完成前启动完整文本成稿。

### 3.2 Finalization 阶段

用户结束输入且最后一个 ASR final 返回后，将有序确认片段冻结为 `FrozenTranscript`。Router 结合冻结原文和会话开始时固定的事实生成 `ProcessingPlan`；处理器不得读取后来切换到的前台窗口来悄然改变本次写作画像或交付目标。

候选决策优先级：

1. 用户本次显式选择的处理模式；
2. 触发绑定携带的稳定意图；
3. 用户为目标应用或输入表面设置的画像；
4. 隐私边界内可获得的目标场景信号；
5. 完整文本的保守意图推断；
6. 通用智能成稿默认值。

该优先级仍是候选设计，需结合 Feature Dossier 中的 Open Assumption 继续确认。

## 4. ProcessingPlan 而不是动态 Prompt

Router 应输出类型化计划，而不是直接拼接可执行 Prompt：

```text
ProcessingPlan
├── intent: SmartDictation
├── profile: office_formal
├── operations: [correct_asr, remove_fillers, restructure]
├── meaningPolicy: preserve_facts_and_stance
├── outputPolicy: paste_to_captured_target
├── processingDeadline: 20s
├── fallbackPolicy: deliver_frozen_transcript
└── reason: explicit shortcut + target profile
```

处理层再把稳定的画像 ID 解析为具体模型、模板、参数和版本。这样快捷键配置不需要知道模型名称、temperature 或供应商协议；替换 Chatbot adapter 也不改变触发和 Delivery 契约。

首版优先使用有限、可审计的画像和操作集合，不建立任意节点、任意 Prompt 的通用工作流引擎。多个逻辑操作可以由一次模型调用完成。

### 4.1 独立画像 Prompt 文件

四种画像分别使用 `general.md`、`chat.md`、`office.md` 和 `coding_request.md`。每个文件独立拥有自己的角色、改写目标、保留项、禁止项和画像示例，不 include、继承或拼接其他画像。manifest 把有限 `profile_id` 映射到文件、版本和内容哈希；缺失、未知或哈希不符时失败关闭，不降级加载另一个画像。

JSON Output 指令、用户文本的 JSON 数据封装、输出 schema、8000 字符上限和通用安全规则由无风格 `PromptEnvelope` 统一提供。这样公共协议只维护一份，而修改办公画像不会改变聊天或编码画像的有效语义。MVP 不提供用户 Prompt 编辑器或远程 Prompt 下发，画像随内部构建独立版本化发布。

### 4.2 DeepSeek JSON Output adapter 契约

智能成稿的 DeepSeek adapter 采用官方 [JSON Output](https://api-docs.deepseek.com/zh-cn/guides/json_mode/) 契约：

```json
{
  "response_format": { "type": "json_object" },
  "stream": false
}
```

system 或 user prompt 必须包含字面量 `json`，并提供应用拥有的响应示例：

```json
{
  "text": "最终可直接交付的文本"
}
```

模型只负责返回 `text`。画像、路由原因、fallback、目标和 Delivery 策略由本地类型化计划持有，不允许模型通过额外 JSON 字段改变。JSON mode 保证 JSON 语法，不等同于应用 schema 合法；adapter 仍必须依次检查：

1. 请求在 20 秒硬截止内返回并完成解析；
2. `finish_reason` 表示正常完成，而不是 `length`、过滤或资源不足；
3. `content` 存在且不是空白；
4. 根值为 JSON object，且存在唯一需要的字符串字段 `text`；
5. `text` 不是空白；规范化受控换行后按与 Delivery 相同的 Unicode 字符定义计数，字符数不超过 8000；
6. 校验通过的完整 `text` 作为 `ProcessedText` 交给 Delivery，不做静默截断。

任一检查失败都产生类型化 `ProcessingFailure` 并选择 `FrozenTranscript` 兜底。官方文档明确提示 JSON Output 可能返回空 `content`，因此 HTTP 200 不能单独成为成功条件。`max_tokens` 必须显式设置，避免 `finish_reason=length` 导致截断；具体值属于处理画像/adapter 配置，应在供应商允许范围内设置为足以覆盖本次成稿的值。`max_tokens` 与产品字符数不是同一单位；应用在响应解析后独立执行 8000 Unicode 字符校验。

### 4.3 内部分发与共享 DeepSeek Credential

HotwordAgent 和 TextProcessing 共同引用一份由内部部署预置的 `DeepSeekSharedCredential`，员工设置页不出现 API Key。秘密只保存于 Windows Credential Manager，不进入源码、前端 bundle、普通配置、Prompt 或日志；缺失时 TextProcessing 类型化失败并立即选择 ASR 原文。

共享 Key 不合并数据用途。`HotwordAgent` 与 `TextProcessing` 仍使用独立 purpose 执行 trust/policy 检查和审计，只有各自用途通过后才能读取同一 credential reference。线下内部分发和流量监控是 MVP 接受的信任模型，但桌面共享秘密仍可能被本机账户提取；详细候选边界见 [ADR-0015](../adr/0015-internal-shared-deepseek-credential-and-isolated-prompts.md)。

Chat Completions 当前文档显示思考模式可能默认开启。对 20 秒内完成的快速成稿，Proposal 建议显式关闭思考模式；该参数和模型 ID 属于 DeepSeek adapter 配置，不进入 Router 或 Delivery。实施时必须以届时的官方 [Chat Completions API](https://api-docs.deepseek.com/zh-cn/api/create-chat-completion/) 重新确认字段和可用模型。

## 5. 语义保护边界

智能成稿模型只编辑用户话语，不回答或执行话语中的任务。输入应作为待编辑数据处理，即使其中包含“忽略以上规则”“删除文件”或代码指令，也不能改变本次处理器自身的系统约束。

每次处理至少保留以下不同来源：

- `FrozenTranscript`：ASR 确认原文，只读；
- `ProcessedText`：带画像、处理器版本和处理结果来源；
- `DeliveredText`：实际进入目标或 Pending 的文本；
- `DeliveryReceipt`：不可逆注入提交点的结果。

即使无法确定某个词是否为 ASR 错词，模型也被允许根据完整上下文推测并替换；该推测只存在于 `ProcessedText`，不得覆盖 `FrozenTranscript`。这种授权不等于允许改变整体意图、事实关系、否定、承诺或权限，也不允许 Vibe coding 画像把合理猜测伪装成用户要求。

## 6. 场景信号不能等同于意图

只按 EXE 分类会产生系统性误路由：聊天应用有正式工作沟通，Word 有私人笔记，IDE 中既有代码编辑器也有 AI 对话框。因此建议把应用分类降为默认画像信号，并允许用户覆盖；未知或冲突时使用保守通用画像。

未来若要识别更细的输入表面，应单独评审 Accessibility 信息、窗口标题、选区、屏幕或文档内容的数据采集边界。当前 Feature Dossier 不授权读取这些内容。

## 7. Delivery 与提交副作用

目标 Delivery 只消费最终文本和捕获目标，执行校验与注入并返回 receipt。历史保存和热词学习应在 receipt 之后分别提交，并记录使用的是 ASR 原文还是智能成稿结果。热词系统不得把润色生成的新词直接当成可靠 ASR 学习样本。

快捷键 A 的 Chatbot 成功响应必须是单一、非空并可直接交付的最终文本；成功结果直接进入 Target Delivery。Chatbot 超时、网络/服务错误、空响应或无效响应时，处理层选择原始 `FrozenTranscript` 进入同一 Delivery。用户主动取消不属于处理失败，不能触发原文兜底；Delivery 自身失败继续遵守目标复验和 Pending 契约。

办公和 vibe coding 画像允许在 JSON `text` 中返回多段文本。Delivery 契约应把 CRLF、裸 CR 和 LF 规范化为内部 LF，允许 LF 作为唯一受控换行表示，同时继续拒绝 NUL、双向覆盖/隔离符和其他控制字符。SmartDictation 必须在目标输入框外完整定稿，再把单行或多行结果作为一个纯文本载荷一次性整体粘贴到普通可编辑目标；不要求用户按 EXE 预先启用剪贴板兼容，也不为 LF 生成 Enter。自动粘贴需要串行化完整剪贴板快照、目标复验、单次 Ctrl+V 和并发安全恢复；已知终端或命令执行表面不自动接收含 LF 的生成文本。详细候选边界见 [ADR-0014](../adr/0014-atomic-smart-dictation-paste.md)。

Processing 与 Delivery 统一使用当前 `delivery.max-output-characters` 的 8000 Unicode 字符上限。Prompt 应要求最终 `text` 不超过该上限；本地校验仍是权威。模型返回超过 8000 字符的 `text` 时视为处理失败并选择完整 `FrozenTranscript` 兜底，不截取前 8000 字符。若 `FrozenTranscript` 本身也超过上限，则进入 Delivery 的既有验证失败/Pending 语义，而不是再次循环调用 Chatbot。

未来 Agent 或只复制模式可以选择其他 output sink；这不应迫使 Target Delivery 承担路由职责。

## 8. Chatbot 超时与 IncidentVault

Chatbot 从 adapter 接收请求开始，到完整响应完成 JSON 解析和应用层校验为止，共享一个 20 秒硬截止。超时需要作为智能成稿阶段的类型化故障轨迹非阻塞提交给 IncidentVault。候选轨迹至少包含会话/attempt 关联、处理阶段、写作画像、开始与结束单调时间、实际耗时、20 秒超时预算、fallback 已选择和稳定原因码；不把完整 Prompt、凭据或服务响应正文写入普通事件字段。

ASR 原文、模型响应或其他用户内容是否作为恢复材料保存，继续服从一次会话开始时固定的 IncidentVault 内容与文本授权快照。IncidentVault 队列满、写线程故障或内容未授权都不得阻止 ASR 原文进入 Delivery 兜底。

## 9. 尚待产品确认

1. 场景冲突时，用户显式画像、应用默认画像与文本推断的最终优先级。
2. 松开快捷键到 Chatbot 请求开始前的本地预算，以及完整端到端延迟目标。
3. 除超时和明确的 JSON/响应错误外，是否需要语义偏移检测来触发原文兜底。

## 10. 接受条件

整体粘贴已形成 Proposed ADR-0014，共享凭据与 Prompt 隔离已形成 Proposed ADR-0015；其余产品问题确认后，再补充 Router、Processing 和场景优先级的长期决策。只有代码落地并完成源码符合性复核后，Router、Processing 和新的 Delivery 边界才能进入 Current C4、Runtime View 和代码地图。
