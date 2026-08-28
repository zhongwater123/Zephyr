# 场景感知智能成稿 MVP 实施计划

> 状态：Proposed implementation plan。本文不是 Current 架构事实，不表示功能已经实现或验证。
>
> 产品契约：[FEAT-SMART-DICTATION](../../features/smart-dictation.md)；候选架构：[场景感知文本路由与智能成稿](../../architecture/proposals/context-aware-text-routing.md)。

## 1. MVP 目标

用户按住现有快捷键 A 自然口述，松开后等待本次 ASR 权威 final；系统以不可变 `FrozenTranscript` 保存 ASR 原文，根据捕获目标的本地应用画像选择聊天、办公、编码请求或通用写作策略，在 20 秒内调用 DeepSeek 完成一次智能成稿，并将合法结果交给既有 Delivery。Chatbot 失败时使用 ASR 原文兜底；用户主动取消时不交付任何文本。

首版完成后，用户应能获得：

- 聊天输入：去语气词、重复和语病，保留口吻与立场；
- 办公输入：更正式、分段、清楚且不新增事实或承诺；
- 编码助手输入：把发散口述整理为目标、背景、约束和待办；
- 未识别场景：使用保守的通用智能成稿画像；
- 模型不可用：20 秒内失败关闭并自动交付 ASR 原文；
- 单行和多行输出：完整定稿后作为一个纯文本载荷一次性整体粘贴到普通输入框，不模拟 Enter。

## 2. MVP 明确范围

### 2.1 包含

- 现有快捷键 A 固定请求 `SmartDictation`；保留统一 begin/finish/cancel 控制面。
- 一个纯本地 Router，输出有限、类型化 `ProcessingPlan`。
- 四个内置写作画像：`general`、`chat`、`office`、`coding_request`。
- 按目标 EXE 的内置分类、用户逐应用覆盖和 unknown → `general`。
- 独立 DeepSeek text-processing adapter、endpoint purpose、凭据和设置。
- DeepSeek Chat Completions、非流式、非思考、JSON Output，响应形状固定为 `{ "text": "..." }`。
- 20 秒硬截止、8000 Unicode 字符统一上限、原文 fallback。
- SmartDictation 专用 AtomicPaste、受控换行、剪贴板并发保护、终端防护和真实目标应用互操作验证。
- History provenance 的最小迁移，以及阻断润色文本进入热词自动学习。
- IncidentVault 的 processing 阶段、稳定原因码和非阻塞超时轨迹。

### 2.2 不包含

- 第二快捷键、翻译、Agent、任意 Prompt 工作流或多模型编排。
- 根据屏幕正文、聊天历史、源代码、选区或 Accessibility tree 自动推断场景。
- LLM 流式输出、部分结果交付、运行时自动重试或澄清对话。
- 用户自定义系统 Prompt；首版 Prompt 和 schema 由应用版本控制。
- 完整热词系统重构；MVP 只建立 provenance 和学习隔离栅栏。
- UI Automation 文本插入、未知控件中模拟 Enter、自动发送消息。
- 云同步、远程日志或把完整 Prompt/响应写入普通日志。

## 3. MVP 决策门

`DG-SD-01` 已由用户本轮要求关闭；实施前仍需确认其余默认方案：

| ID | 状态 | 默认方案 / 影响 |
| --- | --- | --- |
| `DG-SD-01` | 已关闭 | 普通可编辑目标无需按 EXE 预配置；完整结果使用单次 AtomicPaste，不为 LF 生成 Enter；已知终端的含 LF 结果失败关闭 |
| `DG-SD-02` | 待确认 | 场景判断只使用目标 EXE + 用户覆盖；浏览器等混合应用默认 `general`。若要求识别具体控件，需要新增 Accessibility 与隐私边界 |
| `DG-SD-03` | 待确认 | `text_processing` 使用独立 endpoint purpose 和 Credential Manager 槽；不复用 `hotword_agent` 的授权或密钥路径 |
| `DG-SD-04` | 待确认 | 默认模型使用可配置的 `deepseek-v4-flash` 并显式关闭思考模式；若固定 Pro，需要重新评估 20 秒延迟与成本 |

## 4. 目标运行链路

