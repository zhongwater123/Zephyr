---
{
  "schemaVersion": 1,
  "featureId": "FEAT-SHORTCUT-BINDING",
  "specStatus": "confirmed",
  "implementationStatus": "implemented",
  "validationStatus": "partial",
  "components": ["frontend.features", "frontend.ipc", "backend.commands", "backend.shortcut", "backend.services", "backend.repositories", "platform.windows"],
  "decisions": ["ADR-0010"],
  "validationSlices": [
    { "id": "AC-SC-01", "components": ["frontend.features"] },
    { "id": "AC-SC-02", "components": ["frontend.features"] },
    { "id": "AC-SC-03", "components": ["frontend.features", "backend.shortcut"] },
    { "id": "AC-SC-04", "components": ["frontend.features", "frontend.ipc", "backend.commands", "backend.shortcut", "backend.repositories", "platform.windows"] },
    { "id": "AC-SC-05", "components": ["frontend.features", "backend.shortcut", "platform.windows"] },
    { "id": "AC-SC-06", "components": ["frontend.features", "backend.services", "backend.shortcut"] },
    { "id": "AC-SC-07", "components": ["backend.shortcut", "backend.repositories", "platform.windows"] }
  ],
  "evidence": [
    {
      "id": "EV-SC-7026768",
      "acceptanceIds": ["AC-SC-01", "AC-SC-02", "AC-SC-03", "AC-SC-04", "AC-SC-05", "AC-SC-06"],
      "method": "automated",
      "result": "pass",
      "freshness": "potentially_stale",
      "sourceRevision": "702676870f2c4289c79cef24f2fe5d96777505b7",
      "worktreeState": "clean",
      "changedPaths": ["src/features/shortcut", "src-tauri/src/shortcut_manager.rs", "src-tauri/src/windows_keyboard.rs"],
      "environment": "Windows development workspace; frontend and backend automated suites recorded, build identity not retained",
      "validatedAt": "2026-08-26"
    }
  ]
}
---

# 快捷键换绑

## 用户目标

用户点击当前快捷键字段后立即进入录入；输入框实时显示已经观察到的完整物理组合。合法候选自动提交，失败明确反馈并恢复权威旧绑定。

## 验收场景

| ID | 用户可观察结果 | 当前验证要求 |
| --- | --- | --- |
| `AC-SC-01` | 点击字段后当帧进入录入，不等待 begin IPC 回执 | DOM 组件测试 + Windows/WebView2 |
| `AC-SC-02` | 左右 Ctrl/Alt/Shift/Win 逐键实时显示，完整候选始终可见 | DOM 事件测试 + Windows/WebView2 |
| `AC-SC-03` | 裸 Escape、再次点击或点击字段外取消，字段和旧绑定恢复 | 组件/控制器测试 + Windows/WebView2 |
| `AC-SC-04` | 合法候选自动保存；新键在当前进程生效，旧键不再触发 | 事务测试 + 真实 Windows Hook |
| `AC-SC-05` | 非法候选保留并提示；应用或持久化失败时回滚并明确反馈 | 前后端测试 + 故障注入 |
| `AC-SC-06` | 功能禁用时允许保存并提示“已保存，开启后生效”，保存本身不意外启用功能 | 控制器/服务测试 + 实机 |
| `AC-SC-07` | 应用重启后读取最后一次成功持久化的绑定并可真实触发 | 打包程序重启验收 |

## 明确不规定的实现

- 本规格不要求 DOM、某种 Tauri Event 或特定 Win32 API；它只要求有焦点的编辑输入与正式全局监听故障隔离。
- 本规格不规定内部 IPC 名称、状态机类型、线程模型或日志字段。
- 本次不增加第二次确认按钮，也不扩展为一个动作绑定多个快捷键。

## 局部假设

- `ASM-SC-01`（Open）：运行时匹配快捷键后是否必须吞掉系统原有行为仍需单独产品确认；在确认前不得把“Ctrl+C 不执行复制”升级为通用验收标准。
- Windows/WebView2 当前能够通过有焦点输入边界提供所需的左右物理键粒度；若该条件失效，按 [ADR-0010](../architecture/adr/0010-separate-focused-shortcut-editing.md) 的复核条件重新评估。

## 架构决策

- [ADR-0010：分离有焦点的设置录入与全局运行时监听](../architecture/adr/0010-separate-focused-shortcut-editing.md)
- 文档治理遵循 [ADR-0009](../architecture/adr/0009-evidence-aware-document-governance.md)。

## 当前实现入口

- 前端 feature：`src/features/shortcut/`
- 后端事务：`src-tauri/src/shortcut_manager.rs`
- Windows 运行时：`src-tauri/src/windows_keyboard.rs`
- 当前时序：[快捷键录入与提交](../architecture/runtime-views.md#快捷键录入与提交)

源码与运行配置是实现事实；本节不复制具体状态机、时序和线程细节。

## 验证状态

当前为 `partial`。修复实现位于 `702676870f2c4289c79cef24f2fe5d96777505b7`；现有前端自动化覆盖即时录入、物理候选、取消、非法重试和失败回滚，但尚未在本 Dossier 中登记带 build/worktree/environment 的完整 Windows/WebView2 与 Hook 纵向验收。

用户曾报告问题已由该版本修复，但当时没有记录构建身份和工作树状态，因此只作为历史观察，不升级为 `validated` 证据。

## 澄清历史

- 2026-08-26：确认点击字段即进入换绑，捕获期间完整候选实时显示；外部点击与裸 Escape 取消；合法候选自动提交；成功、失败和未变化均有明确结果。
- 2026-08-26：确认设置录入与正式全局 Hook 是两个职责平面，后端保留最终校验、事务、运行时应用和回滚权威。
- 事故与错误架构传播见[非规范性复盘](../postmortems/shortcut-capture-misalignment.md)。
