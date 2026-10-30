//! # Patch Verification Loop (SPEC.md Section 5.3 & Task T11)
//!
//! Multi-round repair verification engine for findings with status `PROVEN`.
//! `PATCH_VERIFIED` requires ALL of (SPEC.md 5.3), checked in at most 3 rounds:
//! - Guard: static anti-bypass + AST signature check (`patch_guard::validate_patch`)
//! - (a) the exact same `warrant_proof::proof_*` harness is clean under Miri (exit 0)
//! - (b) the crate's existing tests pass natively (`cargo test --lib`)
//! - (c) the `warrant_proof::sanity_*` tests (valid input -> expected output, written before
//!   patching and passing on the original code) pass under Miri
//!
//! After 3 failed rounds the badge is `PATCH_UNVERIFIED`; the `PROVEN` finding is never downgraded.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use tracing::{debug, info, warn};

use crate::diff::generate_unified_diff;
use crate::injector::HarnessInjector;
use crate::patch_guard;
use crate::runner::ProofRunner;
use crate::VerifyError;
use warrant_llm::{Candidate, CostTracker, LlmClient, PatchEngine, PatchRequest, PatchStatus};

/// Maximum rounds allowed per candidate in patch verification (SPEC.md Section 7)
pub const MAX_PATCH_ROUNDS: usize = 3;

/// Full diagnostic outcome of a patch verification session.
/// The `step*` flags describe the LAST round executed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchVerificationResult {
    pub candidate_id: String,
    pub status: PatchStatus,
    pub rounds_used: usize,
    pub patch_code: Option<String>,
    pub patch_diff: Option<String>,
    pub step1_guard_passed: bool,
    pub step2_miri_passed: bool,
    pub step3_native_passed: bool,
    pub step4_sanity_passed: bool,
    pub error_history: Vec<String>,
}

/// Patch Verification Engine coordinating generation and verification
pub struct PatchVerificationLoop<'a> {
    pub runner: &'a dyn ProofRunner,
    pub injector: HarnessInjector,
    pub max_rounds: usize,
}

/// True if libtest reported `test result: ok. N passed` with N >= 1, so a filter that matched
/// zero tests can never be mistaken for a clean run.
pub fn ran_at_least_one_passing_test(stdout: &str) -> bool {
    stdout.lines().any(|l| {
        l.trim_start().starts_with("test result: ok.")
            && l.split_whitespace()
                .collect::<Vec<_>>()
                .windows(2)
                .any(|w| w[1] == "passed;" && w[0].parse::<u32>().map(|n| n >= 1).unwrap_or(false))
    })
}

/// Keep verifier feedback verbatim but bounded (first 40 lines / 4000 chars).
fn clip_verbatim(s: &str) -> String {
    let joined = s.lines().take(40).collect::<Vec<_>>().join("\n");
    joined.chars().take(4000).collect()
}

impl<'a> PatchVerificationLoop<'a> {
    pub fn new(runner: &'a dyn ProofRunner) -> Self {
        Self {
            runner,
            injector: HarnessInjector::new(),
            max_rounds: MAX_PATCH_ROUNDS,
        }
    }

    pub fn with_rounds(runner: &'a dyn ProofRunner, max_rounds: usize) -> Self {
        Self {
            runner,
            injector: HarnessInjector::new(),
            max_rounds: max_rounds.clamp(1, MAX_PATCH_ROUNDS),
        }
    }

    /// Prepare an isolated Cargo package: `code` + `module` appended at the END of the file copy.
    /// Reads the written file back and asserts the module text is present byte-for-byte at the end.
    fn prepare_temp_package(
        &self,
        code: &str,
        module: &str,
    ) -> Result<(TempDir, PathBuf), VerifyError> {
        let temp_dir = tempfile::tempdir()?;
        let pkg_path = temp_dir.path().to_path_buf();

        let cargo_toml = r#"[package]
name = "warrant-patch-verification"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
"#;
        fs::write(pkg_path.join("Cargo.toml"), cargo_toml)?;

        let toolchain_toml = format!(
            r#"[toolchain]
channel = "{}"
components = ["rust-src", "miri"]
profile = "minimal"
"#,
            crate::bundle::PINNED_TOOLCHAIN
        );
        fs::write(pkg_path.join("rust-toolchain.toml"), toolchain_toml)?;

        let src_dir = pkg_path.join("src");
        fs::create_dir_all(&src_dir)?;

        let validated = self.injector.prepare_harness(module)?;
        let expected_in_source = if let Some(idx) = validated.find("#[cfg(test)]") {
            &validated[idx..]
        } else {
            &validated[..]
        };
        let combined_source = self.injector.inject_to_target_copy(code, &validated)?;
        fs::write(src_dir.join("lib.rs"), combined_source)?;

        // Real invariant check on the file Miri will execute (not a self-comparison).
        let on_disk = fs::read_to_string(src_dir.join("lib.rs"))?;
        patch_guard::verify_harness_in_source(&on_disk, expected_in_source)
            .map_err(|e| VerifyError::HarnessViolation(e.to_string()))?;

        Ok((temp_dir, pkg_path))
    }

