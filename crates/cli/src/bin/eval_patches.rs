//! # Task T11: Patch Verification Loop & Anti-Bypass Guard evaluation
//!
//! For each of the 8 benchmark mutants (4 R1 + 4 R2):
//! 1. Re-prove the finding for real (`PROVEN_*` via Miri, location attested) - no patch is attempted
//!    for anything that is not PROVEN.
//! 2. Check that the `sanity_*` test (bench/patch_sanity/) passes under Miri on the ORIGINAL code.
//! 3. Ask the selected backend (`--backend meta` = Meta Model API `muse-spark-1.3`, or `mock`) for a
//!    patch, at most 3 rounds, verified by: guard, same proof harness clean under Miri,
//!    native `cargo test --lib`, `warrant_proof::sanity_*` under Miri (SPEC.md 5.3).
//!
//! Results are reported as measured. There is no pass threshold in this binary: SPEC.md 5.3 defines
//! the badge per finding, and a `PATCH_UNVERIFIED` outcome never changes a PROVEN verdict.

use anyhow::{bail, Context, Result};
use clap::Parser;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use warrant_cli::eval_summary::EvalSummary;
use warrant_cli::reporter::{AuditReport, FindingTier, ReportFinding};
use warrant_extract::extract_candidates_from_file;
use warrant_llm::{
    CallTotals, CostTracker, Hypothesis, LlmClient, MetaApiConfig, MetaClient, MockLlmClient,
    PatchStatus, UbClass, VerificationStatus,
};
use warrant_verify::{
    extract_matched_signature,
    patch_guard::{self, PatchGuardError},
    ClopperPearsonInterval, LocalMiriRunner, PatchVerificationLoop, VerificationPipeline,
};

#[derive(Parser, Debug)]
#[command(about = "WARRANT T11: patch verification loop evaluation")]
struct Args {
    /// `meta` (Meta Model API, muse-spark-1.3, key from .env) or `mock` (deterministic, no usage)
    #[arg(long, default_value = "mock")]
    backend: String,

    /// Hard spend ceiling for this run in USD (enforced by CostTracker before every call)
    #[arg(long, default_value_t = 2.5)]
    budget_usd: f64,

    /// Optional target filter (e.g. `smallvec_r1_demo`)
    #[arg(long)]
    target: Option<String>,
}

struct PatchTarget {
    id: &'static str,
    rule: &'static str,
    file_path: &'static str,
}

const PRIMARY_TARGETS: &[PatchTarget] = &[
    PatchTarget { id: "r1_01_slice_get", rule: "R1", file_path: "bench/pairs/r1_01_slice_get_mutant.rs" },
    PatchTarget { id: "r1_02_ptr_arithmetic", rule: "R1", file_path: "bench/pairs/r1_02_ptr_arithmetic_mutant.rs" },
    PatchTarget { id: "r1_03_from_raw_parts", rule: "R1", file_path: "bench/pairs/r1_03_from_raw_parts_mutant.rs" },
    PatchTarget { id: "r1_04_vec_queue", rule: "R1", file_path: "bench/pairs/r1_04_vec_queue_mutant.rs" },
    PatchTarget { id: "r1_05_copy_nonoverlapping", rule: "R1", file_path: "bench/pairs/r1_05_copy_nonoverlapping_mutant.rs" },
    PatchTarget { id: "r1_06_byte_offset", rule: "R1", file_path: "bench/pairs/r1_06_byte_offset_mutant.rs" },
    PatchTarget { id: "r1_07_chunk_reader", rule: "R1", file_path: "bench/pairs/r1_07_chunk_reader_mutant.rs" },
    PatchTarget { id: "r1_08_lookup_table", rule: "R1", file_path: "bench/pairs/r1_08_lookup_table_mutant.rs" },
    PatchTarget { id: "r2_01_cell_sync", rule: "R2", file_path: "bench/pairs/r2_01_cell_sync_mutant.rs" },
    PatchTarget { id: "r2_02_rc_send", rule: "R2", file_path: "bench/pairs/r2_02_rc_send_mutant.rs" },
    PatchTarget { id: "r2_03_unsync_buffer", rule: "R2", file_path: "bench/pairs/r2_03_unsync_buffer_mutant.rs" },
    PatchTarget { id: "r2_04_refcell_sync", rule: "R2", file_path: "bench/pairs/r2_04_refcell_sync_mutant.rs" },
    PatchTarget { id: "r2_05_split_borrow", rule: "R2", file_path: "bench/pairs/r2_05_split_borrow_mutant.rs" },
    PatchTarget { id: "r2_06_atomic_relaxed_race", rule: "R2", file_path: "bench/pairs/r2_06_atomic_relaxed_race_mutant.rs" },
    PatchTarget { id: "r2_07_lazy_init", rule: "R2", file_path: "bench/pairs/r2_07_lazy_init_mutant.rs" },
    PatchTarget { id: "r2_08_alias_mut", rule: "R2", file_path: "bench/pairs/r2_08_alias_mut_mutant.rs" },
    PatchTarget { id: "smallvec_r1_demo", rule: "R1", file_path: "bench/real_world_demo/smallvec_r1_demo.rs" },
];

