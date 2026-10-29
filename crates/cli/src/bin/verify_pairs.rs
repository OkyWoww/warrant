//! # Verify Pairs Benchmark Set (Task T7)
//!
//! Automated deterministic verification utility for the 16-pair (32-file) benchmark dataset
//! in `bench/pairs/`. Executes both `<id>_safe.rs` and `<id>_mutant.rs` under Miri via
//! `VerificationPipeline`, verifying that:
//! 1. All 16 `_safe.rs` files pass Miri cleanly with Exit Code 0 (`SoundSafe`).
//! 2. All 16 `_mutant.rs` files fail Miri with the exact expected R1 (`UbOob`) or R2 (`DataRace`)
//!    Undefined Behavior signature and location attestation (`ProvenBug`).

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;
use std::time::Duration;
use warrant_extract::extract_candidates_from_file;
use warrant_llm::{Candidate, CandidateKind, Hypothesis, UbClass, VerificationStatus};
use warrant_verify::{
    extract_matched_signature, HarnessInjector, LocalMiriRunner, ProofRunner, VerificationPipeline,
};

struct PairSpec {
    id: &'static str,
    rule: &'static str,
    expected_ub: UbClass,
    staged_or_concurrent: &'static str,
}

const PAIRS: &[PairSpec] = &[
    // 8 R1 Pairs (Memory Precondition & Out-of-Bounds)
    PairSpec {
        id: "r1_01_slice_get",
        rule: "R1",
        expected_ub: UbClass::UbOob,
        staged_or_concurrent: "Single-call OOB",
    },
    PairSpec {
        id: "r1_02_ptr_arithmetic",
        rule: "R1",
        expected_ub: UbClass::UbOob,
        staged_or_concurrent: "Single-call OOB",
    },
    PairSpec {
        id: "r1_03_from_raw_parts",
        rule: "R1",
        expected_ub: UbClass::UbOob,
        staged_or_concurrent: "Single-call OOB",
    },
    PairSpec {
        id: "r1_04_vec_queue",
        rule: "R1",
        expected_ub: UbClass::UbOob,
        staged_or_concurrent: "Staged state (new -> push -> drain -> pop)",
    },
    PairSpec {
        id: "r1_05_copy_nonoverlapping",
        rule: "R1",
        expected_ub: UbClass::UbOob,
        staged_or_concurrent: "Staged state (new -> write_patch -> read_exceeding)",
    },
    PairSpec {
        id: "r1_06_byte_offset",
        rule: "R1",
        expected_ub: UbClass::UbOob,
        staged_or_concurrent: "Staged state (new -> advance -> read_current)",
    },
    PairSpec {
        id: "r1_07_chunk_reader",
        rule: "R1",
        expected_ub: UbClass::UbOob,
        staged_or_concurrent: "Staged state (new -> skip_chunks -> take_chunk)",
    },
    PairSpec {
        id: "r1_08_lookup_table",
        rule: "R1",
        expected_ub: UbClass::UbOob,
        staged_or_concurrent: "Staged state (new -> write_slot_mut)",
    },
    // 8 R2 Pairs (Send/Sync Concurrency & Data Race)
    PairSpec {
        id: "r2_01_cell_sync",
        rule: "R2",
        expected_ub: UbClass::DataRace,
        staged_or_concurrent: "Multithreaded (thread::scope)",
    },
    PairSpec {
        id: "r2_02_rc_send",
        rule: "R2",
        expected_ub: UbClass::DataRace,
        staged_or_concurrent: "Multithreaded (thread::scope)",
    },
    PairSpec {
        id: "r2_03_unsync_buffer",
        rule: "R2",
        expected_ub: UbClass::DataRace,
        staged_or_concurrent: "Multithreaded (thread::scope)",
    },
    PairSpec {
        id: "r2_04_refcell_sync",
        rule: "R2",
        expected_ub: UbClass::DataRace,
        staged_or_concurrent: "Multithreaded (thread::scope)",
    },
    PairSpec {
        id: "r2_05_split_borrow",
        rule: "R2",
        expected_ub: UbClass::DataRace,
        staged_or_concurrent: "Multithreaded (thread::scope)",
    },
    PairSpec {
        id: "r2_06_atomic_relaxed_race",
        rule: "R2",
        expected_ub: UbClass::DataRace,
        staged_or_concurrent: "Multithreaded (thread::scope)",
    },
    PairSpec {
        id: "r2_07_lazy_init",
        rule: "R2",
        expected_ub: UbClass::DataRace,
        staged_or_concurrent: "Multithreaded (thread::scope)",
    },
    PairSpec {
        id: "r2_08_alias_mut",
        rule: "R2",
        expected_ub: UbClass::DataRace,
        staged_or_concurrent: "Multithreaded (thread::scope)",
    },
];

