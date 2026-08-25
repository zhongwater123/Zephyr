# 架构文档维护手册

## 维护目标

这套文档采用“代码触发复核、依赖传播、清单自动校验、决策追加记录”的方式维护。它不要求每次修改都重画全部图，而是要求变更者能回答：**哪些架构组件被改动，哪些上游消费者、视图和决策需要同步复核？**

项目级行为由 [architecture.config.json](architecture.config.json) 配置；组件结构由符合 [code-map.schema.json](code-map.schema.json) 的 [code-map.json](code-map.json) 描述；安全与资源常量由 [architecture-facts.json](architecture-facts.json) 追踪。

## 日常工作流

### 1. 变更前：查看影响面

当前工作区：

```powershell
npm run architecture:impact
```

相对分支基线：

```powershell
npm run architecture:impact -- --base origin/main
```

脚本合并已暂存、未暂存、未跟踪文件和可选 Git base diff。源码直接命中组件后，再沿 `dependsOn` 反向传播到依赖该组件的消费者；输出 owner、命中原因、变更触发条件、需复核文档和 ADR。它提供影响提示，不阻止开发。

### 2. 变更中：按类型更新

| 代码变化 | 至少复核 |
| --- | --- |
| 新增外部服务、用户或信任边界 | C4 L1、L2、arc42 上下文、风险、ADR |
| 新增进程、WebView、数据存储 | C4 L2、部署视图、代码地图、ADR |
| 拆分或合并 Rust/前端组件 | 对应 C4 L3、代码地图和 dependsOn |
| 修改录音、识别、取消、交付时序 | 运行时视图、质量场景、相关 ADR |
| 修改容量、时限或安全上限 | 代码常量、架构事实、不变量页及 mentions |
| 修改配置 schema、revision、凭据或 endpoint 授权 | arc42 横切概念、ADR、代码地图 |
| 修改注入方式或 Pending 语义 | 后端组件图、运行时视图、ADR |
| 仅修改组件内部算法且边界不变 | 确认公共契约、依赖与叙事仍准确 |
| 推翻既有架构决策 | 新增 ADR，不覆写旧 ADR 的历史结论 |

### 3. 变更后：执行一致性校验

```powershell
npm run architecture:check
```

校验覆盖：

- 架构配置中的必需文档是否存在；
- `code-map.json` 是否符合正式 JSON Schema；
- 组件 ID、状态、owner、依赖和公共契约是否合法；
- 依赖引用是否存在、是否形成环；
- 配置范围内的每个生产源码文件是否映射到组件；
- 每个源码、文档和 ADR 路径是否存在并留在仓库内；
- 每个组件 ID 与架构 fact 是否至少有一个 Markdown marker；
- Rust 具名常量是否与 `architecture-facts.json` 一致；
- fact mentions 中的叙事值是否仍与代码一致；
- 架构 Markdown 的相对链接是否有效；
- Mermaid flowchart 和 sequenceDiagram 是否能被官方解析器解析；
- ADR 元数据、编号和索引是否完整。

CI 在 Windows 作业中执行同一命令。

## 添加或调整组件

1. 在 `code-map.json` 增加稳定、语义化 ID；不要使用文件名作为 ID。
2. 填入 `status`、`owner`、源码、主文档、ADR、`dependsOn`、公共契约和变更触发条件。
3. 在 C4 L3 或代码地图中加入 `[component:<id>]` marker。
4. 如果引入新边界或不可逆约束，创建 ADR。
5. 运行 `npm run architecture:check`，确认生产源码覆盖仍为 100%。

组件被删除后，先从依赖图、图和叙事中移除，再删除清单项。组件只是重命名时，优先保留旧 ID，避免历史 ADR 和链接失效。

## 添加或修改架构不变量

1. 在 Rust 中使用具名数值常量，避免架构相关的裸字面量。
2. 在 `architecture-facts.json` 登记值、单位、代码 symbol 和叙事 mentions。
3. 在 [架构不变量](invariants.md) 加入 `[fact:<id>]` marker。
4. 更新 mentions 指向的叙事。
5. 运行 `npm run architecture:check`；代码值、事实值和叙事值任一不一致都会失败。

## 新增 ADR

1. 复制 [ADR 模板](adr/template.md)，命名为 `NNNN-short-kebab-title.md`。
2. 编号只增不复用。
3. 初始状态用 Proposed；评审后改为 Accepted 或 Rejected。
4. 在 [ADR 索引](adr/README.md) 登记。
5. 在 `code-map.json` 的相关组件中建立引用。
6. 如果替代旧决策，在新旧 ADR 中互相链接。

## 复用到其他仓库

复制 `scripts/check-architecture-docs.mjs` 与架构目录后，主要调整：

- `architecture.config.json` 的目录、必需文档和源码覆盖范围；
- `code-map.json` 的组件及依赖；
- `architecture-facts.json` 的代码事实源；
- package scripts 和 CI 步骤。

校验引擎不依赖本项目运行时代码，只要求 Node、AJV、Mermaid 和一个 DOM 测试实现。项目叙事与组件数据保持在配置/JSON 中。

## 更新频率与责任

- 架构变更与代码在同一个 PR/变更集中更新。
- 评审者根据 `architecture:impact -- --base <ref>` 输出核对遗漏。
- 每个版本发布前运行完整校验，并抽查系统上下文、容器图、运行时主链路和风险表。
- “最后核对日期”只表示人工复核点，不代替 Schema、路径、代码常量和 Mermaid 校验。

