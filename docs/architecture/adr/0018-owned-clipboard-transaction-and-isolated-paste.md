# ADR-0018：以自有剪贴板事务和隔离粘贴进程替代 OLE 活对象恢复

- Status: Proposed
- Date: 2026-08-31
- Deciders: Project maintainers
- Drivers: 真实 Windows AtomicPaste 重复触发主进程栈溢出；保护用户剪贴板数据；把剪贴板清空、载荷发布、不可逆粘贴提交、恢复竞争和原生故障边界表达清楚
- Related features: FEAT-SMART-DICTATION
- Assumptions: ASM-SD-12
- Evidence: 2026-08-28 与 2026-08-31 用户提供的 `npm run tauri dev` 崩溃日志，其中 2026-08-31 运行后观察到 revision `41b8702177692aae88cdf434f22e5c6b26577faa`；revision `180ba6d474007c9c063ac945a357dceff5a8215b` 确认 `delivery.rs`、`inject.rs` 和 finalize 危险路径随后仍未变化；OpenWhispr `clipboard.js` 与 `windows-fast-paste.c` 只读比较
- Supersedes: ADR-0014
- Superseded by: None

## Context

ADR-0014 已接受“完整定稿后一次性整体粘贴、不为 LF 模拟 Enter、目标失败关闭、提交后不自动重复交付”的用户侧目标，但同时把“保存完整 OLE `IDataObject` 快照，再通过 `OleSetClipboard` 与 `OleFlushClipboard` 恢复”写成了具体决策。

被 revision `f93bc4d` 替换前，生产实现调用 `OleGetClipboard` 后只持有一个可能转发 COM/RPC 和延迟渲染的活对象，并未把所有格式复制为 Zephyr 自己拥有的不可变数据。交付写入语音文本、发送 Ctrl+V、等待 80ms，然后把该读取对象重新设为剪贴板数据源并 Flush。真实 Windows 已至少两次在 `delivery_inject` 后发生 `tokio-rt-worker` 栈溢出并以 `0xc000041d` 终止主进程；其中一次只有 13 个字符，且 provider final、relay、aggregate 和 delivery payload 的长度与哈希完全一致。该结果否定了目标环境完成声明，但现有日志尚不足以把具体栈溢出指令断言为 `OleSetClipboard` 或 `OleFlushClipboard`。

OpenWhispr 提供了较安全的方向：它把有限的文本、HTML、RTF 和图片格式按值读入 Electron 主进程，串行等待恢复完成，并让独立 Windows helper 发送粘贴快捷键。它仍有格式白名单不完整、固定延迟、只比较文本以及目标恢复失败后可能粘贴到当前窗口等限制，不能直接复制。

### 未验证的实现进度

revision `3ea6d9f` 已实现本 ADR 描述的共享协议 crate、单例事务 service、独立 Windows helper、DPAPI 快照、恢复竞争保护、三态仲裁与 Tauri sidecar；revision `ce04cfb` 增加实际 NSIS 的 helper 架构、协议、自检、运行目录哈希和发布清单门禁。自动交付主进程源码不再调用 OLE、Win32 剪贴板写入或 `SendInput`。这只是实现和开发打包事实，不改变本 ADR 的 `Proposed` 状态：尚无安装后的完整真实 Windows 应用/格式/强杀矩阵，Dossier 仍为 `invalidated`。

2026-09-01，基于 revision `a69242240d7da4e3d4f086b61548bfa019f93bdf` 的脏工作树修复了注册格式在检查实际数据载体前被名称名单直接拒绝的问题，并只允许 SmartDictation 单行在捕获明确失败、尚未覆盖剪贴板时改用 Unicode；Legacy 兼容语义保持失败关闭。Windows 打包前检查和自动化通过，用户随后报告暂未复现阻塞。该结果没有 clean revision、安装包身份、受控恢复比对或完整目标矩阵，只是实现进度和冒烟反馈，不构成接受本 ADR 或恢复 Dossier 验证状态的依据。

## Decision

如果本 ADR 被接受，保留 ADR-0014 的“完整文本、单次粘贴、不模拟 Enter、终端失败关闭”产品语义，但替代其 OLE 活对象恢复、进程内原生事务和布尔提交回执：

