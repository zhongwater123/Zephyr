# ADR-0015：内部分发共享 DeepSeek 凭据并隔离写作画像 Prompt

- Status: Proposed
- Date: 2026-08-28
- Deciders: Project maintainers
- Drivers: 内部团队员工无需配置 API Key；热词 Agent 与智能成稿复用同一 DeepSeek 账户；不同润色画像需要独立迭代且不能互相污染
- Related features: FEAT-SMART-DICTATION
- Assumptions: ASM-SD-08, ASM-SD-09
- Evidence: 2026-08-28 用户确认同一 Key、用途分离授权、画像 Prompt 独立文件，以及线下内部安装包和流量监控的 MVP 部署模型
- Supersedes: None
- Superseded by: None

## Context

当前项目为 HotwordAgent 单独保存 `hotword-agent-api-key`，设置页允许用户配置相关参数。智能成稿原计划新增独立的 TextProcessing Credential Manager 槽，这会让同一 DeepSeek 账户重复存储 Key，并把内部部署责任暴露给普通员工。

MVP 只在线下分发给公司内部团队，API 流量由维护者监控和管理。员工不应看到、输入、测试或轮换 API Key。这个受信任部署模型降低了意外配置和外部分发风险，但不能证明桌面端秘密不可提取：能够运行应用的本机账户仍可能从安装包、进程内存或凭据存储中取得共享 Key。该剩余风险必须作为显式接受的内部 MVP 假设，而不是“不会泄露”的安全保证。

同时，`general`、`chat`、`office` 和 `coding_request` 的语义规则会以不同速度迭代。如果多个画像共用一个可变大 Prompt 或相互 include，修改办公风格可能改变聊天或编码输出，破坏并行开发和回归归因。

## Decision

如果本 ADR 被接受：

1. HotwordAgent 与 TextProcessing 引用同一个 `DeepSeekSharedCredential`。Key 在 Credential Manager 中只保存一份，不复制到每个 feature purpose。
2. 共享秘密不合并用途。`EndpointPurpose::HotwordAgent` 与 `EndpointPurpose::TextProcessing` 继续独立执行 trust/policy 检查和审计；启用或授权一个用途不能自动启用另一个用途。两条路径只有在各自检查通过后才能读取同一个 credential reference。
3. 员工设置页不提供 API Key 输入、显示、测试、复制或删除控件。内部部署流程在安装或首次运行前由管理员/打包工具非交互预置 Credential Manager；缺失时 TextProcessing 立即返回 `processing_missing_key` 并使用 ASR 原文兜底。
4. Key 不进入源码仓库、前端 bundle、普通 JSON 配置、Prompt 文件、日志、Incident artifact 或错误详情。线下安装介质若携带可恢复的共享 Key，视为受控秘密介质并纳入分发、轮换和吊销流程。
5. 监控只记录 purpose、模型、请求结果、token/字符用量、延迟和稳定原因码，不记录 Authorization header、Key、完整 Prompt 或响应正文。
6. 每个写作画像使用独立、版本化、应用拥有的 Prompt 文件：
   - `resources/prompts/smart_dictation/<version>/general.md`
   - `resources/prompts/smart_dictation/<version>/chat.md`
   - `resources/prompts/smart_dictation/<version>/office.md`
   - `resources/prompts/smart_dictation/<version>/coding_request.md`
7. 画像文件只描述自己的角色、改写目标、保留项、禁止项和画像示例，不 include、继承或拼接其他画像文件。修改一个画像不能改变另一个画像的有效内容或版本。
8. JSON Output 协议、安全数据封装、`{ "text": "..." }` schema、8000 字符上限和通用禁止泄露规则由代码中的共享 `PromptEnvelope` 负责；共享 envelope 不包含聊天、办公或编码风格语义。
9. 一个受版本控制的 manifest 把 `profile_id` 映射到文件、`prompt_version` 和内容哈希。Router 只返回有限 `profile_id`；未知 ID、缺失文件或哈希不符失败关闭，不能串到另一个画像。
10. MVP 不提供用户 Prompt 编辑器或远程 Prompt 下发。Prompt 更新随内部构建发布，并在语料回归中按画像分别验收。

## Consequences

### Positive

- 员工无需理解 API Key，内部维护者只轮换一个 DeepSeek credential。
- 同一秘密仍有两个独立的数据用途边界和审计轨迹。
- 四种画像可以由不同开发任务并行迭代，修改和回归影响可单独定位。
- 共享 JSON 协议不会在四个文件中复制后逐渐漂移。

### Negative

- 一个共享 Key 被滥用或吊销会同时影响热词 Agent 和智能成稿，故障半径大于每用途独立 Key。
- 桌面端共享秘密对本机受信任员工不是不可提取的；流量监控只能降低发现和处置时间，不能消除泄露可能。
- 独立 Prompt 文件会产生版本和语料矩阵维护成本；共享规则必须谨慎限制在协议 envelope。
- Prompt 更新需要重新打包或内部发布，不能由普通用户即时修改。

## Alternatives considered

- **每用途保存一份相同 Key：** 增加重复秘密、迁移和轮换状态，不采用。
- **把 Key 编译进 Rust/前端：** 简化安装但更容易从二进制或 bundle 提取，也容易误入源码和构建日志，不采用。
- **员工首次启动时输入 Key：** 与零员工配置目标冲突，不采用。
- **四种画像共用一个大 Prompt：** 修改互相影响，难以并行开发和独立回归，不采用。
- **每个画像复制完整 JSON/安全协议：** 隔离表面更强但公共契约会漂移；采用共享无风格 envelope + 独立语义文件。

## Revisit when

- 安装包开始面向外部用户、承包商或不受信任设备分发；
- DeepSeek 或内部网关支持短期 token、设备身份、OAuth 或每用途密钥；
- 共享 Key 的流量异常、吊销或轮换影响两个功能的可用性；
- Prompt 需要不随安装包发布而独立上线，或需要签名的远程配置通道。
