---
{
  "schemaVersion": 3,
  "featureId": "FEAT-SHORTCUT-BINDING",
  "confirmation": {
    "confirmedBy": "user",
    "confirmedAt": "2026-08-26",
    "sourceRef": "Codex task: shortcut binding requirements confirmed by user on 2026-08-26"
  },
  "specStatus": "confirmed",
  "implementationStatus": "implemented",
  "validationStatus": "partial",
  "implementationReview": {
    "status": "partial",
    "sourceRevision": "38e54443bb4357771c9c789f83d5fc7e4ed3830c",
    "worktreeState": "dirty",
    "changedPaths": ["src-tauri/src/shortcut_manager", "src-tauri/src/windows_keyboard.rs", "src/features/shortcut"],
    "reviewedAt": "2026-08-27",
    "summary": "快捷键编辑事务和回滚边界已有进程内复核，真实 Hook、WebView2、重启和外部 Hook 互操作仍未完成实现符合性复核。",
    "knownDeviations": []
  },
  "components": ["frontend.features", "frontend.ipc", "backend.commands", "backend.shortcut", "backend.services", "backend.repositories", "platform.windows"],
  "decisions": ["ADR-0010", "ADR-0011"],
  "validationSlices": [
    { "id": "AC-SC-01", "components": ["frontend.features"], "requiredEvidence": ["automated", "windows_webview2"] },
    { "id": "AC-SC-02", "components": ["frontend.features"], "requiredEvidence": ["automated", "windows_webview2"] },
    { "id": "AC-SC-03", "components": ["frontend.features", "backend.shortcut"], "requiredEvidence": ["automated", "windows_webview2"] },
    { "id": "AC-SC-04", "components": ["frontend.features", "frontend.ipc", "backend.commands", "backend.shortcut", "backend.repositories", "platform.windows"], "requiredEvidence": ["automated", "windows_webview2", "runtime_hook"] },
    { "id": "AC-SC-05", "components": ["frontend.features", "backend.shortcut", "platform.windows"], "requiredEvidence": ["automated", "fault_injection"] },
    { "id": "AC-SC-06", "components": ["frontend.features", "backend.services", "backend.shortcut"], "requiredEvidence": ["automated", "windows_webview2"] },
    { "id": "AC-SC-07", "components": ["backend.shortcut", "backend.repositories", "platform.windows"], "requiredEvidence": ["restart_persistence", "runtime_hook"] },
    { "id": "AC-SC-08", "components": ["backend.shortcut", "platform.windows"], "requiredEvidence": ["external_app_interop", "runtime_hook"] }
  ],
  "evidence": [
    {
      "id": "EV-SC-7026768",
      "acceptanceIds": ["AC-SC-01", "AC-SC-02", "AC-SC-03"],
      "acceptanceCoverage": [
        { "acceptanceId": "AC-SC-01", "coverage": "partial" },
        { "acceptanceId": "AC-SC-02", "coverage": "partial" },
        { "acceptanceId": "AC-SC-03", "coverage": "partial" }
      ],
      "method": "automated",
      "result": "pass",
      "freshness": "potentially_stale",
      "capabilities": ["automated"],
      "scope": "Frontend DOM and controller checks for immediate capture, physical-key display and cancellation",
      "testRefs": ["src/features/shortcut/ShortcutCaptureField.test.tsx", "src/features/shortcut/useShortcutBindingController.test.tsx", "src/features/shortcut/shortcutCapture.test.ts"],
      "limitations": ["Build identity was not retained", "ShortcutManager transaction, persistence and rollback paths were not exercised", "Windows/WebView2, runtime Hook, restart persistence and external-app interoperability were not proven"],
      "sourceRevision": "702676870f2c4289c79cef24f2fe5d96777505b7",
      "worktreeState": "clean",
      "changedPaths": ["src/features/shortcut", "src-tauri/src/shortcut_manager.rs", "src-tauri/src/windows_keyboard.rs"],
      "environment": "Windows development workspace; frontend automated suite recorded, build identity not retained",
      "validatedAt": "2026-08-26"
    },
    {
      "id": "EV-SC-TRANSACTION-20260827",
      "acceptanceIds": ["AC-SC-04", "AC-SC-05"],
      "acceptanceCoverage": [
        { "acceptanceId": "AC-SC-04", "coverage": "partial" },
        { "acceptanceId": "AC-SC-05", "coverage": "partial" }
      ],
      "method": "automated",
      "result": "pass",
      "freshness": "current",
      "capabilities": ["automated", "fault_injection"],
      "scope": "EditCoordinator in-process transaction tests for runtime apply failure, persistence failure, rollback failure, revision conflict, successful commit, the disabled-state no-enable invariant, cancellation and unchanged bindings",
      "testRefs": ["shortcut_manager::coordinator::tests::runtime_apply_failure_never_commits_configuration", "shortcut_manager::coordinator::tests::persistence_failure_restores_authoritative_runtime_binding", "shortcut_manager::coordinator::tests::rollback_failure_reports_runtime_rollback_error", "shortcut_manager::coordinator::tests::revision_conflict_restores_the_new_authoritative_binding", "shortcut_manager::coordinator::tests::successful_commit_keeps_runtime_and_configuration_consistent"],
      "limitations": ["Runs against in-memory configuration, runtime and observer ports", "Does not verify the AC-SC-06 user-visible disabled-save message", "Does not start Tauri, WebView2 or a real Windows keyboard Hook", "Dirty worktree evidence has no immutable build identity and depends on all listed shortcut-related changes"],
      "sourceRevision": "dc4be390846b0a54e00cadf868db4b9c6db9686b",
      "worktreeState": "dirty",
      "changedPaths": ["src-tauri/src/shortcut_manager", "src-tauri/src/physical_shortcut.rs", "src-tauri/src/windows_keyboard.rs", "src/features/shortcut"],
      "environment": "Windows development workspace; cargo test --all-features; in-process fault injection",
      "validatedAt": "2026-08-27"
    },
    {
      "id": "EV-SC-DISABLED-MESSAGE-20260827",
      "acceptanceIds": ["AC-SC-06"],
      "acceptanceCoverage": [{ "acceptanceId": "AC-SC-06", "coverage": "partial" }],
      "method": "automated",
      "result": "pass",
      "freshness": "current",
      "capabilities": ["automated"],
      "scope": "Backend disabled-save outcome returns the next-enable message and the frontend controller keeps it as a user-visible status after a successful commit",
      "testRefs": ["shortcut_manager::coordinator::tests::disabled_commit_persists_without_enabling_or_reinstalling_hook", "src/features/shortcut/useShortcutBindingController.test.tsx: shows that a disabled shortcut was saved for the next enable"],
      "limitations": ["Frontend test runs in happy-dom rather than Windows WebView2", "Does not prove that a later real Hook enable uses the saved binding", "Dirty worktree evidence has no immutable build identity and depends on all listed shortcut-related changes"],
      "sourceRevision": "dc4be390846b0a54e00cadf868db4b9c6db9686b",
      "worktreeState": "dirty",
      "changedPaths": ["src-tauri/src/shortcut_manager", "src-tauri/src/physical_shortcut.rs", "src-tauri/src/windows_keyboard.rs", "src/features/shortcut"],
      "environment": "Windows development workspace; cargo shortcut_manager tests and Vitest shortcut controller test; happy-dom",
      "validatedAt": "2026-08-27"
    }
  ],
  "impactAssessments": []
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
| `AC-SC-08` | 与采用全局低级键盘监听的其他应用绑定同一物理键时，不误报 Zephyr 拥有系统级独占；不同启动、重装和退出顺序下的触发与吞键边界可被复现和解释 | 真实冲突应用矩阵 + Zephyr Hook generation/Pressed/Released 日志 + 必要的系统追踪 |

## 明确不规定的实现

- 本规格不要求 DOM、某种 Tauri Event 或特定 Win32 API；它只要求有焦点的编辑输入与正式全局监听故障隔离。
- 本规格不规定内部 IPC 名称、状态机类型、线程模型或日志字段。
- 本次不增加第二次确认按钮，也不扩展为一个动作绑定多个快捷键。

## 局部假设

- `ASM-SC-01`（Open）：运行时匹配快捷键后是否必须吞掉系统原有行为仍需单独产品确认；在确认前不得把“Ctrl+C 不执行复制”升级为通用验收标准。
- `ASM-SC-02`（Challenged，待确认）：Zephyr 是否必须在另一个更晚安装且会中断 Hook 链的用户态低级键盘 Hook 面前保持优先级。Windows Hook 链不提供永久所有权；若产品要求“无论启动顺序都由 Zephyr 优先触发”，需要单独确认可实现边界，不能以周期性重装 Hook 伪装成稳定保证。
- Windows/WebView2 当前能够通过有焦点输入边界提供所需的左右物理键粒度；若该条件失效，按 [ADR-0010](../architecture/adr/0010-separate-focused-shortcut-editing.md) 的复核条件重新评估。

## 架构决策

- [ADR-0010：分离有焦点的设置录入与全局运行时监听](../architecture/adr/0010-separate-focused-shortcut-editing.md)
- 文档治理遵循 [ADR-0009](../architecture/adr/0009-evidence-aware-document-governance.md)。
- 验证能力与有效新鲜度遵循 [ADR-0011](../architecture/adr/0011-capability-aware-effective-validation.md)。

## 当前实现入口

- 前端 feature：`src/features/shortcut/`
- 后端事务：`src-tauri/src/shortcut_manager/`
- Windows 运行时：`src-tauri/src/windows_keyboard.rs`
- 当前时序：[快捷键录入与提交](../architecture/runtime-views.md#快捷键录入与提交)

源码与运行配置是实现事实；本节不复制具体状态机、时序和线程细节。

## 验证状态

当前为 `partial`。修复实现位于 `702676870f2c4289c79cef24f2fe5d96777505b7`；现有前端自动化覆盖即时录入、物理候选、取消、非法重试和模拟后端失败回滚。当前脏工作树新增 `EditCoordinator` 进程内故障注入，直接覆盖运行时应用失败不持久化、持久化失败回滚、回滚失败、revision 冲突、成功提交、禁用状态不启用 Hook、cancel 和 unchanged；另有后端结果与前端控制器测试证明 disabled 保存后会显示“快捷键已保存，开启后生效。”。这些证据仍没有不可变 build identity，也没有 Windows/WebView2、真实 Hook、重启持久化或外部 Hook 互操作能力，因此不能升级为 `validated`。

用户曾报告问题已由该版本修复，但当时没有记录构建身份和工作树状态，因此只作为历史观察，不升级为 `validated` 证据。

`AC-SC-08` 当前为待验证。2026-08-26 用户在同一台 Windows 机器上观察到：Zephyr（Z）与 Typeless（T）都绑定 `RightAlt` 时，按 `T → Z` 顺序启动会同时触发两者；按 `Z → T` 顺序启动只触发 T；退出 T 后 Z 又可触发。该矩阵符合“更晚安装的 Hook 位于链首，链首可阻止后续 Hook 收到事件”的解释，也证明 Z 的 `RightAlt` 绑定并非始终失效；但尚未保存 Z 的 build/worktree、T 的版本、完整关联日志或系统级追踪，因此只记录为待验证的目标环境观察，不作为已闭环证据。

## 澄清历史

- 2026-08-26：确认点击字段即进入换绑，捕获期间完整候选实时显示；外部点击与裸 Escape 取消；合法候选自动提交；成功、失败和未变化均有明确结果。
- 2026-08-26：确认设置录入与正式全局 Hook 是两个职责平面，后端保留最终校验、事务、运行时应用和回滚权威。
- 2026-08-26：记录 Z/T 同绑 `RightAlt` 的启动顺序差异为 `AC-SC-08` 待验证观察；不据此声称 Typeless 的具体内部实现，也不把 Zephyr 的 Hook generation 重装视为跨应用永久抢占机制。
- 事故与错误架构传播见[非规范性复盘](../postmortems/shortcut-capture-misalignment.md)。
