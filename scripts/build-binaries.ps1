# Build LSP server binaries for Windows
# Run this on a Windows machine to build native binaries

$ErrorActionPreference = "Stop"

Write-Host "Building CodeGraph LSP server binaries for Windows..." -ForegroundColor Cyan

# Ensure we're on Windows
if ($env:OS -ne "Windows_NT") {
    Write-Host "This script is for Windows only. Use build-binaries.sh for macOS." -ForegroundColor Red
    exit 1
}

# Check for Rust
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "Error: Rust/Cargo not found. Please install from https://rustup.rs" -ForegroundColor Red
    exit 1
}

# Install Rust target if needed
Write-Host "Ensuring Rust target is installed..." -ForegroundColor Yellow
rustup target add x86_64-pc-windows-msvc 2>$null

# Create bin directory
$binDir = Join-Path $PSScriptRoot ".." "bin"
if (-not (Test-Path $binDir)) {
    New-Item -ItemType Directory -Path $binDir | Out-Null
}

Write-Host ""
Write-Host "Building for x86_64-pc-windows-msvc (Windows x64)..." -ForegroundColor Yellow
cargo build --release --target x86_64-pc-windows-msvc

$sourcePath = Join-Path $PSScriptRoot ".." "target" "x86_64-pc-windows-msvc" "release" "codegraph-lsp.exe"
$destPath = Join-Path $binDir "codegraph-lsp-win32-x64.exe"

Copy-Item -Path $sourcePath -Destination $destPath -Force
Write-Host "Built bin/codegraph-lsp-win32-x64.exe" -ForegroundColor Green

Write-Host ""
Write-Host "============================================" -ForegroundColor Cyan
Write-Host "Build complete! Windows binary is in bin/" -ForegroundColor Cyan
Write-Host "============================================" -ForegroundColor Cyan

Get-ChildItem -Path $binDir -Filter "codegraph-lsp-win32-*" | Format-Table Name, Length, LastWriteTime

Write-Host ""
Write-Host "Note: macOS and Linux binaries are built on their respective platforms." -ForegroundColor Yellow
