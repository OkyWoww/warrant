# ==============================================================================
# WARRANT Standalone Reproducer Script (Windows PowerShell)
# Bundle ID: evb-12a32e6860484419b19688be96eb8789
# Candidate ID: c-0001
# Verdict: PROVEN_PUBLIC
# Toolchain: nightly-2026-10-20
# ==============================================================================
$ErrorActionPreference = "Continue"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$nightly = if ($env:WARRANT_NIGHTLY) { $env:WARRANT_NIGHTLY } else { "nightly-2026-10-20" }

Write-Host "============================================================"
Write-Host "  WARRANT VERIFIER STANDALONE REPRODUCER (PowerShell)"
Write-Host "  Candidate: c-0001"
Write-Host "  Toolchain: $nightly"
Write-Host "  Target: $scriptDir\target_with_harness.rs"
Write-Host "============================================================"

$tmpDir = [System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), "warrant_reproduce_" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path (Join-Path $tmpDir "src") -Force | Out-Null
Copy-Item (Join-Path $scriptDir "target_with_harness.rs") (Join-Path $tmpDir "src\lib.rs") -Force

@'
[package]
name = "warrant-reproduction"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
'@ | Set-Content (Join-Path $tmpDir "Cargo.toml") -Encoding utf8

Push-Location $tmpDir
try {
    Write-Host "[RUNNING] cargo +$nightly miri test --lib warrant_proof::proof_ ..."
    & cargo "+$nightly" miri test --lib warrant_proof::proof_
    $exitCode = $LASTEXITCODE
    Write-Host "[FINISHED] Miri exit code: $exitCode"
    if ($exitCode -ne 0) {
        Write-Host "[VERIFIED] Reproducer successfully confirmed failure under Miri."
    } else {
        Write-Host "[CLEAN] Safe execution without error."
    }
} finally {
    Pop-Location
    Remove-Item -Path $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
}
