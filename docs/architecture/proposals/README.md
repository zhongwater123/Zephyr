# Architecture Proposals

本目录只保存尚未成为当前实现事实的架构设计。Proposal 不得进入 `code-map.json` 的 `docs` 或 `adrs`，也不得被 Current C4、Runtime View 或 arc42 描述成已经实现。

## 当前提案

- [macOS 单仓能力切片开发与安全交付边界](macos-parallel-development.md)：记录共享产品链路、macOS 纵向能力单元、有界原生 helper、交付提交三态、权限、浮层和已确认直接 DMG 路线的候选实现边界。
- [场景感知文本路由与智能成稿](context-aware-text-routing.md)：记录尚未成为当前实现事实的文本路由与成稿设计历史；其中部分判断已被后续 ADR 和 Feature 契约替代，阅读时以当前 Dossier 与 ADR 状态为准。

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
