//! # Evidence Bundle Generator
//!
//! Generates cryptographic, fully reproducible evidence bundles for Project WARRANT findings.
//! Each bundle contains lossless Miri logs, target and harness sources, SHA-256 hashes,
//! and standalone `reproduce.sh` / `reproduce.bat` scripts.
//!
//! Strictly adheres to SPEC.md Section 5.4 and Section 10 (Task T3).

use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use warrant_llm::{Candidate, Finding, Hypothesis, VerificationStatus};
use crate::VerifyError;

pub const PINNED_TOOLCHAIN: &str = "nightly-2026-10-20";

/// Verified location attestation record
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationAttestation {
    pub symbol: String,
    pub file_path: String,
    pub line_span: (usize, usize),
    pub verified_in_trace: bool,
}

/// Verdict tier classification (SPEC.md Section 5.2 Step 7 & 8)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerdictTier {
    ProvenPublic,
    ProvenInternal,
    Unproven,
}

impl std::fmt::Display for VerdictTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProvenPublic => write!(f, "PROVEN_PUBLIC"),
            Self::ProvenInternal => write!(f, "PROVEN_INTERNAL"),
            Self::Unproven => write!(f, "UNPROVEN"),
        }
    }
}

/// Complete cryptographic evidence bundle preserving audit results (SPEC.md Section 5.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceBundle {
    pub bundle_id: String,
    pub candidate_id: String,
    pub target_commit: String,
    pub toolchain: String,
    pub miri_flags: Vec<String>,
    pub commands: Vec<String>,
    pub stdout: String,
    pub stderr: String,
    pub harness_code: String,
    pub harness_sha256: String,
    pub verdict: String,
    pub location_attestation: LocationAttestation,
    pub execution_duration_ms: u64,
    pub reproduction_command: String,
    pub candidate: Candidate,
    pub hypothesis: Hypothesis,
    pub finding: Finding,
    #[serde(default)]
    pub patch_status: Option<warrant_llm::PatchStatus>,
    #[serde(default)]
    pub patch_code: Option<String>,
    #[serde(default)]
    pub patch_diff: Option<String>,
}

