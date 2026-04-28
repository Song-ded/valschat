param(
  [string]$OutputDir = "dist",
  [string]$TargetDir = "target-release-dist"
)

$ErrorActionPreference = "Stop"

Write-Host "Building release binaries into $TargetDir ..."
cargo build --release --bins --target-dir $TargetDir

$clientDir = Join-Path $OutputDir "client"
$serverDir = Join-Path $OutputDir "server"
New-Item -ItemType Directory -Force -Path $clientDir | Out-Null
New-Item -ItemType Directory -Force -Path $serverDir | Out-Null

Copy-Item (Join-Path $TargetDir "release\messanger.exe") (Join-Path $clientDir "messanger.exe") -Force
Copy-Item (Join-Path $TargetDir "release\server.exe") (Join-Path $serverDir "server.exe") -Force
Copy-Item "packaging\windows\client\*.bat" $clientDir -Force
Copy-Item "packaging\windows\server\*.bat" $serverDir -Force

@"
Windows client package

Files:
- messanger.exe
- register.bat USER PASSWORD SERVER_URL
- login.bat USER PASSWORD SERVER_URL
- chat.bat
- status.bat
- logout.bat [SERVER_URL]

Example:
register.bat alice 1234 https://valschat.onrender.com
chat.bat --server https://valschat.onrender.com
"@ | Set-Content (Join-Path $clientDir "README.txt")

@"
Windows server package

Files:
- server.exe
- start-server.bat

Render or other Linux hosting should build from source.
This package is for running the Windows server executable directly.
"@ | Set-Content (Join-Path $serverDir "README.txt")

Write-Host "Release packages created in $OutputDir"
