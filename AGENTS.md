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

## Human-controlled version control and parallel agent delivery

1. The maintainer owns Git integration. An Agent's default delivery is an **unstaged, uncommitted workspace diff** plus proportionate test results and a concise handoff. A dirty worktree containing the Agent's completed changes is an expected review state, not a failure to finish.
2. Editing permission does not imply version-control permission. Unless the user explicitly authorizes the specific action in the current request, Agents must not run `git add`, `git commit`, `git pull`, `git push`, `git merge`, `git rebase`, `git cherry-pick`, create/delete tags, or bypass branch protection. A past authorization, administrator access, repository configuration, or a desire to make remote links/CI work does not count as current authorization.
3. Authorization is action-specific: permission to edit does not authorize staging; staging does not authorize committing; committing does not authorize pushing. Issue, PR, release, and other remote mutations likewise require an explicit request for that mutation. When wording is ambiguous, leave the change local and report what is ready for the maintainer.
4. Before changing a tracked file, run `git rev-parse --show-toplevel`, `git worktree list --porcelain`, and `git status --short --branch`. Confirm that the current path is the workspace assigned to the task. Existing modifications are user-owned; preserve them, identify overlap before editing, and never clean, reset, stash, overwrite, or commit them merely to obtain a clean tree.
5. Only one writing Agent may use a checkout at a time. Multiple Agents may analyze read-only in parallel. Parallel writing requires one independent Git worktree per Agent, normally created by the Codex Worktree workflow or explicitly requested by the user; one worktree still has one writer.
6. A branch name does not isolate working files. Never use `git switch`, `git checkout`, or `git switch -c` in a shared checkout to simulate isolation. If the current checkout already contains another task's changes and safe non-overlapping work is not explicit, remain read-only and report the conflict or use an assigned worktree.
7. After editing, run checks in proportion to risk and report changed files, verification performed, failures or unverified target environments, and `git status`. Leave files unstaged unless the user explicitly asks otherwise. Do not create a commit merely to make the worktree clean.
8. If the user explicitly requests a commit or push, re-check the exact diff, branch, worktree, and remote immediately before that action. A direct push to a protected branch requires explicit authorization for that push; never infer permission to use an administrator bypass. Report any bypass or skipped required check before proceeding unless the user has already explicitly accepted it.
9. Development Agents may change code, tests, and an explicitly affected MVP contract. They may report evidence or deviations, but must not self-promote a Dossier to `validated`, a Current View to `reviewed`, or an ADR from Proposed to Accepted.
10. A designated integration/documentation owner performs semantic promotions only after comparing the maintainer-integrated clean revision with the relevant contract, view, decision, and target-environment evidence. Do not create per-task documentation ledgers; use the workspace diff and handoff, then the maintainer's eventual commit/CI or optional Issue/PR for ordinary delivery details.

Detailed document roles and maintenance rules are in [the feature documentation guide](docs/features/README.md) and [the architecture maintenance guide](docs/architecture/maintenance.md).
