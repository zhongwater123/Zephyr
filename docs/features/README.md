# Feature Dossiers

Feature Dossier 是跨组件或高风险用户功能的入口，负责保存产品应该表现出的行为、明确假设和带版本的验证状态。它不替代 ADR、源码、C4 或 Runtime View。

## 材料角色

| 类型 | 权威内容 |
| --- | --- |
| Feature Dossier | 产品应该提供的用户行为 |
| ADR | 长期架构边界及其取舍 |
| 源码与运行配置 | 当前实现事实 |
| C4、Runtime View、arc42 | 人对当前或拟议架构的解释 |
| 测试与实机记录 | 特定版本和环境已经证明的行为 |
| Assumption | 尚未确认但会影响设计或验收的判断 |

这些材料不能线性互相覆盖。发现冲突时先分类，再修改对应权威来源。

## MVP 契约边界

Feature Dossier 服务于当前 MVP，但其章节并不具有相同约束力：

- **MVP 契约**：`用户目标`、已确认的 `验收场景` 和 `明确不规定的实现`。它们描述当前迭代要验证的用户行为与边界。
- **上下文与证据**：`局部假设`、可选的`概念迭代记录`、`架构决策`链接、`当前实现入口`、`验证状态` 和 `澄清历史`。它们帮助判断与复核，但不能单独阻止一个更符合最新用户需求的实现。
- `specStatus=draft` 的 Dossier 全部为候选材料；`confirmed` 只表示其中的产品契约已有可追溯的用户确认，不代表当前实现、验收方法或技术方案被永久确认。

所有 Dossier 可选声明 `authority`：`mvp_contract` 表示当前可调整的验证契约，`hard_boundary` 仅用于安全、数据完整性或外部承诺边界，`reference` 表示背景材料。未声明时按 `mvp_contract` 处理。

## 创建门槛

只有以下内容使用 Feature Dossier 和稳定 ID：

- 跨越多个组件的用户功能；
- 依赖 Windows、WebView2、硬件、第三方服务或手工验收的高风险行为；
- 失败会造成数据、副作用、安全或长期不可用的功能。

普通 UI 文案、局部算法和不改变边界的小修复不创建 Dossier。局部假设保存在功能文件内；只有跨功能假设增长到需要独立生命周期时才建立全局索引。

## 元数据

Dossier 以 `---` 包围的 JSON 对象开头。JSON 同时是 YAML 的合法子集，可直接用 `JSON.parse` 和 [JSON Schema](feature-dossier.schema.json) 校验，无需新增解析依赖。

```markdown
---
{
  "schemaVersion": 3,
  "featureId": "FEAT-EXAMPLE",
  "authority": "mvp_contract",
  "specStatus": "draft",
  "implementationStatus": "not_started",
  "implementationReview": {
    "status": "unreviewed",
    "sourceRevision": "abcdef0",
    "worktreeState": "clean",
    "reviewedAt": "2026-08-27",
    "summary": "尚未开始实现复核",
    "knownDeviations": []
  },
  "validationStatus": "unverified",
  "components": ["frontend.features"],
  "decisions": [],
  "validationSlices": [
    { "id": "AC-EX-01", "components": ["frontend.features"], "requiredEvidence": ["automated"] }
  ],
  "evidence": [],
  "impactAssessments": []
}
---
```

状态含义：

- `specStatus`: `draft | confirmed | superseded`
- `authority`: `mvp_contract | hard_boundary | reference`（省略时为 `mvp_contract`）
- `implementationStatus`: `not_started | in_progress | implemented | deprecated | superseded`
- `implementationReview.status`: `unreviewed | partial | conformant | deviating`
- `validationStatus`: `unverified | partial | validated | invalidated`
- 单条证据 `freshness`: `current | potentially_stale | stale | revalidated`

`validated` 只允许用于所有关键验收均有当前目标环境证据的功能。单元测试通过但缺少真实环境验证时使用 `partial`。

`implemented` 只表示当前实现已经过对应 revision 的源码符合性复核且没有登记中的实现偏差；它不表示目标环境已经验证。发现 Actor 绕行、部分提交、未实现契约或其他与 Accepted ADR/验收不一致的行为时，必须使用 `in_progress` 并在 `implementationReview.knownDeviations` 中列出。Accepted ADR 只表示决策已接受，不自动升级实现状态。

`confirmed` 不是作者自我声明。它必须包含 `confirmation.confirmedBy`、`confirmedAt` 和可追溯的 `sourceRef`；尚无确认来源时保持 `draft`。

