//! # eval_gate1
//!
//! Benchmark evaluation tool for Project WARRANT Gate 1 (SPEC.md Section 8 & Section 10 Task T5).
//! Measures valid@1, valid@4, proof rate, and false proof rate across 8 seeded bug fixtures
//! and their sound control counterparts, enforcing autonomous repair loops (max 4 rounds)
//! and tracking token expenditures via CostTracker.

use anyhow::{Context, Result};
use clap::Parser;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use warrant_cli::eval_summary::EvalSummary;
use warrant_extract::extract_candidates_from_file;
use warrant_llm::{
    Candidate, CandidateHarness, CostTracker, LlmClient, MetaApiConfig,
    MetaClient, MockLlmClient, RepairEngine, VerificationStatus,
};
use warrant_verify::{ClopperPearsonInterval, LocalMiriRunner, ProofRunner, VerificationPipeline};

#[derive(Parser, Debug)]
#[command(author, version, about = "WARRANT Gate 1 Benchmark & Repair Loop Evaluation")]
struct Args {
    /// LLM backend to utilize ('mock', 'meta', or 'meta-api')
    #[arg(long, default_value = "mock")]
    backend: String,

    /// Directory to output generated evidence bundles
    #[arg(long, default_value = "evidence/gate1")]
    evidence_dir: PathBuf,

    /// Verbose logging output
    #[arg(short, long)]
    verbose: bool,
}

#[allow(dead_code)]
#[derive(Debug)]
struct CaseEvalResult {
    id: String,
    name: String,
    rule: String,
    valid_at_1: bool,
    valid_at_4: bool,
    rounds_executed: usize,
    proven: bool,
    status: VerificationStatus,
    duration_ms: u128,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    println!("============================================================");
    println!("  WARRANT TASK T5: GATE 1 BENCHMARK EVALUATION");
    println!("  Target: 8 Seeded Cases (4 R1, 4 R2) + Sound Control Pairs");
    println!("  Threshold: valid@4 >= 50%, false_proof == 0");
    println!("============================================================\n");

    let cost_tracker = Arc::new(CostTracker::default());
    let runner: Box<dyn ProofRunner> = Box::new(LocalMiriRunner::default());
    let pipeline = Arc::new(VerificationPipeline::new(runner));
    let repair_engine = RepairEngine::new(4);

    let backend_key = match args.backend.as_str() {
        "meta" | "meta-api" => "meta",
        _ => "mock",
    };

    let meta_client_opt: Option<Arc<MetaClient>> = if backend_key == "meta" {
        dotenvy::dotenv().ok();
        println!("[BACKEND] Using Live Meta Model API (MetaClient: muse-spark-1.3)...");
        Some(Arc::new(MetaClient::new(
            MetaApiConfig::default(),
            cost_tracker.clone(),
        )?))
    } else {
        println!("[BACKEND] Using Deterministic Mock Client (mock-client)...");
        None
    };
    let mock_client = Arc::new(MockLlmClient::default());
    let client_ref: &dyn LlmClient = match &meta_client_opt {
        Some(mc) => mc.as_ref(),
        None => mock_client.as_ref(),
    };

    // -------------------------------------------------------------
    // Define the 8 Seeded Mutant Cases (Gate 1 Set)
    // -------------------------------------------------------------
    let seeded_cases = [
        ("Case 1 (R1)", "bench/pairs/r1_reference.rs", "R1", "FastBuffer slice get_unchecked OOB"),
        ("Case 2 (R1)", "bench/pairs/r1_ptr_deref.rs", "R1", "RawBuffer raw pointer arithmetic OOB"),
        ("Case 3 (R1)", "bench/pairs/r1_from_raw_parts.rs", "R1", "SliceWindow from_raw_parts overread"),
        ("Case 4 (R1)", "bench/pairs/r1_vec_queue.rs", "R1", "FastQueue get_unchecked_mut (Repair Test)"),
        ("Case 5 (R2)", "bench/pairs/r2_reference.rs", "R2", "SharedCounter Cell<usize> unsafe Sync"),
        ("Case 6 (R2)", "bench/pairs/r2_rc_sync.rs", "R2", "SharedRc Rc<String> unsafe Send"),
        ("Case 7 (R2)", "bench/pairs/r2_unsync_buffer.rs", "R2", "UnsyncBuffer raw pointer unsafe Sync"),
        ("Case 8 (R2)", "bench/pairs/r2_refcell_sync.rs", "R2", "SharedRefCell RefCell<u64> unsafe Sync"),
    ];

