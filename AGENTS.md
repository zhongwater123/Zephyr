# Repository Agent Rules

## Windows PowerShell encoding

Codex may run Windows PowerShell without the user profile. Terminal mojibake does not imply source corruption. Before diagnosing encoding problems, load the profile with `. $PROFILE` and re-check the file as UTF-8.

## Product alignment

1. Before changing a cross-component or high-risk user feature, read its dossier under `docs/features/`.
2. A user's latest explicit requirement may directly revise a Feature Dossier's MVP contract. Stop the conflicting implementation and report the impact only when the change is unclear, crosses a safety/data-integrity/external-commitment boundary, or overturns a costly-to-reverse Accepted ADR decision. In that case, update the affected specification and decision record before continuing.
3. Architecture proposals are not current implementation facts. Only documents marked `current` may be used as current C4 or Runtime View evidence.
4. If the same user-visible problem remains unchanged after two attempted fixes, stop adding retries, polling, states, or compensation. Rebuild the end-to-end causal chain from the real input boundary.
5. A target-environment failure invalidates a completion claim even when unit or integration tests pass. It does not, by itself, prove a root cause.
6. Do not claim a feature is fully validated when its dossier is `unverified`, `partial`, or `invalidated`.
7. For a cross-component or high-risk feature, at task start, after context compaction, and before declaring completion, re-read the relevant Dossier's `用户目标`, `验收场景`, `明确不规定的实现`, open assumptions, and validation status. Treat `局部假设`, implementation details, evidence, and history as context rather than implementation constraints unless they are explicitly confirmed requirements or hard boundaries. Open C4, Runtime View, or ADR material only when the planned change reaches the component, timing, or boundary it describes.

Detailed document roles and maintenance rules are in [the feature documentation guide](docs/features/README.md) and [the architecture maintenance guide](docs/architecture/maintenance.md).