```text
Shortcut A / SmartDictation
        ↓
VoiceSessionActor 接受 Activation，固定 config revision 与目标身份
        ↓
录音 + 流式 preview（仅展示）
        ↓
用户松开 → 等待 ASR 权威 final
        ↓
FrozenTranscript（会话内只读）
        ↓
Router(target EXE, user overrides) → ProcessingPlan
        ↓
TextProcessor / DeepSeek JSON Output（20 秒）
   ┌────┴──────────────────────────┐
 success                        failure
 ProcessedText                  FrozenTranscript fallback
   └──────────────┬───────────────┘
                  ↓
Actor ReadyToInject 授权 + 再次取消检查
                  ↓
Delivery validate → inject / Pending
                  ↓
DeliveryReceipt → History commit + learning eligibility
```

当前 Provider 只返回一个权威 final `String`。MVP 不建立假装存在多个 final 的通用队列；先建立不可变 `FrozenTranscript` 类型。未来 Provider 真正产生多个确认片段时，再在 ASR 边界增加有序 accumulator，而不改变 Router/Processor 契约。

## 5. 领域与接口契约

### 5.1 Activation

在 `voice_trigger.rs` 中把来源与意图分开：

- `TriggerSource`：快捷键、界面或外部适配器；
- `TriggerBehavior`：PushToTalk；
- `ActivationIntent`：MVP 只生产使用 `SmartDictation`，为后续 `RawDictation`、Translate、Agent 保留类型空间。

快捷键管理器仍只负责把物理 Pressed/Released 配对成同一 `ActivationId`，不读取模型配置，不拼 Prompt。

### 5.2 Router

Router 是同步、纯函数组件：

```text
RouteRequest
├── activation_intent
├── target_executable
├── config_revision
└── writing_profile_overrides snapshot

ProcessingPlan
├── profile_id
├── processor_profile_id
├── deadline = 20s
├── max_characters = 8000
├── output = captured_target
└── fallback = frozen_transcript
```

Router 不访问网络、凭据、History、IncidentVault 或 Delivery，也不直接生成 Prompt。

### 5.3 Processing

新增可替换端口 `TextProcessor`。DeepSeek adapter 负责：

- 在请求前按 `scheme + host + effective port + text_processing purpose` 重新检查授权，再读取独立凭据；撤销授权立即阻止请求；
- 使用会话开始时固定的 endpoint、model 和画像配置，但不绕过实时 trust 撤销；
- `response_format={"type":"json_object"}`、`stream=false`、非思考模式；
- system prompt 含字面量 `json` 和 `{ "text": "..." }` 示例；用户转写通过 JSON 序列化作为数据嵌入，不用字符串拼接；
- 单次请求，不做自动重试；adapter 请求、响应读取、JSON 解析和 schema 校验共享 20 秒 deadline；
- 验证 `finish_reason=stop`、content 非空、object schema、`text` 非空、换行规范化后不超过 8000 Unicode 字符；
- 任何失败都返回类型化 `ProcessingFailure`，绝不返回部分文本或截断文本。

`max_tokens` 是 adapter 参数，不等于 8000 字符；实现时按所选 DeepSeek 模型能力设置足够值，并以 120 秒中英文最长语料验证不会产生 `finish_reason=length`。

### 5.4 Delivery 一次性整体粘贴

- 把 CRLF、裸 CR 和 LF 规范化为内部 LF；LF 计入 8000 字符。
- Delivery 允许 LF，继续拒绝 NUL、双向覆盖/隔离符和其他控制字符。
- SmartDictation 的单行和多行最终文本都使用 `AtomicPaste`；普通可编辑目标不要求按 EXE 预先启用 `clipboard_compatibility`。
- Processing 必须先完整结束，Delivery 只接收一个不可变最终纯文本载荷，不接收 token、部分 JSON 或分段调用。
- AtomicPaste 串行执行完整 OLE 快照、目标复验/恢复、一次 Ctrl+V 和并发安全恢复，绝不为 LF 发送 VK_RETURN/Enter。
- `PasteReceipt` 分开表示 Ctrl+V 是否已提交和剪贴板是否已恢复；提交后发现 sequence 改变只跳过恢复并记录异常，不再进入可能导致重复粘贴的 Pending。
- 写剪贴板、目标复验或 Ctrl+V 提交前失败时进入 Pending；已知终端或命令执行表面的含 LF 结果只允许 Pending/主动复制。
- 不因 Chatbot 失败绕过目标复验、Actor ReadyToInject 或 Pending 语义。

### 5.5 History 与热词隔离

History schema 做最小向前迁移，现有记录默认保持兼容：

