//! # Real Fixtures Integration Test Suite
//!
//! End-to-end integration tests executing VerificationPipeline with LocalMiriRunner
//! against real benchmark fixtures (R1 and R2 references) and testing negative rejection invariants.
//!
//! Strictly enforces SPEC.md Section 5.2 and Section 10.

use std::path::Path;
use std::time::Duration;
use warrant_llm::{
    Candidate, CandidateHarness, CandidateKind, VerificationStatus,
};
use warrant_verify::{LocalMiriRunner, ProofRunner, VerificationPipeline};

/// Helper to read target fixture code excluding any old reference harness module
fn load_target_fixture(rel_path: &str) -> String {
    // Search relative to workspace root or crate directory
    let candidates = [
        format!("../../{}", rel_path),
        format!("../{}", rel_path),
        rel_path.to_string(),
    ];

    let mut raw = None;
    for c in &candidates {
        if Path::new(c).exists() {
            raw = Some(std::fs::read_to_string(c).unwrap());
            break;
        }
    }

    let content = raw.unwrap_or_else(|| panic!("Could not locate fixture file at {}", rel_path));
    if let Some(idx) = content.find("// ==============================================================================") {
        content[..idx].trim_end().to_string()
    } else if let Some(idx) = content.find("#[cfg(test)]") {
        content[..idx].trim_end().to_string()
    } else {
        content
    }
}

#[tokio::test]
async fn test_e2e_r1_reference_real_miri_execution() {
    let target_code = load_target_fixture("bench/pairs/r1_reference.rs");
    assert!(target_code.contains("pub struct FastBuffer"));
    assert!(target_code.contains("read_unchecked_elem"));

    let candidate = Candidate {
        id: "c-r1-gold".to_string(),
        crate_name: "target_lib".to_string(),
        file_path: "bench/pairs/r1_reference.rs".to_string(),
        start_line: 19,
        end_line: 24,
        kind: CandidateKind::UnsafeBlock,
        code_snippet: "unsafe { *self.data.get_unchecked(index) }".to_string(),
        enclosing_item: Some("FastBuffer::read_unchecked_elem".to_string()),
        safety_comment: None,
        trait_name: None,
        rule: Some("R1".to_string()),
        visibility: Some("pub".to_string()),
    };

    let harness = CandidateHarness {
        rule: "R1".to_string(),
        explanation: "Provoke out-of-bounds access past vector buffer length".to_string(),
        harness_code: r#"#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_next_frame_oob() {
        let buffer = FastBuffer::new(vec![10, 20, 30, 40]);
        let val = buffer.read_unchecked_elem(10);
        std::hint::black_box(val);
    }
}
"#.to_string(),
        estimated_budget_usd: 0.0,
    };

    let runner: Box<dyn ProofRunner> = Box::new(LocalMiriRunner::new(
        Duration::from_secs(60),
        vec!["-Zmiri-tree-borrows".to_string()],
    ));

    let pipeline = VerificationPipeline::new(runner);
    let outcome = pipeline
        .verify(&target_code, &candidate, &harness)
        .await
        .expect("Verification execution must succeed");

    assert_eq!(outcome.status, VerificationStatus::Verified);
    assert_eq!(outcome.status, VerificationStatus::ProvenBug);
    assert!(outcome.is_proven());
    assert!(outcome.location_attestation_passed);

    let sig = outcome.signature_matched.expect("Must have matched signature");
    assert!(
        sig.contains("unsafe precondition(s) violated") || sig.contains("ub_oob"),
        "Signature must match R1 precondition panic: {}",
        sig
    );

    let output = outcome.miri_output.expect("Miri output must exist");
    assert!(
        output.stderr.contains("FastBuffer::read_unchecked_elem"),
        "Backtrace must attribute location to target function: {}",
        output.stderr
    );
}