1. Bootstrap 创建唯一的 `ClipboardTransactionService`。它串行化首次自动交付和 Pending 重新交付；锁从原数据捕获开始，跨越载荷发布、目标复验、粘贴提交、提交后的剪贴板保留窗口、恢复以及一次受控故障恢复，直到事务达到终态。用户明确点击“复制文本”产生的普通剪贴板替换不属于自动交付事务；如果它与事务竞争，恢复检查必须把它视为外部修改并放弃恢复。
2. 独立、可终止、带超时的 Windows `PasteHelper` 拥有整个自动剪贴板事务和 `SendInput`，而不是只拥有 Ctrl+V。Tauri 主进程只负责协议、事务互斥、超时、子进程终止和回执仲裁；主进程自动交付代码不得调用 OLE/Win32 剪贴板读写或 `SendInput`。主进程已有的目标捕获可以作为早期失败关闭，但 helper 在不可逆写入和发送按键前都必须用 HWND、PID、进程创建时间和 EXE 重新验证原目标。
3. 原剪贴板只能保存为 Zephyr 自己拥有的数据。Helper 枚举并同步深复制文本、HTML、RTF、DIB/DIBV5/PNG、文件列表及经验证可安全重建的有界 `HGLOBAL` 格式。动态注册格式的名称不是安全边界：即使名称不在内置解析集合，只要当前实例能够被同步物化、`GlobalSize` 有界、`GlobalLock` 成功并复制到应用自有内存，就以“注册名称 + 不透明原始字节”保存；不得仅因名称未知拒绝。不得把 `OleGetClipboard` 返回的活 `IDataObject`、远程代理或延迟渲染提供者称为完整快照。已知结构格式仍必须验证内部长度、终止符和整数边界，不能把“可以取得一个句柄”等同于“可以安全重建”。
4. 捕获报告 `Complete` 或 `UnsupportedFormats`。发现延迟渲染、OwnerDisplay、无法物化或锁定的非 `HGLOBAL`、私有句柄、单格式超过 64 MiB、总计超过 128 MiB，或任何覆盖后可能丢失且无法安全复制的数据时，事务必须在写入前失败关闭：单行文本优先使用 helper 内不触碰剪贴板的 Unicode 安全注入，多行或该策略也失败时再转入 Pending；不得静默丢弃原格式，也不得在提交状态不确定时尝试另一种交付方式。
5. 快照必须在首次不可逆剪贴板修改前，以当前用户 DPAPI 加密并原子写入受限应用数据目录。文件名只接受本事务 UUID；快照包含协议版本、格式清单、捕获 sequence 和完整性信息。正常终态删除快照，应用启动时只清理过期、可验证属于 Zephyr 的快照。快照内容和格式元数据不得进入 Prompt、History 或普通日志。
6. Clipboard 发布不是一个可假定原子的 API。Helper 必须为 `EmptyClipboard`、事务标记写入、每个 `SetClipboardData` 和关闭剪贴板建立单调事务阶段，并在每个阶段支持故障注入。事务标记应尽可能在清空后首先发布，但“当前剪贴板没有标记”不能单独证明本事务尚未修改剪贴板；清空后、标记前崩溃必须由加密快照、持久阶段和 sequence 共同仲裁。无法证明恢复不会覆盖用户并发写入时不得自动恢复，并必须记录数据完整性事件。
7. 完整载荷写入时同时发布私有事务 ID，并记录 sequence 与载荷指纹。提交后的恢复必须同时确认事务 ID、sequence 和载荷仍属于本次事务；用户或其他程序已经修改剪贴板时只跳过恢复并记录非内容诊断，不得覆盖竞争写入。恢复 helper 只恢复原数据，永远不发送粘贴按键；同一事务最多自动启动一次恢复 helper。
8. Helper 在发送按键前再次确认捕获 HWND 仍是允许的前台目标；目标不存在、不匹配或无法成为允许的前台目标时返回 `NotSubmitted`，绝不得退化为向任意当前前台窗口粘贴。普通目标发送一次 Ctrl+V；明确允许的终端单行场景可使用独立策略，含 LF 的命令表面继续失败关闭。
9. Delivery 回执使用 `NotSubmitted | Submitted | Unknown`，并把剪贴板恢复结果作为独立字段。状态只允许单调推进：观察到 `paste_submitted` 后，即使 helper 在恢复阶段崩溃，也保持 `Submitted`；进入 `paste_submitting` 后失联、超时或发生部分 `SendInput` 是 `Unknown`；只有可证明没有输入事件被提交的结果才是 `NotSubmitted`。该规则同时适用于 Phase 0 的 Unicode 安全降级和最终 helper。只有 `Submitted` 提交 History；`Unknown` 保留为明确提示“可能已经输入”的 Pending，未经用户再次明确确认不得重发。
10. `SendInput` 成功只表示事件已提交，不表示目标控件已经消费文本。提交后的默认 500ms 只能称为“剪贴板载荷保留窗口”，不能称为消费确认或原子插入证明；代码、日志、History 和 Incident 不得把 `Submitted` 表述为目标已插入。真正的外部应用互操作仍需目标环境验证。
11. Helper 使用版本化 stdin 请求和逐阶段 NDJSON stdout，正文不进入命令行。阶段至少包括 `snapshot_complete`、`payload_write_started`、`payload_written`、`target_verified`、`paste_submitting`、`paste_submitted`、`restore_started` 和一个终态；默认日志只携带事务 ID、格式类别、长度、指纹、错误码和耗时。总超时为 3 秒，剪贴板占用重试预算最多 250ms；超时后主进程终止并回收 helper，再按最后一个可信阶段仲裁提交状态和是否启动一次 recover。
12. Helper 使用独立 crate 和不含 Win32 实现的共享协议 crate，通过 Tauri `externalBin` 打包。开发与发布脚本必须先构建带目标 triple 后缀的 helper；打包门禁验证 helper 存在、协议版本匹配、安装包内可执行，并把 helper 版本和哈希纳入发布工件追踪。Helper 缺失、版本不匹配或自检失败时只回退到 Phase 0 安全模式，绝不重新启用旧 OLE 路径。

