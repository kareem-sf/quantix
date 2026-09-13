$ErrorActionPreference = 'Stop'
$quantixRoot = Split-Path -Parent $PSScriptRoot
$quantixHome = Join-Path ([Environment]::GetFolderPath('UserProfile')) '.quantix'
$quantixNode = (Get-Command node.exe).Source
# Recovery must finish before PowerShell opens redirected log handles. Those
# handles are inherited by the development process tree and block reset.
& $quantixNode (Join-Path $PSScriptRoot 'reset-preflight.mjs')
if ($LASTEXITCODE -ne 0) {
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.MessageBox]::Show('Quantix could not finish reset recovery. Close other Quantix windows and related programs, then reopen it. If the reset record cannot be verified, contact support before changing the application home.', 'Quantix could not open', 'OK', 'Error') | Out-Null
    exit 1
}
$quantixLogs = Join-Path $quantixHome 'logs'
New-Item -ItemType Directory -Force -Path $quantixLogs | Out-Null
$quantixLauncher = Join-Path $PSScriptRoot 'start-desktop.mjs'
Start-Process -FilePath $quantixNode -ArgumentList @(('"' + $quantixLauncher + '"')) -WorkingDirectory $quantixRoot -WindowStyle Hidden -RedirectStandardOutput (Join-Path $quantixLogs 'desktop.out.log') -RedirectStandardError (Join-Path $quantixLogs 'desktop.err.log')
