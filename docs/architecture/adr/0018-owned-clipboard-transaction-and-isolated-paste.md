# ADR-0018：以自有剪贴板事务和隔离粘贴进程替代 OLE 活对象恢复

- Status: Proposed
- Date: 2026-08-31
- Deciders: Project maintainers
- Drivers: 真实 Windows AtomicPaste 重复触发主进程栈溢出；保护用户剪贴板数据；把不可逆粘贴提交、恢复竞争和原生故障边界表达清楚
- Related features: FEAT-SMART-DICTATION
- Assumptions: ASM-SD-12
- Evidence: 2026-08-28 与 2026-08-31 用户提供的 `npm run tauri dev` 崩溃日志，其中 2026-08-31 运行后观察到 revision `41b8702177692aae88cdf434f22e5c6b26577faa`；revision `180ba6d474007c9c063ac945a357dceff5a8215b` 确认 `delivery.rs`、`inject.rs` 和 finalize 危险路径随后仍未变化；OpenWhispr `clipboard.js` 与 `windows-fast-paste.c` 只读比较
- Supersedes: ADR-0014
- Superseded by: None

## Context

ADR-0014 已接受“完整定稿后一次性整体粘贴、不为 LF 模拟 Enter、目标失败关闭、提交后不自动重复交付”的用户侧目标，但同时把“保存完整 OLE `IDataObject` 快照，再通过 `OleSetClipboard` 与 `OleFlushClipboard` 恢复”写成了具体决策。

当前实现调用 `OleGetClipboard` 后只持有一个可能转发 COM/RPC 和延迟渲染的活对象，并未把所有格式复制为 Zephyr 自己拥有的不可变数据。交付写入语音文本、发送 Ctrl+V、等待 80ms，然后把该读取对象重新设为剪贴板数据源并 Flush。真实 Windows 已至少两次在 `delivery_inject` 后发生 `tokio-rt-worker` 栈溢出并以 `0xc000041d` 终止主进程；其中一次只有 13 个字符，且 provider final、relay、aggregate 和 delivery payload 的长度与哈希完全一致。该结果否定了目标环境完成声明，但现有日志尚不足以把具体栈溢出指令断言为 `OleSetClipboard` 或 `OleFlushClipboard`。

OpenWhispr 提供了较安全的方向：它把有限的文本、HTML、RTF 和图片格式按值读入 Electron 主进程，串行等待恢复完成，并让独立 Windows helper 发送粘贴快捷键。它仍有格式白名单不完整、固定延迟、只比较文本以及目标恢复失败后可能粘贴到当前窗口等限制，不能直接复制。

## Decision

如果本 ADR 被接受，保留 ADR-0014 的“完整文本、单次粘贴、不模拟 Enter、终端失败关闭”产品语义，但替代其 OLE 活对象恢复和布尔提交回执：

1. `ClipboardTransactionService` 是自动交付期间剪贴板事务的唯一写入者。一次事务从原数据捕获开始，跨越载荷写入、粘贴提交、延迟消费窗口和恢复/明确放弃恢复；后续新会话交付与 Pending 重新交付必须等待当前事务达到终态。
2. 原剪贴板只能保存为 Zephyr 自己拥有的数据。实现按明确格式白名单深复制文本、HTML、RTF、图片、文件列表及经验证可安全重建的格式，不得把 `OleGetClipboard` 返回的活 `IDataObject`、远程代理或延迟渲染提供者称为完整快照。
3. 捕获必须枚举格式并报告 `Complete` 或 `UnsupportedFormats`。发现任何无法安全复制且覆盖后可能丢失的数据时，事务必须在写入前失败关闭：可以使用不触碰剪贴板的安全注入策略，或把文本转入 Pending 供用户主动交付；不得静默丢弃原格式。
4. 写入语音文本时同时写入进程私有的事务 ID，并记录 sequence 与载荷指纹。恢复前必须同时确认事务 ID、sequence 和载荷仍属于本次事务；用户或其他程序已经修改剪贴板时只跳过恢复并记录非内容诊断，不得覆盖竞争写入。
5. Windows 按键注入迁移到可终止、带超时的独立 `PasteHelper` 进程。Helper 接收捕获目标的 HWND、PID、进程创建时间和预期粘贴类型，在发送按键前重新验证目标；目标不存在、不匹配或无法成为允许的前台目标时返回 `NotSubmitted`，绝不得退化为向当前前台窗口粘贴。普通目标发送一次 Ctrl+V；明确允许的终端单行场景可使用独立策略，含 LF 的命令表面继续失败关闭。
6. Delivery 回执使用 `NotSubmitted | Submitted | Unknown`，并把剪贴板恢复结果作为独立字段。只有可证明按键尚未提交的 `NotSubmitted` 才能自动进入可重试 Pending；Helper 在提交附近崩溃、超时或失联形成 `Unknown`，不得自动重放可能已经落入目标的文本，而应保留可恢复内容并明确提示交付状态不确定。
7. `SendInput` 成功只表示事件已提交，不表示目标控件已经消费文本。代码、日志、History 和 Incident 不得把 `Submitted` 表述为目标已插入；真正的外部应用互操作仍需目标环境验证。
8. 日志按事务阶段记录 `snapshot_complete`、`payload_written`、`target_verified`、`paste_not_submitted|submitted|unknown`、`restore_started|restored|skipped|failed`，只携带事务 ID、格式类别、长度、指纹、错误码和耗时；默认不记录剪贴板内容。

