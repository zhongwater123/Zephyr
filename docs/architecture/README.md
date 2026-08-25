# GY Typing 架构知识库

本目录是项目的可增量维护代码地图。它不复制实现细节，而是把代码事实组织成四种互补视图：

- **C4**：回答系统在什么边界内、由哪些容器和组件组成。
- **arc42-Lean**：解释目标、约束、运行时、质量属性与风险。
- **ADR**：记录为什么采用当前方案，以及未来何时应重新评估。
- **代码地图**：把稳定组件 ID 映射到源码、图、叙事和 ADR，支持自动影响分析。

当前基线：Windows-only Tauri 2 桌面应用；前端为 Preact/Vite，核心语音链路为 Rust；本页最后按工作区代码核对于 2026-08-25。

## 推荐阅读路径

新成员按以下顺序阅读：

1. [系统上下文图](c4-context.md)
2. [容器图](c4-container.md)
3. [arc42-Lean 叙事骨架](arc42-lean.md)
4. [后端组件图](c4-components-backend.md) 或 [前端组件图](c4-components-frontend.md)
5. [关键运行时视图](runtime-views.md)
6. [架构不变量](invariants.md)
7. [ADR 索引](adr/README.md)
8. [代码地图](code-map.md)

排查或改代码时，先运行：

```powershell
npm run architecture:impact
```

完成改动后运行：

```powershell
npm run architecture:check
```

详细维护方法见 [维护手册](maintenance.md)。

## 视图边界

| 视图 | 回答的问题 | 主要读者 |
| --- | --- | --- |
| C4 L1 上下文 | 谁使用系统，依赖哪些外部系统？ | 产品、开发、安全 |
| C4 L2 容器 | 进程内有哪些可独立理解的运行单元？ | 开发、测试 |
| C4 L3 组件 | Rust 后端和 WebView 内部如何分工？ | 开发、评审者 |
| 运行时视图 | 录音、识别、交付和配置变更如何按时序发生？ | 开发、运维 |
| arc42-Lean | 架构目标、约束、策略、质量与风险是什么？ | 全体 |
| ADR | 哪些决策已被接受，替代方案是什么？ | 维护者 |
| 代码地图 | 一个源码变更需要复核哪些文档和决策？ | 提交者、评审者 |

## 文档规则

- 图使用 GitHub 可渲染的 Mermaid；C4 语义通过 Person / System / Container / Component 标签明确表达。
- 每个可追踪组件使用稳定 ID，如 `backend.voice-controller`。
- Markdown 中用 `[component:backend.voice-controller]` 标记组件归属。
- `code-map.json` 按 [JSON Schema](code-map.schema.json) 记录 owner、依赖、契约和追踪关系；源码依然是行为事实。
- [architecture.config.json](architecture.config.json) 定义必需文档与生产源码覆盖范围。
- 安全、容量和时限常量必须登记在 [架构不变量](invariants.md)，并从 Rust 具名常量自动核验。
- ADR 一经 Accepted 不重写结论；需要改变时新增 ADR 并标记 supersedes/superseded by。
- 文档只描述当前已实现能力。规划内容必须明确写为“拟议”或记录在 ADR 的备选方案中。

## 导航

- [C4 L1：系统上下文](c4-context.md)
- [C4 L2：容器](c4-container.md)
- [C4 L3：后端组件](c4-components-backend.md)
- [C4 L3：前端组件](c4-components-frontend.md)
- [运行时与部署视图](runtime-views.md)
- [arc42-Lean](arc42-lean.md)
- [架构不变量](invariants.md)
- [代码地图](code-map.md)
- [维护手册](maintenance.md)
- [ADR 索引](adr/README.md)
