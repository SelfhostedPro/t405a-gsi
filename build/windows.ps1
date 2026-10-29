$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)

cargo build --release -p t405a-gsi -p t405a-gui
New-Item -ItemType Directory -Force -Path dist/windows | Out-Null
Copy-Item target/release/t405a-gsi.exe dist/windows/
Copy-Item target/release/t405a-gsi-gui.exe dist/windows/
Write-Host "built:"
Write-Host "  dist/windows/t405a-gsi-gui.exe"
Write-Host "  dist/windows/t405a-gsi.exe"
