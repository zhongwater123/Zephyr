# ADR-0007：采用 C4、arc42-Lean、ADR 和机器可读代码地图

- Status: Accepted
- Date: 2026-08-24
- Deciders: Project maintainers
- Supersedes: None
- Superseded by: None

## Context

单篇 `architecture.md` 能描述当前实现，却无法稳定表达不同抽象层次，也不能回答某个源码变更需要复核哪些图和决策。纯手工图容易与模块拆分、IPC 和安全边界漂移。

## Decision

采用组合式文档即代码体系：

- C4 L1/L2/L3 用 Mermaid 表达系统、容器和组件边界；
- arc42-Lean 用 12 个精简主题提供连续叙事；
- ADR 以追加方式保存重要决策与替代方案；
- `code-map.json` 用稳定组件 ID 连接源码、文档、ADR 和变更触发条件；
- `architecture:impact` 根据 Git 变更提示复核范围；
- `architecture:check` 在 CI 校验路径、链接、组件覆盖、ADR 元数据与 Mermaid 围栏。

源码仍是行为事实，JSON 清单是追踪事实，Markdown 是面向人的解释。

## Consequences

### Positive

- 不同读者可按抽象层阅读，不需要从实现文件猜系统边界。
- 代码变更能得到可执行的文档影响提示。
- 决策历史不会被“更新当前说明”悄悄抹除。
- 不引入新的文档生成依赖。

### Negative

- 语义是否准确仍需人工评审，路径存在不等于描述正确。
- 组件 ID、图、叙事和 ADR 多一套维护纪律。
- Mermaid 适合架构关系，不替代 UI 或低层协议图。

## Alternatives considered

- 单篇架构说明：简单，但抽象层混合且难追踪。
- 自动从 imports 生成全部图：能反映语法依赖，却不能表达信任、提交点和设计意图。
- 外部 SaaS 架构工具：可视化更强，但离开仓库、难与 CI 和离线评审结合。

## Revisit when

仓库扩展为多包/多服务、组件数量显著增长，或需要发布独立架构网站与版本化 API 文档时重新评估。
