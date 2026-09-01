# Contributing

Thanks for considering a contribution.

## Development Environment

Current target platform is Windows.

Install:

- Node.js and npm
- Rust with the `x86_64-pc-windows-msvc` toolchain
- Visual Studio Build Tools or Visual Studio Community with the C++ desktop workload

Recommended PowerShell setup:

```powershell
$env:CARGO_TARGET_DIR = "E:\cargo-target"
```

If MSVC tools are not available in the shell, load the Visual Studio developer shell before running Cargo commands.

## Install And Run

```powershell
npm install
npm run tauri dev
```

## Checks

Frontend build:

```powershell
npm run build
```

Rust tests:

```powershell
cd src-tauri
cargo test
```

Tauri build:

```powershell
npm run tauri build
```

## Coding Notes

- Keep the realtime input path in Rust.
- Keep the WebView UI out of the recording and provider critical path.
- Do not log API keys, audio content, recognized text, or history body text.
- Keep ASR protocol changes covered by provider tests.
- Keep history and hotword database changes covered by focused tests.
- Do not commit generated output such as `node_modules/`, `dist/`, `target/`, or local provider demo packages.

## MVP Trunk And Agent Workspaces

This single-maintainer MVP uses direct-to-main trunk development with serialized Agent writes:

- Multiple Agents may analyze different tasks concurrently. Only one Agent may write, test, stage, commit, pull, or push the canonical checkout at a time.
- Ordinary fixes and MVP features commit directly to `main` after acquiring the main-writer lease defined in [the maintenance guide](docs/architecture/maintenance.md#main-写入租约). They do not require a task branch, worktree, PR, or per-commit maintainer review.
- Before editing canonical main, verify the repository root, worktree list, clean `main` status, lease ownership, and current `HEAD`. Re-read affected files after acquiring the lease.
- Keep each direct-to-main commit atomic, tested in proportion to risk, and independently revertible. Release the lease only after the canonical checkout is clean.
- Use an independent worktree only for genuinely concurrent writers, long-running changes, risky experiments, large refactors, migrations, or security/privacy/data-integrity work that benefits from isolation. One writer per worktree applies.
- A worktree branch can be integrated by cherry-pick under the main lease. It does not automatically require a PR.
- Never create or switch branches in a shared canonical checkout. A branch name does not isolate working files and changes `HEAD` for every chat using that directory.
- PRs are optional checkpoints for release batches, costly-to-reverse decisions, external review, or explicit maintainer requests. When a PR is used, keep it scoped to one reviewable outcome and delete its short-lived branch/worktree after integration.
- Project status comes from clean main commits, CI, Feature Dossier validation status, and release tags—not from the number of Agent sessions or temporary branches.

Small changes do not need architecture-document edits. Follow the context levels in [the maintenance guide](docs/architecture/maintenance.md#最小上下文分级): local work reads code and tests; cross-component/high-risk work normally adds one primary Feature Dossier; Current Views and ADRs are conditional on actual boundary changes.

## Optional Pull Requests

Routine MVP work may commit directly to `main`. When a change uses a PR, please include:

- What changed.
- How you tested it.
- Any provider/configuration assumptions.
- Screenshots or short clips for UI changes when possible.
- The primary Feature Dossier, if the task is cross-component or high-risk.
- Whether the change alters a product contract, component/timing boundary, long-lived ADR decision, validation claim, or none of these.

Do not copy routine CI output into long-lived documentation. Development Agents may report deviations and evidence in a commit summary, task, Issue, or PR, but validation, Current View review, and ADR acceptance are promoted by the designated integration/documentation owner against a clean main revision.
