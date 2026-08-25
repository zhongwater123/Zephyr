# ADR-0003：以成功注入为提交点，失败进入内存 Pending

- Status: Accepted
- Date: 2026-08-24
- Deciders: Project maintainers
- Supersedes: None
- Superseded by: None

## Context

识别期间用户可能切换窗口，HWND/PID 可能失效或复用，final 文本也可能包含不允许的控制/双向字符。提前写历史或学习热词会把没有真正交付的文本当成成功；误写另一个窗口则是不可接受的副作用。

## Decision

`DeliveryService` 统一执行文本验证、目标身份与前台复验、注入以及成功后的副作用。自动注入要求捕获的 HWND 仍为前台，PID、进程创建时间和 EXE 保持一致。成功注入是提交点；之后才写历史并触发热词整理。

验证或注入失败时不写目标、历史或热词，结果进入内存 Pending 队列。队列最多 5 条、TTL 10 分钟；满时拒绝新录音。用户可重新验证并发送到原窗口、主动复制或丢弃。

## Consequences

### Positive

- 文本交付与历史/学习状态具有明确一致性边界。
- 焦点变化或 UIPI 拒绝不会把文本写入错误目标。
- 用户仍可恢复未自动交付的识别结果。

### Negative

- Pending 退出即丢失，不能作为持久草稿。
- 注入成功后无法可靠回滚目标应用中的文本。
- 手动发送必须再次激活和复验窗口，流程更严格。

## Alternatives considered

- 识别完成即写历史：会记录未交付或无效文本。
- 目标变化时粘贴到当前窗口：风险不可接受。
- Pending 持久化：增加敏感文本驻留和清理义务，本轮不采用。

## Revisit when

用户明确需要跨重启草稿恢复，或引入可事务化的目标编辑 API 时重新评估。
