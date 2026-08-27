# ADR-0011：按验收能力计算有效验证状态

- Status: Accepted
- Date: 2026-08-26
- Deciders: Project maintainers
- Drivers: 防止普通自动化证据冒充目标环境验收，并让相关源码变化可以被持续追踪而不由路径规则直接判定语义失效
- Related features: FEAT-SHORTCUT-BINDING, FEAT-INCIDENT-VAULT
- Assumptions: 组件与验收切片映射足以提供保守的变更相关性提示，但最终语义判断仍需要人工评估或重新验证
- Evidence: 文档治理只读评审、快捷键捕获错位复盘、Schema v2 检查器回归测试
- Supersedes: None
- Superseded by: None

## Context

ADR-0009 建立了 Feature Dossier、独立验证状态和带版本证据，但仅凭 `result=pass` 与声明的新鲜度，普通单元测试仍可能被登记成真实 Windows、WebView2 或运行时 Hook 的验证。影响分析也只产生瞬时告警，无法阻止已经声明为 `validated` 的功能在相关源码改变后继续保持虚假的完成状态。

超详细实现文档如果继续作为 Current 架构主文档被代码地图引用，也会绕过 Dossier、ADR 与验证状态，重新形成单一错误叙事的权威入口。

## Decision

- 每个高风险验收切片声明 `requiredEvidence`；每条证据声明实际 `capabilities`、`scope` 和 `limitations`。证据方法不能隐式获得目标环境能力。
- `confirmed` 规格必须记录确认人、日期和可追溯来源；作者不能只通过修改状态值自行确认规格。
- Dossier 的 `validationStatus` 是声明状态；检查器根据证据或影响评估 revision 之后传播到相关组件的源码变化计算每个切片的 `effectiveFreshness`。
- 路径与依赖传播只产生 `potentially_stale`，不直接宣判语义失效。维护者可以重新验证，或提交绑定 revision、切片和理由的 `impactAssessment`。
- `partial` 与 `unverified` 的受影响切片只报告；声明为 `validated` 且存在未处理 `potentially_stale` 的切片时，结构检查与 CI 阻断。
- 详细状态机、日志和排障快照归类为 `normative=false` 的 Implementation Guide，并绑定源码 revision、工作树与复核状态。代码地图通过独立字段引用，不把它当作 Current 架构规范。

## Consequences

### Positive

- 自动化测试不能再冒充 Windows/WebView2、运行时 Hook、重启或跨应用互操作证据。
- 已验证功能在相关实现变化后不能静默保留完成声明。
- 人工影响评估仍有入口，避免任意文案或不相关代码变化触发机械式全面重测。
- 实现指南保留排障价值，但不会成为新的产品或架构权威旁路。

### Negative

- 高风险验收需要维护证据能力和限制，元数据比 ADR-0009 初版更详细。
- 组件映射只能保守估计影响范围，误报仍需人工评估。
- `validated` 功能的相关源码变更会增加 CI 处置成本。

## Alternatives considered

- 只保留非阻断告警：成本低，但无法阻止过期的 `validated` 声明进入主分支。
- 任意相关路径变化自动把证据改为 `stale`：机械明确，但把相关性错误提升为语义结论。
- 只记录测试方法而不记录能力：字段更少，但无法区分模拟事件与真实目标环境。
- 继续把详细实现文档当作 Current 架构主文档：导航方便，但会恢复未经验证叙事的权威旁路。

## Revisit when

- 组件到验收切片的传播长期产生不可接受的误报；
- 项目引入可证明测试能力和目标环境的统一验证平台；
- Implementation Guide 元数据维护成本超过其排障价值；
- 多团队需要将验证状态迁移到独立的发布或质量管理系统。
