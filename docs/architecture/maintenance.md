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

在 MVP 迭代中，Dossier 的用户目标、已确认验收和明确不规定的实现构成当前契约；假设、实现入口、验证记录和历史只提供上下文。用户最新明确需求可以直接修订 MVP 契约。只有涉及安全、数据完整性、外部承诺或难以逆转的 Accepted ADR 决策时，才先暂停并更新决策记录。

`current`、`reviewed`、`accepted` 和 `validated` 不是同义词。Current View 只解释它绑定的源码快照；ADR Accepted 只说明决策已经接受；Dossier validated 只说明列出的验收在指定 revision 与环境获得了足够证据。结构检查通过也不升级任何语义状态。

## 最小上下文分级

文档读取按实际影响面升级，不按链接数量扩散：

| 等级 | 典型变化 | 默认读取 | 默认文档写入 |
| --- | --- | --- | --- |
| L0 局部实现 | 重构、局部算法、测试、样式或文案修正，不改变公共行为 | `AGENTS.md`、代码、测试 | 无 |
| L1 契约内功能 | 在既有用户契约内修改跨组件/高风险功能 | L0 + 一份主 Dossier 的契约、开放假设和验证状态 | 通常无；测试留在 commit/CI 或可选 PR |
| L2 产品契约 | 用户目标、验收结果或明确非目标变化 | L1 | 更新一份 Dossier；只有时序/边界也变化时更新一份 View |
| L3 架构边界 | 组件责任、依赖、信任边界、不可逆副作用顺序或长期决策变化 | L1 + 一份相关 Current View + 相关 ADR | 更新 View；决策变化时新增 Proposed/Superseding ADR |
| L4 验证/发布 | 升级验证状态、目标环境验收、安全/故障验证或发布工件 | 相关 Dossier、证据范围和必要的符合性材料 | 写入可追溯证据并由收口者升级状态 |

“一份”表示主入口，不是硬性禁止在确有跨边界影响时多读；但普通任务若需要三份以上规范性材料，应先检查任务是否过宽或文档是否重复。Architecture impact 输出用于选择入口，不是要求逐项阅读和修改的完成清单。

## 人工控制版本集成与工作区隔离

本项目由维护者人工控制 Git 集成。Agent 的默认交付物是经过检查、仍留在当前工作区中的未暂存 diff，而不是 commit、push 或 PR。工作区出现与当前任务一致的未提交修改是正常的人工审查状态；“工作树不干净”不表示 Agent 必须继续执行版本控制操作。

| 通道 | 适用范围 | Agent 默认写入方式 | 人工收口方式 |
| --- | --- | --- | --- |
| Local 审查通道 | 普通 MVP 功能、bug、局部重构、测试、UI 和文档修正 | 在已分配 checkout 中编辑、测试，保留未暂存 diff | 维护者检查后自行暂存、提交和推送 |
| Worktree 隔离通道 | 并行写入、长任务、实验、大型重构、迁移、安全/隐私/数据完整性变化 | 每个 Agent 使用独立 branch + 独立 worktree，仍默认不提交 | 维护者决定如何暂存、提交、合并或放弃 |
| 审计通道 | 代码阅读、诊断、评审、方案比较 | 不修改跟踪文件 | 把结论交给维护者或后续写入者 |

### 版本控制授权边界

编辑文件的授权不包含任何 Git 集成授权。除非用户在当前请求中明确要求对应动作，Agent 不得执行：

- `git add`、`git commit`；
- `git pull`、`git push`；
- `git merge`、`git rebase`、`git cherry-pick`；
- 创建或删除分支、tag，或切换共享 checkout 的分支；
- 创建 PR、release，或使用管理员权限绕过 branch protection / required checks。

授权按动作独立判断：“修改”不等于“暂存”，“暂存”不等于“提交”，“提交”不等于“推送”。历史会话中的授权、当前账户具有管理员权限、仓库允许 direct push、远端链接需要生效、CI 需要触发，均不能替代当次明确授权。用户表达不清时，默认把完成的修改留在工作区并报告，不把方便收尾当成扩大权限的理由。

若用户当次明确要求提交或推送，Agent 必须在动作前重新检查精确 diff、分支、worktree、remote 和所需检查。保护分支若需要管理员 bypass，应先报告将被绕过的规则；只有用户已经明确接受该次 bypass 时才可继续。任何“提交并推送”之外的远端动作仍需单独授权。

### 工作区与并行写入

所有指向同一 checkout 的会话共享 `HEAD`、索引和未提交文件，因此同一个 checkout 同时只能有一个写入 Agent。多个 Agent 可以并行只读分析；并行写入必须为每个 Agent 分配独立 Git worktree，通常由 Codex Worktree 工作流或用户明确安排。分支名本身不隔离文件，禁止在共享 checkout 中通过 `git switch`、`git checkout` 或 `git switch -c` 模拟隔离。