#[tokio::test]
async fn test_e2e_r2_reference_real_miri_execution() {
    let target_code = load_target_fixture("bench/pairs/r2_reference.rs");
    assert!(target_code.contains("pub struct SharedCounter"));
    assert!(target_code.contains("unsafe impl Sync for SharedCounter"));

    let candidate = Candidate {
        id: "c-r2-gold".to_string(),
        crate_name: "target_lib".to_string(),
        file_path: "bench/pairs/r2_reference.rs".to_string(),
        start_line: 21,
        end_line: 35,
        kind: CandidateKind::UnsafeImpl,
        code_snippet: "unsafe impl Sync for SharedCounter {}".to_string(),
        enclosing_item: Some("SharedCounter::increment".to_string()),
        safety_comment: None,
        trait_name: Some("Sync".to_string()),
        rule: Some("R2".to_string()),
        visibility: Some("pub".to_string()),
    };

    let harness = CandidateHarness {
        rule: "R2".to_string(),
        explanation: "Trigger data race via non-thread-safe Cell in concurrent threads".to_string(),
        harness_code: r#"#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r2_sync_data_race() {
        let counter = SharedCounter::new(0);
        std::thread::scope(|s| {
            s.spawn(|| {
                for _ in 0..100 {
                    counter.increment();
                }
            });
            s.spawn(|| {
                for _ in 0..100 {
                    counter.increment();
                }
            });
        });
        std::hint::black_box(counter.get());
    }
}
"#.to_string(),
        estimated_budget_usd: 0.0,
    };

    let runner: Box<dyn ProofRunner> = Box::new(LocalMiriRunner::new(
        Duration::from_secs(60),
        vec!["-Zmiri-preemption-rate=0".to_string()],
    ));

    let pipeline = VerificationPipeline::new(runner);
    let outcome = pipeline
        .verify(&target_code, &candidate, &harness)
        .await
        .expect("Verification execution must succeed");

    assert_eq!(outcome.status, VerificationStatus::Verified);
    assert_eq!(outcome.status, VerificationStatus::ProvenBug);
    assert!(outcome.is_proven());
    assert!(outcome.location_attestation_passed);

    let sig = outcome.signature_matched.expect("Must have matched signature");
    assert!(
        sig.contains("Data race detected") || sig.contains("data_race"),
        "Signature must match R2 data race: {}",
        sig
    );

    let output = outcome.miri_output.expect("Miri output must exist");
    assert!(
        output.stderr.contains("SharedCounter::increment"),
        "Backtrace must attribute location to target function: {}",
        output.stderr
    );
}

#[tokio::test]
async fn test_e2e_negative_rejection_banned_unsafe() {
    let target_code = load_target_fixture("bench/pairs/r1_reference.rs");

    let candidate = Candidate {
        id: "c-r1-smuggle".to_string(),
        crate_name: "target_lib".to_string(),
        file_path: "bench/pairs/r1_reference.rs".to_string(),
        start_line: 19,
        end_line: 24,
        kind: CandidateKind::UnsafeBlock,
        code_snippet: "unsafe { *self.data.get_unchecked(index) }".to_string(),
        enclosing_item: Some("FastBuffer::read_unchecked_elem".to_string()),
        safety_comment: None,
        trait_name: None,
        rule: Some("R1".to_string()),
        visibility: Some("pub".to_string()),
    };

    // Malicious harness attempting to use an unsafe block
    let harness = CandidateHarness {
        rule: "R1".to_string(),
        explanation: "Attempting forbidden unsafe construct".to_string(),
        harness_code: r#"#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_banned_unsafe() {
        let b = FastBuffer::new(vec![1, 2, 3]);
        let val = unsafe { *b.data.as_ptr().add(10) };
        std::hint::black_box(val);
    }
}
"#.to_string(),
        estimated_budget_usd: 0.0,
    };

    let runner: Box<dyn ProofRunner> = Box::new(LocalMiriRunner::default());
    let pipeline = VerificationPipeline::new(runner);
    let outcome = pipeline
        .verify(&target_code, &candidate, &harness)
        .await
        .expect("Verification invocation should handle rejection gracefully");

    // Static filter must reject the harness BEFORE running Miri
    assert!(!outcome.feedback.static_filter_passed);
    assert!(!outcome.feedback.miri_executed);
    assert!(!outcome.is_proven());
    assert_ne!(outcome.status, VerificationStatus::ProvenBug);
    assert!(outcome.signature_matched.is_none());
}