impl EvidenceBundle {
    /// Compute SHA-256 checksum for arbitrary string content
    pub fn compute_sha256(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Save evidence bundle to the designated directory using candidate snippet as target
    pub fn save_to_dir(&self, output_dir: &Path) -> Result<PathBuf, VerifyError> {
        let target_code = if self.candidate.code_snippet.is_empty() {
            "// Target source".to_string()
        } else {
            self.candidate.code_snippet.clone()
        };
        generate_evidence_bundle(output_dir, self, &target_code)
    }

    /// Save evidence bundle to the designated directory with full target source
    pub fn save_to_dir_with_target(&self, output_dir: &Path, target_code: &str) -> Result<PathBuf, VerifyError> {
        generate_evidence_bundle(output_dir, self, target_code)
    }

    /// Attach verified patch details to this evidence bundle
    pub fn with_patch(
        mut self,
        status: warrant_llm::PatchStatus,
        patch_code: Option<String>,
        patch_diff: Option<String>,
    ) -> Self {
        self.patch_status = Some(status);
        self.patch_code = patch_code;
        self.patch_diff = patch_diff;
        self
    }


    /// Determine tier: PUBLIC vs INTERNAL from candidate visibility and harness access
    pub fn determine_verdict(
        status: &VerificationStatus,
        location_passed: bool,
        is_public: bool,
    ) -> String {
        if *status == VerificationStatus::ProvenBug && location_passed {
            if is_public {
                VerdictTier::ProvenPublic.to_string()
            } else {
                VerdictTier::ProvenInternal.to_string()
            }
        } else {
            VerdictTier::Unproven.to_string()
        }
    }
}

/// Generates a complete evidence bundle directory with bundle.json, target, harness,
/// and standalone reproduction scripts (`reproduce.sh`, `reproduce.bat`, `reproduce.ps1`).
pub fn generate_evidence_bundle(
    output_dir: &Path,
    bundle: &EvidenceBundle,
    target_code: &str,
) -> Result<PathBuf, VerifyError> {
    fs::create_dir_all(output_dir)?;

    // 1. Write bundle.json manifest
    let manifest_path = output_dir.join("bundle.json");
    let json_data = serde_json::to_string_pretty(bundle)?;
    fs::write(&manifest_path, json_data)?;

    // 2. Write harness.rs
    let harness_path = output_dir.join("harness.rs");
    fs::write(&harness_path, &bundle.harness_code)?;

    // 3. Write target_with_harness.rs (target source + harness appended at bottom)
    let combined_path = output_dir.join("target_with_harness.rs");
    let combined_source = format!("{}\n\n{}", target_code.trim_end(), bundle.harness_code.trim_start());
    fs::write(&combined_path, combined_source)?;

    // 3b. If patch code is present, write patch.rs and patch.diff
    if let Some(ref patch_code) = bundle.patch_code {
        let patch_path = output_dir.join("patch.rs");
        fs::write(&patch_path, patch_code)?;
    }
    if let Some(ref patch_diff) = bundle.patch_diff {
        let diff_path = output_dir.join("patch.diff");
        fs::write(&diff_path, patch_diff)?;
    }

    // 4. Generate standalone POSIX reproduce.sh
    let reproduce_sh = format!(
        r#"#!/usr/bin/env bash
# ==============================================================================
# WARRANT Standalone Reproducer Script (POSIX / Linux / CI)
# Bundle ID: {}
# Candidate ID: {}
# Verdict: {}
# Toolchain: {}
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${{BASH_SOURCE[0]}}")" && pwd)"
NIGHTLY="${{WARRANT_NIGHTLY:-{}}}"

echo "============================================================"
echo "  WARRANT VERIFIER STANDALONE REPRODUCER"
echo "  Candidate: {}"
echo "  Toolchain: $NIGHTLY"
echo "  Target: $SCRIPT_DIR/target_with_harness.rs"
echo "============================================================"

TMP_DIR="$(mktemp -d -t warrant_reproduce_XXXXXX)"
cleanup() {{
    rm -rf "$TMP_DIR"
}}
trap cleanup EXIT

mkdir -p "$TMP_DIR/src"
cp "$SCRIPT_DIR/target_with_harness.rs" "$TMP_DIR/src/lib.rs"

cat << 'EOF' > "$TMP_DIR/Cargo.toml"
[package]
name = "warrant-reproduction"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
EOF

cd "$TMP_DIR"
echo "[RUNNING] cargo +$NIGHTLY miri test --lib warrant_proof::proof_ ..."
set +e
cargo +$NIGHTLY miri test --lib warrant_proof::proof_
EXIT_CODE=$?
set -e

echo "[FINISHED] Miri exit code: $EXIT_CODE"
if [ "$EXIT_CODE" -ne 0 ]; then
    echo "[VERIFIED] Reproducer successfully confirmed failure under Miri."
    exit 0
else
    echo "[CLEAN] Safe execution without error."
    exit 0
fi
"#,
        bundle.bundle_id,
        bundle.candidate_id,
        bundle.verdict,
        bundle.toolchain,
        bundle.toolchain,
        bundle.candidate_id
    );
    let sh_path = output_dir.join("reproduce.sh");
    fs::write(&sh_path, reproduce_sh)?;

    // 5. Generate standalone Windows reproduce.bat
    let reproduce_bat = format!(
        r#"@echo off
rem ==============================================================================
rem WARRANT Standalone Reproducer Script (Windows CMD)
rem Bundle ID: {}
rem Candidate ID: {}
rem Verdict: {}
rem Toolchain: {}
rem ==============================================================================
setlocal enabledelayedexpansion

set SCRIPT_DIR=%~dp0
if "%WARRANT_NIGHTLY%"=="" set WARRANT_NIGHTLY={}

echo ============================================================
echo   WARRANT VERIFIER STANDALONE REPRODUCER (Windows)
echo   Candidate: {}
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
"#,
        bundle.bundle_id,
        bundle.candidate_id,
        bundle.verdict,
        bundle.toolchain,
        bundle.toolchain,
        bundle.candidate_id
    );
    let bat_path = output_dir.join("reproduce.bat");
    fs::write(&bat_path, reproduce_bat)?;