开始编辑前必须运行：

```powershell
git rev-parse --show-toplevel
git worktree list --porcelain
git status --short --branch
```

确认当前路径确实是分配给该任务的 workspace。已有未提交修改一律视为用户所有：

- 先识别是否与当前任务重叠；
- 不为了获得 clean tree 而 reset、stash、覆盖、暂存或提交；
- 能确定不重叠时可以在用户指定 workspace 中继续，否则保持只读并报告冲突，或使用用户/Codex 分配的独立 worktree；
- 一个 Agent 完成后可以留下 dirty worktree，下一位 Agent 必须基于真实 diff 判断，而不是假设这些修改可以接管或清理。

Agent 完成编辑后运行与风险匹配的检查，并向维护者报告：修改文件、验证结果、失败或未覆盖的目标环境，以及 `git status`。默认保持文件 unstaged/uncommitted。只有用户明确要求时才执行后续 Git 动作；不得创建 commit 只为让工作树恢复干净。

### 项目状态与文档权限

- 项目的已集成状态由维护者最终提交的 revision、CI、Dossier 验证状态和 release tag 表达；Agent 工作区中的 diff 只表示待审修改。
- PR 是维护者可选择的审查与集成工具，不由 Agent 因任务规模自行创建。
- 开发 Agent 可以修改代码、测试和用户明确改变的 MVP 契约，也可以报告验证与偏差；不得自行把 Dossier 升为 `validated`、Current View 升为 `reviewed`，或把 ADR 从 Proposed 升为 Accepted。
- 指定收口者只在维护者完成集成后，基于 clean revision 和目标环境证据执行语义升级；未提交 workspace diff 不得被写成已经进入 main 的事实。
- 同一事实出现冲突时，不以最后写入者为准。源码决定实现事实，Dossier 契约决定用户行为目标，Accepted ADR 决定仍有效的长期边界，目标环境证据决定对应验收是否成立。
- 不建立 `docs/changes/<task-id>` 流水账。普通交付信息先保存在 workspace diff 和 Agent handoff，集成后进入维护者选择的 commit、CI 或可选 Issue/PR；只有跨任务持续存在且会影响架构判断的偏差，才使用稳定 Issue ID 回链到 `knownDeviations`。

## 日常工作流

### 1. 变更前：确定产品与影响面

跨组件或高风险用户功能先读取一份主 Dossier 的最小契约范围，再运行：

```powershell
npm run architecture:impact
```

相对分支基线：

```powershell
npm run architecture:impact -- --base origin/main
```

脚本合并已暂存、未暂存、未跟踪文件和可选 Git base diff。源码命中组件后沿 `dependsOn` 反向传播，并计算相关 Dossier 验收切片的 `effectiveFreshness`。输出分为主 Dossier 候选、条件阅读的 View/ADR 和仅审计的证据新鲜度；开发者按任务实际边界选择，不递归阅读全部候选。脚本不修改 Dossier；`partial` / `unverified` 只告警，只有已声明 `validated` 且缺少重新验证或有效 `impactAssessment` 时阻断门禁。

默认输出只显示路由和按 Feature 聚合的审计摘要；需要调查传播原因时才使用 `npm run architecture:impact -- --verbose`（可与 `--base origin/main` 组合）展开逐组件和逐验收切片明细。

### 2. 设计中：隔离 Proposed

- 拟议 C4 或 Runtime View 进入 `docs/architecture/proposals/`，不得进入当前代码地图。
- 跨边界、长期或难以撤销的决定先创建 Proposed ADR。
- Open Assumption 可以支持决策，但必须写明风险与可观察的重新评估条件。
- 快速变化且尚未确认的产品判断记录在 Dossier 可选的`概念迭代记录`，不要塞进验收场景，也不要为了保存每次讨论而创建 ADR。
- 技术探针和薄实现验证后，方案才能转为 Accepted；Accepted 只表示决策已接受，不表示已经实现。实现与源码核对、登记已知偏差后才更新 Current C4。

### 3. 实现中：按变更类型复核

| 变化 | 至少复核 | 何时写文档 |
| --- | --- | --- |
| 用户可观察行为 | 一份 Feature Dossier 的 MVP 契约 | 只有契约或验证判断变化时更新 Dossier |
| 外部系统、用户或信任边界 | 相关 C4 L1/L2、arc42 风险、ADR | 边界实际变化时更新 View；长期取舍变化时新增 ADR |
| 进程、WebView、存储或组件责任 | 相关 C4 L2/L3、代码地图、ADR | 责任或依赖变化时更新，不记录私有重构 |
| 录音、识别、交付、配置或快捷键关键时序 | 一份主 Dossier、Runtime View、相关 ADR | 不可逆副作用或关键顺序变化时更新 Runtime View |
| 容量、时限或安全上限 | 代码常量、架构事实、不变量及 mentions | 事实值或安全边界变化时同步 |
| 仅组件内部算法 | 公共契约和测试 | 默认不写文档 |
| 推翻既有架构决策 | 旧 ADR 与受影响契约 | 新增 ADR 并建立替代关系，不改写历史结论 |