- `text_origin`: `asr_direct | processed | asr_fallback`；
- `processor_profile`: nullable；
- `processor_version`: nullable；
- `learning_eligible`: boolean，旧记录默认 true。

MVP 默认不把第二份 ASR 原文持久化到正式历史：

- `processed` 行保存实际交付文本，`learning_eligible=false`；
- `asr_direct/asr_fallback` 行保存 ASR 文本，`learning_eligible=true`；
- 热词 pending count、批次读取和手工整理都只读取 `learning_eligible=true` 的行；
- 历史编辑不自动把 processed 行升级为可学习样本。

这样先阻断语义污染和数据重复驻留，完整的热词 provenance、批次游标和重新学习语义继续留在既有重构待办。

### 5.6 IncidentVault

新增 `Processing` stage 及固定原因码：

- `processing_timeout`；
- `processing_unauthorized` / `processing_missing_key`；
- `processing_http_failed`；
- `processing_empty_content`；
- `processing_invalid_json` / `processing_invalid_schema`；
- `processing_finish_length`；
- `processing_output_too_long`。

记录 session/attempt、画像、模型、开始与结束单调时间、实际耗时、20 秒预算和 fallback 选择。普通事件不记录 Prompt、原文或模型正文；内容恢复继续服从 IncidentVault 会话开始时固定的文本授权。Vault 队列满或写入失败不能阻塞原文 fallback。

## 6. 分阶段实施任务

### Task 0：基线与决策记录

**文档：** Dossier、Proposal、新 Proposed ADR、ADR 索引。

- [x] `DG-SD-01` 已关闭并形成 Proposed ADR-0014；实现前评审是否接受该特性级 ADR-0006 例外。
- [ ] 确认 `DG-SD-02` 至 `DG-SD-04`。
- [ ] 为独立 text-processing purpose、20 秒 deadline、原文 fallback 和 8000 字符形成 Proposed ADR；不要与 AtomicPaste 的数据完整性决策混写。
- [ ] 复核 Voice Control Dossier 中 Starting 快速松开的当前偏差；它可以与智能成稿开发并行，但必须在目标环境 MVP 验收前关闭或明确阻断完成声明。
- [ ] 保存基线测试和 dirty worktree 变更清单，避免覆盖现有用户修改。

**退出条件：** 产品决策门关闭；Current C4 尚不加入未实现组件。

### Task 1：领域类型与纯 Router

**主要文件：** `voice_trigger.rs`、新增 `text_processing/model.rs`、`router.rs`、`profiles.rs`。

- [ ] 先添加 Activation intent、FrozenTranscript、ProcessingPlan、ProcessedText、ProcessingFailure 和 DeliveryTextOrigin 单元测试。
- [ ] 实现 EXE → profile 的确定性映射、大小写归一、用户覆盖优先和 unknown fallback。
- [ ] 增加架构边界测试：Router 不依赖网络、存储、IncidentVault 或 Delivery adapter。

**退出条件：** 纯函数测试覆盖四种画像、覆盖优先级和未来未知枚举失败关闭。

### Task 2：配置、凭据与授权

**主要文件：** `config.rs`、`repositories.rs`、`services.rs`、config commands、`src/domain.ts`、设置 controller/UI。

- [ ] 配置 schema 升级并迁移 base URL、model、profile overrides；快捷键 A 不增加绕过 Chatbot 的产品开关。
- [ ] 未配置、凭据不可用或配置不受信任时立即产生类型化 `ProcessingFailure`，并按失败契约回退 `FrozenTranscript`。
- [ ] 新增 `EndpointPurpose::TextProcessing`、独立 CredentialStore 方法和 Windows Credential Manager key。
- [ ] 新增保存、测试连接、授权、撤销及 revision CAS/凭据回滚测试。
- [ ] 证明未授权 endpoint 在读取密钥前失败；HotwordAgent 授权不能替代 TextProcessing 授权。

**退出条件：** 设置往返、旧配置迁移、并发 revision、授权撤销和 trust-before-credential 全部通过。

### Task 3：DeepSeek adapter 与 Prompt profiles

**主要文件：** 新增 `text_processing/deepseek.rs`、`prompt.rs`、测试 fake server/adapter。

