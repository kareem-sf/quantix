$ErrorActionPreference = 'Stop'
$quantixRoot = Split-Path -Parent $PSScriptRoot
$quantixLogs = Join-Path $quantixRoot '.quantix-dev'
New-Item -ItemType Directory -Force -Path $quantixLogs | Out-Null
$quantixNode = (Get-Command node.exe).Source
$quantixLauncher = Join-Path $PSScriptRoot 'start-desktop.mjs'
Start-Process -FilePath $quantixNode -ArgumentList @(('"' + $quantixLauncher + '"')) -WorkingDirectory $quantixRoot -WindowStyle Hidden -RedirectStandardOutput (Join-Path $quantixLogs 'desktop.out.log') -RedirectStandardError (Join-Path $quantixLogs 'desktop.err.log')
