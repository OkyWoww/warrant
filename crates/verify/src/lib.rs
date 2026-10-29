//! # warrant-verify
//!
//! Deterministic verifier, inline harness injector, Miri execution runners,
//! signature classification, and evidence bundle collector for project WARRANT.
//!
//! Enforces that all generated test harnesses strictly begin with
//! `#![forbid(unsafe_code)]` followed by `extern crate std;`.
//! Gathers execution logs, classifies Undefined Behavior, and computes
//! cryptographic evidence bundles.
//!
//! Strictly enforces SPEC.md Section 5.2, Section 5.4, and Section 6.

pub mod bundle;
pub mod classifier;
pub mod diff;
pub mod injector;
pub mod patch_guard;
pub mod patch_loop;
pub mod pipeline;
pub mod runner;
pub mod stats;

pub use bundle::*;
pub use classifier::*;
pub use diff::*;
pub use injector::*;
pub use patch_guard::*;
pub use patch_loop::*;
pub use pipeline::*;
pub use runner::*;
pub use stats::*;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;
use warrant_llm::{Candidate, Hypothesis, UbClass, VerificationStatus};


#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("Harness rejected: {0}")]
    HarnessViolation(String),

    #[error("IO error during harness execution: {0}")]
    Io(#[from] std::io::Error),

    #[error("Verification timed out after {0:?}")]
    Timeout(Duration),

    #[error("Evidence serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Sandbox error: {0}")]
    Sandbox(String),

    #[error("Target compilation failed: {0}")]
    CompilationFailed(String),
}

/// Configuration options for the Miri verifier
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifierConfig {
    pub miri_flags: Vec<String>,
    pub timeout_seconds: u64,
    pub use_sandbox: bool,
    pub sandbox_script: Option<PathBuf>,
}

impl Default for VerifierConfig {
    fn default() -> Self {
        Self {
            miri_flags: vec![
                "-Zmiri-tree-borrows".to_string(),
                "-Zmiri-strict-provenance".to_string(),
                "-Zmiri-preemption-rate=0".to_string(),
            ],
            timeout_seconds: 60,
            use_sandbox: false,
            sandbox_script: None,
        }
    }
}


/// Legacy Verifier wrapper providing backward compatibility
pub struct Verifier {
    pub config: VerifierConfig,
    pub injector: HarnessInjector,
}

impl Verifier {
    pub fn new(config: VerifierConfig) -> Self {
        Self {
            config,
            injector: HarnessInjector::new(),
        }
    }

    /// Analyze Miri stderr to detect Undefined Behavior class
    pub fn classify_ub_output(&self, stdout: &str, stderr: &str, exit_code: i32) -> (VerificationStatus, UbClass) {
        let output = MiriOutput {
            exit_code: Some(exit_code),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            duration_ms: 0,
        };
        classify_ub_signature(&output, "GENERIC")
    }

