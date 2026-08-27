---
{
  "schemaVersion": 3,
  "featureId": "FEAT-INCIDENT-VAULT",
  "specStatus": "draft",
  "implementationStatus": "implemented",
  "validationStatus": "partial",
  "implementationReview": {
    "status": "partial",
    "sourceRevision": "38e54443bb4357771c9c789f83d5fc7e4ed3830c",
    "worktreeState": "dirty",
    "changedPaths": ["src-tauri/src/incident", "src-tauri/src/voice_controller.rs", "src/features/history"],
    "reviewedAt": "2026-08-27",
    "summary": "IncidentVault 的隔离边界已有历史实现核对，但当前脏工作树及目标环境行为尚未形成完整的实现符合性复核。",
    "knownDeviations": []
  },
  "components": ["frontend.features", "backend.incident-vault", "backend.repositories", "storage.local"],
  "decisions": ["ADR-0008", "ADR-0011"],
  "validationSlices": [
    { "id": "AC-IV-01", "components": ["backend.incident-vault"], "requiredEvidence": ["automated"] },
    { "id": "AC-IV-02", "components": ["backend.incident-vault", "storage.local"], "requiredEvidence": ["automated"] },
    { "id": "AC-IV-03", "components": ["frontend.features", "backend.incident-vault"], "requiredEvidence": ["automated", "windows_webview2"] }
  ],
  "evidence": [
    {
      "id": "EV-IV-20260825",
      "acceptanceIds": ["AC-IV-01", "AC-IV-02", "AC-IV-03"],
      "acceptanceCoverage": [
        { "acceptanceId": "AC-IV-01", "coverage": "partial" },
        { "acceptanceId": "AC-IV-02", "coverage": "partial" },
        { "acceptanceId": "AC-IV-03", "coverage": "partial" }
      ],
      "method": "automated",
      "result": "partial",
      "freshness": "potentially_stale",
      "capabilities": ["automated"],
      "scope": "Historical automated implementation checks for IncidentVault boundaries and storage behavior",
      "testRefs": ["cargo test incident::", "npm test"],
      "limitations": ["Artifact identity and target-environment UI validation were not retained"],
      "sourceRevision": "8206806efa9ab3169daa3059e6929f20419c84bd",
      "worktreeState": "unknown",
      "environment": "Windows development workspace; artifact identity not recorded",
      "validatedAt": "2026-08-25"
    }
  ],
  "impactAssessments": []
}
---

# IncidentVault

## 用户目标

本功能已经实现，但尚缺独立确认的产品规格。本草案暂时描述为：在不改变正式历史和语音关键路径的前提下，为异常会话提供本地、授权隔离、可诊断和可删除的恢复材料。

## 验收场景

| ID | 暂定结果 | 验证层级 |
| --- | --- | --- |
| `AC-IV-01` | 异常事件入口有界且不会把存储背压传回语音关键路径 | Rust 单元/性能测试 |
| `AC-IV-02` | 恢复材料按内容、音频、文本授权隔离，并保持路径、摘要和删除完整性 | Rust 存储与安全测试 |
| `AC-IV-03` | 用户可在 History 界面查看、复制、播放、导出或删除被授权材料 | 前端测试 + 目标环境验收 |

## 明确不规定的实现

- 产品规格不指定 SQLite、队列类型、ZIP 库或前端组件结构。
- IncidentVault 不得成为正式历史、ASR 或文本交付的提交权威。

## 局部假设

- 当前用户目标是从既有实现和 ADR 推导的草案，尚未经过独立产品确认。
- 普通日志附件的精确时间窗口仍未定义；当前实现只截取受限尾部并脱敏。

## 架构决策

- [ADR-0008：产品前端融合、后端隔离的本地异常恢复](../architecture/adr/0008-incident-vault-isolated-recovery.md)

## 当前实现入口

- 后端：`src-tauri/src/incident/`
- 前端：History/Incident Recovery feature
- 当前组件和运行时说明：[后端 C4](../architecture/c4-components-backend.md)、[运行时视图](../architecture/runtime-views.md)

## 验证状态

2026-08-25 的实施核对来自提交 `8206806efa9ab3169daa3059e6929f20419c84bd`，当时记录了 Rust、前端、构建、Clippy、安全和架构检查通过。由于没有保存构建制品身份，且之后代码已经变化，该证据标记为 `potentially_stale`，当前整体状态为 `partial`。

该历史核对覆盖有界队列、授权收窄、崩溃恢复、SHA-256、路径边界、删除可重试、ZIP 字段授权、脱敏和稳定 ID 查询；它不能替代当前目标环境的用户验收。

## 澄清历史

- 2026-08-25：IncidentVault 实现核对和测试快照最初被写入架构维护手册。
- 2026-08-26：将其迁入本 Dossier，恢复维护手册的单一文档角色；由于缺少独立产品确认，规格保持 `draft`。
