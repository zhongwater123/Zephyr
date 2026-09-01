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

## MVP Trunk、写入租约与隔离

本项目由一位维护者负责，多个会话和 Agent 可以异步处理不同功能。默认采用 trunk-based 的 **main 单写租约**，而不是“每个 Agent 一条分支和一个 PR”。Agent 身份不是交付单位；一次可回滚的用户目标或原子修改才是提交单位。

| 通道 | 适用范围 | 写入方式 | 收口方式 |
| --- | --- | --- | --- |
| Main 快速通道 | 普通 MVP 功能、bug、局部重构、测试、UI 和文档修正 | 获得租约后直接修改并提交 `main` | 范围测试通过即可；PR 可省略 |
| Main 批次通道 | 同一目标跨多个短会话连续完成，但不需要并行写文件 | 后续会话轮流取得同一租约，继续在 `main` 创建原子提交 | 在功能或每日节点统一 push/CI |
| 隔离通道 | 真正并行的写入者、长任务、实验、大型重构、迁移、安全/隐私/数据完整性变化 | 独立 branch + 独立 worktree；一个 worktree 一个写入者 | 由 main 租约持有者 cherry-pick/合并；PR 仅按风险需要 |
| 审计通道 | 代码阅读、诊断、评审、方案比较 | 不取得租约，不修改跟踪文件 | 把结论交给后续写入者 |

### Main 写入租约

所有指向 canonical Local checkout 的会话共享 `HEAD`、索引和未提交文件，因此可以并行分析，但不能并行编辑、测试、暂存、提交、pull 或 push。直接写 main 前必须取得 Git common directory 下的 `codex-main-writer.lock` 目录；目录的原子创建就是租约获取，禁止先检查后覆盖，也禁止使用 `-Force` 抢锁。

PowerShell 获取示例：

```powershell
$repoRoot = (git rev-parse --show-toplevel).Trim()
$gitCommonValue = (git rev-parse --git-common-dir).Trim()
$gitCommon = if ([IO.Path]::IsPathRooted($gitCommonValue)) { $gitCommonValue } else { Join-Path $repoRoot $gitCommonValue }
$leasePath = Join-Path ([IO.Path]::GetFullPath($gitCommon)) "codex-main-writer.lock"
New-Item -ItemType Directory -Path $leasePath -ErrorAction Stop
@{
  task = "<task-id-or-short-description>"
  owner = "<thread-or-session-id>"
  baseRevision = (git rev-parse HEAD).Trim()
  acquiredAt = (Get-Date).ToString("o")
} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $leasePath "owner.json") -Encoding UTF8
```

获取失败表示已有写入者。该 Agent 继续只读分析、等待，或在任务确实满足隔离门槛时使用 worktree；不得修改已有租约。租约不能仅因时间过去而自动窃取。只有原持有者，或在确认原会话已经结束且 canonical checkout 干净后执行恢复的 Agent，才能删除陈旧租约。

持有租约后必须再次执行：

```powershell
git rev-parse --show-toplevel
git worktree list --porcelain
git status --short --branch
git rev-parse HEAD
```

快速通道要求当前路径是 canonical checkout、分支为 `main`、工作区干净，并重新读取受影响文件，不能把租约前的旧分析直接当成当前事实。Agent 在一个租约内只处理一个原子任务：修改、执行匹配风险的检查、创建可回滚 commit、确认 `git status --short` 为空，再释放租约。分析尽量在获取租约前完成；不要在等待用户、外部服务或长时间实验时占用 main。

释放时先确认 `$leasePath` 是本仓库 Git common directory 下的精确租约目录，并核对 `owner.json` 属于当前任务，然后删除文件和空目录：

```powershell
Remove-Item -LiteralPath (Join-Path $leasePath "owner.json")
Remove-Item -LiteralPath $leasePath
```

如果实现未完成、测试失败或工作区无法安全恢复为干净状态，Agent 不得释放租约后把脏 main 留给下一会话；应报告当前状态并继续收口，或在取得用户同意后把剩余工作迁移到隔离通道。禁止用 `git reset --hard` 或覆盖他人文件清理现场。

### 分支、PR 与状态

- 分支名不隔离工作文件。禁止在共享 canonical checkout 中用 `git switch`、`git checkout` 或 `git switch -c` 开启并行任务；需要分支时创建独立 worktree。
- worktree 是并发和风险隔离工具，不自动产生 PR。隔离任务完成后可以由 main 租约持有者 cherry-pick 一个或多个原子提交，并及时删除临时 worktree/branch。
- PR 只用于发布批次、难以逆转的长期决策、需要外部审查的安全/隐私/迁移变更，或用户明确要求的检查点。普通 MVP 修改不需要逐提交等待用户审核。
- 本地 commit、pull、push 都改变共享项目状态，必须在 main 租约内串行执行。可以每个成功提交后 push，也可以按功能/每日批次 push；push 前运行与批次匹配的汇总检查。
- 项目状态由 main 的 clean commits、CI、Dossier 验证状态和 release tag 表达，不由 Agent 数量、会话数量或临时分支数量表达。
- 开发 Agent 可以修改代码、测试和用户明确改变的 MVP 契约，也可以报告验证与偏差；不得自行把 Dossier 升为 `validated`、Current View 升为 `reviewed`，或把 ADR 从 Proposed 升为 Accepted。指定收口者基于 clean main revision 和目标环境证据执行语义升级。
- 同一事实出现冲突时，不以最后写入者为准。源码决定实现事实，Dossier 契约决定用户行为目标，Accepted ADR 决定仍有效的长期边界，目标环境证据决定对应验收是否成立。
- 不建立 `docs/changes/<task-id>` 流水账。普通交付信息保存在 commit、CI 和可选 Issue/PR；只有跨任务持续存在且会影响架构判断的偏差，才使用稳定 Issue ID 回链到 `knownDeviations`。

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
