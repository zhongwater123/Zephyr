# ADR-0009：按材料角色和证据状态治理架构文档

- Status: Accepted
- Date: 2026-08-26
- Deciders: Project maintainers
- Drivers: 防止未经验证的设计通过多份文档和同源测试扩散为伪事实
- Related features: FEAT-SHORTCUT-BINDING, FEAT-INCIDENT-VAULT
- Assumptions: 结构化元数据只能验证机械一致性，语义仍需人和目标环境证据判断
- Evidence: shortcut capture misalignment postmortem
- Supersedes: None
- Superseded by: None

## Context

[ADR-0007](0007-architecture-docs-as-code.md) 建立了 C4、arc42-Lean、ADR 和代码地图，但一次快捷键事故证明：多份文档可以一致地描述同一个错误，结构检查也可能被误解为语义正确性。项目还缺少直接承载用户行为、假设和目标环境验证状态的轻量入口。

## Decision

- 保留既有文档类型，并新增高风险功能的 Feature Dossier。
- 区分规范、决策、实现事实、描述、证据和假设；这些材料不能用单一权威排行榜互相覆盖。
- 分离规格、实现和验证状态；验证证据绑定 source revision、worktree、环境和日期。
- C4/Runtime View 明确区分 Proposed 与 Current，Proposal 与当前代码地图物理隔离。
- 结构、引用和非法状态由 CI 阻断；代码影响只把相关验收报告为 `Potentially Stale`，不自动判定语义失效。
- Feature Dossier 只用于跨组件或高风险功能，其他修改不建立完整追踪链。
- Accepted ADR 可以依赖未决假设，但必须公开风险和可观察的重新评估条件。

## Consequences

### Positive

- 用户目标、实现事实和验证证据不会再被同一份架构叙事替代。
- 后续会话能够区分已实现但未实机验证的功能。
- Proposed C4 保留设计价值，同时不会伪装为当前架构。
- 文档创建门槛限制治理成本和稳定 ID 数量。

### Negative

- 高风险功能需要维护一份额外 Dossier 和验证状态。
- 影响分析只能提供复核线索，不能消除人工语义判断。
- 旧证据与 dirty worktree 的适用范围需要维护者诚实记录。

## Alternatives considered

- 删除 C4/arc42/ADR：失去抽象层、连续叙事和决策历史，不能解决错误假设来源问题。
- 为每个功能建立独立规格、假设、验证和事故文件：追踪更细，但会迅速产生官僚化维护成本。
- 让 CI 自动判断架构语义：不可可靠实现，容易把关键词和路径相关性误当成产品正确性。

## Revisit when

- Dossier 数量或状态更新成本显著高于其发现的问题；
- 多包/多团队需要独立规格平台；
- `Potentially Stale` 告警长期误报且无法通过验收切片改善；
- 机器可读元数据开始复制大量实现细节。
