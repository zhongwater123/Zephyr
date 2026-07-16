# Troubleshooting

## Port 1420 Is Already In Use

Vite uses port `1420`.

Find the process:

```powershell
Get-NetTCPConnection -LocalPort 1420 -ErrorAction SilentlyContinue |
  Select-Object LocalAddress,LocalPort,State,OwningProcess
```

Stop only non-zero owning processes:

```powershell
Get-NetTCPConnection -LocalPort 1420 -ErrorAction SilentlyContinue |
  Where-Object { $_.OwningProcess -ne 0 } |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force }
```

`OwningProcess = 0` usually means a transient `TIME_WAIT` entry. Do not try to kill process 0.

## `cl.exe` Or `link.exe` Not Found

Open a Visual Studio Developer PowerShell or load:

```powershell
& "C:\Program Files\Microsoft Visual Studio\2022\Community\Common7\Tools\Launch-VsDevShell.ps1" -Arch amd64 -HostArch amd64
```

Then check:

```powershell
where.exe cl
where.exe link
```

## Build Uses Too Much C Drive Space

Set Cargo target output to another drive:

```powershell
$env:CARGO_TARGET_DIR = "E:\cargo-target"
```

For a persistent setup, add it to your PowerShell profile or user environment variables.

## ASR 401 Unauthorized

Check:

- auth mode: `api_key` versus `app_access`
- correct Volcengine key type
- resource ID
- endpoint path

For old console credentials, use App Key + Access Key.

For new console credentials, use API Key mode.

## No Text Appears In The Preinput Overlay

Check:

- microphone permission and default input device
- shortcut is actually triggering
- provider settings are saved
- terminal logs for ASR connection or auth errors

Short presses and silent sessions are treated as quiet cancellation and should not show failure.

## Preinput Overlay Appears On The Wrong Monitor

The overlay targets the foreground window monitor first, then cursor monitor, then primary monitor.

If it still appears on the wrong screen, click the target application once before pressing the shortcut.

## Shortcut Does Not Work

Try a different combination in settings.

The app rejects known reserved system combinations and attempts real registration before saving.

Some combinations may already be owned by Windows or another app.

## Clipboard Text Changes During Paste

The app temporarily writes final transcript text to the clipboard and simulates `Ctrl+V`.

The MVP restores text clipboard content when possible. Non-text clipboard formats are not fully snapshotted yet.

## DeepSeek Hotword Agent Says It Is Not Enabled

Check:

- DeepSeek API key is saved
- automatic organization switch is enabled if you expect background runs
- manual "整理热词" can still be used to process pending history

DeepSeek is not required for normal ASR input.
