# Architecture Decision Records

ADR 记录已经影响代码边界、数据安全或演进成本的决策。Accepted ADR 不因实现调整而改写历史结论；若决策改变，新增 ADR 并建立替代关系。

| 编号 | 状态 | 决策 |
| --- | --- | --- |
| [ADR-0001](0001-tauri-local-desktop-boundary.md) | Accepted | Tauri 本地桌面边界，不建立本地 HTTP 控制面 |
| [ADR-0002](0002-single-owner-bounded-voice-session.md) | Accepted | 单所有者、有界、失败关闭的语音会话 |
| [ADR-0003](0003-delivery-commit-point-and-pending-output.md) | Accepted | 注入成功作为提交点，失败进入内存 Pending |
| [ADR-0004](0004-trust-before-credentials.md) | Accepted | 自定义 origin 授权必须先于凭据读取 |
| [ADR-0005](0005-revisioned-atomic-local-storage.md) | Accepted | revision CAS、原子 JSON、SQLite 与 Credential Manager |
| [ADR-0006](0006-unicode-injection-default.md) | Accepted | Unicode SendInput 默认，剪贴板按应用显式兼容 |
| [ADR-0007](0007-architecture-docs-as-code.md) | Accepted | C4 + arc42-Lean + ADR + 机器可读代码地图 |
| [ADR-0008](0008-incident-vault-isolated-recovery.md) | Accepted | 产品前端融合、后端隔离的本地异常恢复 |
| [ADR-0009](0009-evidence-aware-document-governance.md) | Accepted | 按材料角色、状态和带版本证据治理文档 |
| [ADR-0010](0010-separate-focused-shortcut-editing.md) | Accepted | 分离有焦点的设置录入与全局运行时监听 |
| [ADR-0011](0011-capability-aware-effective-validation.md) | Accepted | 按验收能力计算有效验证状态并隔离非规范性实现指南 |
| [ADR-0012](0012-unified-voice-input-control-plane.md) | Accepted | 统一语音输入控制面所有权与触发端口 |
| [ADR-0013](0013-strict-mailbox-owned-voice-runtime.md) | Accepted | 严格 mailbox-owned 语音运行时与控制/执行分层 |

## 状态
| [ADR-0014](0014-atomic-smart-dictation-paste.md) | Accepted | 智能成稿先完整定稿，再通过一次性整体粘贴交付 |
| [ADR-0015](0015-internal-shared-deepseek-credential-and-isolated-prompts.md) | Superseded | 内部分发共享 DeepSeek 凭据，并以独立文件隔离写作画像 Prompt |
| [ADR-0016](0016-deterministic-mvp-routing-and-deepseek-flash.md) | Superseded | Router 使用用户覆盖 > 内置 EXE 分类 > general，文本处理默认 DeepSeek Flash |
| [ADR-0017](0017-unified-app-aware-polishing-with-strength.md) | Accepted | 单一 Prompt 接收应用上下文与三档强度，由模型自主完成场景化润色 |
| [ADR-0018](0018-owned-clipboard-transaction-and-isolated-paste.md) | Proposed | 以自有剪贴板事务和隔离粘贴进程替代 OLE 活对象恢复 |

- **Proposed**：已提出，尚未承诺。
- **Accepted**：当前实现和演进应遵循。
- **Rejected**：评估后不采用，保留原因。
- **Deprecated**：仍有历史价值，但不再建议用于新代码。
- **Superseded**：已被后续 ADR 替代。

新增记录请复制 [模板](template.md)，并同时更新本索引与 [code-map.json](../code-map.json)。