### 分阶段迁移门禁

1. **Phase 0 — 止血：** 从 SmartDictation 首次交付、Pending 重新交付和 Legacy `clipboard_compatibility` 全部生产路径停用 OLE 活对象恢复。SmartDictation 单行可临时使用 Unicode 注入，多行进入原因码稳定的 Pending；旧兼容配置返回安全错误。Unicode 的零事件、部分事件和完整事件分别映射 `NotSubmitted`、`Unknown` 和 `Submitted`。该阶段作为独立可回滚提交，必须先在真实 Windows 上证明长短文本、Pending 操作和旧配置都不再导致主进程退出。
2. **Phase 1 — 隔离剪贴板事务：** 一次性实现共享协议、完整 helper、自有格式快照、DPAPI 快照、事务阶段、事务标记、三态提交、恢复竞争检查和跨提交后保留窗口的单一串行所有权。所有自动 Win32 剪贴板操作和 `SendInput` 均在 helper；对不支持格式在覆盖前失败关闭，对清空/逐格式写入/提交/恢复的每个阶段注入退出与超时。
3. **Phase 2 — 构建与发布闭环：** 把 helper 接入开发脚本、Tauri `externalBin`、权限、版本协商、安装包检查和发布清单；验证缺失、篡改、版本不匹配、自检失败和强杀 helper 时主进程存活且不会回退旧路径。
4. **Phase 3 — 目标环境验证：** 在真实 Windows 安装包上覆盖空剪贴板、纯文本、HTML、RTF、图片、文件、自定义/延迟渲染格式、用户并发复制、Helper 崩溃、目标切换及连续交付；验证记事本、浏览器/WebView2、Office、VS Code/编码助手和终端。只有全部关键验收有当前 clean revision 与工件证据后，指定收口者才能升级 Dossier 或接受本 ADR。

## Consequences

### Positive

- 不再把外部活对象伪装成不可变快照，剪贴板数据所有权和恢复范围可以测试与审计。
- 自动剪贴板解析、发布、恢复和按键注入故障都被限制在辅助进程，单次交付不能直接杀死主应用。
- 单一事务所有者覆盖延迟恢复窗口，避免新会话、Pending 和用户剪贴板恢复互相覆盖。
- 三态提交回执阻止“辅助进程失联后自动重放”造成重复文本。
- 保留完整定稿后一次性整体粘贴和终端失败关闭的用户目标。

### Negative

- Windows helper 增加构建、签名、打包、版本协商和进程清理成本。
- 任意 Windows 剪贴板格式无法被低成本完整复制；动态注册且可证明为有界 `HGLOBAL` 的格式可以作为不透明字节恢复，其余非内存句柄、私有 owner 协议或无法物化的数据仍会降低自动粘贴可用率，必须提供清楚的安全降级体验。
- `GlobalSize`、`GlobalLock` 和字节复制只能证明当前实例可以被应用拥有和重新发布，不能证明其内部语义不包含已经失效的源进程 token、句柄或其他 owner 关联。未知注册格式的通用值恢复是可用性优先的候选策略，仍需要受控 round-trip 和真实消费者矩阵；发现稳定的 owner 关联格式时应增加专用适配或显式策略，而不是继续扩大无条件信任。
- Windows 剪贴板没有跨 `EmptyClipboard` 与多次 `SetClipboardData` 的通用原子提交；helper 崩溃时必须在“恢复原数据”和“不得覆盖用户并发复制”之间保守仲裁，仍可能只能报告数据完整性事件而不能自动恢复。
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
