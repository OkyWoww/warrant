//! # warrant-llm cost tracker
//!
//! Enforces global spend ceilings ($150 credit ceiling, 70% global stop = $105.00),
//! per-candidate repair loop bounds (max 4 rounds), and patch bounds (max 3 rounds).
//!
//! Strictly adheres to SPEC.md Section 7 and Section 10 (Task T1 & Task T2).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use crate::client::LlmError;

/// Global default circuit-breaker stop threshold (70% of $150.00 waiver credit = $105.00)
pub const DEFAULT_MAX_BUDGET_USD: f64 = 105.00;

/// Maximum harness repair attempts allowed per candidate (SPEC.md Section 7)
pub const MAX_HARNESS_REPAIR_ROUNDS: usize = 4;

/// Maximum patch proposal attempts allowed per candidate (SPEC.md Section 7)
pub const MAX_PATCH_ROUNDS: usize = 3;

/// Muse Spark Standard Mode Pricing (per 1,000,000 tokens)
pub const MUSE_SPARK_INPUT_COST_PER_M: f64 = 1.25;
pub const MUSE_SPARK_CACHED_INPUT_COST_PER_M: f64 = 0.15;
pub const MUSE_SPARK_OUTPUT_COST_PER_M: f64 = 4.25;

/// Muse Spark Contributor Mode Pricing (per 1,000,000 tokens)
pub const MUSE_SPARK_CONTRIBUTOR_INPUT_COST_PER_M: f64 = 0.10;
pub const MUSE_SPARK_CONTRIBUTOR_OUTPUT_COST_PER_M: f64 = 0.20;

/// One real HTTP model call as reported by the API `usage` object.
/// Nothing in this record is estimated: token counts come from the response JSON,
/// latency from a wall-clock timer around the HTTP request.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CallRecord {
    pub label: String,
    pub model: String,
    pub http_status: u16,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub cached_tokens: u32,
    pub latency_ms: u64,
    pub cost_micro_usd: u64,
    /// False when the response carried no `usage` object (then tokens/cost are 0 and flagged).
    pub usage_reported: bool,
}

/// Aggregate over the call ledger.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CallTotals {
    pub calls: usize,
    pub calls_without_usage: usize,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cached_tokens: u64,
    pub latency_ms_sum: u64,
    pub cost_micro_usd: u64,
}

impl CallTotals {
    pub fn cost_usd(&self) -> f64 {
        self.cost_micro_usd as f64 / 1_000_000.0
    }
    pub fn mean_latency_ms(&self) -> u64 {
        if self.calls == 0 { 0 } else { self.latency_ms_sum / self.calls as u64 }
    }
}

/// Thread-safe cost tracker and execution limiter with micro-USD precision ($1.00 = 1_000_000 micros).
pub struct CostTracker {
    max_budget_micros: u64,
    spent_micros: AtomicU64,
    repair_rounds: Mutex<HashMap<String, usize>>,
    patch_rounds: Mutex<HashMap<String, usize>>,
    calls: Mutex<Vec<CallRecord>>,
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_BUDGET_USD)
    }
}