#[tokio::test]
async fn test_e2e_negative_rejection_standard_panic() {
    let target_code = load_target_fixture("bench/pairs/r1_reference.rs");

    let candidate = Candidate {
        id: "c-r1-std-panic".to_string(),
        crate_name: "target_lib".to_string(),
        file_path: "bench/pairs/r1_reference.rs".to_string(),
        start_line: 19,
        end_line: 24,
        kind: CandidateKind::UnsafeBlock,
        code_snippet: "unsafe { *self.data.get_unchecked(index) }".to_string(),
        enclosing_item: Some("FastBuffer::read_unchecked_elem".to_string()),
        safety_comment: None,
        trait_name: None,
        rule: Some("R1".to_string()),
        visibility: Some("pub".to_string()),
    };

    // Benign harness that triggers a safe standard panic (division by zero)
    // Note: does not contain the word 'panic' or 'unsafe', avoiding static filter
    let harness = CandidateHarness {
        rule: "R1".to_string(),
        explanation: "Triggering regular arithmetic panic without UB".to_string(),
        harness_code: r#"#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_arithmetic_panic() {
        let zero = 0;
        let val = 100 / zero;
        std::hint::black_box(val);
    }
}
"#.to_string(),
        estimated_budget_usd: 0.0,
    };

    let runner: Box<dyn ProofRunner> = Box::new(LocalMiriRunner::default());
    let pipeline = VerificationPipeline::new(runner);
    let outcome = pipeline
        .verify(&target_code, &candidate, &harness)
        .await
        .expect("Verification invocation should succeed");

    // Filter passed and Miri ran, but standard panic must NOT count as proof
    assert!(outcome.feedback.static_filter_passed);
    assert!(outcome.feedback.miri_executed);
    assert!(!outcome.is_proven());
    assert_eq!(outcome.status, VerificationStatus::Reject);
    assert_eq!(outcome.status, VerificationStatus::Inconclusive);
    assert!(outcome.signature_matched.is_none());
}

#[tokio::test]
async fn test_e2e_r1_ptr_deref_gold_case_with_evidence_bundle() {
    let target_code = load_target_fixture("bench/pairs/r1_ptr_deref.rs");
    assert!(target_code.contains("pub struct RawBuffer"));

    let candidate = Candidate {
        id: "c-r1-ptr-deref".to_string(),
        crate_name: "target_lib".to_string(),
        file_path: "bench/pairs/r1_ptr_deref.rs".to_string(),
        start_line: 23,
        end_line: 27,
        kind: CandidateKind::UnsafeBlock,
        code_snippet: "unsafe { *self.ptr.add(offset) }".to_string(),
        enclosing_item: Some("RawBuffer::read_offset".to_string()),
        safety_comment: None,
        trait_name: None,
        rule: Some("R1".to_string()),
        visibility: Some("pub".to_string()),
    };

    let hypothesis = warrant_llm::Hypothesis {
        id: "hyp-r1-ptr".to_string(),
        candidate_id: candidate.id.clone(),
        ub_class: warrant_llm::UbClass::UbOob,
        explanation: "Unchecked pointer arithmetic and dereference beyond slice allocation".to_string(),
        harness_code: r#"#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_raw_ptr_oob() {
        let data = vec![10u8, 20, 30, 40];
        let buf = RawBuffer::new(&data);
        let val = buf.read_offset(100);
        std::hint::black_box(val);
    }
}
"#.to_string(),
        suggested_miri_flags: vec!["-Zmiri-strict-provenance".to_string()],
        expected_trigger: "dereferencing pointer failed".to_string(),
    };

    let tmp = tempfile::tempdir().unwrap();
    let runner: Box<dyn ProofRunner> = Box::new(LocalMiriRunner::new(
        Duration::from_secs(60),
        vec!["-Zmiri-strict-provenance".to_string()],
    ));
    let pipeline = VerificationPipeline::new(runner);

    let bundle = pipeline
        .verify_and_bundle(&target_code, &candidate, &hypothesis, tmp.path(), true)
        .await
        .expect("Verification must succeed");

    assert_eq!(bundle.finding.status, VerificationStatus::ProvenBug);
    assert_eq!(bundle.verdict, "PROVEN_PUBLIC");
    assert!(bundle.location_attestation.verified_in_trace);
    assert!(tmp.path().join("bundle.json").exists());
    assert!(tmp.path().join("harness.rs").exists());
    assert!(tmp.path().join("target_with_harness.rs").exists());
    assert!(tmp.path().join("reproduce.sh").exists());
    assert!(tmp.path().join("reproduce.bat").exists());
}