    let mut results = Vec::new();
    let overall_start = Instant::now();

    println!("[EVALUATION] Commencing evaluation of 8 seeded mutant cases...\n");

    for (idx, (case_id, file_path, rule, desc)) in seeded_cases.iter().enumerate() {
        let case_start = Instant::now();
        println!("------------------------------------------------------------");
        println!("Evaluating {}: {} [{}]", case_id, desc, file_path);

        let path = Path::new(file_path);
        let raw_code = fs::read_to_string(path)
            .with_context(|| format!("Failed to read target fixture: {}", file_path))?;

        let target_code = if let Some(pos) = raw_code.find("// ==============================================================================") {
            raw_code[..pos].trim_end().to_string()
        } else if let Some(pos) = raw_code.find("#[cfg(test)]") {
            raw_code[..pos].trim_end().to_string()
        } else {
            raw_code.clone()
        };

        // 1. Extract Candidate
        let candidates = extract_candidates_from_file(path, "target_lib")?;
        if candidates.is_empty() {
            println!("  [FAIL] No candidate extracted from {}", file_path);
            continue;
        }
        let mut candidate = candidates[0].clone();
        candidate.id = format!("c-gate1-{:02}", idx + 1);
        candidate.rule = Some((*rule).to_string());
        if let Some(ref mc) = meta_client_opt {
            mc.register_module_source(&candidate.id, &target_code);
        }
        println!("  Candidate extracted: id={} span={:?} enclosing={:?}", candidate.id, candidate.line_span(), candidate.enclosing_item);

        // 2. Propose initial hypothesis (if JSON/validation fails on Round 1, feed into repair loop)
        let initial_hyp = match client_ref.propose_hypothesis(&candidate).await {
            Ok(h) => {
                println!("  Initial hypothesis generated: {}", h.id);
                h
            }
            Err(e) => {
                println!("  Initial hypothesis failed validation ({}), entering repair loop...", e);
                warrant_llm::Hypothesis {
                    id: format!("hyp-fallback-{}", idx + 1),
                    candidate_id: candidate.id.clone(),
                    ub_class: if *rule == "R2" {
                        warrant_llm::UbClass::DataRace
                    } else {
                        warrant_llm::UbClass::UbOob
                    },
                    explanation: format!("Initial round parse error: {}", e),
                    harness_code: format!("// initial hypothesis failed to parse: {}", e),
                    suggested_miri_flags: vec![],
                    expected_trigger: String::new(),
                }
            }
        };

        // 3. Run Autonomous Repair Loop (Max 4 rounds)
        let pipeline_clone = pipeline.clone();
        let target_code_clone = target_code.clone();
        let target_code_compile = target_code.clone();

        let session = repair_engine
            .execute(
                client_ref,
                &candidate,
                initial_hyp.clone(),
                &cost_tracker,
                |harness_code| {
                    pipeline_clone
                        .injector
                        .prepare_harness(harness_code)
                        .map(|_| ())
                        .map_err(|e| format!("{}", e))
                },
                |harness_code| {
                    let pipe = pipeline_clone.clone();
                    let t_code = target_code_compile.clone();
                    let h_code = harness_code.to_string();
                    async move {
                        let out = pipe.check_compilation(&t_code, &h_code).await;
                        match out {
                            Ok(output) => {
                                if output.exit_code == Some(0) && !output.stderr.contains("error[E") {
                                    Ok(true)
                                } else {
                                    Err(output.stderr)
                                }
                            }
                            Err(e) => Err(format!("{}", e)),
                        }
                    }
                },
            )
            .await?;

        println!(
            "  Repair Loop finished in {} round(s). valid@1: {} | valid@4: {}",
            session.total_rounds_executed, session.valid_at_1, session.valid_at_4
        );
        for r in &session.rounds {
            if let Some(ref err) = r.error_message {
                println!("    Round {} err: {}", r.round, err.lines().take(3).collect::<Vec<_>>().join(" "));
            }
        }

        // 4. Verify under Miri if valid@4 succeeded
        let mut proven = false;
        let mut final_status = VerificationStatus::Inconclusive;

        if let Some(ref valid_hyp) = session.final_hypothesis {
            let candidate_harness = CandidateHarness {
                rule: rule.to_string(),
                explanation: valid_hyp.explanation.clone(),
                harness_code: valid_hyp.harness_code.clone(),
                estimated_budget_usd: 0.0,
            };

            let outcome = pipeline
                .verify(&target_code_clone, &candidate, &candidate_harness)
                .await?;

            final_status = outcome.status.clone();
            proven = outcome.is_proven();

            println!(
                "  Miri Execution: status={:?} signature={:?} proven={}",
                outcome.status, outcome.signature_matched, proven
            );

            // Export bundle
            let bundle_dir = args.evidence_dir.join(case_id.replace([' ', '(', ')'], "_"));
            let _ = pipeline.verify_and_bundle(
                &target_code_clone,
                &candidate,
                valid_hyp,
                &bundle_dir,
                true,
            ).await;
        } else {
            println!("  [WARN] Candidate failed to compile or pass static filter within 4 rounds");
        }

        let elapsed = case_start.elapsed().as_millis();
        results.push(CaseEvalResult {
            id: case_id.to_string(),
            name: desc.to_string(),
            rule: rule.to_string(),
            valid_at_1: session.valid_at_1,
            valid_at_4: session.valid_at_4,
            rounds_executed: session.total_rounds_executed,
            proven,
            status: final_status,
            duration_ms: elapsed,
        });
    }

