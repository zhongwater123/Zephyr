# ADR-0016：MVP 确定性路由与 DeepSeek Flash 默认模型

- Status: Superseded
- Date: 2026-08-28
- Deciders: Project maintainers
- Drivers: MVP 需要可解释、可复现且不消费 ASR 正文的 Router；内部员工需要稳定的默认快速成稿模型
- Related features: FEAT-SMART-DICTATION
- Assumptions: ASM-SD-01, ASM-SD-02, ASM-SD-10
- Evidence: 2026-08-28 用户确认 Router 使用目标 EXE + 用户覆盖、浏览器默认 general，并确认默认模型为 deepseek-v4-flash
- Supersedes: None
- Superseded by: ADR-0017

## 背景

Smart Dictation 需要在 ASR 确认文本与智能润色之间增加 Router。MVP 必须先给出可解释、可复现的路由规则，同时为后续引入更多快捷键和意图信号保留边界。内部部署还需要一个无需员工配置的默认文本处理模型。

## 决策

1. 快捷键 A 明确启动 `SmartDictation` 意图；它不与普通原文直出快捷键混用。
2. Router 是纯决策组件，只读取会话开始时冻结的目标 EXE 与路由配置快照；MVP 不读取 `rawText`、屏幕内容或网络信息来推断场景。
3. Profile 选择优先级固定为：
   `用户逐应用覆盖 > 内置 EXE 分类 > general`。
4. 内置分类只负责把已知聊天、办公和编程应用映射到对应 Profile。浏览器与未知 EXE 一律默认 `general`；用户可通过逐应用覆盖改变结果。
5. Router 只输出类型化的 `ProcessingPlan`，其中至少包含 `profileId`、`routeReason` 和配置版本；它不拼装 Prompt，也不调用模型。
6. Text Processing 的默认模型为 `deepseek-v4-flash`。请求关闭思考模式、使用非流式 JSON Output，并遵守 Feature Dossier 已定义的 20 秒总截止时间、8000 Unicode 字符上限与原文兜底契约。
7. 模型名由内部部署配置管理，`deepseek-v4-flash` 是 MVP 默认值，不向普通员工暴露模型或 API Key 配置入口。一次处理使用冻结的模型配置快照。
8. MVP 不做基于转写语义的自动路由。以后新增快捷键、显式用户意图或上下文信号时，应扩展 Router 输入契约并补充决策记录，不得把判断偷偷放进 Prompt 或 Delivery。

## 结果

- 同一目标 EXE 与同一配置快照产生相同路由结果，便于测试、审计和回放。
- 浏览器中的具体网页意图无法由 EXE 判断，因此默认 `general` 是有意的保守降级，不代表 Router 已理解网页场景。
- 用户覆盖可以修正内置分类，但 MVP 不会自动学习覆盖规则。
- 默认模型可由内部部署统一升级；升级必须保留会话快照、超时、JSON 校验和原文兜底语义。

## 未采用方案

- **依据 `rawText` 做语义路由**：会让路由消费可能含 ASR 错词的内容，增加语义污染与不可解释性，MVP 不采用。
- **浏览器按域名或页面内容路由**：需要额外的前台上下文采集与隐私边界，不进入 MVP。
- **让每个 Prompt 自行选择模型或目标 Profile**：破坏 Router、Prompt Registry 与 Processor 的职责隔离。

## 验证要求

- 单元测试覆盖用户覆盖、已知 EXE、浏览器和未知 EXE 的优先级与默认行为。
- 契约测试确认 Router 不接收正文，且 `ProcessingPlan.profileId` 固定解析到对应的独立 Prompt 文件。
- 请求级测试确认未配置模型时使用 `deepseek-v4-flash`，并显式关闭思考模式。
- 运行时证据必须证明一次会话使用冻结的路由与模型配置快照。

## Revisit when

- MVP 需要引入第二快捷键、翻译、Agent 或其他显式处理意图。
- 产品需要按浏览器域名、窗口标题、Accessibility tree、屏幕或文本语义路由。
- `deepseek-v4-flash` 不再满足可用性、延迟、成本或 JSON Output 契约。

