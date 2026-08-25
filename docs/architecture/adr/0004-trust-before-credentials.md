# ADR-0004：自定义 origin 授权先于凭据读取

- Status: Accepted
- Date: 2026-08-24
- Deciders: Project maintainers
- Supersedes: None
- Superseded by: None

## Context

产品支持自定义 ASR 与 DeepSeek-compatible endpoint。若 WebView 可自行标记已授权，或代码先读取 Keyring 再检查 endpoint，恶意/错误配置可能把秘密发送到未确认主机。ASR 和热词 Agent 即使同源也代表不同数据用途。

## Decision

授权键绑定 `scheme + host + effective port + purpose`。官方 ASR 和 Agent origin 默认信任；自定义 origin 首次使用凭据前，由 Rust 弹出带父窗口的 Windows 原生确认。所有测试、录音和热词整理路径必须先检查 trust，再读取 CredentialStore。撤销立即使 endpoint 不可用，但不删除 Keyring 中的秘密。

ASR 与 Hotword Agent 使用不同 purpose，授权互不替代。生产连接要求安全协议：ASR 使用 `wss://`，Agent 使用 HTTPS 语义。

## Consequences

### Positive

- WebView 不能伪造原生授权结果。
- 未授权路径可通过 mock CredentialStore 验证为零秘密读取。
- 撤销不破坏用户凭据，重新授权后可继续使用。

### Negative

- 自定义主机首次配置多一个原生确认步骤。
- endpoint 规范化和默认端口必须保持一致。
- 每个新增外部用途都需要独立 purpose 与 UI 管理。

## Alternatives considered

- 仅依赖 TLS：只能保护传输，不能确认目标主机是用户期望的。
- 按 host 授权：忽略 scheme、port 和 purpose，边界过宽。
- 授权时删除/复制凭据：增加秘密生命周期和恢复复杂度。

## Revisit when

引入 OAuth、每 endpoint 独立秘密、多租户配置或证书固定策略时重新评估。