    // -------------------------------------------------------------
    // Sound Controls Evaluation (Confirm False Proof == 0)
    // -------------------------------------------------------------
    println!("\n------------------------------------------------------------");
    println!("[SOUND CONTROLS] Evaluating 8 sound partner fixtures for false proof check...");
    let sound_controls = [
        "bench/pairs/r1_reference_safe.rs",
        "bench/pairs/r1_ptr_deref_safe.rs",
        "bench/pairs/r1_from_raw_parts_safe.rs",
        "bench/pairs/r1_vec_queue_safe.rs",
        "bench/pairs/r2_reference_safe.rs",
        "bench/pairs/r2_rc_sync_safe.rs",
        "bench/pairs/r2_unsync_buffer_safe.rs",
        "bench/pairs/r2_refcell_sync_safe.rs",
    ];

    let mut false_proof_count = 0;
    for control_path in sound_controls {
        let path = Path::new(control_path);
        let target_code = fs::read_to_string(path)?;
        let _cand = Candidate {
            id: format!("cand-control-{}", path.file_stem().unwrap().to_string_lossy()),
            crate_name: "control_crate".to_string(),
            file_path: control_path.to_string(),
            start_line: 1,
            end_line: 1,
            kind: warrant_llm::CandidateKind::UnsafeBlock,
            code_snippet: "// sound control".to_string(),
            enclosing_item: None,
            safety_comment: None,
            trait_name: None,
            rule: Some("R1".to_string()),
            visibility: Some("pub".to_string()),
        };

        // Run Miri directly on sound control
        let (temp_dir, pkg_path) = {
            let temp = tempfile::tempdir()?;
            let p = temp.path().to_path_buf();
            fs::write(p.join("Cargo.toml"), "[package]\nname = \"target\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[lib]\npath = \"src/lib.rs\"\n")?;
            fs::create_dir_all(p.join("src"))?;
            fs::write(p.join("src").join("lib.rs"), &target_code)?;
            (temp, p)
        };

        let miri_out = pipeline.runner.run_miri(&pkg_path, "warrant_proof::").await?;
        let (status, _class) = warrant_verify::classify_ub_signature(&miri_out, "R1");
        if status == VerificationStatus::ProvenBug {
            println!("  [FALSE PROOF DETECTED] in {}", control_path);
            false_proof_count += 1;
        } else {
            println!("  [SOUND CLEAN] {} verified with exit code {:?}", control_path, miri_out.exit_code);
        }
        drop(temp_dir);
    }

    // -------------------------------------------------------------
    // Gate 1 Metrics Summary Table
    // -------------------------------------------------------------
    let total_cases = results.len();
    let valid_1_count = results.iter().filter(|r| r.valid_at_1).count();
    let valid_4_count = results.iter().filter(|r| r.valid_at_4).count();
    let proven_count = results.iter().filter(|r| r.proven).count();

    let valid_1_pct = (valid_1_count as f64 / total_cases as f64) * 100.0;
    let valid_4_pct = (valid_4_count as f64 / total_cases as f64) * 100.0;
    let proof_rate_pct = (proven_count as f64 / total_cases as f64) * 100.0;

    let gate_1_passed = valid_4_pct >= 50.0 && false_proof_count == 0;

