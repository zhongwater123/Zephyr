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

## Pull Requests

Please include:

- What changed.
- How you tested it.
- Any provider/configuration assumptions.
- Screenshots or short clips for UI changes when possible.
