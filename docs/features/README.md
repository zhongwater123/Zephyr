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
  "schemaVersion": 1,
  "featureId": "FEAT-EXAMPLE",
  "specStatus": "draft",
  "implementationStatus": "not_started",
  "validationStatus": "unverified",
  "components": ["frontend.features"],
  "decisions": [],
  "validationSlices": [
    { "id": "AC-EX-01", "components": ["frontend.features"] }
  ],
  "evidence": []
}
---
```

状态含义：

- `specStatus`: `draft | confirmed | superseded`
- `implementationStatus`: `not_started | in_progress | implemented | deprecated | superseded`
- `validationStatus`: `unverified | partial | validated | invalidated`
- 单条证据 `freshness`: `current | potentially_stale | stale | revalidated`

`validated` 只允许用于所有关键验收均有当前目标环境证据的功能。单元测试通过但缺少真实环境验证时使用 `partial`。

## 正文模板

每份 Dossier 固定使用以下章节，避免把产品规范和实现叙事混写：

1. `用户目标`
2. `验收场景`
3. `明确不规定的实现`
4. `局部假设`
5. `架构决策`
6. `当前实现入口`
7. `验证状态`
8. `澄清历史`

“当前实现入口”只列组件、源码入口和 Runtime View 链接，不复制完整时序。完整历史事故进入非规范性 Postmortem。

## 冲突矩阵

| 冲突 | 默认动作 |
| --- | --- |
| 用户确认 vs Feature Dossier | 暂停冲突实现；含义明确时更新规格，范围不清时先复述影响并确认 |
| Feature Dossier vs Accepted ADR | 重新评估；决策变化时新增 Superseding ADR，不改写历史 ADR |
| ADR vs 源码 | 分类为实现漂移或决策失效，分别修复实现或新增替代 ADR |
| 源码 vs C4 | 先核对规格和 ADR；若只是描述落后则更新 C4 |
| 验收标准 vs 运行证据 | 将验收标记失败，禁止声称完成，但不预判根因 |
| Assumption vs 用户反馈 | 同一范围内明确矛盾时 Rejected，范围不清时 Challenged |
| 测试通过 vs 实机失败 | 保留测试结论但标记覆盖不足，以实机失败否定完整验收声明 |

## 验证证据

普通开发证据记录 source revision、worktree 状态、变更路径、环境和日期。发布级或关键系统验收再增加 build ID 和 artifact SHA-256。代码影响分析只能提示 `Potentially Stale`；是否真正失效由验收切片和人工复核决定。
