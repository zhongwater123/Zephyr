# ADR-0014：智能成稿使用一次性整体粘贴交付

- Status: Accepted
- Date: 2026-08-28
- Deciders: Project maintainers
- Drivers: 智能成稿必须在目标输入框外完整处理后一次性交付；办公和编码请求需要保留多段纯文本；不得用 Enter 逐段输入；同时保护目标身份和剪贴板并发数据
- Related features: FEAT-SMART-DICTATION
- Assumptions: ASM-SD-06
- Evidence: 2026-08-28 用户确认“先处理好，再统一输入”；OpenWhispr `audioManager.js`、`clipboard.js` 与 `selectionManager.js` 只读技术复核
- Supersedes: ADR-0006（仅 SmartDictation 最终交付范围；本 ADR Accepted 后生效）
- Superseded by: None

## Context

ADR-0006 当前规定默认使用 Unicode `SendInput`，剪贴板只能按 EXE 显式启用。该边界避免默认触碰用户剪贴板，但不能自然表达多段文本：把 LF 映射成 Enter 可能在聊天或 AI 输入框中触发发送，把 LF 当作 Unicode code unit 也不能保证目标控件产生段落。

用户对智能成稿的最新要求是：ASR 原文、路由和 Chatbot 处理都必须在目标输入框外完成，只有一份完整、已校验的最终文本能够一次性进入输入框。用户不应为了正常粘贴一段多行纯文本而预先理解或配置 `clipboard_compatibility`。

OpenWhispr 的实现提供了可借鉴但不能照搬的证据：它先冻结处理后的完整文本，再把整段文本写入剪贴板，只发送一次 Ctrl+V，并延迟恢复原剪贴板；粘贴操作串行化，Windows 还尝试恢复录音开始时捕获的目标窗口。其生成文本的 caret delivery 会拒绝已知终端目标，因为粘贴内含换行的内容可能直接执行命令。

## Decision

如果本 ADR 被接受，SmartDictation 的最终交付采用独立的 `AtomicPaste` 语义：

1. Processing 必须先完成 JSON 解析、文本校验、换行规范化和最终文本选择。Delivery 不接收流式 token、部分 JSON 或逐段结果。
2. 对普通可编辑文本目标，SmartDictation 无需按 EXE 预先启用兼容模式。Delivery 保存完整 OLE `IDataObject` 快照，把完整纯文本一次写入剪贴板，复验并恢复捕获目标，只发送一次 Ctrl+V；绝不为文本中的 LF 生成 Enter 键事件。
3. 所有自动粘贴共享一个有界串行通道，后一个操作必须等待前一个操作完成剪贴板恢复或明确放弃恢复。
4. `PasteReceipt` 分开记录 `paste_submitted` 与 `clipboard_restoration`。Ctrl+V 已成功提交后，如果 sequence 表明用户或其他程序修改了剪贴板，系统跳过恢复并记录非阻塞异常；不得把可能已经落入目标的文本重新放入 Pending 造成重复交付。
5. 在写入剪贴板、目标复验或提交 Ctrl+V 之前失败时，文本进入既有 Pending；粘贴失败时可以保留最终纯文本供用户手动复制，但不能假装自动交付成功。
6. 已知终端、shell 或其他“粘贴换行可能执行命令”的目标不自动接收含 LF 的模型生成文本；它们进入 Pending 或只提供用户主动复制。不得仅凭窗口是 IDE 就把集成终端误判为普通编辑器。
7. ADR-0006 对非 SmartDictation 输出继续有效；是否把 AtomicPaste 扩展为全项目默认交付属于后续独立决策。

## Consequences

### Positive

- 用户获得与日常粘贴大段文本一致的体验，不需要提前配置每个普通应用。
- 聊天、办公和编码助手中的多段文本只触发一次粘贴，不会因模拟 Enter 意外发送。
- Processing 和 Delivery 之间保持一份不可变最终文本，失败兜底仍走同一交付入口。
- 剪贴板恢复状态与粘贴提交状态解耦，避免恢复竞争导致重复交付。

### Negative

- SmartDictation 会临时使用系统剪贴板，必须维护完整格式快照、sequence 检查、串行化和异常轨迹。
- Ctrl+V 提交只能证明输入事件已被系统接受，真实目标是否插入仍需外部应用互操作验证。
- 终端和未知命令输入表面需要更保守的识别与 Pending 体验。
- 与 ADR-0006 形成特性级例外，增加 Delivery 策略迁移和测试成本。

## Alternatives considered

- **LF 转 Enter：** 在聊天、AI 输入框和终端中可能发送消息或执行命令，不采用。
- **继续按 EXE 显式启用剪贴板：** 安全但把正常多行粘贴的技术配置暴露给用户，与本次产品要求冲突。
- **先用 Unicode，遇到多行再自动切换剪贴板：** 形成两套用户不可预测的交付语义，且仍需解决剪贴板事务；MVP 不采用。
- **直接照搬 OpenWhispr：** 其 Electron、多平台工具链和错误契约与本项目不同；只吸收“完整文本、单次粘贴、目标恢复、串行恢复、终端防护”的方法。

## Revisit when

- Windows UI Automation 或目标控件 API 能稳定提供不触碰剪贴板的原子多行插入；
- 实机矩阵发现某类普通输入框的 Ctrl+V 会提交而非插入；
- 能可靠识别 IDE 内编辑器与集成终端等控件级输入表面；
- 产品决定把 AtomicPaste 扩展为 RawDictation 或全项目默认 Delivery。