### 4. 变更后：结构校验与验证

```powershell
npm run architecture:check
```

结构检查覆盖：

- 当前架构必需文件、Dossier、Proposal、Postmortem、非规范性 Implementation Guide 和兼容入口；
- 代码地图 Schema、组件覆盖、路径、依赖和 marker；
- Dossier 元数据、实现符合性 revision、确认来源、组件、ADR、验收切片、逐项覆盖、testRefs、证据能力和影响评估引用；
- Current 视图的 revision、工作树、复核状态和已知偏差，Current/Proposed 状态隔离，以及 Postmortem / Implementation Guide 的非规范性声明；
- `implemented` 功能的可配置源码边界门禁，以及超大组件的非阻断内聚复核提醒；
- ADR 编号、状态、索引和新元数据；
- Rust 架构事实、Markdown 链接、围栏和 Mermaid 语法。

成功输出必须明确说明：结构检查通过不证明语义或目标环境验收。

结构检查也不推断真实生产调用图，不证明某个单元测试实际执行了 Windows 原生、外部进程、系统 Hook、剪贴板或 WebView2 路径。高风险原生边界若要求“主进程不得调用某 API”“故障必须隔离在 helper”或“部分副作用不得自动重试”，必须另外建立可执行的源码边界门禁和目标环境/故障注入测试；只在 Dossier、ADR 或测试替身中写出期望回执，不构成实现证明。结构门禁、代码边界门禁和目标环境验收必须分别报告，不能用其中一个代替另两个。

## Feature Dossier

- 只有跨组件、高风险或依赖目标环境验证的功能创建 Dossier。
- `authority` 必须显式填写，禁止通过缺省值猜测权威级别。
- 规格状态、实现状态和验证状态独立维护。
- 实现状态由 `implementationReview` 绑定源码 revision、工作树和已知偏差；存在未关闭偏差时不得声明 `implemented`。
- `confirmed` 必须记录确认人、日期和来源，不允许作者自行升级。
- `validated` 要求所有关键验收的 requiredEvidence 都有成功、带版本/环境且有效新鲜的证据能力。
- 普通测试结果保留在 commit/CI、任务摘要或可选 PR，不自动抄入 Dossier。只有验证状态升级、目标环境/人工/故障结果、证据失效评估或发布工件追溯需要时，才记录长期证据。
- 进入 Dossier 的开发证据记录 revision、worktree、变更路径、环境、日期、scope 和 limitations；发布级证据再记录 build ID 与 artifact SHA-256。
- 自动化证据必须提供 `testRefs`，并用 `acceptanceCoverage` 逐验收声明完整或部分覆盖；不得用一条笼统的全量测试记录替代不同验收语义。
- 生成式功能的工程正确性和产品质量分开验证：协议与兜底使用 `automated`，结果质量使用 `human_quality_eval`，用户是否理解和能否预测交互使用 `usability_observation`。
- 相关源码变化产生 `potentially_stale` 的有效状态；可以重新验证，或提交绑定 revision、切片和理由的 `impactAssessment`。

## Current 与 Proposed

- `c4-*.md`、`runtime-views.md` 和 `arc42-lean.md` 是 Current 文档，必须声明 `viewStatus=current`，并记录 `sourceRevision`、`worktreeState`、`reviewStatus`、`reviewedAt` 和 `knownDeviations`；脏工作树还必须记录 `changedPaths`。
- `viewStatus=current` 只表示这份文件承担 Current 角色；当 `reviewStatus=stale|partial` 时，Agent 不得把它当作当前源码的完整事实。只有收口者能基于 clean main revision 将其升级为 `reviewed`。
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

只有 L2-L4 变更需要文档完成定义；L0/L1 若没有触发文档写入条件，以代码、测试和 commit/CI（或可选 PR）为完成依据。L2-L4 完成时必须同时满足：

- 源码仍能从代码地图导航，生产源码覆盖保持完整；
- Current C4 与实际进程、组件和外部边界一致；
- 用户行为变化已进入 Dossier，关键验收有对应测试或明确 Pending；
- 关键决策变化已形成 ADR；
- 每个验收切片的声明状态与计算出的有效新鲜度一致；已验证切片不存在未处理的 `potentially_stale`；
- `npm run architecture:check` 通过；
- 完成声明与 Dossier 的 validation status 一致。
