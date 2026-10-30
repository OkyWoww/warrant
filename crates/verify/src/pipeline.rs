//! # warrant-verify pipeline
//!
//! Complete end-to-end deterministic verification pipeline for project WARRANT.
//! Orchestrates harness validation, isolated workspace injection, Miri execution,
//! output classification, and evidence bundle generation.
//!
//! Strictly enforces SPEC.md Section 5.2 (Decision procedure).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use uuid::Uuid;
use warrant_llm::{
    Candidate, CandidateHarness, Finding, Hypothesis, UbClass, VerificationFeedback,
    VerificationStatus,
};
use crate::classifier::{
    classify_ub_signature, extract_matched_signature, verify_location_attestation,
};
use crate::injector::HarnessInjector;
use crate::runner::{LocalMiriRunner, MiriOutput, ProofRunner};
use crate::{EvidenceBundle, LocationAttestation, VerdictTier, VerifyError};


/// Detailed outcome of a candidate verification run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationOutcome {
    pub status: VerificationStatus,
    pub ub_class: Option<UbClass>,
    pub signature_matched: Option<String>,
    pub location_attestation_passed: bool,
    pub miri_output: Option<MiriOutput>,
    pub feedback: VerificationFeedback,
}

impl VerificationOutcome {
    pub fn is_proven(&self) -> bool {
        self.status == VerificationStatus::ProvenBug && self.location_attestation_passed
    }
}

/// Complete deterministic verification pipeline
pub struct VerificationPipeline {
    pub runner: Box<dyn ProofRunner>,
    pub injector: HarnessInjector,
    pub miri_flags: Vec<String>,
}

impl Default for VerificationPipeline {
    fn default() -> Self {
        Self {
            runner: Box::new(LocalMiriRunner::default()),
            injector: HarnessInjector::new(),
            miri_flags: vec![
                "-Zmiri-tree-borrows".to_string(),
                "-Zmiri-strict-provenance".to_string(),
                "-Zmiri-preemption-rate=0".to_string(),
            ],
        }
    }
}

impl VerificationPipeline {
    pub fn new(runner: Box<dyn ProofRunner>) -> Self {
        Self {
            runner,
            injector: HarnessInjector::new(),
            miri_flags: vec![
                "-Zmiri-tree-borrows".to_string(),
                "-Zmiri-strict-provenance".to_string(),
                "-Zmiri-preemption-rate=0".to_string(),
            ],
        }
    }

    pub fn with_flags(runner: Box<dyn ProofRunner>, miri_flags: Vec<String>) -> Self {
        Self {
            runner,
            injector: HarnessInjector::new(),
            miri_flags,
        }
    }

    /// Prepare an isolated temporary Cargo package containing the target and harness
    fn prepare_temp_package(
        &self,
        target_code: &str,
        harness_code: &str,
    ) -> Result<(tempfile::TempDir, PathBuf), VerifyError> {
        let temp_dir = tempfile::tempdir()?;
        let pkg_path = temp_dir.path().to_path_buf();

        // 1. Create Cargo.toml and rust-toolchain.toml
        let cargo_toml = r#"[package]
name = "warrant-verification-target"
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

        // 2. Create src/ directory
        let src_dir = pkg_path.join("src");
        fs::create_dir_all(&src_dir)?;

        // 3. Inject harness at the end of the file copy without line shifts
        let combined_source = self.injector.inject_to_target_copy(target_code, harness_code)?;
        fs::write(src_dir.join("lib.rs"), combined_source)?;

        Ok((temp_dir, pkg_path))
    }

    /// Check compilation of target code with injected harness (`cargo test --no-run`) (SPEC.md Section 5.2 Step 2)
    pub async fn check_compilation(
        &self,
        target_code: &str,
        harness_code: &str,
    ) -> Result<MiriOutput, VerifyError> {
        let (_guard, pkg_path) = self.prepare_temp_package(target_code, harness_code)?;
        self.runner.check_compilation(&pkg_path).await
    }

