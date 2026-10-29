//! # Verify Gold CLI (Task T3: Verifier Core with Hand-Written Harnesses)
//!
//! Validates the verifier core on 3 hand-written gold reproducer cases (producing PROVEN),
//! 1 safe control case (producing UNPROVEN), generates evidence bundles for each, and
//! executes the standalone `reproduce.sh` / `reproduce.bat` to independently verify reproducibility.
//!
//! Strictly adheres to SPEC.md Section 5.4, Section 10 (Task T3), and AGENTS.md.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use warrant_llm::{Candidate, CandidateKind, Hypothesis, UbClass, VerificationStatus};
use warrant_verify::{
    run_reproduction_script, LocalMiriRunner, ProofRunner, VerificationPipeline,
};

fn extract_target_source(file_path: &Path) -> Result<String> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read fixture: {}", file_path.display()))?;

    // Strip out the reference harness module at the bottom so we test clean injection
    if let Some(idx) = content.find("// ==============================================================================") {
        Ok(content[..idx].trim_end().to_string())
    } else if let Some(idx) = content.find("#[cfg(test)]") {
        Ok(content[..idx].trim_end().to_string())
    } else {
        Ok(content)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("============================================================");
    println!("  WARRANT TASK T3: VERIFIER CORE & EVIDENCE BUNDLE SUITE");
    println!("============================================================");

    let runner: Box<dyn ProofRunner> = Box::new(LocalMiriRunner::new(
        Duration::from_secs(60),
        vec![
            "-Zmiri-tree-borrows".to_string(),
            "-Zmiri-strict-provenance".to_string(),
            "-Zmiri-preemption-rate=0".to_string(),
        ],
    ));
    let pipeline = VerificationPipeline::new(runner);

    let base_evidence_dir = PathBuf::from("evidence");
    fs::create_dir_all(&base_evidence_dir)?;

    // -----------------------------------------------------------------
    // 1. Kasus Gold 1: R1 Slice Precondition OOB (FastBuffer)
    // -----------------------------------------------------------------
    println!("\n[1/5] Verifying Gold Case 1: R1 Slice Precondition OOB (FastBuffer)...");
    let r1_path = Path::new("bench/pairs/r1_reference.rs");
    let r1_target = extract_target_source(r1_path)?;

    let cand_r1 = Candidate {
        id: "c-gold-r1-slice".to_string(),
        crate_name: "target_crate".to_string(),
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

    let hyp_r1 = Hypothesis {
        id: "hyp-r1-gold".to_string(),
        candidate_id: cand_r1.id.clone(),
        ub_class: UbClass::UbOob,
        explanation: "Caller passes index exceeding vector length, violating slice::get_unchecked precondition.".to_string(),
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
        expected_trigger: "unsafe precondition(s) violated: slice::get_unchecked".to_string(),
    };

    let bundle_r1_dir = base_evidence_dir.join("gold_r1_slice_oob");
    let bundle_r1 = pipeline
        .verify_and_bundle(&r1_target, &cand_r1, &hyp_r1, &bundle_r1_dir, true)
        .await
        .context("Gold Case 1 verification failed")?;

    println!("  [STATUS] {:?}", bundle_r1.finding.status);
    println!("  [VERDICT] {}", bundle_r1.verdict);
    println!("  [LOCATION ATTESTATION] {}", bundle_r1.location_attestation.verified_in_trace);
    println!("  [HARNESS SHA256] {}", bundle_r1.harness_sha256);
    println!("  [BUNDLE DIRECTORY] {}", bundle_r1_dir.display());
    assert_eq!(bundle_r1.finding.status, VerificationStatus::ProvenBug);
    assert_eq!(bundle_r1.verdict, "PROVEN_PUBLIC");
    assert!(bundle_r1.location_attestation.verified_in_trace);

    // -----------------------------------------------------------------
    // 2. Kasus Gold 2: R1 Raw Pointer Arithmetic OOB (RawBuffer)
    // -----------------------------------------------------------------
    println!("\n[2/5] Verifying Gold Case 2: R1 Raw Pointer Arithmetic OOB (RawBuffer)...");
    let r1_ptr_path = Path::new("bench/pairs/r1_ptr_deref.rs");
    let r1_ptr_target = extract_target_source(r1_ptr_path)?;

    let cand_r1_ptr = Candidate {
        id: "c-gold-r1-ptr".to_string(),
        crate_name: "target_crate".to_string(),
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

    let hyp_r1_ptr = Hypothesis {
        id: "hyp-r1-ptr-gold".to_string(),
        candidate_id: cand_r1_ptr.id.clone(),
        ub_class: UbClass::UbOob,
        explanation: "Caller supplies offset exceeding allocated pointer memory, dereferencing invalid address.".to_string(),
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

    let bundle_r1_ptr_dir = base_evidence_dir.join("gold_r1_ptr_deref");
    let bundle_r1_ptr = pipeline
        .verify_and_bundle(&r1_ptr_target, &cand_r1_ptr, &hyp_r1_ptr, &bundle_r1_ptr_dir, true)
        .await
        .context("Gold Case 2 verification failed")?;

    println!("  [STATUS] {:?}", bundle_r1_ptr.finding.status);
    println!("  [VERDICT] {}", bundle_r1_ptr.verdict);
    println!("  [LOCATION ATTESTATION] {}", bundle_r1_ptr.location_attestation.verified_in_trace);
    println!("  [HARNESS SHA256] {}", bundle_r1_ptr.harness_sha256);
    println!("  [BUNDLE DIRECTORY] {}", bundle_r1_ptr_dir.display());
    assert_eq!(bundle_r1_ptr.finding.status, VerificationStatus::ProvenBug);
    assert_eq!(bundle_r1_ptr.verdict, "PROVEN_PUBLIC");
    assert!(bundle_r1_ptr.location_attestation.verified_in_trace);

    // -----------------------------------------------------------------
    // 3. Kasus Gold 3: R2 Concurrent Data Race (SharedCounter)
    // -----------------------------------------------------------------
    println!("\n[3/5] Verifying Gold Case 3: R2 Concurrent Data Race (SharedCounter)...");
    let r2_path = Path::new("bench/pairs/r2_reference.rs");
    let r2_target = extract_target_source(r2_path)?;

    let cand_r2 = Candidate {
        id: "c-gold-r2-cell".to_string(),
        crate_name: "target_crate".to_string(),
        file_path: "bench/pairs/r2_reference.rs".to_string(),
        start_line: 22,
        end_line: 35,
        kind: CandidateKind::UnsafeImpl,
        code_snippet: "unsafe impl Sync for SharedCounter {}".to_string(),
        enclosing_item: Some("SharedCounter::increment".to_string()),
        safety_comment: None,
        trait_name: Some("Sync".to_string()),
        rule: Some("R2".to_string()),
        visibility: Some("pub".to_string()),
    };

    let hyp_r2 = Hypothesis {
        id: "hyp-r2-gold".to_string(),
        candidate_id: cand_r2.id.clone(),
        ub_class: UbClass::DataRace,
        explanation: "Unsound unsafe impl Sync on Cell enables concurrent unsynchronized reads/writes across threads.".to_string(),
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
        suggested_miri_flags: vec!["-Zmiri-preemption-rate=0".to_string()],
        expected_trigger: "Data race detected between".to_string(),
    };

    let bundle_r2_dir = base_evidence_dir.join("gold_r2_cell_sync");
    let bundle_r2 = pipeline
        .verify_and_bundle(&r2_target, &cand_r2, &hyp_r2, &bundle_r2_dir, true)
        .await
        .context("Gold Case 3 verification failed")?;

    println!("  [STATUS] {:?}", bundle_r2.finding.status);
    println!("  [VERDICT] {}", bundle_r2.verdict);
    println!("  [LOCATION ATTESTATION] {}", bundle_r2.location_attestation.verified_in_trace);
    println!("  [HARNESS SHA256] {}", bundle_r2.harness_sha256);
    println!("  [BUNDLE DIRECTORY] {}", bundle_r2_dir.display());
    assert_eq!(bundle_r2.finding.status, VerificationStatus::ProvenBug);
    assert_eq!(bundle_r2.verdict, "PROVEN_PUBLIC");
    assert!(bundle_r2.location_attestation.verified_in_trace);

    // -----------------------------------------------------------------
    // 4. Kasus Kontrol 1: Safe Control (SafeBuffer)
    // -----------------------------------------------------------------
    println!("\n[4/5] Verifying Control Case 1: Safe Bounds Checking (SafeBuffer)...");
    let safe_path = Path::new("bench/pairs/safe_control.rs");
    let safe_target = extract_target_source(safe_path)?;

    let cand_safe = Candidate {
        id: "c-safe-control".to_string(),
        crate_name: "target_crate".to_string(),
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

    let hyp_safe = Hypothesis {
        id: "hyp-safe-control".to_string(),
        candidate_id: cand_safe.id.clone(),
        ub_class: UbClass::UbOob,
        explanation: "Attempting to provoke bounds violation on sound safe method.".to_string(),
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

    let bundle_safe_dir = base_evidence_dir.join("control_safe_buffer");
    let bundle_safe = pipeline
        .verify_and_bundle(&safe_target, &cand_safe, &hyp_safe, &bundle_safe_dir, true)
        .await
        .context("Safe Control verification failed")?;

    println!("  [STATUS] {:?}", bundle_safe.finding.status);
    println!("  [VERDICT] {}", bundle_safe.verdict);
    println!("  [BUNDLE DIRECTORY] {}", bundle_safe_dir.display());
    assert_ne!(bundle_safe.finding.status, VerificationStatus::ProvenBug);
    assert_eq!(bundle_safe.verdict, "UNPROVEN");

    // -----------------------------------------------------------------
    // 5. Standalone Reproducer Script Execution
    // -----------------------------------------------------------------
    println!("\n[5/5] Executing Standalone Reproducer Script (from Gold Case 1 Bundle)...");
    println!("  [SCRIPT TARGET] {}", bundle_r1_dir.display());

    let (exit_code, stdout, stderr) = run_reproduction_script(&bundle_r1_dir)
        .context("Failed to execute standalone reproducer script")?;

    println!("  [REPRODUCER EXIT CODE] {}", exit_code);
    println!("  [REPRODUCER STDOUT EXCERPT]\n{}", stdout.lines().take(12).map(|l| format!("    {}", l)).collect::<Vec<_>>().join("\n"));
    if !stderr.is_empty() {
        println!("  [REPRODUCER STDERR EXCERPT]\n{}", stderr.lines().take(6).map(|l| format!("    {}", l)).collect::<Vec<_>>().join("\n"));
    }
    assert_eq!(exit_code, 0, "Standalone reproduction script must succeed and confirm reproduction");
    assert!(stdout.contains("[VERIFIED]") || stdout.contains("Reproducer successfully confirmed failure"));

    println!("\n============================================================");
    println!("  TASK T3 VERIFICATION SUITE PASSED SUCCESSFULLY!");
    println!("  - 3 Gold Cases Yielded PROVEN with complete bundles");
    println!("  - 1 Safe Control Yielded UNPROVEN");
    println!("  - Standalone reproduce script reproduced verdict cleanly");
    println!("============================================================");

    Ok(())
}
