@echo off
rem ==============================================================================
rem WARRANT Standalone Reproducer Script (Windows CMD)
rem Bundle ID: evb-9cc49c5a0e8c4ebca078e86f3299a77d
rem Candidate ID: c-0001
rem Verdict: PROVEN_PUBLIC
rem Toolchain: nightly-2026-10-20
rem ==============================================================================
setlocal enabledelayedexpansion

set SCRIPT_DIR=%~dp0
if "%WARRANT_NIGHTLY%"=="" set WARRANT_NIGHTLY=nightly-2026-10-20

echo ============================================================
echo   WARRANT VERIFIER STANDALONE REPRODUCER (Windows)
echo   Candidate: c-0001
echo   Toolchain: %WARRANT_NIGHTLY%
echo   Target: %SCRIPT_DIR%target_with_harness.rs
echo ============================================================

set TMP_DIR=%TEMP%\warrant_reproduce_%RANDOM%_%RANDOM%
mkdir "%TMP_DIR%\src"
copy "%SCRIPT_DIR%target_with_harness.rs" "%TMP_DIR%\src\lib.rs" >nul

(
echo [package]
echo name = "warrant-reproduction"
echo version = "0.1.0"
echo edition = "2021"
echo [lib]
echo path = "src/lib.rs"
) > "%TMP_DIR%\Cargo.toml"

cd /d "%TMP_DIR%"
echo [RUNNING] cargo +%WARRANT_NIGHTLY% miri test --lib warrant_proof::proof_ ...
cargo +%WARRANT_NIGHTLY% miri test --lib warrant_proof::proof_
set EXIT_CODE=%ERRORLEVEL%

echo [FINISHED] Miri exit code: %EXIT_CODE%
if not "%EXIT_CODE%"=="0" (
    echo [VERIFIED] Reproducer successfully confirmed failure under Miri.
) else (
    echo [CLEAN] Safe execution without error.
)

cd /d "%SCRIPT_DIR%"
rmdir /s /q "%TMP_DIR%" 2>nul
exit /b 0