    /// Execute full end-to-end verification returning a complete VerificationOutcome
    pub async fn verify(
        &self,
        target_code: &str,
        candidate: &Candidate,
        harness: &CandidateHarness,
    ) -> Result<VerificationOutcome, VerifyError> {
        // Step 1: Static filter check
        if let Err(e) = self.injector.validate_static_filter(&harness.harness_code) {
            let feedback = VerificationFeedback {
                compile_passed: false,
                static_filter_passed: false,
                miri_executed: false,
                signature_matched: None,
                stderr_excerpt: format!("Static filter rejection: {}", e),
            };
            return Ok(VerificationOutcome {
                status: VerificationStatus::Inconclusive,
                ub_class: None,
                signature_matched: None,
                location_attestation_passed: false,
                miri_output: None,
                feedback,
            });
        }

        // Step 2: Prepare isolated workspace
        let (_guard, pkg_path) = match self.prepare_temp_package(target_code, &harness.harness_code) {
            Ok(res) => res,
            Err(e) => {
                let feedback = VerificationFeedback {
                    compile_passed: false,
                    static_filter_passed: true,
                    miri_executed: false,
                    signature_matched: None,
                    stderr_excerpt: format!("Workspace preparation failure: {}", e),
                };
                return Ok(VerificationOutcome {
                    status: VerificationStatus::CompilationError,
                    ub_class: None,
                    signature_matched: None,
                    location_attestation_passed: false,
                    miri_output: None,
                    feedback,
                });
            }
        };

        // Step 3: Run Miri
        let miri_output = match self.runner.run_miri(&pkg_path, "warrant_proof::proof_").await {
            Ok(out) => out,
            Err(VerifyError::Timeout(d)) => {
                let feedback = VerificationFeedback {
                    compile_passed: true,
                    static_filter_passed: true,
                    miri_executed: true,
                    signature_matched: None,
                    stderr_excerpt: format!("Execution timed out after {:?}", d),
                };
                return Ok(VerificationOutcome {
                    status: VerificationStatus::Timeout,
                    ub_class: None,
                    signature_matched: None,
                    location_attestation_passed: false,
                    miri_output: None,
                    feedback,
                });
            }
            Err(e) => return Err(e),
        };

        // Step 4: Classify signature & location
        let (status, ub_class) = classify_ub_signature(&miri_output, &harness.rule);
        let location_ok = verify_location_attestation(&miri_output.stderr, candidate);
        let exact_sig = extract_matched_signature(&miri_output.stderr, &harness.rule);

        let final_status = if status == VerificationStatus::ProvenBug && location_ok {
            VerificationStatus::ProvenBug
        } else if status == VerificationStatus::ProvenBug && !location_ok {
            VerificationStatus::Inconclusive
        } else {
            status
        };

        let signature_matched = if final_status == VerificationStatus::ProvenBug {
            exact_sig.clone().or_else(|| Some(ub_class.to_string()))
        } else {
            None
        };

        let stderr_excerpt = miri_output
            .stderr
            .lines()
            .take(15)
            .collect::<Vec<&str>>()
            .join("\n");

        let feedback = VerificationFeedback {
            compile_passed: !miri_output.stderr.contains("error[E"),
            static_filter_passed: true,
            miri_executed: true,
            signature_matched: signature_matched.clone(),
            stderr_excerpt,
        };

        let is_proven = final_status == VerificationStatus::ProvenBug;

        Ok(VerificationOutcome {
            status: final_status,
            ub_class: if is_proven { Some(ub_class) } else { None },
            signature_matched,
            location_attestation_passed: location_ok,
            miri_output: Some(miri_output),
            feedback,
        })
    }

    /// Execute a verification on Candidate + CandidateHarness for LLM feedback loops
    pub async fn verify_candidate_harness(
        &self,
        target_code: &str,
        candidate: &Candidate,
        harness: &CandidateHarness,
    ) -> Result<VerificationFeedback, VerifyError> {
        // Step 1: Static filter check
        if let Err(e) = self.injector.validate_static_filter(&harness.harness_code) {
            return Ok(VerificationFeedback {
                compile_passed: false,
                static_filter_passed: false,
                miri_executed: false,
                signature_matched: None,
                stderr_excerpt: format!("Static filter rejection: {}", e),
            });
        }

        // Step 2: Prepare isolated workspace
        let (_guard, pkg_path) = match self.prepare_temp_package(target_code, &harness.harness_code) {
            Ok(res) => res,
            Err(e) => {
                return Ok(VerificationFeedback {
                    compile_passed: false,
                    static_filter_passed: true,
                    miri_executed: false,
                    signature_matched: None,
                    stderr_excerpt: format!("Workspace preparation failure: {}", e),
                });
            }
        };

        // Step 3: Run Miri
        let miri_output = match self.runner.run_miri(&pkg_path, "warrant_proof::proof_").await {
            Ok(out) => out,
            Err(VerifyError::Timeout(d)) => {
                return Ok(VerificationFeedback {
                    compile_passed: true,
                    static_filter_passed: true,
                    miri_executed: true,
                    signature_matched: None,
                    stderr_excerpt: format!("Execution timed out after {:?}", d),
                });
            }
            Err(e) => {
                return Ok(VerificationFeedback {
                    compile_passed: false,
                    static_filter_passed: true,
                    miri_executed: false,
                    signature_matched: None,
                    stderr_excerpt: format!("Miri runner execution error: {}", e),
                });
            }
        };

        // Step 4: Classify signature
        let (status, ub_class) = classify_ub_signature(&miri_output, &harness.rule);
        let location_ok = verify_location_attestation(&miri_output.stderr, candidate);

        let signature_matched = if status == VerificationStatus::ProvenBug && location_ok {
            Some(ub_class.to_string())
        } else {
            None
        };

        let stderr_excerpt = miri_output
            .stderr
            .lines()
            .take(15)
            .collect::<Vec<&str>>()
            .join("\n");

        Ok(VerificationFeedback {
            compile_passed: !miri_output.stderr.contains("error[E"),
            static_filter_passed: true,
            miri_executed: true,
            signature_matched,
            stderr_excerpt,
        })
    }