每个验收切片用 `requiredEvidence` 声明完成它真正需要的证据能力，例如 `automated`、`windows_webview2`、`runtime_hook`、`fault_injection`、`restart_persistence` 或 `external_app_interop`。证据用 `capabilities` 声明实际覆盖能力，并通过 `acceptanceCoverage` 对每个关联验收单独标记 `full` 或 `partial`；只有 `result=pass`、覆盖为 `full` 且新鲜度有效的能力才能完成验收。自动化证据还必须提供可追踪的 `testRefs`。`method=automated` 本身不能证明真实 WebView2 或 Windows Hook。

`validationStatus` 是作者声明；检查器还会根据证据 revision 之后传播到验收组件的源码变化计算 `effectiveFreshness`。声明为 `validated` 的功能若出现未处理的 `potentially_stale` 切片，结构检查和 CI 都会失败。

## 正文模板

每份 Dossier 固定包含以下必需章节，避免把产品规范和实现叙事混写：

1. `用户目标`
2. `验收场景`
3. `明确不规定的实现`
4. `局部假设`
5. `架构决策`
6. `当前实现入口`
7. `验证状态`
8. `澄清历史`

`概念迭代记录`是可选章节，位于`局部假设`之后，用于保存不值得升级为 ADR、但如果删除就容易被重新提出的产品判断。它不属于 MVP 契约，不能覆盖已确认验收。其条目使用稳定的 `CI-<FEATURE>-NN` 标识和 `Open | Confirmed | Challenged | Rejected | Superseded` 状态，并写明当前结论、依据或下一次复核条件。

“当前实现入口”只列组件、源码入口和 Runtime View 链接，不复制完整时序。完整历史事故进入非规范性 Postmortem；普通产品方向的撤回和试验失败进入`概念迭代记录`，不为每次想法变化创建 ADR 或 Postmortem。

## 冲突矩阵

| 冲突 | 默认动作 |
| --- | --- |
| 用户确认 vs Feature Dossier | 最新明确需求直接更新 MVP 契约；只有含义不清或触及 `hard_boundary` 时暂停并确认影响 |
| Feature Dossier vs Accepted ADR | 重新评估；决策变化时新增 Superseding ADR，不改写历史 ADR |
| ADR vs 源码 | 分类为实现漂移或决策失效，分别修复实现或新增替代 ADR |
| 源码 vs C4 | 先核对规格和 ADR；若只是描述落后则更新 C4 |
| 验收标准 vs 运行证据 | 将验收标记失败，禁止声称完成，但不预判根因 |
| Assumption vs 用户反馈 | 同一范围内明确矛盾时 Rejected，范围不清时 Challenged |
| 测试通过 vs 实机失败 | 保留测试结论但标记覆盖不足，以实机失败否定完整验收声明 |

## 快速概念迭代

MVP 讨论中的想法先按权威级别分类，避免“说过”自动变成“已经确认”：

- 用户明确要求现在实现的可观察行为进入 MVP 契约和验收场景。
- 用户明确说“先讨论”、尚待比较或缺少结果定义的方案进入 `Open` 或 `Challenged` 的概念迭代记录。
- 已被用户否定、被目标环境结果推翻或与更新后的产品目标冲突的方案标记为 `Rejected`；不要删除，否则后续容易重复走回旧路。
- 被新方案替代但当时合理的判断标记为 `Superseded`，并链接替代条目或 ADR。
- 只有跨边界、长期、难以撤销的技术取舍才升级为 ADR。UI 文案、交互候选和模型策略试验通常不需要 ADR。

概念状态不等于实现状态：源码中已经存在的试验仍可能是 `Challenged`；自动化通过也不能把产品判断升级为 `Confirmed`。升级的依据必须与判断类型相符。

## 验证证据

普通开发证据记录 source revision、worktree 状态、变更路径、环境和日期，并明确它证明的能力、范围和限制。发布级证据再增加 build ID 和 artifact SHA-256。

- `human_quality_eval`：对模型或生成结果进行有协议的人工质量评测。至少记录语料范围、对照基线、评审维度、评审者范围和主要失败类型；单纯“看起来不错”不构成该能力。
- `usability_observation`：让目标用户完成代表性任务，观察其是否理解控件、能否预测结果并完成操作。组件测试、快照和作者自评不能替代该能力。
- 两种能力可以由同一次研究产生，但证据必须分别说明输出质量与交互理解覆盖了什么。
- 对 LLM 功能，`automated` 通常只能证明协议、边界、兜底和回归样例，不自动证明结果“有用”“自然”或“比原文更好”。

影响分析不直接改写 Dossier。相关源码变化会使受影响切片的有效新鲜度变成 `potentially_stale`：

- 功能仍为 `partial` 或 `unverified` 时只报告，不妨碍继续开发；
- 功能声明为 `validated` 时会阻断门禁，直到补充重新验证证据；
- 如果评审确认变更不影响既有证据，可以提交绑定评估 revision、验收切片和理由的 `impactAssessment`。检查器会继续追踪该评估之后的源码变化，路径匹配不能自行作最终语义判决。
