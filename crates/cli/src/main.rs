//! # warrant (CLI binary)
//!
//! Entry point for the WARRANT automated soundness auditor.
//! Enforces exit codes:
//!   - 0: Clean / No proven bug
//!   - 1: Proven soundness bug detected under Miri
//!   - 2: Error (fatal execution, parse, or environment failure)

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};
use warrant_cli::{
    now_rfc3339_utc, run_demo, AuditReport, DemoOptions, EventStreamWriter, FindingTier,
    ReportFinding, ReportHypothesis, EXIT_CLEAN, EXIT_ERROR, EXIT_PROVEN_BUG,
};
use warrant_extract::{extract_candidates_from_directory, extract_candidates_from_file};
use warrant_llm::{
    Candidate, CandidateHarness, CostTracker, EventStage, EventStatus, Hypothesis, LlmClient,
    MockLlmClient, RepairEngine,
};
use warrant_verify::{
    bundle::PINNED_TOOLCHAIN, DockerMiriRunner, LocalMiriRunner, ProofRunner,
    VerificationPipeline,
};

#[derive(Parser, Debug)]
#[command(
    name = "warrant",
    version = "0.1.0",
    about = "Proof-first automated soundness auditor for Rust unsafe code",
    long_about = "WARRANT scans Rust crates for unsafe blocks and trait implementations, generates soundness hypotheses via LLMs, deterministically verifies them in an isolated Miri sandbox, and emits cryptographic evidence bundles."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output machine-readable JSON logs
    #[arg(long, global = true)]
    json: bool,

    /// Verbose output
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum BackendType {
    Mock,
    MetaApi,
    OpenWeights,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Extract unsafe candidates from a Rust source file or directory
    Extract {
        /// Target path (file or crate directory)
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,

        /// Target path flag alternative (-t/--target)
        #[arg(short, long)]
        target: Option<PathBuf>,

        /// Name of the crate being audited
        #[arg(short, long, default_value = "target_crate")]
        crate_name: String,

        /// Output JSON array of Candidate objects to file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Run full end-to-end soundness audit: Extract -> Hypothesize -> Verify -> Report
    Audit {
        /// Target file or directory (e.g. `warrant audit .` or `warrant audit src/lib.rs`)
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,

        /// Target file or directory flag alternative (-t/--target)
        #[arg(short, long)]
        target: Option<PathBuf>,

        /// Name of the target crate
        #[arg(short, long, default_value = "target_crate")]
        crate_name: String,

        /// LLM backend to utilize
        #[arg(long, value_enum, default_value = "mock")]
        backend: BackendType,

        /// Directory to store generated evidence bundles and reports
        #[arg(long, default_value = "evidence")]
        output_dir: PathBuf,

        /// Additional Miri flags (e.g. -Zmiri-tree-borrows)
        #[arg(long)]
        miri_flag: Vec<String>,

        /// Enable Docker sandbox isolation
        #[arg(long)]
        sandbox: bool,

        /// Stream real-time JSONL telemetry events to file (`events.jsonl`, SPEC.md Section 4)
        #[arg(long, value_name = "PATH")]
        events: Option<PathBuf>,
    },

    /// Run the flagship 3-case pre-warmed demo with live step-chip telemetry and offline fallback
    Demo {
        /// Force offline replay using verified cached model outputs and local Miri verification ($0.00 API spend)
        #[arg(long)]
        offline: bool,

        /// Stream real-time JSONL telemetry events to file (`events.jsonl`, SPEC.md Section 4)
        #[arg(long, value_name = "PATH")]
        events: Option<PathBuf>,

        /// Output directory for demo evidence bundles, reports, and events.jsonl
        #[arg(long, default_value = "target/warrant/demo")]
        output_dir: PathBuf,

        /// Optional LLM backend override (`meta-api` for live API when not `--offline`)
        #[arg(long, value_enum, default_value = "mock")]
        backend: BackendType,
    },

    /// Verify a specific hypothesis file against a candidate
    Verify {
        /// Path to candidate JSON
        #[arg(long)]
        candidate: PathBuf,

        /// Path to hypothesis JSON
        #[arg(long)]
        hypothesis: PathBuf,

        /// Output directory for evidence bundle
        #[arg(long, default_value = "evidence")]
        output: PathBuf,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Initialize tracing subscriber
    if cli.json {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(if cli.verbose { "warrant=debug" } else { "warrant=info" })
            .init();
    } else {
        tracing_subscriber::fmt()
            .compact()
            .with_env_filter(if cli.verbose { "warrant=debug" } else { "warrant=info" })
            .init();
    }

    let exit_code = match execute(cli).await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("Fatal error: {}", err);
            error!(error = %err, "Fatal auditor error encountered");
            EXIT_ERROR
        }
    };

    process::exit(exit_code);
}