    /// Full end-to-end verification generating an EvidenceBundle
    pub async fn verify_hypothesis(
        &self,
        target_code: &str,
        candidate: &Candidate,
        hypothesis: &Hypothesis,
    ) -> Result<EvidenceBundle, VerifyError> {
        let start_time = Instant::now();

        // Step 1: Validate static filter & header
        let prepared_harness = self.injector.prepare_harness(&hypothesis.harness_code)?;

        // Step 2: Prepare isolated workspace
        let (_guard, pkg_path) = self.prepare_temp_package(target_code, &prepared_harness)?;

        // Step 3: Run Miri
        let miri_output = self.runner.run_miri(&pkg_path, "warrant_proof::proof_").await?;
        let duration_ms = start_time.elapsed().as_millis() as u64;

        // Step 4: Classify output
        let rule_str = match hypothesis.ub_class {
            UbClass::UbOob => "R1",
            UbClass::DataRace => "R2",
            _ => "UNKNOWN",
        };
        let (status, ub_class) = classify_ub_signature(&miri_output, rule_str);
        let location_ok = verify_location_attestation(&miri_output.stderr, candidate);

        let final_status = if status == VerificationStatus::ProvenBug && location_ok {
            VerificationStatus::ProvenBug
        } else if status == VerificationStatus::ProvenBug && !location_ok {
            VerificationStatus::Inconclusive
        } else {
            status
        };

        let evidence_hash = Finding::compute_evidence_hash(
            &prepared_harness,
            &miri_output.stderr,
            miri_output.exit_code.unwrap_or(-1),
        );

        let finding = Finding {
            id: format!("fnd-{}", Uuid::new_v4().simple()),
            candidate_id: candidate.id.clone(),
            hypothesis_id: hypothesis.id.clone(),
            ub_class,
            status: final_status.clone(),
            harness_code: prepared_harness.clone(),
            miri_stdout: miri_output.stdout.clone(),
            miri_stderr: miri_output.stderr.clone(),
            exit_code: miri_output.exit_code.unwrap_or(-1),
            evidence_hash,
            timestamp_rfc3339: "2026-10-28T00:00:00Z".to_string(),
            patch_status: None,
            patch_code: None,
            patch_diff: None,
        };

        let reproduction_cmd = format!(
            "cargo +{} miri test --lib warrant_proof::proof_",
            crate::bundle::PINNED_TOOLCHAIN
        );

        let verdict = EvidenceBundle::determine_verdict(&final_status, location_ok, true);

        let location_attestation = LocationAttestation {
            symbol: candidate.enclosing_item.clone().unwrap_or_else(|| candidate.id.clone()),
            file_path: candidate.file_path.clone(),
            line_span: (candidate.start_line, candidate.end_line),
            verified_in_trace: location_ok,
        };

        Ok(EvidenceBundle {
            bundle_id: format!("evb-{}", Uuid::new_v4().simple()),
            candidate_id: candidate.id.clone(),
            target_commit: "0000000000000000000000000000000000000000".to_string(),
            toolchain: crate::bundle::PINNED_TOOLCHAIN.to_string(),
            miri_flags: self.miri_flags.clone(),
            commands: vec![reproduction_cmd.clone()],
            stdout: miri_output.stdout,
            stderr: miri_output.stderr,
            harness_code: prepared_harness.clone(),
            harness_sha256: EvidenceBundle::compute_sha256(&prepared_harness),
            verdict,
            location_attestation,
            execution_duration_ms: duration_ms,
            reproduction_command: reproduction_cmd,
            candidate: candidate.clone(),
            hypothesis: hypothesis.clone(),
            finding,
            patch_status: None,
            patch_code: None,
            patch_diff: None,
        })
    }

