---
{
  "documentType": "postmortem",
  "normative": false,
  "incidentDate": "2026-08-26",
  "affectedRevisions": [
    "07493d0e4de10ff59da8b68e09a5b23d65279eeb",
    "702676870f2c4289c79cef24f2fe5d96777505b7"
  ]
}
---

# 快捷键录入职责错位复盘

本文是历史复盘，不是产品规格或当前架构权威。当前用户行为见[快捷键 Dossier](../features/shortcut-binding.md)，稳定边界见 [ADR-0010](../architecture/adr/0010-separate-focused-shortcut-editing.md)。

## 事故表现

修复前版本 `07493d0e4de10ff59da8b68e09a5b23d65279eeb` 中，用户点击快捷键字段后界面进入等待后端生命周期的状态；真实按键没有可靠形成前端候选，日志却持续显示后端 `Capturing`，最终只能取消或超时。

## 错误边界

旧方案让正式 `WH_KEYBOARD_LL` 运行时监听器同时承担设置录入，把后端生命周期快照作为候选和录入状态的唯一权威，并用事件、sequence、operationId 和 250ms 轮询补偿跨边界传输。C4 L3、Runtime View、代码地图和测试从同一假设派生，形成了内部一致但产品错误的闭环。

## 根因与促成因素

- 未经真实交互验证的职责假设过早进入 Current C4 和 Runtime View。
- 编辑录入、配置提交和运行时触发被合并为一个“capture lifecycle”。
- 状态机可以在底层资源未确认就绪时声明 `capturing`，控制面日志掩盖了数据面没有按键的事实。
- 测试偏重 revision、回滚、乱序和事件补偿等 Safety 属性，缺少“真实按键最终到达并即时显示”的 Liveness 证据。
- 前端 Mock 直接产生后端候选事件，绕过了 Hook、队列、Manager、Tauri Event、WebView2 焦点和真实 Windows 输入。
- 用户可见症状连续未改善时，没有触发推翻组件边界的诊断熔断。

## 代表性错误测试预言

旧测试 `enters capture only after the backend snapshot acknowledges it` 明确要求等待后端快照后才进入录入；用户需要的行为恰好相反：点击后立即进入本地录入，并在 begin IPC pending 期间继续显示按键。

## 修复边界

版本 `702676870f2c4289c79cef24f2fe5d96777505b7` 将有焦点设置录入放回前端输入边界，后端只保留校验、事务、运行时应用和回滚，Windows Hook 只负责已提交绑定的全局 Pressed/Released。

## 为什么既有门禁没有发现

架构检查验证路径、Schema、组件覆盖、ADR 元数据和 Mermaid 语法，不验证语义正确性。单元测试、Mock 和文档又来自同一个架构假设，因此测试通过只证明实现符合该假设，不证明真实 Windows 用户链路可用。

## 纠正措施

- 为高风险用户功能建立 Feature Dossier 和带版本的验证状态。
- Proposed 与 Current 架构物理隔离。
- C4 只描述稳定组件责任，交互细节留给规格和 Current Runtime View。
- 影响分析只提示验收可能过期，不自动宣判语义。
- 同一用户可见问题连续两次未改善时，停止局部补丁并重建端到端因果链。
- 未记录目标环境证据时不得宣称完整验证。