### 分阶段迁移门禁

1. **Phase 0 — 止血：** 从 SmartDictation 成功路径停用 OLE 活对象恢复。安全替代尚未可用时，使用不触碰剪贴板的注入策略或 Pending；不得继续要求用户在会杀死进程的构建上重复测试。
2. **Phase 1 — 自有快照与事务：** 实现格式枚举、按值快照、事务 ID、恢复竞争检查和跨延迟窗口的单一串行所有权；对不支持格式在覆盖前失败关闭。
3. **Phase 2 — 原生故障隔离：** 引入并打包 Windows PasteHelper，建立严格目标身份校验、修饰键处理、结构化退出码、超时和 `Unknown` 提交语义。Helper 崩溃不得终止 Tauri 主进程。
4. **Phase 3 — 目标环境验证：** 在真实 Windows 上覆盖空剪贴板、纯文本、HTML、RTF、图片、文件、自定义/延迟渲染格式、用户并发复制、Helper 崩溃、目标切换及连续交付；验证记事本、浏览器/WebView2、Office、VS Code/编码助手和终端。只有全部关键验收有当前 clean revision 与工件证据后，Dossier 才能从 `invalidated` 重新升级。

## Consequences

### Positive

- 不再把外部活对象伪装成不可变快照，剪贴板数据所有权和恢复范围可以测试与审计。
- 原生按键注入故障被限制在辅助进程，单次交付不能直接杀死主应用。
- 单一事务所有者覆盖延迟恢复窗口，避免新会话、Pending 和用户剪贴板恢复互相覆盖。
- 三态提交回执阻止“辅助进程失联后自动重放”造成重复文本。
- 保留完整定稿后一次性整体粘贴和终端失败关闭的用户目标。

### Negative

- Windows helper 增加构建、签名、打包、版本协商和进程清理成本。
- 任意 Windows 剪贴板格式无法被低成本完整复制；白名单外数据会降低自动粘贴可用率，必须提供清楚的安全降级体验。
- 恢复仍缺少通用的目标控件消费确认；固定延迟只能作为受测策略，不能升级为“原子插入”证明。
- 迁移期间需要同时维护旧 Pending/History 契约与新的三态提交语义。

## Alternatives considered

- **继续 OLE 活对象恢复并增加栈大小、等待或重试：** 两次相同用户可见崩溃已经要求重建真实因果链；这些补偿既不建立数据所有权，也可能扩大重复提交和数据丢失，不采用。
- **完全照搬 OpenWhispr：** 其值快照、恢复队列和 helper 隔离值得吸收，但格式白名单不完整、文本相等检查不足，且目标恢复失败后可能粘贴到当前窗口；不直接复制。
- **永久只保存纯文本：** 实现简单，但会静默破坏图片、文件和富格式剪贴板，不作为默认策略。它只能在已经证明原剪贴板仅含受支持文本时使用。
- **所有 SmartDictation 永久改为 Unicode SendInput：** 不触碰剪贴板，但多行文本和不同目标控件语义不可预测，也违背一次性整体粘贴目标；只允许作为有能力判断的临时安全降级。
- **在 Tauri 主进程继续执行所有原生操作：** 即使改用自有快照，SendInput、Hook 和平台 API 的未处理原生故障仍能终止主进程；不采用为最终边界。

## Revisit when

- Windows UI Automation、Text Services Framework 或目标控件 API 能为支持矩阵提供不触碰剪贴板的可靠多行原子插入与消费回执；
- 真实企业工作负载中的白名单外格式比例使失败关闭无法满足可用性目标；
- Helper 的签名、升级或安全审计成本高于它带来的故障隔离收益；
- 产品决定把相同剪贴板事务扩展到 RawDictation、手动 Pending 或其他平台。
