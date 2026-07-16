# Development Setup

This project currently targets Windows.

## Required Tools

- Node.js and npm
- Rust stable MSVC toolchain
- Visual Studio Build Tools or Visual Studio Community with C++ desktop workload
- WebView2 Runtime, normally already present on modern Windows

Check Rust:

```powershell
cargo --version
rustc --version
```

Check MSVC tools in a developer shell:

```powershell
where.exe cl
where.exe link
```

## Recommended PowerShell Profile

UTF-8 output helps avoid mojibake when reading Chinese text:

```powershell
chcp 65001 > $null
[Console]::InputEncoding = [System.Text.Encoding]::UTF8
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8
```

If you installed Visual Studio Community at the default path:

```powershell
$vsDevShell = "C:\Program Files\Microsoft Visual Studio\2022\Community\Common7\Tools\Launch-VsDevShell.ps1"
if (Test-Path $vsDevShell) {
    & $vsDevShell -Arch amd64 -HostArch amd64
}
```

To keep Rust build artifacts off the system drive:

```powershell
$env:CARGO_TARGET_DIR = "E:\cargo-target"
```

## Install

```powershell
npm install
```

## Run In Development

```powershell
npm run tauri dev
```

Vite uses port `1420` by default.

## Build

```powershell
npm run build
npm run tauri build
```

## Tests

Rust tests:

```powershell
cd src-tauri
cargo test
```

Frontend type/build check:

```powershell
npm run build
```

## Common Local Artifacts

Do not commit:

- `node_modules/`
- `dist/`
- `target/`
- `target-check/`
- `.vs/`
- `.env*`
- `sauc_go/`
- `sauc_go.zip`
- `*.log`
