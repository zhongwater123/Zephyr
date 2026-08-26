# Repository Agent Rules

## Windows PowerShell encoding

Codex may run Windows PowerShell without the user profile. Terminal mojibake does not imply source corruption. Before diagnosing encoding problems, load the profile with `. $PROFILE` and re-check the file as UTF-8.

## Product alignment

1. Before changing a cross-component or high-risk user feature, read its dossier under `docs/features/`.
2. If a confirmed user requirement conflicts with a Feature Dossier or Accepted ADR, stop the conflicting implementation, report the impact, then update the specification and decision record before continuing.
3. Architecture proposals are not current implementation facts. Only documents marked `current` may be used as current C4 or Runtime View evidence.
4. If the same user-visible problem remains unchanged after two attempted fixes, stop adding retries, polling, states, or compensation. Rebuild the end-to-end causal chain from the real input boundary.
5. A target-environment failure invalidates a completion claim even when unit or integration tests pass. It does not, by itself, prove a root cause.
6. Do not claim a feature is fully validated when its dossier is `unverified`, `partial`, or `invalidated`.
7. After context compaction, at the start of a new task, and before declaring completion, re-read the relevant Feature Dossier, open assumptions, and validation status.

Detailed document roles and maintenance rules are in [the feature documentation guide](docs/features/README.md) and [the architecture maintenance guide](docs/architecture/maintenance.md).