    /// Execute a hypothesis against a candidate and generate an evidence bundle
    pub async fn verify_hypothesis(
        &self,
        candidate: &Candidate,
        hypothesis: &Hypothesis,
    ) -> Result<EvidenceBundle, VerifyError> {
        let target_code = format!(
            "// Candidate definition\n{}\n",
            candidate.code_snippet
        );

        let runner: Box<dyn ProofRunner> = if self.config.use_sandbox {
            Box::new(DockerMiriRunner::default())
        } else {
            Box::new(LocalMiriRunner::new(
                Duration::from_secs(self.config.timeout_seconds),
                self.config.miri_flags.clone(),
            ))
        };

        let mut pipeline = VerificationPipeline::with_flags(runner, self.config.miri_flags.clone());
        pipeline.injector = self.injector.clone();
        pipeline.verify_hypothesis(&target_code, candidate, hypothesis).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use warrant_llm::{CandidateHarness, CandidateKind, REQUIRED_HARNESS_HEADER};


    #[test]
    fn test_harness_injector_rejects_missing_forbid() {
        let injector = HarnessInjector::new();
        let raw = "extern crate std;\nfn main() {}";
        let res = injector.prepare_harness(raw);
        assert!(res.is_err(), "Must reject harness without forbid(unsafe_code)");
    }

    #[test]
    fn test_harness_injector_accepts_valid_header() {
        let injector = HarnessInjector::new();
        let raw = "#![forbid(unsafe_code)]\nextern crate std;\nfn main() {}";
        let res = injector.prepare_harness(raw);
        assert!(res.is_ok(), "Must accept valid forbid and extern crate header");
    }

    #[test]
    fn test_static_filter_accepts_valid_header_and_harness() {
        let injector = HarnessInjector::new();
        let raw = "#![forbid(unsafe_code)]\nextern crate std;\nfn proof_test() { let x = 42; }";
        let res = injector.prepare_harness(raw);
        assert!(res.is_ok(), "Must accept valid header without banned words");
    }

    #[test]
    fn test_static_filter_rejects_allow_attribute() {
        let injector = HarnessInjector::new();
        let sample = "#![forbid(unsafe_code)]\nextern crate std;\n#[allow(dead_code)]\nfn proof_test() {}";
        let res = injector.prepare_harness(sample);
        assert!(res.is_err(), "Must reject sample with #[allow(...)]");
        let err_msg = res.unwrap_err().to_string();
        assert!(err_msg.contains("allow"), "Error must mention banned word allow: {}", err_msg);
    }

    #[test]
    fn test_static_filter_rejects_banned_words() {
        let injector = HarnessInjector::new();
        for banned in &["unsafe", "expect", "panic", "print", "println", "eprintln", "Box::leak", "mem::forget", "std::process"] {
            let sample = format!("#![forbid(unsafe_code)]\nextern crate std;\nfn proof_test() {{ {} }}", banned);
            let res = injector.prepare_harness(&sample);
            assert!(res.is_err(), "Must reject banned word/path: {}", banned);
        }
    }

    #[test]
    fn test_static_filter_rejects_forged_signatures_in_string_literal() {
        let injector = HarnessInjector::new();
        let sample = "#![forbid(unsafe_code)]\nextern crate std;\nfn proof_test() { let _s = \"Undefined Behavior\"; }";
        let res = injector.prepare_harness(sample);
        assert!(res.is_err(), "Must reject signature phrase forgery in string literal");
        assert!(res.unwrap_err().to_string().contains("forged signatures"));
    }

    #[test]
    fn test_classifier_r1_real_signature() {
        let output = MiriOutput {
            exit_code: Some(1),
            stdout: String::new(),
            stderr: "thread 'warrant_proof::proof_r1' (1) panicked at src/lib.rs:21:29:\nunsafe precondition(s) violated: slice::get_unchecked requires that the index is within the slice\nstack backtrace:\n   0: FastBuffer::read_unchecked_elem\n".to_string(),
            duration_ms: 45,
        };

        let (status, ub_class) = classify_ub_signature(&output, "R1");
        assert_eq!(status, VerificationStatus::ProvenBug);
        assert_eq!(ub_class, UbClass::UbOob);
    }

    #[test]
    fn test_classifier_r2_real_signature() {
        let output = MiriOutput {
            exit_code: Some(1),
            stdout: String::new(),
            stderr: "error: Undefined Behavior: Data race detected between (1) non-atomic write on thread `unnamed-2` and (2) non-atomic read on thread `unnamed-3` at alloc48176\n".to_string(),
            duration_ms: 120,
        };

        let (status, ub_class) = classify_ub_signature(&output, "R2");
        assert_eq!(status, VerificationStatus::ProvenBug);
        assert_eq!(ub_class, UbClass::DataRace);
    }

    #[test]
    fn test_classifier_rejects_assertion_panic_false_positive() {
        // Plain rust assertion panic (without Miri UB report)
        let output = MiriOutput {
            exit_code: Some(101),
            stdout: String::new(),
            stderr: "thread 'warrant_proof::proof' panicked at src/lib.rs:10:5: assertion `left == right` failed\n".to_string(),
            duration_ms: 30,
        };

        let (status, _class) = classify_ub_signature(&output, "R1");
        assert_eq!(status, VerificationStatus::Inconclusive, "Standard panic must NOT count as proof");
    }

    #[test]
    fn test_classifier_rejects_memory_leak() {
        // Miri memory leak report must be rejected
        let output = MiriOutput {
            exit_code: Some(1),
            stdout: String::new(),
            stderr: "error: memory leaked: alloc1234 (16 bytes)\n".to_string(),
            duration_ms: 50,
        };

        let (status, _class) = classify_ub_signature(&output, "R1");
        assert_eq!(status, VerificationStatus::Inconclusive, "Memory leak must NOT count as proof");
    }

    #[test]
    fn test_classifier_location_attestation() {
        let candidate = Candidate {
            id: "c-001".to_string(),
            crate_name: "test_crate".to_string(),
            file_path: "src/lib.rs".to_string(),
            start_line: 20,
            end_line: 25,
            kind: CandidateKind::UnsafeBlock,
            code_snippet: "unsafe { *ptr }".to_string(),
            enclosing_item: Some("FastBuffer::read_unchecked_elem".to_string()),
            safety_comment: None,
            trait_name: None,
            rule: Some("R1".to_string()),
            visibility: Some("pub".to_string()),
        };

        let valid_stderr = "stack backtrace:\n   0: FastBuffer::read_unchecked_elem at src/lib.rs:21:19\n";
        assert!(verify_location_attestation(valid_stderr, &candidate));

        let invalid_stderr = "stack backtrace:\n   0: foreign_crate::random_func at foreign.rs:10:5\n";
        assert!(!verify_location_attestation(invalid_stderr, &candidate));
    }

    #[tokio::test]
    async fn test_runner_timeout_handling() {
        let mock_runner = MockProofRunner::with_timeout();
        let res = mock_runner.run_miri(Path::new("."), "warrant_proof::proof_").await;
        assert!(res.is_err());
        match res.unwrap_err() {
            VerifyError::Timeout(_) => {} // expected
            other => panic!("Expected Timeout error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_pipeline_with_mock_runner() {
        let canned = MiriOutput {
            exit_code: Some(1),
            stdout: String::new(),
            stderr: "unsafe precondition(s) violated: slice::get_unchecked requires that the index is within the slice\nstack backtrace:\n   0: FastBuffer::read_unchecked_elem\n".to_string(),
            duration_ms: 40,
        };

        let pipeline = VerificationPipeline::new(Box::new(MockProofRunner::new(canned)));
        let target_code = r#"
pub struct FastBuffer { data: Vec<u8> }
impl FastBuffer {
    pub fn read_unchecked_elem(&self, idx: usize) -> u8 {
        unsafe { *self.data.get_unchecked(idx) }
    }
}
"#;
        let candidate = Candidate {
            id: "c-001".to_string(),
            crate_name: "test_crate".to_string(),
            file_path: "src/lib.rs".to_string(),
            start_line: 5,
            end_line: 5,
            kind: CandidateKind::UnsafeBlock,
            code_snippet: "unsafe { *self.data.get_unchecked(idx) }".to_string(),
            enclosing_item: Some("FastBuffer::read_unchecked_elem".to_string()),
            safety_comment: None,
            trait_name: None,
            rule: Some("R1".to_string()),
            visibility: Some("pub".to_string()),
        };

        let harness = CandidateHarness {
            rule: "R1".to_string(),
            explanation: "Trigger out-of-bounds read".to_string(),
            harness_code: format!(
                r#"{}
#[cfg(test)]
mod warrant_proof {{
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_oob() {{
        let b = FastBuffer {{ data: vec![1, 2, 3] }};
        let _ = b.read_unchecked_elem(10);
    }}
}}
"#,
                REQUIRED_HARNESS_HEADER.trim_end()
            ),
            estimated_budget_usd: 0.0,
        };

        let feedback = pipeline
            .verify_candidate_harness(target_code, &candidate, &harness)
            .await
            .expect("pipeline should succeed");

        assert!(feedback.static_filter_passed);
        assert!(feedback.compile_passed);
        assert!(feedback.miri_executed);
        assert_eq!(feedback.signature_matched, Some("ub_oob".to_string()));
    }
}
