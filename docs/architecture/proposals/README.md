# Architecture Proposals

本目录只保存尚未成为当前实现事实的架构设计。Proposal 不得进入 `code-map.json` 的 `docs` 或 `adrs`，也不得被 Current C4、Runtime View 或 arc42 描述成已经实现。

每个 Proposal 必须以 JSON front matter 开头：

```json
{
  "documentType": "architecture-proposal",
  "viewStatus": "proposed",
  "owner": "maintainer-id",
  "createdAt": "2026-08-26",
  "revisitWhen": "可观察的接受、拒绝或失效条件",
  "relatedFeatures": ["FEAT-EXAMPLE"]
}
```

设计被接受后先形成 ADR；实现完成并与源码核对后，再把相应结构写入 Current C4。被放弃的 Proposal 标记 Rejected 或归档，但不能静默改写为当前事实。