    println!("\n============================================================");
    println!("             GATE 1 BENCHMARK EVALUATION RESULTS            ");
    println!("============================================================");
    println!("| Case ID     | Rule | valid@1 | valid@4 | Rounds | Proven | Status     |");
    println!("|-------------|------|---------|---------|--------|--------|------------|");
    for r in &results {
        println!(
            "| {:<11} | {:<4} | {:<7} | {:<7} | {:<6} | {:<6} | {:<10?} |",
            r.id, r.rule, r.valid_at_1, r.valid_at_4, r.rounds_executed, r.proven, r.status
        );
    }
    println!("============================================================");
    println!("METRICS SUMMARY (GATE 1):");
    println!("  * Total Seeded Cases:        {}", total_cases);
    println!(
        "  * valid@1 Rate:              {:.1}% ({}/{}) 95% CI {}",
        valid_1_pct,
        valid_1_count,
        total_cases,
        ClopperPearsonInterval::compute_95(valid_1_count, total_cases).format_interval_pct()
    );
    println!(
        "  * valid@4 Rate:              {:.1}% ({}/{}) 95% CI {}",
        valid_4_pct,
        valid_4_count,
        total_cases,
        ClopperPearsonInterval::compute_95(valid_4_count, total_cases).format_interval_pct()
    );
    println!(
        "  * Proof Rate:                {:.1}% ({}/{}) 95% CI {}",
        proof_rate_pct,
        proven_count,
        total_cases,
        ClopperPearsonInterval::compute_95(proven_count, total_cases).format_interval_pct()
    );
    println!(
        "  * False Proofs (Safe Pairs): {} (Target: 0) 95% CI {}",
        false_proof_count,
        ClopperPearsonInterval::compute_95(false_proof_count, 8).format_interval_pct()
    );
    println!("  * Total Duration:            {:.2}s", overall_start.elapsed().as_secs_f64());

    let call_log = cost_tracker.call_log();
    let totals = cost_tracker.call_totals();
    if !call_log.is_empty() {
        println!("------------------------------------------------------------");
        println!("LIVE HTTP MODEL CALL LEDGER (FROM API RESPONSE `usage` JSON):");
        println!(
            "{:<30} | {:<8} | {:<10} | {:<10} | {:<12}",
            "Call Label", "Prompt", "Completion", "Latency", "Cost (USD)"
        );
        println!("{}", "-".repeat(82));
        for rec in &call_log {
            println!(
                "{:<30} | {:<8} | {:<10} | {:<7} ms | ${:<11.6}",
                rec.label,
                rec.prompt_tokens,
                rec.completion_tokens,
                rec.latency_ms,
                rec.cost_micro_usd as f64 / 1_000_000.0
            );
        }
        println!("{}", "-".repeat(82));
    }

    println!("------------------------------------------------------------");
    println!("COST TRACKER & BUDGET ACCOUNTING:");
    println!("  * HTTP Model Calls:          {} (missing usage: {})", totals.calls, totals.calls_without_usage);
    println!("  * Total Prompt Tokens:       {}", totals.prompt_tokens);
    println!("  * Total Completion Tokens:   {}", totals.completion_tokens);
    println!("  * Mean Call Latency:         {} ms", totals.mean_latency_ms());
    println!("  * Cumulative Spend:          ${:.6} USD", cost_tracker.total_spent());
    println!("  * Remaining Circuit Breaker: ${:.6} USD (Stop ceiling: $105.00)", cost_tracker.remaining_budget());
    println!("------------------------------------------------------------");
    println!(
        "GATE 1 DECISION: {}",
        if gate_1_passed {
            "PASSED (Threshold: valid@4 >= 50% and false_proof == 0)"
        } else {
            "FAILED (Threshold: valid@4 >= 50% and false_proof == 0)"
        }
    );
    println!("============================================================");

    let mut counts = BTreeMap::new();
    counts.insert("valid_at_1".to_string(), valid_1_count);
    counts.insert("valid_at_4".to_string(), valid_4_count);
    counts.insert("proven".to_string(), proven_count);
    counts.insert("false_proofs".to_string(), false_proof_count);
    let summary = EvalSummary {
        experiment: "gate1".to_string(),
        backend: backend_key.to_string(),
        model: if backend_key == "meta" { "muse-spark-1.3".to_string() } else { "mock".to_string() },
        n: total_cases,
        counts,
        calls: totals.calls,
        calls_without_usage: totals.calls_without_usage,
        prompt_tokens: totals.prompt_tokens,
        completion_tokens: totals.completion_tokens,
        mean_latency_ms: totals.mean_latency_ms(),
        cost_usd: totals.cost_usd(),
    };
    if let Ok(saved) = summary.save() {
        println!("  [SUMMARY SAVED] {}", saved.display());
    }

    Ok(())
}