    /// Full end-to-end verification generating an EvidenceBundle and saving it to directory
    pub async fn verify_and_bundle(
        &self,
        target_code: &str,
        candidate: &Candidate,
        hypothesis: &Hypothesis,
        output_dir: &std::path::Path,
        is_public: bool,
    ) -> Result<EvidenceBundle, VerifyError> {
        let mut bundle = self.verify_hypothesis(target_code, candidate, hypothesis).await?;
        if is_public && bundle.finding.status == VerificationStatus::ProvenBug {
            bundle.verdict = VerdictTier::ProvenPublic.to_string();
        } else if !is_public && bundle.finding.status == VerificationStatus::ProvenBug {
            bundle.verdict = VerdictTier::ProvenInternal.to_string();
        } else {
            bundle.verdict = VerdictTier::Unproven.to_string();
        }
        bundle.save_to_dir_with_target(output_dir, target_code)?;
        Ok(bundle)
    }

    /// Construct and save an `EvidenceBundle` directly from an already-executed `VerificationOutcome`
    /// without re-invoking Miri a second time.
    pub fn save_bundle_from_outcome(
        &self,
        target_code: &str,
        candidate: &Candidate,
        hypothesis: &Hypothesis,
        outcome: &VerificationOutcome,
        output_dir: &std::path::Path,
        is_public: bool,
    ) -> Result<EvidenceBundle, VerifyError> {
        let prepared_harness = self.injector.prepare_harness(&hypothesis.harness_code)?;
        let miri_out = outcome.miri_output.clone().unwrap_or(MiriOutput {
            exit_code: Some(-1),
            stdout: String::new(),
            stderr: outcome.feedback.stderr_excerpt.clone(),
            duration_ms: 0,
        });
        let ub_class = outcome.ub_class.clone().unwrap_or(hypothesis.ub_class.clone());
        let evidence_hash = Finding::compute_evidence_hash(
            &prepared_harness,
            &miri_out.stderr,
            miri_out.exit_code.unwrap_or(-1),
        );
        let finding = Finding {
            id: format!("fnd-{}", Uuid::new_v4().simple()),
            candidate_id: candidate.id.clone(),
            hypothesis_id: hypothesis.id.clone(),
            ub_class,
            status: outcome.status.clone(),
            harness_code: prepared_harness.clone(),
            miri_stdout: miri_out.stdout.clone(),
            miri_stderr: miri_out.stderr.clone(),
            exit_code: miri_out.exit_code.unwrap_or(-1),
            evidence_hash,
            timestamp_rfc3339: "2026-10-29T00:00:00Z".to_string(),
            patch_status: None,
            patch_code: None,
            patch_diff: None,
        };
        let reproduction_cmd = format!(
            "cargo +{} miri test --lib warrant_proof::proof_",
            crate::bundle::PINNED_TOOLCHAIN
        );
        let verdict = if is_public && outcome.status == VerificationStatus::ProvenBug {
            VerdictTier::ProvenPublic.to_string()
        } else if !is_public && outcome.status == VerificationStatus::ProvenBug {
            VerdictTier::ProvenInternal.to_string()
        } else {
            VerdictTier::Unproven.to_string()
        };
        let location_attestation = LocationAttestation {
            symbol: candidate
                .enclosing_item
                .clone()
                .unwrap_or_else(|| candidate.id.clone()),
            file_path: candidate.file_path.clone(),
            line_span: (candidate.start_line, candidate.end_line),
            verified_in_trace: outcome.location_attestation_passed,
        };
        let bundle = EvidenceBundle {
            bundle_id: format!("evb-{}", Uuid::new_v4().simple()),
            candidate_id: candidate.id.clone(),
            target_commit: "0000000000000000000000000000000000000000".to_string(),
            toolchain: crate::bundle::PINNED_TOOLCHAIN.to_string(),
            miri_flags: self.miri_flags.clone(),
            commands: vec![reproduction_cmd.clone()],
            stdout: miri_out.stdout,
            stderr: miri_out.stderr,
            harness_code: prepared_harness.clone(),
            harness_sha256: EvidenceBundle::compute_sha256(&prepared_harness),
            verdict,
            location_attestation,
            execution_duration_ms: miri_out.duration_ms,
            reproduction_command: reproduction_cmd,
            candidate: candidate.clone(),
            hypothesis: hypothesis.clone(),
            finding,
            patch_status: None,
            patch_code: None,
            patch_diff: None,
        };
        bundle.save_to_dir_with_target(output_dir, target_code)?;
        Ok(bundle)
    }
}
