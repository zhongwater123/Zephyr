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

## Branches And Agent Worktrees

This repository uses trunk-based development with short-lived task branches:

- Start each task from the latest `origin/main`.
- Use one independent Git worktree per editing Agent, with only one writer in that worktree.
- Prefer `codex/<issue-or-task>-<slug>` for Codex-created branches; use the team prefix for other tools.
- Keep branches scoped to one reviewable outcome and merge through a PR after CI passes.
- A branch is an implementation sandbox, not the source of project status. Use Issue/PR state, CI, Feature Dossier validation status, and release tags for status.
- Delete the task branch and worktree after merge.

Small changes do not need architecture-document edits. Follow the context levels in [the maintenance guide](docs/architecture/maintenance.md#最小上下文分级): local work reads code and tests; cross-component/high-risk work normally adds one primary Feature Dossier; Current Views and ADRs are conditional on actual boundary changes.

## Pull Requests

Please include:

- What changed.
- How you tested it.
- Any provider/configuration assumptions.
- Screenshots or short clips for UI changes when possible.
- The primary Feature Dossier, if the task is cross-component or high-risk.
- Whether the change alters a product contract, component/timing boundary, long-lived ADR decision, validation claim, or none of these.

Do not copy routine CI output into long-lived documentation. Development Agents may report deviations and evidence in the PR, but validation, Current View review, and ADR acceptance are promoted by the designated integration/documentation owner against the merged clean revision.
