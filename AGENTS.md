# Repository Agent Rules

## Windows PowerShell encoding

Codex may run Windows PowerShell without the user profile. Terminal mojibake does not imply source corruption. Before diagnosing encoding problems, load the profile with `. $PROFILE` and re-check the file as UTF-8.

## Product alignment

1. Before changing a cross-component or high-risk user feature, read its dossier under `docs/features/`.
2. A user's latest explicit requirement may directly revise a Feature Dossier's MVP contract. Stop the conflicting implementation and report the impact only when the change is unclear, crosses a safety/data-integrity/external-commitment boundary, or overturns a costly-to-reverse Accepted ADR decision. In that case, update the affected specification and decision record before continuing.
3. Architecture proposals are not current implementation facts. Only documents marked `current` are eligible as Current C4 or Runtime Views; a Current View with `reviewStatus=stale|partial` is navigation context, not proof of the current implementation.
4. If the same user-visible problem remains unchanged after two attempted fixes, stop adding retries, polling, states, or compensation. Rebuild the end-to-end causal chain from the real input boundary.
5. A target-environment failure invalidates a completion claim even when unit or integration tests pass. It does not, by itself, prove a root cause.
6. Do not claim a feature is fully validated when its dossier is `unverified`, `partial`, or `invalidated`.
7. For a cross-component or high-risk feature, at task start, after context compaction, and before declaring completion, re-read the relevant Dossier's `用户目标`, `验收场景`, `明确不规定的实现`, open assumptions, and validation status. Treat `局部假设`, implementation details, evidence, and history as context rather than implementation constraints unless they are explicitly confirmed requirements or hard boundaries. Open C4, Runtime View, or ADR material only when the planned change reaches the component, timing, or boundary it describes.

## Context budget

1. Local changes that do not alter user-visible behavior or a public boundary read this file, the affected code, and tests only.
2. A cross-component or high-risk task normally reads one primary Feature Dossier. Do not recursively open every linked C4, Runtime View, ADR, evidence entry, or history item.
3. Open one relevant Current View only when component responsibility, dependency, external boundary, or critical timing changes. Open an ADR only when its decision boundary may be changed or violated.
4. `npm run architecture:impact` is a routing aid, not a checklist requiring every reported document to be read or edited.

## MVP trunk and parallel agent delivery

1. This is a single-maintainer MVP repository. Ordinary Agent work may commit directly to the canonical `main` checkout; a session, Agent, or small task does not require its own branch, worktree, or PR.
2. Multiple sessions may analyze in parallel, but only one Agent may modify, test, stage, commit, pull, or push the canonical checkout at a time. Before the first tracked-file edit in that checkout, acquire the main-writer lease described in `docs/architecture/maintenance.md`; if another owner holds it, remain read-only or use an isolated worktree.
3. Before editing, run `git rev-parse --show-toplevel`, `git worktree list --porcelain`, and `git status --short --branch`. For the direct-to-main lane, require the canonical checkout, branch `main`, a clean worktree, a valid lease, and a freshly re-read `HEAD`. Never continue on another Agent's uncommitted files.
4. While holding the lease, keep one atomic task in progress: edit, run proportionate checks, commit a revertible change directly to `main`, confirm the worktree is clean, then release the lease. Do not leave a dirty canonical checkout for another Agent. Analysis should happen before acquiring the lease when practical.
5. Use an independent worktree only for genuinely concurrent writers, long-running work that would monopolize the lease, risky experiments, large cross-component refactors, migrations, or security/privacy/data-integrity changes that need isolation. One writer per worktree still applies. A worktree branch may be integrated by cherry-pick and does not automatically require a PR.
6. A branch name does not isolate working files. Never run `git switch`, `git checkout`, or `git switch -c` in the shared canonical checkout while other sessions may use it. Create a separate worktree when branch isolation is needed.
7. PRs are optional checkpoints for release batches, costly-to-reverse decisions, external review, or changes the user explicitly asks to review. Ordinary MVP commits do not wait for a PR or per-commit user approval. Pushes to the shared remote remain serialized under the main-writer lease.
8. Development Agents may change code, tests, and an explicitly affected MVP contract. They may report evidence or deviations, but must not self-promote a Dossier to `validated`, a Current View to `reviewed`, or an ADR from Proposed to Accepted.
9. A designated integration/documentation owner performs semantic promotions only after comparing a clean main revision with the relevant contract, view, decision, and target-environment evidence. Direct-to-main does not weaken validation standards.
10. Do not create per-task documentation ledgers. Use commits, CI, and optional Issues/PRs for ordinary delivery details; create durable documentation records only for a product contract, long-lived architecture decision, persistent cross-task deviation, or validation promotion.

Detailed document roles and maintenance rules are in [the feature documentation guide](docs/features/README.md) and [the architecture maintenance guide](docs/architecture/maintenance.md).
