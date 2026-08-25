# ADR-0005：采用 revision CAS、原子 JSON、SQLite 与 Credential Manager

- Status: Accepted
- Date: 2026-08-24
- Deciders: Project maintainers
- Supersedes: None
- Superseded by: None

## Context

配置可能被多个异步 UI mutation 更新，迟到响应不能覆盖新状态；进程或磁盘错误不能留下半写 JSON。历史与热词需要查询和事务，而 API keys 不应进入普通配置文件。配置和凭据更新还存在跨存储一致性问题。

## Decision

非秘密配置保存在 OS app config 目录的 `config.json`，包含 schema version 与单调 revision。所有 mutation 携带 expected revision，由单所有者 `ConfigService` 串行化并执行 CAS。保存使用同目录临时文件、flush、`sync_all` 与 Windows 原子替换，并保留最后一份验证通过的 `.bak`。

历史和热词共享 `history.db`，启用 SQLite WAL、NORMAL synchronous 和 3 秒 busy timeout。秘密存入 Windows Credential Manager。跨配置/凭据 mutation 先快照并更新凭据；JSON 保存失败则恢复快照。

## Consequences

### Positive

- 迟到 UI 响应和并发 mutation 可检测冲突。
- 写盘中断后可回退最后有效配置；主备都坏时安全禁用。
- 数据类型选择与访问模式匹配，秘密不落普通 JSON/SQLite。

### Negative

- JSON 与 Keyring 之间是补偿事务，不是单一 ACID 事务。
- SQLite history/hotword 适配器共享文件，需要 schema 协调。
- schema migration、备份和 revision 都需要契约测试。

## Alternatives considered

- 所有数据放一个 SQLite：秘密保护不如 Credential Manager，配置人工检查也更困难。
- last-write-wins JSON：迟到 mutation 可能覆盖新设置。
- 直接覆盖配置文件：崩溃时可能产生截断文件。

## Revisit when

需要多设备同步、多进程并发写入、强跨存储事务或大规模历史数据时重新评估。
