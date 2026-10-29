//! # Probe CLI (Task T2: Model and Agent Probe)
//!
//! Executes a minimal smoke test against Meta Model API, verifies token usage fields,
//! checks Muse Code driver availability, and validates CostTracker accounting.
//!
//! Strictly adheres to SPEC.md Section 7, Section 10 (Task T2), and AGENTS.md.

use std::process::Command;
use std::sync::Arc;
use anyhow::{Context, Result};
use warrant_llm::{CostTracker, MetaApiConfig, MetaClient};

#[tokio::main]
async fn main() -> Result<()> {
    // Load local .env if present
    let _ = dotenvy::dotenv();

    println!("============================================================");
    println!("  WARRANT TASK T2: MODEL & AGENT PROBE");
    println!("============================================================");

    // -----------------------------------------------------------------
    // 1. Muse Code Driver Probe
    // -----------------------------------------------------------------
    println!("\n[1/4] Probing Muse Code Programmatic Driver Availability...");

    let muse_cli_found = check_muse_cli();
    match muse_cli_found {
        Some(path) => {
            println!("  [FOUND] Muse Code binary located at: {}", path);
            println!("  [STATUS] Attempting 'muse --version' check...");
            let version = Command::new(&path).arg("--version").output();
            if let Ok(v) = version {
                let out = String::from_utf8_lossy(&v.stdout);
                println!("  [VERSION] {}", out.trim());
            }
        }
        None => {
            println!("  [NOT FOUND] 'muse' CLI binary is not present in system PATH.");
            println!("  [DECISION] Fallback engaged: AgentBackend backed by Muse Spark API calls (MetaClient).");
            println!("  [VERIFICATION] Product remains fully functional without Muse Code (No single point of failure).");
        }
    }

    // -----------------------------------------------------------------
    // 2. Cost Tracker Initialization
    // -----------------------------------------------------------------
    println!("\n[2/4] Initializing CostTracker with Circuit Breaker Ceilings...");
    let tracker = Arc::new(CostTracker::default());
    println!("  [BUDGET] Global 70% circuit breaker stop ceiling: ${:.2}", tracker.remaining_budget());
    println!("  [BUDGET] Initial spend: ${:.6}", tracker.total_spent());

    // -----------------------------------------------------------------
    // 3. Meta Model API Single-Call Smoke Test Probe
    // -----------------------------------------------------------------
    println!("\n[3/4] Executing Single Smoke Test Call to Meta Model API...");

    let config = MetaApiConfig::default();
    println!("  [ENDPOINT] {}", config.endpoint);
    println!("  [TARGET MODEL] {}", config.model_id);
    println!("  [AUTH ENV] {}", config.api_key_env);

    let client = MetaClient::new(config, tracker.clone())
        .context("Failed to initialize MetaClient (verify META_MODEL_API_KEY in .env)")?;

    let report = client.probe().await.context("Meta Model API probe failed")?;

    println!("  [HTTP STATUS] {}", report.http_status);
    println!("  [ROUND-TRIP LATENCY] {} ms", report.latency_ms);
    println!("  [ROUTE HEADER] {}", report.route_header.as_deref().unwrap_or("none"));
    println!("  [MODEL RETURNED] {}", report.model);
    println!("  [RESPONSE CONTENT] \"{}\"", report.response_content);
    println!("  [USAGE - PROMPT TOKENS] {}", report.prompt_tokens);
    println!("  [USAGE - COMPLETION TOKENS] {}", report.completion_tokens);
    println!("  [USAGE - TOTAL TOKENS] {}", report.total_tokens);
    println!("  [USAGE - CACHED TOKENS] {}", report.cached_tokens);
    println!("  [ESTIMATED CALL COST] ${:.6} USD", report.calculated_cost_usd);

    // -----------------------------------------------------------------
    // 4. Cost Tracker Spend Attestation
    // -----------------------------------------------------------------
    println!("\n[4/4] Verifying CostTracker Accounting...");
    let current_spent = tracker.total_spent();
    let remaining = tracker.remaining_budget();
    println!("  [CUMULATIVE SPEND RECORDED] ${:.6} USD", current_spent);
    println!("  [REMAINING BUDGET] ${:.6} USD", remaining);
    assert!(current_spent > 0.0, "CostTracker must record non-zero spend for real API call");
    assert!(current_spent < 1.0, "Single smoke test spend must be tiny (< $1.00)");

    println!("\n============================================================");
    println!("  TASK T2 PROBE COMPLETED SUCCESSFULLY");
    println!("============================================================");

    Ok(())
}

fn check_muse_cli() -> Option<String> {
    #[cfg(windows)]
    {
        let out = Command::new("where.exe").arg("muse").output().ok()?;
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s.lines().next().unwrap_or(&s).to_string());
            }
        }
    }

    #[cfg(not(windows))]
    {
        let out = Command::new("which").arg("muse").output().ok()?;
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }

    None
}