fn split_source_and_harness(file_path: &Path) -> Result<(String, String)> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read {}", file_path.display()))?;
    let idx = content
        .find("#[cfg(test)]")
        .with_context(|| format!("Missing #[cfg(test)] harness in {}", file_path.display()))?;
    let target = content[..idx].trim_end().to_string();
    let harness = content[idx..].trim().to_string();
    Ok((target, harness))
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("==========================================================================================");
    println!("  WARRANT TASK T7: 16-PAIR (32-FILE) BENCHMARK VERIFICATION SUITE");
    println!("==========================================================================================");

    let runner: Box<dyn ProofRunner> = Box::new(LocalMiriRunner::new(
        Duration::from_secs(60),
        vec![
            "-Zmiri-tree-borrows".to_string(),
            "-Zmiri-strict-provenance".to_string(),
            "-Zmiri-preemption-rate=0".to_string(),
        ],
    ));
    let pipeline = VerificationPipeline::new(runner);
    let injector = HarnessInjector::new();

    let mut safe_passed = 0usize;
    let mut mutant_proven = 0usize;

    println!(
        "\n{:<3} | {:<26} | {:<4} | {:<12} | {:<14} | {:<32}",
        "#", "Pair ID", "Rule", "Safe Status", "Mutant Status", "Matched Miri Signature"
    );
    println!("{}", "-".repeat(104));

    let filter_arg = std::env::args().nth(1);

    for (i, spec) in PAIRS.iter().enumerate() {
        if let Some(ref f) = filter_arg {
            if !spec.id.contains(f.as_str()) {
                continue;
            }
        }
        let safe_path = Path::new("bench/pairs").join(format!("{}_safe.rs", spec.id));
        let mutant_path = Path::new("bench/pairs").join(format!("{}_mutant.rs", spec.id));

        // 1. Verify Safe Version
        let (safe_target, safe_harness) = split_source_and_harness(&safe_path)?;
        injector
            .prepare_harness(&safe_harness)
            .with_context(|| format!("Safe harness failed static filter in {}", spec.id))?;

        let dummy_cand = Candidate {
            id: format!("safe-{}", spec.id),
            crate_name: "bench_pairs".to_string(),
            file_path: safe_path.display().to_string(),
            start_line: 1,
            end_line: 10,
            kind: CandidateKind::UnsafeBlock,
            code_snippet: String::new(),
            enclosing_item: None,
            safety_comment: None,
            trait_name: None,
            rule: Some(spec.rule.to_string()),
            visibility: Some("pub".to_string()),
        };
        let safe_hyp = Hypothesis {
            id: format!("hyp-safe-{}", spec.id),
            candidate_id: dummy_cand.id.clone(),
            ub_class: spec.expected_ub.clone(),
            explanation: format!("Safe control verification for {}", spec.id),
            harness_code: safe_harness,
            suggested_miri_flags: vec![],
            expected_trigger: String::new(),
        };

        let safe_bundle = pipeline
            .verify_hypothesis(&safe_target, &dummy_cand, &safe_hyp)
            .await
            .with_context(|| format!("Miri execution failed on {}", safe_path.display()))?;

        // 2. Verify Mutant Version
        let (mutant_target, mutant_harness) = split_source_and_harness(&mutant_path)?;
        injector
            .prepare_harness(&mutant_harness)
            .with_context(|| format!("Mutant harness failed static filter in {}", spec.id))?;

        let candidates = extract_candidates_from_file(&mutant_path, "bench_pairs")
            .with_context(|| format!("Extractor failed on {}", mutant_path.display()))?;
        let rule_cand = candidates
            .iter()
            .find(|c| c.rule.as_deref() == Some(spec.rule))
            .or_else(|| candidates.first())
            .with_context(|| format!("No candidate extracted from {}", mutant_path.display()))?;

        let mutant_hyp = Hypothesis {
            id: format!("hyp-mutant-{}", spec.id),
            candidate_id: rule_cand.id.clone(),
            ub_class: spec.expected_ub.clone(),
            explanation: format!(
                "Mutant verification ({}) for {}",
                spec.staged_or_concurrent, spec.id
            ),
            harness_code: mutant_harness,
            suggested_miri_flags: vec![],
            expected_trigger: String::new(),
        };

        let mutant_bundle = pipeline
            .verify_hypothesis(&mutant_target, rule_cand, &mutant_hyp)
            .await
            .with_context(|| format!("Miri execution failed on {}", mutant_path.display()))?;

        let sig = extract_matched_signature(&mutant_bundle.stderr, spec.rule)
            .unwrap_or_else(|| "NONE".to_string());

        if safe_bundle.finding.status == VerificationStatus::SoundSafe {
            safe_passed += 1;
        }
        if mutant_bundle.finding.status == VerificationStatus::ProvenBug {
            mutant_proven += 1;
        }

        println!(
            "{:<3} | {:<26} | {:<4} | {:<12} | {:<14} | {:<32}",
            i + 1,
            spec.id,
            spec.rule,
            format!("{:?}", safe_bundle.finding.status),
            format!("{:?}", mutant_bundle.finding.status),
            sig
        );

        if safe_bundle.finding.status != VerificationStatus::SoundSafe {
            bail!(
                "Safe benchmark {} did not pass cleanly! Status: {:?}\nLog:\n{}",
                spec.id,
                safe_bundle.finding.status,
                safe_bundle.stderr
            );
        }
        if mutant_bundle.finding.status != VerificationStatus::ProvenBug {
            bail!(
                "Mutant benchmark {} was not proven! Status: {:?}\nStdout:\n{}\nStderr:\n{}",
                spec.id,
                mutant_bundle.finding.status,
                mutant_bundle.stdout,
                mutant_bundle.stderr
            );
        }
    }

    println!("{}", "-".repeat(104));
    println!(
        "SUMMARY: {}/{} Safe Pairs = SoundSafe (Exit 0) | {}/{} Mutant Pairs = ProvenBug (Miri UB Verified)",
        safe_passed,
        PAIRS.len(),
        mutant_proven,
        PAIRS.len()
    );
    println!("==========================================================================================");

    Ok(())
}
