# GY Typing 架构知识库

本目录是项目的架构知识库。源码与运行配置是实现事实；这里的 C4、Runtime View、arc42-Lean 和代码地图是对当前或拟议架构的解释，不因结构完整而自动获得语义权威。

- **Feature Dossier**：规定重要功能的用户行为，并独立记录实现与验证状态。
- **Current C4 / Runtime View / arc42-Lean**：解释当前实现的边界、时序、质量属性与风险；必须绑定源码 revision、工作树和语义复核状态，`current` 不等于“符合目标 ADR”。
- **Proposed architecture**：物理隔离尚未成为实现事实的设计视图。
- **Implementation Guide / runbook**：绑定源码 revision 的非规范性实现快照，可以过期，不规定产品行为或长期边界。
- **ADR**：记录为什么采用长期边界，以及未来何时应重新评估。
- **代码地图**：把当前组件 ID 映射到源码、Current 叙事、ADR，并将非规范性 Implementation Guide 单独分类，支持自动影响分析。
- **测试与实机记录**：提供绑定版本、工作树和环境的验证证据。

当前基线：Windows-only Tauri 2 桌面应用；前端为 Preact/Vite，核心语音链路为 Rust；Current 视图最后按脏工作区代码复核于 2026-08-27，具体偏差和源码基线记录在各视图 front matter。

## 推荐阅读路径

本路径面向需要建立系统全貌的人类读者，不是 Agent 执行任务时的默认上下文。Agent 的最小读取范围以仓库根目录 `AGENTS.md` 为准，并依据实际影响面按需展开架构材料。

新成员按以下顺序阅读：

1. [对应功能的 Feature Dossier](../features/README.md)
2. [系统上下文图](c4-context.md)
3. [容器图](c4-container.md)
4. [arc42-Lean 叙事骨架](arc42-lean.md)
5. [后端组件图](c4-components-backend.md) 或 [前端组件图](c4-components-frontend.md)
6. [关键运行时视图](runtime-views.md)
7. [热键录入、换绑事务与 Windows 运行时链路（非规范性 Implementation Guide）](shortcut-editing.md)
8. [架构不变量](invariants.md)
9. [ADR 索引](adr/README.md)
10. [代码地图](code-map.md)

排查或改代码时，先运行：

```powershell
npm run architecture:impact
```

完成改动后运行：

```powershell
npm run architecture:check
```

详细维护方法见 [维护手册](maintenance.md)。
`architecture:check` 只证明文档结构和追踪关系通过，不证明架构语义或目标环境验收通过。


## 视图边界

| 视图 | 回答的问题 | 主要读者 |
| --- | --- | --- |
| C4 L1 上下文 | 谁使用系统，依赖哪些外部系统？ | 产品、开发、安全 |
| Feature Dossier | 用户应该获得什么行为，当前实现和验证到什么程度？ | 产品、开发、测试 |
| C4 L2 容器 | 进程内有哪些可独立理解的运行单元？ | 开发、测试 |
| C4 L3 组件 | Rust 后端和 WebView 内部如何分工？ | 开发、评审者 |
| 运行时视图 | 录音、识别、交付和配置变更如何按时序发生？ | 开发、运维 |
| Implementation Guide / runbook | 某个源码 revision 下热键如何录入、提交、恢复、持久化和诊断？ | 开发、测试、运维 |
| arc42-Lean | 架构目标、约束、策略、质量与风险是什么？ | 全体 |
| ADR | 哪些决策已被接受，替代方案是什么？ | 维护者 |
| 代码地图 | 一个源码变更需要复核哪些文档和决策？ | 提交者、评审者 |
| Proposed architecture | 哪个方案仍待探针、评审或实现验证？ | 设计者、评审者 |

## 文档规则

- 图使用 GitHub 可渲染的 Mermaid；C4 语义通过 Person / System / Container / Component 标签明确表达。
- 每个可追踪组件使用稳定 ID，如 `backend.voice-controller`。
- Markdown 中用 `[component:backend.voice-controller]` 标记组件归属。
- `code-map.json` 按 [JSON Schema](code-map.schema.json) 记录 owner、依赖、契约和追踪关系；Current 架构进入 `docs`，非规范性实现快照进入 `implementationGuides`，源码与运行配置仍是实现事实。
- [architecture.config.json](architecture.config.json) 定义必需文档与生产源码覆盖范围。
- 安全、容量和时限常量必须登记在 [架构不变量](invariants.md)，并从 Rust 具名常量自动核验。
- ADR 一经 Accepted 不重写结论；需要改变时新增 ADR 并标记 supersedes/superseded by。
- 拟议 C4/Runtime View 必须进入 [proposals](proposals/README.md) 并声明 `viewStatus=proposed`；当前视图声明 `viewStatus=current`，同时记录 `sourceRevision`、`worktreeState`、`reviewStatus`、`reviewedAt`、`knownDeviations`，脏工作树还记录 `changedPaths`。
- Implementation Guide 必须声明 `normative=false` 并绑定 source revision、worktree 状态、复核状态和关联 Feature；不得用它覆盖 Dossier、ADR 或源码事实。
- 测试通过但目标环境失败时，产品验收仍失败；不得用文档或局部测试解释掉实机结果。

## 导航

- [Feature Dossiers](../features/README.md)
- [Architecture Proposals](proposals/README.md)
- [C4 L1：系统上下文](c4-context.md)
- [C4 L2：容器](c4-container.md)
- [C4 L3：后端组件](c4-components-backend.md)
- [C4 L3：前端组件](c4-components-frontend.md)
- [运行时与部署视图](runtime-views.md)
- [热键录入、换绑事务与 Windows 运行时链路（非规范性 Implementation Guide）](shortcut-editing.md)
- [arc42-Lean](arc42-lean.md)
- [架构不变量](invariants.md)
- [代码地图](code-map.md)
- [维护手册](maintenance.md)
- [ADR 索引](adr/README.md)