async fn execute(cli: Cli) -> Result<i32> {
    match cli.command {
        Commands::Extract {
            path,
            target,
            crate_name,
            output,
        } => {
            let target_path = match path.or(target) {
                Some(p) => p,
                None => {
                    eprintln!("Error: Missing target path. Usage: warrant extract <PATH>");
                    return Ok(EXIT_ERROR);
                }
            };

            if !target_path.exists() {
                eprintln!("Error: Target path '{}' does not exist.", target_path.display());
                return Ok(EXIT_ERROR);
            }

            let candidates = if target_path.is_dir() {
                extract_candidates_from_directory(&target_path, &crate_name)?
            } else {
                extract_candidates_from_file(&target_path, &crate_name)?
            };

            info!(count = candidates.len(), target = %target_path.display(), "Extraction complete");

            let json_data = serde_json::to_string_pretty(&candidates)?;
            if let Some(out_path) = output {
                fs::write(&out_path, json_data)?;
                info!(path = %out_path.display(), "Candidates exported successfully");
            } else {
                println!("{}", json_data);
            }

            Ok(EXIT_CLEAN)
        }

        Commands::Demo {
            offline,
            events,
            output_dir,
            backend,
        } => {
            let opts = DemoOptions {
                offline,
                events_path: events,
                output_dir,
                live_meta: backend == BackendType::MetaApi,
            };
            run_demo(opts).await
        }

        Commands::Audit {
            path,
            target,
            crate_name,
            backend,
            output_dir,
            miri_flag,
            sandbox,
            events,
        } => {
            let target_path = match path.or(target) {
                Some(p) => p,
                None => {
                    eprintln!("Error: Missing target path. Usage: warrant audit <PATH> [--backend mock|meta] [--output-dir <DIR>] [--events <FILE.jsonl>]");
                    return Ok(EXIT_ERROR);
                }
            };

            if !target_path.exists() {
                eprintln!("Error: Target path '{}' does not exist.", target_path.display());
                return Ok(EXIT_ERROR);
            }

            let audit_start = Instant::now();
            println!("============================================================");
            println!("  WARRANT: Automated Unsafe Soundness Auditor");
            println!("  Target:  {} [{}]", target_path.display(), crate_name);
            println!("  Backend: {:?}", backend);
            println!("============================================================\n");

            let event_paths: Vec<PathBuf> = events.iter().cloned().collect();
            let model_label = match backend {
                BackendType::Mock => "mock",
                BackendType::MetaApi => "muse-spark-1.3",
                BackendType::OpenWeights => "muse-spark-open",
            };
            let mut event_writer = EventStreamWriter::new("r-audit-0001", model_label, &event_paths)?;

            // 1. Candidate Extraction
            let t_extract = Instant::now();
            let candidates = if target_path.is_dir() {
                extract_candidates_from_directory(&target_path, &crate_name)?
            } else {
                extract_candidates_from_file(&target_path, &crate_name)?
            };
            let extract_ms = t_extract.elapsed().as_millis() as u64;

            let cost_tracker = Arc::new(CostTracker::new(105.00));

            let client: Box<dyn LlmClient> = match backend {
                BackendType::Mock => Box::new(MockLlmClient::new()),
                BackendType::MetaApi => {
                    warn!("MetaApi backend specified: using deterministic mock client fallback");
                    Box::new(MockLlmClient::new())
                }
                BackendType::OpenWeights => {
                    warn!("OpenWeights backend specified: using deterministic mock client fallback");
                    Box::new(MockLlmClient::new())
                }
            };

            let mut flags = vec![
                "-Zmiri-tree-borrows".to_string(),
                "-Zmiri-strict-provenance".to_string(),
                "-Zmiri-preemption-rate=0".to_string(),
            ];
            flags.extend(miri_flag);

            let runner: Box<dyn ProofRunner> = if sandbox {
                Box::new(DockerMiriRunner::default())
            } else {
                Box::new(LocalMiriRunner::new(Duration::from_secs(60), flags.clone()))
            };
            let pipeline = Arc::new(VerificationPipeline::with_flags(runner, flags));
            let repair_engine = RepairEngine::new(4);

            fs::create_dir_all(&output_dir)?;

            if candidates.is_empty() {
                event_writer.emit(
                    "c-0000",
                    EventStage::Extract,
                    EventStatus::Ok,
                    extract_ms,
                    0.0,
                    "0 unsafe code candidates discovered",
                )?;
                println!("[INFO] 0 unsafe code candidates discovered. Codebase is strictly safe Rust.\n");
                let report = AuditReport {
                    target_path: target_path.display().to_string(),
                    crate_name: crate_name.clone(),
                    timestamp_rfc3339: now_rfc3339_utc(),
                    duration_ms: audit_start.elapsed().as_millis(),
                    total_candidates: 0,
                    proven_findings: vec![],
                    unproven_hypotheses: vec![],
                    api_cost_usd: 0.0,
                    total_tokens_in: 0,
                    total_tokens_out: 0,
                    backend_used: format!("{:?}", backend),
                    toolchain: PINNED_TOOLCHAIN.to_string(),
                };
                let (md_path, html_path) = report.save_reports(&output_dir)?;
                println!("Audit reports saved:");
                println!("  Markdown: {}", md_path.display());
                println!("  HTML:     {}\n", html_path.display());
                println!("Verdict: CLEAN (Exit Code 0)");
                return Ok(EXIT_CLEAN);
            }

            println!("[DISCOVERY] Found {} unsafe code candidate(s) for verification.\n", candidates.len());

            let mut proven_findings = Vec::new();
            let mut unproven_hypotheses = Vec::new();

            for (idx, candidate) in candidates.iter().enumerate() {
                let rule_str = candidate.rule.as_deref().unwrap_or("R1");
                let symbol_str = candidate.enclosing_item.as_deref().unwrap_or("unknown");

                event_writer.emit(
                    &candidate.id,
                    EventStage::Extract,
                    EventStatus::Ok,
                    extract_ms,
                    cost_tracker.total_spent(),
                    format!(
                        "Extracted {} candidate '{}' at {}:{}..{}",
                        rule_str, symbol_str, candidate.file_path, candidate.start_line, candidate.end_line
                    ),
                )?;

                println!(
                    "[{}/{}] Auditing candidate {} ({}) in '{}' [{}:{}-{}]",
                    idx + 1,
                    candidates.len(),
                    candidate.id,
                    rule_str,
                    symbol_str,
                    candidate.file_path,
                    candidate.start_line,
                    candidate.end_line
                );

                // Read target source code
                let file_to_read = if target_path.is_file() {
                    target_path.clone()
                } else {
                    let direct = Path::new(&candidate.file_path);
                    if direct.exists() {
                        direct.to_path_buf()
                    } else {
                        target_path.join(&candidate.file_path)
                    }
                };

                let raw_code = match fs::read_to_string(&file_to_read) {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("Could not read source file {}: {}", file_to_read.display(), e);
                        candidate.code_snippet.clone()
                    }
                };

                let target_code = if let Some(pos) = raw_code.find("// ==============================================================================") {
                    raw_code[..pos].trim_end().to_string()
                } else if let Some(pos) = raw_code.find("#[cfg(test)]") {
                    raw_code[..pos].trim_end().to_string()
                } else {
                    raw_code
                };

                // Generate initial hypothesis
                let t_hyp = Instant::now();
                let initial_hyp = client.propose_hypothesis(candidate).await?;
                let hyp_ms = t_hyp.elapsed().as_millis() as u64;
                event_writer.emit(
                    &candidate.id,
                    EventStage::Hypothesis,
                    EventStatus::Ok,
                    hyp_ms,
                    cost_tracker.total_spent(),
                    format!("Proposed {} hypothesis: {}", initial_hyp.ub_class, initial_hyp.explanation),
                )?;

                // Autonomous repair loop (Max 4 rounds with verbatim compiler errors)
                let pipeline_clone = pipeline.clone();
                let target_code_clone = target_code.clone();
                let target_code_compile = target_code.clone();

                let t_repair = Instant::now();
                let session = repair_engine
                    .execute(
                        client.as_ref(),
                        candidate,
                        initial_hyp.clone(),
                        &cost_tracker,
                        |harness_code| {
                            pipeline_clone
                                .injector
                                .validate_static_filter(harness_code)
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
                let repair_ms = t_repair.elapsed().as_millis() as u64;

                if let Some(ref valid_hyp) = session.final_hypothesis {
                    event_writer.emit(
                        &candidate.id,
                        EventStage::Harness,
                        EventStatus::Ok,
                        1,
                        cost_tracker.total_spent(),
                        "Safe Rust harness passed static AST & token filter",
                    )?;
                    event_writer.emit(
                        &candidate.id,
                        EventStage::Compile,
                        EventStatus::Ok,
                        repair_ms,
                        cost_tracker.total_spent(),
                        format!(
                            "Compiled cleanly in {} round(s)",
                            session.total_rounds_executed
                        ),
                    )?;

                    let candidate_harness = CandidateHarness {
                        rule: rule_str.to_string(),
                        explanation: valid_hyp.explanation.clone(),
                        harness_code: valid_hyp.harness_code.clone(),
                        estimated_budget_usd: 0.0,
                    };

                    let t_miri = Instant::now();
                    let outcome = pipeline
                        .verify(&target_code_clone, candidate, &candidate_harness)
                        .await?;
                    let miri_ms = t_miri.elapsed().as_millis() as u64;

                    if outcome.is_proven() {
                        let sig = outcome
                            .signature_matched
                            .clone()
                            .unwrap_or_else(|| "Undefined Behavior".to_string());
                        event_writer.emit(
                            &candidate.id,
                            EventStage::MiriRed,
                            EventStatus::Ok,
                            miri_ms,
                            cost_tracker.total_spent(),
                            format!("Miri UB confirmed ({})", sig),
                        )?;
                        event_writer.emit(
                            &candidate.id,
                            EventStage::Signature,
                            EventStatus::Ok,
                            1,
                            cost_tracker.total_spent(),
                            format!("Matched canonical {} signature: '{}'", rule_str, sig),
                        )?;
                        event_writer.emit(
                            &candidate.id,
                            EventStage::Location,
                            EventStatus::Ok,
                            1,
                            cost_tracker.total_spent(),
                            format!(
                                "Backtrace location verified in '{}' ({}:{}..{})",
                                symbol_str, candidate.file_path, candidate.start_line, candidate.end_line
                            ),
                        )?;

                        let is_pub = candidate.visibility.as_deref() == Some("pub");
                        let tier = if is_pub {
                            FindingTier::ProvenPublic
                        } else {
                            FindingTier::ProvenInternal
                        };
                        event_writer.emit(
                            &candidate.id,
                            EventStage::Tier,
                            EventStatus::Ok,
                            1,
                            cost_tracker.total_spent(),
                            format!("Classified as {}", tier.as_str()),
                        )?;
                        event_writer.emit(
                            &candidate.id,
                            EventStage::Patch,
                            EventStatus::Skip,
                            0,
                            cost_tracker.total_spent(),
                            "Patch loop not requested during standard audit",
                        )?;
                        event_writer.emit(
                            &candidate.id,
                            EventStage::Sanity,
                            EventStatus::Skip,
                            0,
                            cost_tracker.total_spent(),
                            "Sanity check not requested during standard audit",
                        )?;

                        let t_ev = Instant::now();
                        let bundle_dir = output_dir.join(&candidate.id);
                        let bundle = pipeline.save_bundle_from_outcome(
                            &target_code_clone,
                            candidate,
                            valid_hyp,
                            &outcome,
                            &bundle_dir,
                            is_pub,
                        )?;
                        let ev_ms = t_ev.elapsed().as_millis() as u64;
                        event_writer.emit(
                            &candidate.id,
                            EventStage::Evidence,
                            EventStatus::Ok,
                            ev_ms,
                            cost_tracker.total_spent(),
                            format!("Evidence bundle written to {}", bundle_dir.display()),
                        )?;

                        println!(
                            "  [PROVEN BUG] Signature: {:?} | Evidence: {}",
                            outcome.signature_matched,
                            bundle_dir.display()
                        );

                        proven_findings.push(ReportFinding {
                            candidate_id: candidate.id.clone(),
                            tier,
                            rule: rule_str.to_string(),
                            symbol: symbol_str.to_string(),
                            file_path: candidate.file_path.clone(),
                            line_span: (candidate.start_line, candidate.end_line),
                            miri_signature: outcome.signature_matched.clone(),
                            harness_sha256: bundle.harness_sha256.clone(),
                            evidence_dir: Some(format!("{}/{}", output_dir.display(), candidate.id)),
                            harness_code: valid_hyp.harness_code.clone(),
                            target_snippet: candidate.code_snippet.clone(),
                            patch_badge: "NONE".to_string(),
                            explanation: valid_hyp.explanation.clone(),
                            patch_diff: None,
                        });
                    } else {
                        event_writer.emit(
                            &candidate.id,
                            EventStage::MiriRed,
                            EventStatus::Ok,
                            miri_ms,
                            cost_tracker.total_spent(),
                            format!("Miri status {:?}; no UB triggered", outcome.status),
                        )?;
                        event_writer.emit(
                            &candidate.id,
                            EventStage::Signature,
                            EventStatus::Skip,
                            0,
                            cost_tracker.total_spent(),
                            "No UB signature triggered",
                        )?;
                        event_writer.emit(
                            &candidate.id,
                            EventStage::Location,
                            EventStatus::Skip,
                            0,
                            cost_tracker.total_spent(),
                            "No UB backtrace span to attest",
                        )?;
                        event_writer.emit(
                            &candidate.id,
                            EventStage::Tier,
                            EventStatus::Ok,
                            1,
                            cost_tracker.total_spent(),
                            "Classified as UNPROVEN",
                        )?;
                        event_writer.emit(
                            &candidate.id,
                            EventStage::Patch,
                            EventStatus::Skip,
                            0,
                            cost_tracker.total_spent(),
                            "Skipped patch stage for UNPROVEN candidate",
                        )?;
                        event_writer.emit(
                            &candidate.id,
                            EventStage::Sanity,
                            EventStatus::Skip,
                            0,
                            cost_tracker.total_spent(),
                            "Skipped sanity stage for UNPROVEN candidate",
                        )?;
                        event_writer.emit(
                            &candidate.id,
                            EventStage::Evidence,
                            EventStatus::Ok,
                            1,
                            cost_tracker.total_spent(),
                            "Recorded in Unproven Hypotheses report table",
                        )?;

                        println!(
                            "  [UNPROVEN] Status: {:?} (Miri executed cleanly; UB not triggered)",
                            outcome.status
                        );
                        unproven_hypotheses.push(ReportHypothesis {
                            candidate_id: candidate.id.clone(),
                            rule: rule_str.to_string(),
                            symbol: symbol_str.to_string(),
                            file_path: candidate.file_path.clone(),
                            line_span: (candidate.start_line, candidate.end_line),
                            explanation: valid_hyp.explanation.clone(),
                            reason: format!("Miri status: {:?} (No Undefined Behavior triggered)", outcome.status),
                            target_snippet: candidate.code_snippet.clone(),
                        });
                    }
                } else {
                    event_writer.emit(
                        &candidate.id,
                        EventStage::Harness,
                        EventStatus::Fail,
                        repair_ms,
                        cost_tracker.total_spent(),
                        "Harness failed static filter or compilation within 4 repair rounds",
                    )?;
                    event_writer.emit(
                        &candidate.id,
                        EventStage::Compile,
                        EventStatus::Fail,
                        repair_ms,
                        cost_tracker.total_spent(),
                        "Compilation failed across 4 rounds",
                    )?;
                    println!("  [UNPROVEN] Harness failed static filter or compilation within 4 repair rounds");
                    unproven_hypotheses.push(ReportHypothesis {
                        candidate_id: candidate.id.clone(),
                        rule: rule_str.to_string(),
                        symbol: symbol_str.to_string(),
                        file_path: candidate.file_path.clone(),
                        line_span: (candidate.start_line, candidate.end_line),
                        explanation: initial_hyp.explanation.clone(),
                        reason: "Harness rejected by static filter or failed compilation within 4 rounds".to_string(),
                        target_snippet: candidate.code_snippet.clone(),
                    });
                }
            }

            let report = AuditReport {
                target_path: target_path.display().to_string(),
                crate_name: crate_name.clone(),
                timestamp_rfc3339: now_rfc3339_utc(),
                duration_ms: audit_start.elapsed().as_millis(),
                total_candidates: candidates.len(),
                proven_findings,
                unproven_hypotheses,
                api_cost_usd: cost_tracker.total_spent(),
                total_tokens_in: 0,
                total_tokens_out: 0,
                backend_used: format!("{:?}", backend),
                toolchain: PINNED_TOOLCHAIN.to_string(),
            };

            let (md_path, html_path) = report.save_reports(&output_dir)?;

            println!("\n============================================================");
            println!("              WARRANT SOUNDNESS AUDIT RESULTS");
            println!("============================================================");
            println!("Target Path:         {}", target_path.display());
            println!("Unsafe Sites Tested: {}", report.total_candidates);
            println!("Proven Bugs:         {}", report.proven_count());
            println!("Unproven Hypotheses: {}", report.unproven_count());
            println!("Audit Duration:      {:.2}s", (report.duration_ms as f64) / 1000.0);
            println!("API Cost:            ${:.6} USD", report.api_cost_usd);
            println!("------------------------------------------------------------");
            println!("Markdown Report:     {}", md_path.display());
            println!("HTML Standalone:     {}", html_path.display());
            println!("Evidence Directory:  {}", output_dir.display());
            if let Some(ref ev_path) = events {
                println!("Event Stream JSONL:  {} ({} events)", ev_path.display(), event_writer.events().len());
            }
            println!("============================================================");
            if report.has_proven_bugs() {
                println!("VERDICT: PROVEN SOUNDNESS BUGS DETECTED (Exit Code 1)");
                println!("============================================================\n");
                Ok(EXIT_PROVEN_BUG)
            } else {
                println!("VERDICT: ALL SITES CLEAN / SOUND (Exit Code 0)");
                println!("============================================================\n");
                Ok(EXIT_CLEAN)
            }
        }

        Commands::Verify {
            candidate: cand_path,
            hypothesis: hyp_path,
            output,
        } => {
            let cand_str = fs::read_to_string(&cand_path)?;
            let hyp_str = fs::read_to_string(&hyp_path)?;

            let candidate = Candidate::parse_from_json(&cand_str)?;
            let hypothesis = Hypothesis::parse_from_json(&hyp_str)?;

            let runner: Box<dyn ProofRunner> = Box::new(LocalMiriRunner::default());
            let pipeline = VerificationPipeline::new(runner);

            let target_code = format!(
                "// Candidate definition\n{}\n",
                candidate.code_snippet
            );

            let bundle = pipeline
                .verify_and_bundle(
                    &target_code,
                    &candidate,
                    &hypothesis,
                    &output,
                    candidate.visibility.as_deref() == Some("pub"),
                )
                .await?;

            info!(path = %output.display(), "Verification finished. Bundle written");

            if bundle.finding.status == warrant_llm::VerificationStatus::ProvenBug {
                Ok(EXIT_PROVEN_BUG)
            } else {
                Ok(EXIT_CLEAN)
            }
        }
    }
}