- [ ] 建立应用拥有的 JSON schema 和四个版本化 Prompt profile。
- [ ] 用结构化 JSON 封装 transcript，验证口述中的伪 system 指令不能改变输出协议。
- [ ] 覆盖正常 JSON、空 content、非法 JSON、缺字段、空 text、额外控制字段、`finish_reason=length`、7999/8000/8001 字符、HTTP/连接错误。
- [ ] 使用可暂停时间测试 20 秒边界；禁止后台迟到响应进入 Delivery。
- [ ] 取消 token 优先于 fallback：用户取消时 abort 请求且不交付原文。

**退出条件：** adapter 只返回合法 `ProcessedText` 或类型化 failure，没有第三种半成功状态。

### Task 4：Workflow 集成与 Actor 仲裁

**主要文件：** `voice_controller/workflow/finalize.rs`、contract/resources、Actor finalization effects/reducer、Presenter/overlay DTO。

- [ ] 在 ASR final 非空且 Incident final_transcript 已记录后创建 FrozenTranscript。
- [ ] Router 生成 plan；Presenter 显示“正在智能成稿”，不把模型中间内容当 preview。
- [ ] 执行 processor；成功选择 ProcessedText，失败选择 FrozenTranscript，取消直接结束。
- [ ] 处理后仍必须请求 Actor `ReadyToInject`，验证 SessionId、cancel、disabled 和过期结果。
- [ ] 增加 finish/cancel/timeout/late-result/disable 的竞争测试。

**退出条件：** ASR → Processing → Delivery 只有一条权威文本选择路径；迟到模型结果无副作用。

### Task 5：Delivery AtomicPaste 与受控换行

**主要文件：** `target.rs`、`delivery.rs`、`inject.rs`、Pending reason DTO 和配置测试。

- [ ] 添加 CRLF/CR/LF 规范化和字符安全测试。
- [ ] 保持 8000 字符单一事实源，不复制魔法数。
- [ ] 给 SmartDictation 增加独立 AtomicPaste delivery intent；既有非智能输入仍遵守 ADR-0006，避免无意扩大全局行为。
- [ ] 把自动剪贴板操作放入一个有界串行通道；后续粘贴等待前序恢复完成或明确放弃恢复。
- [ ] 保持完整 OLE 快照，写入完整最终纯文本，只发送一次 Ctrl+V；不得按行循环或发送 Enter。
- [ ] 在粘贴前复验并恢复录音开始时捕获的目标；失败发生在提交前才进入 Pending。
- [ ] 将 `paste_submitted` 和 `clipboard_restoration` 分成两个 receipt 状态；用户并发修改剪贴板时跳过恢复、记录 Incident，不重复交付。
- [ ] 检测已知终端/shell 目标；含 LF 的模型生成文本不自动粘贴，进入 Pending 或主动复制。

**退出条件：** 自动化证明单行/多行都只有一次 Ctrl+V、没有 Enter 路径、剪贴板竞争不会覆盖用户内容或导致重复交付，终端换行失败关闭。

### Task 6：History provenance 与热词学习栅栏

**主要文件：** `history.rs`、HistoryRepository、`hotwords.rs`、Delivery commit service、前端 History DTO。

- [ ] SQLite 幂等迁移和旧记录默认值测试。
- [ ] 将目标注入与成功后的 History/学习提交职责分开，commit 接收文本 origin，而不是 Router/LLM 参数。
- [ ] processed 行不可被自动/手工热词批次读取；fallback 行仍可学习。
- [ ] 修复 pending count 与 last_processed_rowid 在不可学习行之间的行为并添加混合批次测试。
- [ ] History UI 至少展示“智能成稿”或“ASR 兜底”来源徽标，不默认展示第二份原文。

**退出条件：** 任何润色新增词都不能经当前 history query 进入 ASR hints。

### Task 7：IncidentVault 轨迹

**主要文件：** Incident model、guard、event dictionary、recovery UI DTO。

- [ ] 增加 Processing stage 与稳定原因码。
- [ ] 超时、空响应和 schema 错误投递有界事件；事件队列满不阻塞 fallback。
- [ ] 文本未授权时事件和导出都不包含原文/模型正文；授权时仍走 artifact 文件与 SHA-256 边界。
- [ ] Incident UI 显示“智能成稿超时，已回退 ASR 原文”等可理解结果。

**退出条件：** 故障注入证明 IncidentVault unavailable 时原文仍能 Delivery。

### Task 8：设置与用户反馈

**主要文件：** `MoreSettingsPanel.tsx`、`AppShellV2.tsx`、domain/ipc client、overlay/presentation tests。