    /// Precondition of SPEC.md 5.3: the sanity test must pass under Miri on the ORIGINAL code.
    /// Returns (passed, verbatim output excerpt).
    pub async fn sanity_passes_on(
        &self,
        code: &str,
        sanity_code: &str,
    ) -> Result<(bool, String), VerifyError> {
        let (_dir, pkg) = self.prepare_temp_package(code, sanity_code)?;
        let out = self.runner.run_miri(&pkg, "warrant_proof::sanity_").await?;
        let ok = out.exit_code == Some(0) && ran_at_least_one_passing_test(&out.stdout);
        Ok((ok, format!("{}\n{}", clip_verbatim(&out.stdout), clip_verbatim(&out.stderr))))
    }

    /// Execute the multi-round verification loop.
    /// `sanity_code` is a `#[cfg(test)] mod warrant_proof { ... fn sanity_*() ... }` module that
    /// passes on the original code; `harness_code` is the unchanged proof harness.
    #[allow(clippy::too_many_arguments)]
    pub async fn verify_patch_loop<C: ?Sized + LlmClient>(
        &self,
        client: &C,
        cost_tracker: &CostTracker,
        target_code: &str,
        candidate: &Candidate,
        harness_code: &str,
        sanity_code: &str,
        miri_signature: &str,
    ) -> Result<PatchVerificationResult, VerifyError> {
        let engine = PatchEngine::new(self.max_rounds);
        let mut last_error: Option<String> = None;
        let mut error_history = Vec::new();

        let (mut s1, mut s2, mut s3, mut s4) = (false, false, false, false);

        info!(candidate_id = %candidate.id, max_rounds = self.max_rounds, "Patch Verification Loop");

        for round in 1..=self.max_rounds {
            debug!(candidate_id = %candidate.id, round, "patch round");
            (s1, s2, s3, s4) = (false, false, false, false);

            let req = PatchRequest {
                candidate: candidate.clone(),
                original_code: target_code.to_string(),
                harness_code: harness_code.to_string(),
                miri_signature: miri_signature.to_string(),
                round,
                error_feedback: last_error.clone(),
            };

            let patch_code = match engine.request_patch(client, cost_tracker, &req).await {
                Ok(code) => code,
                Err(err) => {
                    warn!(candidate_id = %candidate.id, error = %err, "patch request failed");
                    error_history.push(format!("Round {} request failed: {}", round, err));
                    return Ok(PatchVerificationResult {
                        candidate_id: candidate.id.clone(),
                        status: PatchStatus::PatchUnverified,
                        rounds_used: round,
                        patch_code: None,
                        patch_diff: None,
                        step1_guard_passed: false,
                        step2_miri_passed: false,
                        step3_native_passed: false,
                        step4_sanity_passed: false,
                        error_history,
                    });
                }
            };

            // Guard: static anti-bypass & AST signature check
            if let Err(guard_err) = patch_guard::validate_patch(target_code, &patch_code) {
                let msg = format!("Guard (anti-bypass/signature) failed round {}: {}", round, guard_err);
                warn!(candidate_id = %candidate.id, round, "guard rejected patch");
                last_error = Some(msg.clone());
                error_history.push(msg);
                continue;
            }
            s1 = true;

            // Proof package: patched code + the unchanged proof harness (checked on disk)
            let (_proof_dir, proof_pkg) = match self.prepare_temp_package(&patch_code, harness_code) {
                Ok(pkg) => pkg,
                Err(err) => {
                    let msg = format!("Package preparation failed round {}: {}", round, err);
                    last_error = Some(msg.clone());
                    error_history.push(msg);
                    continue;
                }
            };

            // (a) exact harness under Miri must be clean
            let miri_out = self.runner.run_miri(&proof_pkg, "warrant_proof::proof_").await?;
            let clean = miri_out.exit_code == Some(0)
                && ran_at_least_one_passing_test(&miri_out.stdout)
                && !miri_out.stderr.contains("Undefined Behavior")
                && !miri_out.stderr.contains("panicked at");
            if !clean {
                let msg = format!(
                    "Miri re-execution of proof harness failed round {} (exit={:?}). Output:\n{}",
                    round,
                    miri_out.exit_code,
                    clip_verbatim(&miri_out.stderr)
                );
                last_error = Some(msg.clone());
                error_history.push(msg);
                continue;
            }
            s2 = true;

            // (b) existing tests natively
            let native_out = self.runner.run_native_test(&proof_pkg).await?;
            if native_out.exit_code != Some(0) {
                let msg = format!(
                    "Native `cargo test --lib` failed round {} (exit={:?}). Output:\n{}",
                    round,
                    native_out.exit_code,
                    clip_verbatim(&native_out.stderr)
                );
                last_error = Some(msg.clone());
                error_history.push(msg);
                continue;
            }
            s3 = true;

            // (c) sanity_* tests under Miri (valid input must still give the expected output)
            let (_sanity_dir, sanity_pkg) = match self.prepare_temp_package(&patch_code, sanity_code) {
                Ok(pkg) => pkg,
                Err(err) => {
                    let msg = format!("Sanity package preparation failed round {}: {}", round, err);
                    last_error = Some(msg.clone());
                    error_history.push(msg);
                    continue;
                }
            };
            let sanity_out = self.runner.run_miri(&sanity_pkg, "warrant_proof::sanity_").await?;
            let sanity_ok = sanity_out.exit_code == Some(0) && ran_at_least_one_passing_test(&sanity_out.stdout);
            if !sanity_ok {
                let msg = format!(
                    "Sanity test (valid input -> expected output) failed round {} (exit={:?}). Output:\n{}\n{}",
                    round,
                    sanity_out.exit_code,
                    clip_verbatim(&sanity_out.stdout),
                    clip_verbatim(&sanity_out.stderr)
                );
                last_error = Some(msg.clone());
                error_history.push(msg);
                continue;
            }
            s4 = true;

            info!(candidate_id = %candidate.id, rounds = round, "PATCH_VERIFIED");
            let diff = generate_unified_diff(target_code, &patch_code, &candidate.file_path);
            return Ok(PatchVerificationResult {
                candidate_id: candidate.id.clone(),
                status: PatchStatus::PatchVerified,
                rounds_used: round,
                patch_code: Some(patch_code),
                patch_diff: Some(diff),
                step1_guard_passed: s1,
                step2_miri_passed: s2,
                step3_native_passed: s3,
                step4_sanity_passed: s4,
                error_history,
            });
        }

        Ok(PatchVerificationResult {
            candidate_id: candidate.id.clone(),
            status: PatchStatus::PatchUnverified,
            rounds_used: self.max_rounds,
            patch_code: None,
            patch_diff: None,
            step1_guard_passed: s1,
            step2_miri_passed: s2,
            step3_native_passed: s3,
            step4_sanity_passed: s4,
            error_history,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::{MiriOutput, MockProofRunner};
    use warrant_llm::{CandidateKind, MockLlmClient};

    const SAMPLE_MUTANT: &str = r#"
pub struct SliceBuffer {
    data: Vec<u8>,
    cursor: usize,
}

impl SliceBuffer {
    pub fn new() -> Self {
        Self { data: Vec::new(), cursor: 0 }
    }

    pub fn read_at_offset(&self, offset: usize) -> Option<u8> {
        let idx = self.cursor.checked_add(offset)?;
        Some(unsafe { *self.data.get_unchecked(idx) })
    }
}
"#;

    const SAMPLE_HARNESS: &str = r#"
#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1() {
        let buf = SliceBuffer::new();
        let _ = buf.read_at_offset(5);
    }
}
"#;

    const SAMPLE_SANITY: &str = r#"
#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn sanity_r1() {
        let buf = SliceBuffer::new();
        assert_eq!(buf.read_at_offset(0), None);
    }
}
"#;

    fn cand() -> Candidate {
        Candidate {
            id: "c-slice".to_string(),
            crate_name: "test".to_string(),
            file_path: "bench/pairs/r1_01_slice_get_mutant.rs".to_string(),
            start_line: 14,
            end_line: 17,
            kind: CandidateKind::UnsafeBlock,
            code_snippet: "unsafe { *self.data.get_unchecked(idx) }".to_string(),
            enclosing_item: Some("SliceBuffer::read_at_offset".to_string()),
            safety_comment: None,
            trait_name: None,
            rule: Some("R1".to_string()),
            visibility: Some("pub".to_string()),
        }
    }

    fn ok_runner() -> MockProofRunner {
        MockProofRunner::new(MiriOutput {
            exit_code: Some(0),
            stdout: "test result: ok. 1 passed; 0 failed".to_string(),
            stderr: String::new(),
            duration_ms: 10,
        })
    }

    #[tokio::test]
    async fn test_patch_loop_verified_round_1_requires_sanity() {
        let runner = ok_runner();
        let patch_loop = PatchVerificationLoop::new(&runner);
        let client = MockLlmClient::new();
        let cost_tracker = CostTracker::default();

        let result = patch_loop
            .verify_patch_loop(
                &client,
                &cost_tracker,
                SAMPLE_MUTANT,
                &cand(),
                SAMPLE_HARNESS,
                SAMPLE_SANITY,
                "unsafe precondition(s) violated",
            )
            .await
            .expect("patch verification should run");

        assert_eq!(result.status, PatchStatus::PatchVerified);
        assert_eq!(result.rounds_used, 1);
        assert!(result.step1_guard_passed && result.step2_miri_passed);
        assert!(result.step3_native_passed && result.step4_sanity_passed);
        assert!(result.patch_diff.is_some());
        // Mock backend has no HTTP usage: nothing may be recorded in the ledger.
        assert_eq!(cost_tracker.call_totals().calls, 0);
    }

    #[tokio::test]
    async fn test_patch_loop_unverified_on_miri_failure() {
        let runner = MockProofRunner::new(MiriOutput {
            exit_code: Some(1),
            stdout: String::new(),
            stderr: "error: Undefined Behavior: out-of-bounds pointer arithmetic".to_string(),
            duration_ms: 10,
        });
        let patch_loop = PatchVerificationLoop::with_rounds(&runner, 2);
        let client = MockLlmClient::new();
        let cost_tracker = CostTracker::default();

        let result = patch_loop
            .verify_patch_loop(
                &client,
                &cost_tracker,
                SAMPLE_MUTANT,
                &cand(),
                SAMPLE_HARNESS,
                SAMPLE_SANITY,
                "unsafe precondition(s) violated",
            )
            .await
            .expect("patch verification should run");

        assert_eq!(result.status, PatchStatus::PatchUnverified);
        assert_eq!(result.rounds_used, 2);
        assert!(result.step1_guard_passed);
        assert!(!result.step2_miri_passed);
        assert!(!result.step4_sanity_passed);
        assert!(result.patch_code.is_none());
        assert!(!result.error_history.is_empty());
    }

    #[tokio::test]
    async fn test_patch_unverified_when_sanity_fails_even_if_proof_clean() {
        // Miri clean for everything except that the sanity run reports zero passing tests.
        struct SanityFailRunner;
        #[async_trait::async_trait]
        impl ProofRunner for SanityFailRunner {
            async fn run_miri(
                &self,
                _d: &std::path::Path,
                filter: &str,
            ) -> Result<MiriOutput, VerifyError> {
                let sanity = filter.contains("sanity_");
                Ok(MiriOutput {
                    exit_code: Some(if sanity { 101 } else { 0 }),
                    stdout: if sanity { String::new() } else { "test result: ok. 1 passed; 0 failed".into() },
                    stderr: if sanity { "assertion `left == right` failed".into() } else { String::new() },
                    duration_ms: 1,
                })
            }
            async fn run_native_test(&self, _d: &std::path::Path) -> Result<MiriOutput, VerifyError> {
                Ok(MiriOutput { exit_code: Some(0), stdout: String::new(), stderr: String::new(), duration_ms: 1 })
            }
        }
        let runner = SanityFailRunner;
        let patch_loop = PatchVerificationLoop::with_rounds(&runner, 1);
        let result = patch_loop
            .verify_patch_loop(
                &MockLlmClient::new(),
                &CostTracker::default(),
                SAMPLE_MUTANT,
                &cand(),
                SAMPLE_HARNESS,
                SAMPLE_SANITY,
                "sig",
            )
            .await
            .expect("runs");
        assert_eq!(result.status, PatchStatus::PatchUnverified);
        assert!(result.step2_miri_passed && result.step3_native_passed && !result.step4_sanity_passed);
    }
}