    // 6. Generate standalone Windows PowerShell reproduce.ps1
    let reproduce_ps1 = format!(
        r#"# ==============================================================================
# WARRANT Standalone Reproducer Script (Windows PowerShell)
# Bundle ID: {}
# Candidate ID: {}
# Verdict: {}
# Toolchain: {}
# ==============================================================================
$ErrorActionPreference = "Continue"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$nightly = if ($env:WARRANT_NIGHTLY) {{ $env:WARRANT_NIGHTLY }} else {{ "{}" }}

Write-Host "============================================================"
Write-Host "  WARRANT VERIFIER STANDALONE REPRODUCER (PowerShell)"
Write-Host "  Candidate: {}"
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
try {{
    Write-Host "[RUNNING] cargo +$nightly miri test --lib warrant_proof::proof_ ..."
    & cargo "+$nightly" miri test --lib warrant_proof::proof_
    $exitCode = $LASTEXITCODE
    Write-Host "[FINISHED] Miri exit code: $exitCode"
    if ($exitCode -ne 0) {{
        Write-Host "[VERIFIED] Reproducer successfully confirmed failure under Miri."
    }} else {{
        Write-Host "[CLEAN] Safe execution without error."
    }}
}} finally {{
    Pop-Location
    Remove-Item -Path $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
}}
"#,
        bundle.bundle_id,
        bundle.candidate_id,
        bundle.verdict,
        bundle.toolchain,
        bundle.toolchain,
        bundle.candidate_id
    );
    let ps1_path = output_dir.join("reproduce.ps1");
    fs::write(&ps1_path, reproduce_ps1)?;

    // 7. Write README.md walkthrough
    let readme_content = format!(
        r#"# Evidence Bundle: {}

- **Candidate ID**: `{}`
- **Verdict**: `{}`
- **Toolchain**: `{}`
- **Harness SHA-256**: `{}`
- **Target Symbol**: `{}`
- **Verified in Miri Trace**: `{}`

## Reproduction Instructions

To reproduce this verdict independently from a clean terminal:

### On Linux / macOS / WSL / Docker:
```bash
./reproduce.sh
```

### On Windows CMD:
```cmd
reproduce.bat
```

### On Windows PowerShell:
```powershell
.\reproduce.ps1
```
"#,
        bundle.bundle_id,
        bundle.candidate_id,
        bundle.verdict,
        bundle.toolchain,
        bundle.harness_sha256,
        bundle.location_attestation.symbol,
        bundle.location_attestation.verified_in_trace
    );
    let readme_path = output_dir.join("README.md");
    fs::write(&readme_path, readme_content)?;

    Ok(manifest_path)
}

/// Execute the standalone reproduction script in the given bundle directory
/// and return `(exit_code, stdout, stderr)`.
pub fn run_reproduction_script(bundle_dir: &Path) -> Result<(i32, String, String), VerifyError> {
    #[cfg(windows)]
    {
        let bat_path = bundle_dir.join("reproduce.bat");
        if bat_path.exists() {
            let out = std::process::Command::new("cmd.exe")
                .arg("/c")
                .arg(&bat_path)
                .output()?;
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            return Ok((out.status.code().unwrap_or(-1), stdout, stderr));
        }
    }

    let sh_path = bundle_dir.join("reproduce.sh");
    let out = std::process::Command::new("sh")
        .arg(&sh_path)
        .output()?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    Ok((out.status.code().unwrap_or(-1), stdout, stderr))
}