#[tokio::test]
async fn test_e2e_safe_control_unproven_with_evidence_bundle() {
    let target_code = load_target_fixture("bench/pairs/safe_control.rs");
    assert!(target_code.contains("pub struct SafeBuffer"));

    let candidate = Candidate {
        id: "c-safe-control".to_string(),
        crate_name: "target_lib".to_string(),
        file_path: "bench/pairs/safe_control.rs".to_string(),
        start_line: 14,
        end_line: 18,
        kind: CandidateKind::UnsafeBlock,
        code_snippet: "self.data.get(index).copied()".to_string(),
        enclosing_item: Some("SafeBuffer::read_elem".to_string()),
        safety_comment: None,
        trait_name: None,
        rule: Some("R1".to_string()),
        visibility: Some("pub".to_string()),
    };

    let hypothesis = warrant_llm::Hypothesis {
        id: "hyp-safe-control".to_string(),
        candidate_id: candidate.id.clone(),
        ub_class: warrant_llm::UbClass::UbOob,
        explanation: "Out-of-bounds index on safe method".to_string(),
        harness_code: r#"#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_safe_control() {
        let buffer = SafeBuffer::new(vec![10, 20, 30, 40]);
        let val = buffer.read_elem(8);
        assert_eq!(val, None);
    }
}
"#.to_string(),
        suggested_miri_flags: vec!["-Zmiri-tree-borrows".to_string()],
        expected_trigger: "precondition".to_string(),
    };

    let tmp = tempfile::tempdir().unwrap();
    let runner: Box<dyn ProofRunner> = Box::new(LocalMiriRunner::default());
    let pipeline = VerificationPipeline::new(runner);

    let bundle = pipeline
        .verify_and_bundle(&target_code, &candidate, &hypothesis, tmp.path(), true)
        .await
        .expect("Verification must succeed");

    assert_ne!(bundle.finding.status, VerificationStatus::ProvenBug);
    assert_eq!(bundle.verdict, "UNPROVEN");
    assert!(tmp.path().join("bundle.json").exists());
}

#[tokio::test]
async fn test_e2e_standalone_reproduction_script() {
    let target_code = load_target_fixture("bench/pairs/r1_reference.rs");
    let candidate = Candidate {
        id: "c-r1-repro".to_string(),
        crate_name: "target_lib".to_string(),
        file_path: "bench/pairs/r1_reference.rs".to_string(),
        start_line: 18,
        end_line: 23,
        kind: CandidateKind::UnsafeBlock,
        code_snippet: "unsafe { *self.data.get_unchecked(index) }".to_string(),
        enclosing_item: Some("FastBuffer::read_unchecked_elem".to_string()),
        safety_comment: None,
        trait_name: None,
        rule: Some("R1".to_string()),
        visibility: Some("pub".to_string()),
    };

    let hypothesis = warrant_llm::Hypothesis {
        id: "hyp-r1-repro".to_string(),
        candidate_id: candidate.id.clone(),
        ub_class: warrant_llm::UbClass::UbOob,
        explanation: "Slice get_unchecked OOB".to_string(),
        harness_code: r#"#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_next_frame_oob() {
        let buffer = FastBuffer::new(vec![10, 20, 30, 40]);
        let val = buffer.read_unchecked_elem(8);
        std::hint::black_box(val);
    }
}
"#.to_string(),
        suggested_miri_flags: vec!["-Zmiri-tree-borrows".to_string()],
        expected_trigger: "slice::get_unchecked".to_string(),
    };

    let tmp = tempfile::tempdir().unwrap();
    let runner: Box<dyn ProofRunner> = Box::new(LocalMiriRunner::default());
    let pipeline = VerificationPipeline::new(runner);

    pipeline
        .verify_and_bundle(&target_code, &candidate, &hypothesis, tmp.path(), true)
        .await
        .expect("Bundle creation must succeed");

    // Execute standalone reproducer script
    let (exit_code, stdout, _stderr) = warrant_verify::run_reproduction_script(tmp.path())
        .expect("Reproduction script must run");

    assert_eq!(exit_code, 0);
    assert!(stdout.contains("[VERIFIED]") || stdout.contains("Reproducer successfully confirmed failure"));
}