fn split_source_and_harness(file_path: &Path) -> Result<(String, String)> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read {}", file_path.display()))?;
    let idx = content
        .find("#[cfg(test)]")
        .with_context(|| format!("Missing #[cfg(test)] harness in {}", file_path.display()))?;
    Ok((content[..idx].trim_end().to_string(), content[idx..].trim().to_string()))
}

fn delta(after: &CallTotals, before: &CallTotals) -> CallTotals {
    CallTotals {
        calls: after.calls - before.calls,
        calls_without_usage: after.calls_without_usage - before.calls_without_usage,
        prompt_tokens: after.prompt_tokens - before.prompt_tokens,
        completion_tokens: after.completion_tokens - before.completion_tokens,
        cached_tokens: after.cached_tokens - before.cached_tokens,
        latency_ms_sum: after.latency_ms_sum - before.latency_ms_sum,
        cost_micro_usd: after.cost_micro_usd - before.cost_micro_usd,
    }
}

/// Deterministic adversarial checks of the anti-bypass guard (no model involved).
fn adversarial_suite() -> bool {
    println!("--------------------------------------------------------------------------------------------------");
    println!("  ADVERSARIAL ANTI-BYPASS GUARD (deterministic, no model)");
    println!("--------------------------------------------------------------------------------------------------");
    let orig = r#"
pub struct DataStore { data: Vec<u8> }
impl DataStore {
    pub fn new() -> Self { Self { data: Vec::new() } }
    pub fn read_slot(&self, idx: usize) -> Option<u8> {
        Some(unsafe { *self.data.get_unchecked(idx) })
    }
}
"#;
    type GuardCheck = fn(&PatchGuardError) -> bool;
    let cases: [(&str, &str, GuardCheck); 3] = [
        (
            "ADV-01 removed public fn (signature tampering)",
            "pub struct DataStore { data: Vec<u8> }\nimpl DataStore {\n    pub fn new() -> Self { Self { data: Vec::new() } }\n}\n",
            |e| matches!(e, PatchGuardError::SignatureTampering(_)),
        ),
        (
            "ADV-02 #[cfg(not(miri))] bypass",
            "pub struct DataStore { data: Vec<u8> }\nimpl DataStore {\n    pub fn new() -> Self { Self { data: Vec::new() } }\n    #[cfg(not(miri))]\n    pub fn read_slot(&self, idx: usize) -> Option<u8> { Some(unsafe { *self.data.get_unchecked(idx) }) }\n}\n",
            |e| matches!(e, PatchGuardError::CfgBypass(_)),
        ),
        (
            "ADV-03 todo!() stub",
            "pub struct DataStore { data: Vec<u8> }\nimpl DataStore {\n    pub fn new() -> Self { Self { data: Vec::new() } }\n    pub fn read_slot(&self, idx: usize) -> Option<u8> { todo!() }\n}\n",
            |e| matches!(e, PatchGuardError::UnconditionalAbort(_)),
        ),
    ];
    let mut all = true;
    for (name, bad, is_expected) in cases {
        let ok = matches!(patch_guard::validate_patch(orig, bad), Err(ref e) if is_expected(e));
        println!("  {:<52} {}", name, if ok { "REJECTED" } else { "NOT REJECTED (guard failure)" });
        all &= ok;
    }
    println!("  Adversarial result: {}/3 rejected", if all { 3 } else { 0 });
    all
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let start_all = Instant::now();

    println!("==================================================================================================");
    println!("  WARRANT T11 - PATCH VERIFICATION (backend: {}, budget cap ${:.2})", args.backend, args.budget_usd);
    println!("==================================================================================================");

    if !adversarial_suite() {
        bail!("anti-bypass guard failed an adversarial case");
    }

    let tracker = Arc::new(CostTracker::new(args.budget_usd));
    let cfg = MetaApiConfig::default();
    let model_name;
    let meta_client: Option<Arc<MetaClient>>;
    let mock_client = MockLlmClient::new();
    match args.backend.as_str() {
        "meta" => {
            meta_client = Some(Arc::new(MetaClient::new(cfg.clone(), tracker.clone())?));
            model_name = cfg.model_id.clone();
        }
        "mock" => {
            meta_client = None;
            model_name = "mock (deterministic, no API usage)".to_string();
        }
        other => bail!("unknown --backend '{}': use 'meta' or 'mock'", other),
    }
    let client: &dyn LlmClient = match &meta_client {
        Some(m) => m.as_ref(),
        None => &mock_client,
    };

    let miri_flags = vec![
        "-Zmiri-tree-borrows".to_string(),
        "-Zmiri-strict-provenance".to_string(),
        "-Zmiri-preemption-rate=0".to_string(),
    ];
    let pipeline = VerificationPipeline::with_flags(
        Box::new(LocalMiriRunner::new(Duration::from_secs(60), miri_flags.clone())),
        miri_flags.clone(),
    );
    let loop_runner = LocalMiriRunner::new(Duration::from_secs(60), miri_flags.clone());
    let patch_loop = PatchVerificationLoop::new(&loop_runner);

    let evidence_dir = PathBuf::from(format!("target/warrant/eval_patches_{}/evidence", args.backend));
    let reports_dir = PathBuf::from(format!("target/warrant/eval_patches_{}/reports", args.backend));
    fs::create_dir_all(&evidence_dir)?;
    fs::create_dir_all(&reports_dir)?;

    struct Row {
        id: String,
        rule: String,
        proven: bool,
        sanity_original: bool,
        result: Option<warrant_verify::PatchVerificationResult>,
        usage: CallTotals,
        secs: f64,
    }
    let mut rows: Vec<Row> = Vec::new();
    let mut findings = Vec::new();

    let targets: Vec<&PatchTarget> = PRIMARY_TARGETS
        .iter()
        .filter(|t| args.target.as_ref().map(|tgt| t.id == tgt.as_str()).unwrap_or(true))
        .collect();

    for t in targets {
        let t0 = Instant::now();
        let before = tracker.call_totals();
        let path = PathBuf::from(t.file_path);
        let (original_code, harness_code) = split_source_and_harness(&path)?;
        let sanity_code = fs::read_to_string(format!("bench/patch_sanity/{}.rs", t.id))
            .with_context(|| format!("missing sanity test for {}", t.id))?
            .trim_start_matches('\u{feff}')
            .trim()
            .to_string();

        let candidates = extract_candidates_from_file(&path, "bench_pairs")
            .with_context(|| format!("extractor failed on {}", path.display()))?;
        let mut candidate = candidates
            .iter()
            .find(|c| c.rule.as_deref() == Some(t.rule))
            .or_else(|| candidates.first())
            .cloned()
            .with_context(|| format!("no candidate extracted from {}", path.display()))?;
        candidate.id = format!("cand-{}", t.id);

        // 1. Real red run: the finding must be PROVEN before any patch is attempted.
        let hyp = Hypothesis {
            id: format!("hyp-{}", t.id),
            candidate_id: candidate.id.clone(),
            ub_class: if t.rule == "R2" { UbClass::DataRace } else { UbClass::UbOob },
            explanation: format!("Hand-written benchmark harness for {}", t.id),
            harness_code: harness_code.clone(),
            suggested_miri_flags: vec![],
            expected_trigger: String::new(),
        };
        let bundle_dir = evidence_dir.join(t.id);
        let mut bundle = pipeline
            .verify_and_bundle(&original_code, &candidate, &hyp, &bundle_dir, true)
            .await?;
        let proven = bundle.finding.status == VerificationStatus::ProvenBug;

        let (sanity_original, result) = if proven {
            // 2. Sanity must pass under Miri on the original code.
            let (sane, _out) = patch_loop.sanity_passes_on(&original_code, &sanity_code).await?;
            if sane {
                if let Some(m) = &meta_client {
                    m.register_module_source(&candidate.id, &original_code);
                }
                // 3. Patch loop (max 3 rounds).
                let res = patch_loop
                    .verify_patch_loop(
                        client,
                        &tracker,
                        &original_code,
                        &candidate,
                        &harness_code,
                        &sanity_code,
                        &bundle.stderr,
                    )
                    .await?;
                (true, Some(res))
            } else {
                (false, None)
            }
        } else {
            (false, None)
        };

        let status = match (&result, proven) {
            (Some(r), _) => r.status,
            (None, true) => PatchStatus::PatchUnverified,
            (None, false) => PatchStatus::NotApplicable,
        };
        bundle = bundle.with_patch(
            status,
            result.as_ref().and_then(|r| r.patch_code.clone()),
            result.as_ref().and_then(|r| r.patch_diff.clone()),
        );
        bundle.finding.patch_status = bundle.patch_status;
        bundle.finding.patch_code = bundle.patch_code.clone();
        bundle.finding.patch_diff = bundle.patch_diff.clone();
        bundle.save_to_dir_with_target(&bundle_dir, &original_code)?;

        findings.push(ReportFinding {
            candidate_id: candidate.id.clone(),
            tier: if bundle.verdict == "PROVEN_PUBLIC" { FindingTier::ProvenPublic } else { FindingTier::ProvenInternal },
            rule: t.rule.to_string(),
            symbol: candidate.enclosing_item.clone().unwrap_or_default(),
            file_path: path.display().to_string(),
            line_span: (candidate.start_line, candidate.end_line),
            miri_signature: extract_matched_signature(&bundle.stderr, t.rule),
            harness_sha256: bundle.harness_sha256.clone(),
            evidence_dir: Some(bundle_dir.display().to_string()),
            harness_code: harness_code.clone(),
            target_snippet: original_code.clone(),
            patch_badge: status.badge_str().to_string(),
            explanation: "Miri-verified operational proof (not a formal proof).".to_string(),
            patch_diff: result.as_ref().and_then(|r| r.patch_diff.clone()),
        });

        let usage = delta(&tracker.call_totals(), &before);
        rows.push(Row {
            id: t.id.to_string(),
            rule: t.rule.to_string(),
            proven,
            sanity_original,
            result,
            usage,
            secs: t0.elapsed().as_secs_f64(),
        });
    }

    // ---------------- Result table (as measured) ----------------
    println!("\n{:-<176}", "");
    println!(
        "  {:<22} | {:<4} | {:<7} | {:<6} | {:<7} | {:<6} | {:<9} | {:<9} | {:<6} | {:<16} | {:>7} | {:>7} | {:>7} | {:>8}",
        "Case ID", "Rule", "PROVEN", "Rounds", "Sig/Grd", "MiriOK", "Native", "Sanity(M)", "OrigSan", "Patch Badge", "tok_in", "tok_out", "ms", "secs"
    );
    println!("{:-<176}", "");
    let yn = |b: bool| if b { "yes" } else { "NO" };
    for r in &rows {
        let (rounds, g, m, n, s, badge) = match &r.result {
            Some(x) => (
                format!("{}/3", x.rounds_used),
                yn(x.step1_guard_passed),
                yn(x.step2_miri_passed),
                yn(x.step3_native_passed),
                yn(x.step4_sanity_passed),
                x.status.badge_str(),
            ),
            None => ("-".to_string(), "-", "-", "-", "-", if r.proven { "PATCH_UNVERIFIED" } else { "NOT_APPLICABLE" }),
        };
        println!(
            "  {:<22} | {:<4} | {:<7} | {:<6} | {:<7} | {:<6} | {:<9} | {:<9} | {:<6} | {:<16} | {:>7} | {:>7} | {:>7} | {:>8.2}",
            r.id, r.rule, yn(r.proven), rounds, g, m, n, s, yn(r.sanity_original), badge,
            r.usage.prompt_tokens, r.usage.completion_tokens, r.usage.latency_ms_sum, r.secs
        );
    }
    println!("{:-<176}", "");
    for r in &rows {
        if let Some(x) = &r.result {
            for (i, e) in x.error_history.iter().enumerate() {
                println!("  [{} failure {}] {}", r.id, i + 1, e.lines().next().unwrap_or(""));
            }
        }
    }

    // ---------------- Per-call ledger (real usage) ----------------
    let log = tracker.call_log();
    println!("\n  API CALL LEDGER (values from response `usage` + wall-clock timer):");
    if log.is_empty() {
        println!("  (no API calls: backend '{}' produces no usage data; tokens/cost are NOT measured)", args.backend);
    } else {
        println!("  {:<4} | {:<34} | {:<6} | {:>9} | {:>9} | {:>9} | {:>9} | {:>11}", "#", "label", "http", "prompt", "completion", "cached", "latency_ms", "cost_microUSD");
        for (i, c) in log.iter().enumerate() {
            println!(
                "  {:<4} | {:<34} | {:<6} | {:>9} | {:>9} | {:>9} | {:>9} | {:>11}{}",
                i + 1, c.label, c.http_status, c.prompt_tokens, c.completion_tokens, c.cached_tokens,
                c.latency_ms, c.cost_micro_usd, if c.usage_reported { "" } else { "  (no usage in response)" }
            );
        }
    }

    // ---------------- Summary ----------------
    let n_proven = rows.iter().filter(|r| r.proven).count();
    let n_verified = rows
        .iter()
        .filter(|r| matches!(&r.result, Some(x) if x.status == PatchStatus::PatchVerified))
        .count();
    let totals = tracker.call_totals();
    let ci = ClopperPearsonInterval::compute_95(n_verified, n_proven);
    println!("\n  SUMMARY (measured, backend={}, model={})", args.backend, model_name);
    println!("  * PROVEN mutants entering patch loop: {}/{}", n_proven, rows.len());
    println!("  * PATCH_VERIFIED:                     {}/{}  (exact Clopper-Pearson 95% CI of rate: {})", n_verified, n_proven, ci.format_interval_pct());
    println!("  * PATCH_UNVERIFIED:                   {}/{}", n_proven - n_verified, n_proven);
    println!("  * API calls: {} ({} without usage) | prompt tokens: {} | completion tokens: {} | mean latency: {} ms", totals.calls, totals.calls_without_usage, totals.prompt_tokens, totals.completion_tokens, totals.mean_latency_ms());
    println!("  * Cost this run (from usage x Muse Spark price table): ${:.6} USD | tracker total: ${:.6} USD", totals.cost_usd(), tracker.total_spent());
    println!("  * Wall time: {:.2}s", start_all.elapsed().as_secs_f64());

    let audit = AuditReport {
        target_path: "bench/pairs/ (mutants)".to_string(),
        crate_name: "warrant-benchmark-pairs".to_string(),
        timestamp_rfc3339: String::new(),
        duration_ms: start_all.elapsed().as_millis(),
        total_candidates: rows.len(),
        proven_findings: findings,
        unproven_hypotheses: Vec::new(),
        api_cost_usd: totals.cost_usd(),
        total_tokens_in: totals.prompt_tokens,
        total_tokens_out: totals.completion_tokens,
        backend_used: format!("{} / {}", args.backend, model_name),
        toolchain: "nightly-2026-10-20".to_string(),
    };
    let (md, html) = audit.save_reports(&reports_dir)?;
    println!("  * Reports: {} , {}", md.display(), html.display());

    let mut counts = BTreeMap::new();
    counts.insert("proven".to_string(), n_proven);
    counts.insert("patch_verified".to_string(), n_verified);
    let summary = EvalSummary {
        experiment: "patches".to_string(),
        backend: args.backend.clone(),
        model: model_name,
        n: rows.len(),
        counts,
        calls: totals.calls,
        calls_without_usage: totals.calls_without_usage,
        prompt_tokens: totals.prompt_tokens,
        completion_tokens: totals.completion_tokens,
        mean_latency_ms: totals.mean_latency_ms(),
        cost_usd: totals.cost_usd(),
    };
    println!("  * Summary JSON: {}", summary.save()?.display());
    Ok(())
}