impl CostTracker {
    /// Create a new CostTracker with a designated dollar ceiling
    pub fn new(max_budget_dollars: f64) -> Self {
        Self {
            max_budget_micros: (max_budget_dollars * 1_000_000.0).round() as u64,
            spent_micros: AtomicU64::new(0),
            repair_rounds: Mutex::new(HashMap::new()),
            patch_rounds: Mutex::new(HashMap::new()),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Append a real API call to the ledger and charge its cost (derived from reported token usage).
    /// `usage` is `None` when the response had no `usage` object; nothing is invented in that case.
    pub fn record_call(
        &self,
        label: &str,
        model: &str,
        http_status: u16,
        usage: Option<(u32, u32, u32)>,
        latency_ms: u64,
        is_contributor: bool,
    ) -> CallRecord {
        let (p, c, cached, reported, cost) = match usage {
            Some((p, c, cached)) => {
                let cost = self.record_tokens(p, c, cached, is_contributor);
                (p, c, cached, true, cost)
            }
            None => (0, 0, 0, false, 0.0),
        };
        let rec = CallRecord {
            label: label.to_string(),
            model: model.to_string(),
            http_status,
            prompt_tokens: p,
            completion_tokens: c,
            cached_tokens: cached,
            latency_ms,
            cost_micro_usd: (cost * 1_000_000.0).round() as u64,
            usage_reported: reported,
        };
        if let Ok(mut log) = self.calls.lock() {
            log.push(rec.clone());
        }
        rec
    }

    /// Snapshot of every recorded API call, in order.
    pub fn call_log(&self) -> Vec<CallRecord> {
        self.calls.lock().map(|l| l.clone()).unwrap_or_default()
    }

    /// Aggregate totals over the ledger.
    pub fn call_totals(&self) -> CallTotals {
        let mut t = CallTotals::default();
        for r in self.call_log() {
            t.calls += 1;
            if !r.usage_reported {
                t.calls_without_usage += 1;
            }
            t.prompt_tokens += r.prompt_tokens as u64;
            t.completion_tokens += r.completion_tokens as u64;
            t.cached_tokens += r.cached_tokens as u64;
            t.latency_ms_sum += r.latency_ms;
            t.cost_micro_usd += r.cost_micro_usd;
        }
        t
    }

    /// Check if an upcoming call will exceed the budget before sending the request.
    pub fn check_and_reserve(&self, estimated_call_cost_dollars: f64) -> Result<(), LlmError> {
        let est_micros = (estimated_call_cost_dollars * 1_000_000.0).round() as u64;
        let current = self.spent_micros.load(Ordering::SeqCst);
        if current + est_micros > self.max_budget_micros {
            return Err(LlmError::BudgetExceeded {
                spent: current as f64 / 1_000_000.0,
                limit: self.max_budget_micros as f64 / 1_000_000.0,
            });
        }
        Ok(())
    }

    /// Record actual spend in USD.
    pub fn record_spend(&self, actual_cost_dollars: f64) {
        let micros = (actual_cost_dollars * 1_000_000.0).round() as u64;
        self.spent_micros.fetch_add(micros, Ordering::SeqCst);
    }

    /// Calculate dollar cost from prompt, completion, and cached token counts.
    pub fn calculate_token_cost(
        prompt_tokens: u32,
        completion_tokens: u32,
        cached_tokens: u32,
        is_contributor: bool,
    ) -> f64 {
        let uncached_prompt = prompt_tokens.saturating_sub(cached_tokens);
        if is_contributor {
            let prompt_cost = (uncached_prompt as f64) * (MUSE_SPARK_CONTRIBUTOR_INPUT_COST_PER_M / 1_000_000.0);
            let cached_cost = (cached_tokens as f64) * (0.002 / 1_000_000.0);
            let completion_cost = (completion_tokens as f64) * (MUSE_SPARK_CONTRIBUTOR_OUTPUT_COST_PER_M / 1_000_000.0);
            prompt_cost + cached_cost + completion_cost
        } else {
            let prompt_cost = (uncached_prompt as f64) * (MUSE_SPARK_INPUT_COST_PER_M / 1_000_000.0);
            let cached_cost = (cached_tokens as f64) * (MUSE_SPARK_CACHED_INPUT_COST_PER_M / 1_000_000.0);
            let completion_cost = (completion_tokens as f64) * (MUSE_SPARK_OUTPUT_COST_PER_M / 1_000_000.0);
            prompt_cost + cached_cost + completion_cost
        }
    }

    /// Record token usage directly and return the calculated cost in USD.
    pub fn record_tokens(
        &self,
        prompt_tokens: u32,
        completion_tokens: u32,
        cached_tokens: u32,
        is_contributor: bool,
    ) -> f64 {
        let cost = Self::calculate_token_cost(prompt_tokens, completion_tokens, cached_tokens, is_contributor);
        self.record_spend(cost);
        cost
    }

    /// Record and increment harness repair attempt for a candidate, enforcing max 4 rounds.
    pub fn record_repair_round(&self, candidate_id: &str) -> Result<usize, LlmError> {
        let mut map = self.repair_rounds.lock().unwrap();
        let entry = map.entry(candidate_id.to_string()).or_insert(0);
        if *entry >= MAX_HARNESS_REPAIR_ROUNDS {
            return Err(LlmError::MaxRoundsExceeded {
                candidate_id: candidate_id.to_string(),
                current: *entry,
                max: MAX_HARNESS_REPAIR_ROUNDS,
            });
        }
        *entry += 1;
        Ok(*entry)
    }

    /// Record and increment patch attempt for a candidate, enforcing max 3 rounds.
    pub fn record_patch_round(&self, candidate_id: &str) -> Result<usize, LlmError> {
        let mut map = self.patch_rounds.lock().unwrap();
        let entry = map.entry(candidate_id.to_string()).or_insert(0);
        if *entry >= MAX_PATCH_ROUNDS {
            return Err(LlmError::MaxRoundsExceeded {
                candidate_id: candidate_id.to_string(),
                current: *entry,
                max: MAX_PATCH_ROUNDS,
            });
        }
        *entry += 1;
        Ok(*entry)
    }

    /// Total spent in USD
    pub fn total_spent(&self) -> f64 {
        self.spent_micros.load(Ordering::SeqCst) as f64 / 1_000_000.0
    }

    /// Remaining budget before circuit breaker trips
    pub fn remaining_budget(&self) -> f64 {
        let spent = self.spent_micros.load(Ordering::SeqCst);
        if spent >= self.max_budget_micros {
            0.0
        } else {
            (self.max_budget_micros - spent) as f64 / 1_000_000.0
        }
    }
}