- [ ] 增加 endpoint、model、API key、测试连接和独立授权说明，不提供把快捷键 A 改回原文直出的开关。
- [ ] 增加逐 EXE 写作画像覆盖；不提供任意 Prompt 编辑器。
- [ ] 普通文本框不展示 `clipboard_compatibility` 前置提示；只有真实 Pending、终端防护或手动复制时提供用户可理解的反馈。
- [ ] Overlay 增加 processing 状态；fallback 后给出短暂、非阻塞提示。
- [ ] 保持配置 revision conflict 和凭据保存失败回滚语义。

**退出条件：** 前端测试覆盖加载、保存、冲突、授权拒绝、缺密钥、fallback 提示和 profile override。

### Task 9：全链路验证与发布门禁

- [ ] Rust 全量测试、前端测试、production build、architecture tests/check/asr 全部通过。
- [ ] 使用 fake processor 做成功、20 秒超时、空 content、HTTP 失败、8001 字符、迟到响应、用户取消和 IncidentVault 拥塞测试。
- [ ] 验证 120 秒 ASR final + processing 的资源上限和无界队列扫描。
- [ ] 真实 Windows 快捷键覆盖正常长按、快速按放、慢设备、松开后取消和前台切换。
- [ ] 真实目标矩阵至少覆盖：普通编辑器、一个聊天工具、一个 Office 编辑器、一个编码助手输入框和一个终端；单行/多行都要确认只粘贴一次，聊天不自动发送，终端换行不执行。
- [ ] 保存 build identity、source revision、worktree、目标应用版本和验收录像/记录。
- [ ] 实现复核后才更新 Current C4、Runtime View、code map 和 Dossier implementation status。

**退出条件：** `AC-SD-01` 至 `AC-SD-09` 均有对应证据；缺少真实外部应用互操作时 validation 只能保持 `partial`。

## 7. 测试矩阵

| 维度 | 必测值 |
| --- | --- |
| 路由 | chat / office / coding / general；内置映射、用户覆盖、unknown |
| 模型结果 | valid、empty、invalid JSON、missing text、finish length、HTTP error、timeout、late result |
| 字符边界 | empty、7999、8000、8001；CRLF、CR、LF、NUL、bidi、emoji/代理对 |
| 控制竞争 | cancel before request、cancel during request、disable、stale SessionId、ReadyToInject denied |
| 交付 | AtomicPaste single/multiline、一次 Ctrl+V、无 Enter、target changed、terminal multiline → Pending、clipboard sequence changed、restore failure |
| provenance | processed、asr_fallback、旧历史迁移、混合热词批次、history disabled |
| Incident | consent off/on、queue full、writer failure、timeout metadata、export redaction |

## 8. 发布标准

MVP 可以交付的最低条件：

1. 任何模型失败都不会丢失合法 ASR 原文，也不会交付半个 JSON/半段文本；
2. 用户取消、目标变化和 Actor 拒绝始终优先于原文 fallback；
3. 20 秒后没有合法模型结果就立即选择原文，不进行第二次网络重试；
4. Processing 和 Delivery 统一 8000 Unicode 字符且不静默截断；
5. 普通输入框的单行和多行结果都一次性整体粘贴且不会通过 Enter 误发送；终端含 LF 结果失败关闭；
6. 润色文本不会进入热词学习；
7. 未授权 text-processing endpoint 不读取凭据、不发送完整转写；
8. 目标环境失败会保持 Dossier 为 partial/invalidated，不能以单元测试通过宣称完整验证。

## 9. 风险与后续

- **基础控制面风险：** Starting 快速松开偏差仍需关闭，否则真实快捷键验收不能完成。
- **场景误判：** EXE 分类不能识别同一应用内不同控件；MVP 通过 general fallback 和用户覆盖控制，后续再评估 Accessibility。
- **AtomicPaste 风险：** Ctrl+V 只能证明输入事件已提交，不能单靠 API 证明目标已插入；必须用聊天、Office、编码助手和终端实机矩阵验收，并把剪贴板恢复失败与粘贴提交状态分开。
- **模型语义漂移：** 用户允许模型猜测 ASR 错词；MVP 保留会话内原文和 fallback，但不做二次语义模型校验。
- **热词债务：** MVP 只有学习 eligibility 栅栏，完整 provenance、游标、历史编辑重学仍按既有重构待办推进。
- **延迟：** 已确认的是 Chatbot 20 秒硬截止，不是松开快捷键到交付的完整 SLA；ASR final 最坏等待仍是独立预算。
