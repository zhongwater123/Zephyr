# 架构与功能文档维护手册

## 维护目标

本知识库用代码触发复核、依赖传播、结构校验和追加式决策记录控制文档漂移。自动化只证明结构与追踪关系完整，不证明架构语义或目标环境验收正确。

## 文档角色

| 材料 | 负责回答 |
| --- | --- |
| Feature Dossier | 用户应该获得什么行为，以及当前验证到什么程度 |
| ADR | 为什么采用某个长期边界，何时重新评估 |
| 源码与运行配置 | 当前实现实际做什么 |
| Current C4 / Runtime View / arc42 | 如何理解当前实现 |
| Proposed architecture | 尚未成为实现事实的设计方案 |
| Implementation Guide / runbook | 绑定源码 revision 的非规范性实现快照与排障信息 |
| Test / manual evidence | 哪些行为在哪个版本与环境得到证明 |
| Assumption | 尚未确认但会影响设计或验收的判断 |

材料发生冲突时，按 [Feature Dossier 冲突矩阵](../features/README.md#冲突矩阵) 分类；不得通过重复同一假设来制造伪证据。

## 日常工作流

### 1. 变更前：确定产品与影响面

跨组件或高风险用户功能先读取对应 Dossier，再运行：

```powershell
npm run architecture:impact
```

相对分支基线：

```powershell
npm run architecture:impact -- --base origin/main
```

脚本合并已暂存、未暂存、未跟踪文件和可选 Git base diff。源码命中组件后沿 `dependsOn` 反向传播，并计算相关 Dossier 验收切片的 `effectiveFreshness`。脚本不修改 Dossier；`partial` / `unverified` 只告警，只有已声明 `validated` 且缺少重新验证或有效 `impactAssessment` 时阻断门禁。

### 2. 设计中：隔离 Proposed

- 拟议 C4 或 Runtime View 进入 `docs/architecture/proposals/`，不得进入当前代码地图。
- 跨边界、长期或难以撤销的决定先创建 Proposed ADR。
- Open Assumption 可以支持决策，但必须写明风险与可观察的重新评估条件。
- 技术探针和薄实现验证后，方案才能转为 Accepted；实现与源码核对后才更新 Current C4。

### 3. 实现中：按变更类型复核

| 变化 | 至少复核 |
| --- | --- |
| 用户可观察行为 | Feature Dossier、验收切片、相关自动化与实机证据 |
| 外部系统、用户或信任边界 | C4 L1/L2、arc42 风险、ADR |
| 进程、WebView、存储或组件责任 | C4 L2/L3、代码地图、ADR |
| 录音、识别、交付、配置或快捷键时序 | Current Runtime View、Dossier、相关 ADR |
| 容量、时限或安全上限 | 代码常量、架构事实、不变量及 mentions |
| 仅组件内部算法 | 确认公共契约、验收和叙事仍准确 |
| 推翻既有架构决策 | 新增 ADR 并建立替代关系，不改写历史结论 |

### 4. 变更后：结构校验与验证

```powershell
npm run architecture:check
```

结构检查覆盖：

- 当前架构必需文件、Dossier、Proposal、Postmortem、非规范性 Implementation Guide 和兼容入口；
- 代码地图 Schema、组件覆盖、路径、依赖和 marker；
- Dossier 元数据、确认来源、组件、ADR、验收切片、requiredEvidence、证据能力和影响评估引用；
- Current/Proposed 状态隔离，以及 Postmortem / Implementation Guide 的非规范性声明；
- ADR 编号、状态、索引和新元数据；
- Rust 架构事实、Markdown 链接、围栏和 Mermaid 语法。

成功输出必须明确说明：结构检查通过不证明语义或目标环境验收。

## Feature Dossier

- 只有跨组件、高风险或依赖目标环境验证的功能创建 Dossier。
- 规格状态、实现状态和验证状态独立维护。
- `confirmed` 必须记录确认人、日期和来源，不允许作者自行升级。
- `validated` 要求所有关键验收的 requiredEvidence 都有成功、带版本/环境且有效新鲜的证据能力。
- 普通开发证据记录 revision、worktree、变更路径、环境、日期、scope 和 limitations；发布级证据再记录 build ID 与 artifact SHA-256。
- 相关源码变化产生 `potentially_stale` 的有效状态；可以重新验证，或提交绑定 revision、切片和理由的 `impactAssessment`。

## Current 与 Proposed

- `c4-*.md` 和 `runtime-views.md` 是 Current 文档，必须声明 `viewStatus=current`。
- Proposal 必须声明 owner、创建日期、复核条件和关联 Feature。
- `code-map.json` 只连接当前源码、Current 文档和 Accepted/相关 ADR，不登记 Proposal。
- 如果某个实现细节改变而组件责任和关系不变，该细节通常不属于 C4。
- 详细状态机、日志和排障快照属于 `normative=false` 的 Implementation Guide；代码地图通过 `implementationGuides` 单独引用，不得放入 Current `docs` 冒充架构规范。
- Implementation Guide 必须绑定 source revision、工作树状态、复核状态和关联 Feature；它可以过期，不能覆盖 Dossier、ADR 或源码事实。

## ADR

1. 复制 [ADR 模板](adr/template.md)，编号只增不复用。
2. 新 ADR 填写 Drivers、Related features、Assumptions 和 Evidence。
3. 初始状态为 Proposed；评审后改为 Accepted 或 Rejected。
4. 在 [ADR 索引](adr/README.md) 和相关代码地图组件中登记。
5. 决策变化时新增 ADR，并在新旧记录中建立替代关系。

Accepted ADR 可以依赖 Open Assumption，但必须在 `Revisit when` 中说明假设被推翻后的处理条件。

## 架构事实与不变量

数值型安全、容量和时限事实继续由 `architecture-facts.json` 从 Rust 具名常量核验。结果导向的行为边界可以记录在 `invariants.md`，但不得把当前 API 或传输机制升级为永久禁令。

## 连续失败与完成声明

- 同一用户可见问题连续两次未改善时，停止增加状态、轮询、重试或补偿，重新验证真实输入到用户输出的完整因果链。
- 测试通过但目标环境失败时，保留测试结论并把完整验证标记失败或部分完成。
- 未保存对应 build/worktree/environment 证据时，不得声称目标环境已经验证。

## 文档完成定义

架构或高风险功能变更完成时必须同时满足：

- 源码仍能从代码地图导航，生产源码覆盖保持完整；
- Current C4 与实际进程、组件和外部边界一致；
- 用户行为变化已进入 Dossier，关键验收有对应测试或明确 Pending；
- 关键决策变化已形成 ADR；
- 每个验收切片的声明状态与计算出的有效新鲜度一致；已验证切片不存在未处理的 `potentially_stale`；
- `npm run architecture:check` 通过；
- 完成声明与 Dossier 的 validation status 一致。
