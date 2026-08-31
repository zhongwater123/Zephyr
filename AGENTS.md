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

## Context budget

1. Local changes that do not alter user-visible behavior or a public boundary read this file, the affected code, and tests only.
2. A cross-component or high-risk task normally reads one primary Feature Dossier. Do not recursively open every linked C4, Runtime View, ADR, evidence entry, or history item.
3. Open one relevant Current View only when component responsibility, dependency, external boundary, or critical timing changes. Open an ADR only when its decision boundary may be changed or violated.
4. `npm run architecture:impact` is a routing aid, not a checklist requiring every reported document to be read or edited.

## Parallel agent delivery

1. Use one short-lived task branch and one independent Git worktree per editing Agent. Only one Agent writes to a given worktree at a time.
2. A branch represents isolated implementation work, not the canonical project status. Issue/PR state, CI, Dossier validation status, and release tags carry project status.
3. Development Agents may change code, tests, and an explicitly affected MVP contract. They may report evidence or deviations, but must not self-promote a Dossier to `validated`, a Current View to `reviewed`, or an ADR from Proposed to Accepted.
4. A designated integration/documentation owner performs those promotions only after comparing the merged clean revision with the relevant contract, view, decision, and target-environment evidence.
5. Do not create per-task documentation ledgers. Use the Issue/PR for change summaries and ordinary defects; create durable documentation records only for a product contract, long-lived architecture decision, persistent cross-PR deviation, or validation promotion.

Detailed document roles and maintenance rules are in [the feature documentation guide](docs/features/README.md) and [the architecture maintenance guide](docs/architecture/maintenance.md).
