# ADR-0006：默认 Unicode SendInput，剪贴板按应用显式兼容

- Status: Accepted
- Date: 2026-08-24
- Deciders: Project maintainers
- Supersedes: None
- Superseded by: None

## Context

用剪贴板粘贴识别文本会临时覆盖文件、图片、富文本或自定义格式；快照不完整或用户同时修改剪贴板时恢复可能破坏数据。另一方面，少数应用不接受 Unicode `SendInput`，且 UIPI 可能阻止输入。

## Decision

默认使用 `KEYEVENTF_UNICODE` 按 UTF-16 一次 `SendInput` 整段文本，并核对实际事件数。部分写入、UIPI 或目标拒绝时不自动回退剪贴板，而是生成 Pending。

剪贴板兼容模式只能按可执行文件显式启用，并仍执行完整目标验证。兼容模式使用 OLE `IDataObject` 保存完整快照并记录 clipboard sequence；若无法完整快照，或粘贴后 sequence 已被用户改变，则拒绝自动恢复/执行相应危险动作。用户点击“复制文本”属于主动替换，不恢复旧剪贴板。

## Consequences

### Positive

- 默认路径不改变剪贴板，支持 UTF-16 代理对。
- 兼容降级是用户可见、按应用收窄的策略。
- 输入失败不会触发未经授权的副作用链。

### Negative

- 部分高完整性或特殊控件只能进入 Pending。
- OLE 完整快照比纯文本剪贴板实现复杂。
- 按 EXE 策略不能区分同一程序内不同控件。

## Alternatives considered

- 总是 Ctrl+V：兼容广但默认破坏剪贴板风险过高。
- SendInput 失败自动回退：用户未明确授权剪贴板副作用。
- UI Automation：可能提供语义输入，但目标兼容、权限和实现成本更高。

## Revisit when

Windows 提供更可靠的文本插入 API，或产品需要按窗口类/控件粒度配置时重新评估。