## 文档完成定义

一次影响架构的代码修改只有在以下条件同时满足时才算完成：

- 生产源码覆盖保持 100%；
- 受影响组件仍能从代码地图导航到源码；
- C4 边界与实际进程、WebView、外部依赖一致；
- arc42-Lean 的约束、运行时或风险没有过时陈述；
- 关键常量与架构 facts 一致；
- 关键决策变化已形成 ADR；
- `npm run architecture:check` 通过。
## IncidentVault 实施核对记录（2026-08-25）

本记录描述共享工作区中本会话实际落地并重新静态核对的行为，不把其他并行会话的快捷键/低级钩子改动归入 IncidentVault。正式历史仍使用 `history.db`，异常恢复使用独立 `incident.db` 与 artifact 目录；前端在 History Dialog 聚合，后端 API、schema 和生命周期保持隔离。

### 已实现契约

- `IncidentSink` 提供 Noop 与异步实现。控制、音频与 gap marker 分别使用容量 64 的有界 `ArrayQueue`；投递只做无锁 push、原子计数和 unpark。PCM 使用 `Bytes`，音频 attempt ID 使用 `Arc<str>`。
- writer 线程独占写路径连接/句柄并用 `catch_unwind` 隔离 panic；正常退出最多等待 500ms。音频队列满时 gap marker 不依赖控制队列空位，artifact 会标记 `gapped`。
- 每个 attempt 持久化总内容、音频、文本三个正交授权位；旧数据库新增子授权列时默认拒绝。重复 start 与 writer 重启都只能收窄授权，不能提升授权。
- 崩溃恢复只封存匹配且持久化音频授权的 `.pcm.part`，完整性记为 `truncated` 并生成 SHA-256；孤儿或未授权文件删除。panic emergency 已导入合法行会移除，坏行/失败行保留重试。
- artifact 相对路径只允许一个普通文件名。文本/音频读取验证 SHA-256；删除失败或路径无效时拒绝删除数据库索引，以免产生不可见孤儿材料。
- finding、前端异常、panic message/backtrace、凭据样式行、URL query 与本地路径复用统一限长脱敏器。普通应用日志不自动导入。
- provider canonical final 在成功终止前单独进入 Vault；partial 仍最多每 500ms checkpoint。`history_committed` 与 `discard_recovery_material` 正交：历史提交成功或历史关闭时删除，历史写入失败时保留。
- 查询、复制、报告按稳定 incident ID 工作，不受最近 200 条分页限制。ZIP 默认无文本、音频和普通日志；勾选文本只加入 partial/final，不隐式加入 target app/window。音频通过二进制 IPC，前端 Blob URL 在替换与卸载时撤销。
- 前端 ErrorBoundary、`window.error` 与 `unhandledrejection` 进行限长、去重、限流的本地结构化捕获，投递失败不会改变 UI 或语音链路。

### 本轮静态审查修复

1. 修复重启恢复只检查总授权、未检查音频子授权的问题，并清理无匹配 attempt 的孤儿 PCM。
2. 修复 emergency 导入忽略单行失败却清空整个文件的问题。
3. 修复音频和控制队列同时满时 gap 完整性标记可能丢失的问题。
4. 修复 artifact 删除失败后仍删除数据库索引的问题；同步更新了要求“篡改路径删除仍成功”的旧测试契约。
5. 修复 ZIP 的“附带文本”选项隐式序列化 target app/window 的授权污染。
6. 在 writer 边界增加统一脱敏，避免依赖每个调用点正确处理 provider/frontend 错误。
7. 修复复制/报告按最近 200 条查找导致旧 incident 无法恢复的问题。
8. 修复崩溃恢复先改名后查授权可能留下不可重试 sealed 孤儿的问题；现在只对 `interrupted` attempt 查询成功后封存，已完成 attempt 的意外 part 会删除。

### 验证快照

- `cargo test incident`：32 passed，0 failed，0 ignored。
- `cargo test`：110 passed，0 failed，0 ignored；doc-tests 0 failed。
- `npm test -- --run`：11 files、23 tests 全部通过；其中 IncidentRecoveryPanel 与安全模型定向测试 8/8。
- `npm run build`：TypeScript 与 Vite 生产构建通过；仅保留既有的单 chunk 超过 500kB 警告。
- `cargo clippy --all-targets --all-features -- -D warnings`：通过。
- `npm run security:secrets`：未检测到凭据模式。
- `npm run architecture:check`：20 个组件、75/75 个生产源码、21 个 Markdown、9 张 Mermaid、14 条架构事实、8 条 ADR，全部通过。
- 新增/更新测试覆盖：双队列饱和 gap、子授权崩溃恢复、孤儿 PCM、已完成 attempt 的意外 part、emergency 坏行保留、writer 强制脱敏、删除失败可重试、ZIP 字段授权、稳定 ID 超 200 条、SHA-256/路径边界、历史失败保留与历史关闭删除。
- IncidentVault 性能门槛测试仍验证 `try_emit(AudioChunk)` P99 小于 50μs，并验证 `Bytes` 零复制共享。

### 当前明确边界

可选普通日志附件当前实现为“最近本地日志文件尾部，总计最多 256KB，再统一脱敏”，尚未按 incident 起止时间解析日志行。文档已按真实行为描述；若升级为严格时间窗口，应先定义 tauri log 时间戳格式与跨轮转文件规则，并补解析/边界测试。
