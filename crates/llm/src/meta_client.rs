//! # Meta Model API Client & Probe Implementation
//!
//! Production client for Meta Model API (`https://api.meta.ai/v1`) supporting
//! Muse Spark, token-usage auditing, round-trip latency recording, and CostTracker integration.
//!
//! Strictly adheres to SPEC.md Section 7 & Section 10 (Task T2).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::client::{AgentBackend, LlmClient, LlmError, MetaApiConfig};
use crate::cost_tracker::CostTracker;
use crate::schema::{
    Candidate, CandidateHarness, ExtractionTarget, Finding, Hypothesis, VerificationFeedback,
};

/// Chat completion message for OpenAI-compatible Meta Model API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Request payload for `/v1/chat/completions`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
}

/// Token usage metadata from Meta Model API response
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    #[serde(default)]
    pub prompt_tokens_details: Option<PromptTokensDetails>,
    #[serde(default)]
    pub completion_tokens_details: Option<CompletionTokensDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromptTokensDetails {
    pub cached_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompletionTokensDetails {
    pub reasoning_tokens: Option<u32>,
}

/// Response choice from Meta Model API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChoice {
    pub index: usize,
    pub message: ChatResponseMessage,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

/// Assistant message in response choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponseMessage {
    pub role: String,
    #[serde(default)]
    pub content: Option<String>,
}

/// Full chat completion response body
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub object: String,
    #[serde(default)]
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    pub usage: Option<TokenUsage>,
}

/// Detailed results of single-call smoke test probe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeReport {
    pub http_status: u16,
    pub latency_ms: u128,
    pub model: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub cached_tokens: u32,
    pub calculated_cost_usd: f64,
    pub response_content: String,
    pub route_header: Option<String>,
}

/// Meta Model API Client backed by reqwest
pub struct MetaClient {
    http_client: reqwest::Client,
    config: MetaApiConfig,
    api_key: String,
    cost_tracker: Arc<CostTracker>,
    /// Full module text per candidate id (SPEC.md 5.1: the model receives the whole module file).
    module_sources: Mutex<HashMap<String, String>>,
}

/// Remove markdown fences and surrounding prose, returning the outermost JSON object text if any.
pub fn extract_json_object(raw: &str) -> String {
    let t = raw.trim();
    match (t.find('{'), t.rfind('}')) {
        (Some(a), Some(b)) if b > a => t[a..=b].to_string(),
        _ => t.to_string(),
    }
}

/// Remove a surrounding ```rust ... ``` fence from model code output.
pub fn extract_code_block(raw: &str) -> String {
    let t = raw.trim();
    if let Some(start) = t.find("```") {
        let after = &t[start + 3..];
        let body_start = after.find('\n').map(|i| i + 1).unwrap_or(0);
        let body = &after[body_start..];
        if let Some(end) = body.find("```") {
            return body[..end].trim().to_string();
        }
    }
    t.to_string()
}

impl MetaClient {
    /// Create a new MetaClient, loading API key from `.env` or system environment.
    /// HTTP timeout defaults to 60 s (override: `WARRANT_HTTP_TIMEOUT_SECS`).
    pub fn new(config: MetaApiConfig, cost_tracker: Arc<CostTracker>) -> Result<Self, LlmError> {
        // Load .env if present
        let _ = dotenvy::dotenv();

        let api_key = std::env::var(&config.api_key_env)
            .or_else(|_| std::env::var("META_MODEL_API_KEY"))
            .or_else(|_| std::env::var("META_API_KEY"))
            .map_err(|_| {
                LlmError::Transport(
                    "Missing API key: Neither META_MODEL_API_KEY nor META_API_KEY is set in environment or .env"
                        .to_string(),
                )
            })?;

        let timeout_secs = std::env::var("WARRANT_HTTP_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60);
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .map_err(|e| LlmError::Transport(format!("Failed to build reqwest client: {e}")))?;

        Ok(Self {
            http_client,
            config,
            api_key,
            cost_tracker,
            module_sources: Mutex::new(HashMap::new()),
        })
    }

    /// Register the full module source for a candidate so prompts carry the whole file.
    pub fn register_module_source(&self, candidate_id: &str, source: &str) {
        if let Ok(mut m) = self.module_sources.lock() {
            m.insert(candidate_id.to_string(), source.to_string());
        }
    }

    fn module_source(&self, candidate: &Candidate) -> String {
        self.module_sources
            .lock()
            .ok()
            .and_then(|m| m.get(&candidate.id).cloned())
            .unwrap_or_else(|| candidate.code_snippet.clone())
    }

    /// Access reference to underlying CostTracker
    pub fn cost_tracker(&self) -> &Arc<CostTracker> {
        &self.cost_tracker
    }

    /// Access reference to config
    pub fn config(&self) -> &MetaApiConfig {
        &self.config
    }

    /// Execute exactly ONE minimal smoke test probe to validate connectivity,
    /// measure round-trip latency, verify token usage JSON fields, and record spend.
    pub async fn probe(&self) -> Result<ProbeReport, LlmError> {
        let request_body = ChatCompletionRequest {
            model: self.config.model_id.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Return the single word: OK".to_string(),
            }],
            temperature: Some(0.0),
            max_tokens: Some(16),
        };

        // Budget reserve check prior to execution
        self.cost_tracker.check_and_reserve(0.001)?;

        let timer = Instant::now();
        let response = self
            .http_client
            .post(&self.config.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| LlmError::Transport(format!("HTTP request failed: {e}")))?;

        let latency_ms = timer.elapsed().as_millis();
        let status = response.status().as_u16();

        let route_header = response
            .headers()
            .get("x-route")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        if status != 200 {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(LlmError::ApiStatus { status, body });
        }

        let parsed: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| LlmError::UnparseableOutput(format!("Failed to parse response JSON: {e}")))?;

        let usage = parsed.usage.unwrap_or_default();
        let cached_tokens = usage
            .prompt_tokens_details
            .as_ref()
            .and_then(|d| d.cached_tokens)
            .unwrap_or(0);

        let is_contributor = self.config.model_id.contains("contributor");
        let calculated_cost = self.cost_tracker.record_tokens(
            usage.prompt_tokens,
            usage.completion_tokens,
            cached_tokens,
            is_contributor,
        );

        let content = parsed
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        Ok(ProbeReport {
            http_status: status,
            latency_ms,
            model: parsed.model,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            cached_tokens,
            calculated_cost_usd: calculated_cost,
            response_content: content.trim().to_string(),
            route_header,
        })
    }

    /// Raw chat call. Every call is written to the `CostTracker` ledger with the real
    /// `usage` token counts from the response JSON and measured wall-clock latency.
    pub async fn chat_raw(
        &self,
        label: &str,
        prompt: &str,
        system: Option<&str>,
    ) -> Result<String, LlmError> {
        let mut messages = Vec::new();
        if let Some(sys) = system {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: sys.to_string(),
            });
        }
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: prompt.to_string(),
        });

        let request = ChatCompletionRequest {
            model: self.config.model_id.clone(),
            messages,
            temperature: Some(self.config.temperature),
            max_tokens: Some(self.config.max_tokens),
        };

        self.cost_tracker.check_and_reserve(0.01)?;

        let timer = Instant::now();
        let response = self
            .http_client
            .post(&self.config.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| LlmError::Transport(format!("HTTP chat error: {e}")))?;

        let status = response.status().as_u16();
        if status != 200 {
            let latency_ms = timer.elapsed().as_millis() as u64;
            let body = response.text().await.unwrap_or_default();
            self.cost_tracker
                .record_call(label, &self.config.model_id, status, None, latency_ms, false);
            return Err(LlmError::ApiStatus { status, body });
        }

        let parsed: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| LlmError::UnparseableOutput(format!("Failed to parse chat response: {e}")))?;
        let latency_ms = timer.elapsed().as_millis() as u64;

        let is_contributor = self.config.model_id.contains("contributor");
        let usage = parsed.usage.as_ref().map(|u| {
            let cached = u
                .prompt_tokens_details
                .as_ref()
                .and_then(|d| d.cached_tokens)
                .unwrap_or(0);
            (u.prompt_tokens, u.completion_tokens, cached)
        });
        self.cost_tracker.record_call(
            label,
            &parsed.model,
            status,
            usage,
            latency_ms,
            is_contributor,
        );

        let text = parsed
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        Ok(text)
    }
}

/// Shared harness contract included in every hypothesis / repair prompt (SPEC.md 4, 5.2).
const HARNESS_CONTRACT: &str = r#"Output ONLY one JSON object (no markdown fences, no prose) with exactly these keys:
{"id": "<any short string>", "candidate_id": "<given candidate id>", "ub_class": "ub_oob" | "data_race",
 "explanation": "<one or two sentences>", "harness_code": "<Rust source as a JSON string>",
 "suggested_miri_flags": [], "expected_trigger": "<short phrase>"}

harness_code MUST be exactly this shape (a child test module appended at the END of the module file):
#![forbid(unsafe_code)]
extern crate std;
#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;
    #[test]
    fn proof_<short_name>() { ... }
}

Rules (a deterministic filter rejects violations without running the harness):
- 100% safe Rust. Call only the module's own `pub` items through their safe API; do not redefine any item name that already exists in the module.
- The test must fail under Miri because the module's `unsafe` code is unsound. For rule R1 drive state so a length/offset/index exceeds the buffer; for rule R2 spawn two scoped threads (std::thread::scope) that touch the shared value concurrently.
- Forbidden words anywhere in harness_code: unsafe (except in `forbid(unsafe_code)`), allow, expect, macro_rules, asm, include, print, println, eprintln, panic, Box::leak, mem::forget, std::process.
- No string literal may contain the phrases "Undefined Behavior", "Data race" or "precondition".
- Do not use assert on the buggy result; just exercise the API (use std::hint::black_box on results)."#;

#[async_trait]
impl LlmClient for MetaClient {
    async fn propose_hypothesis(&self, candidate: &Candidate) -> Result<Hypothesis, LlmError> {
        let system_prompt = format!(
            "You are a Rust soundness auditor. Given a module and a located `unsafe` site, hypothesize a soundness bug reachable from safe code and write a safe-Rust Miri harness.\n{}",
            HARNESS_CONTRACT
        );
        let user_prompt = format!(
            "Candidate id: {}\nRule: {}\nFile: {}\nUnsafe site lines: {}-{}\nKind: {:?}\nEnclosing item: {}\n\nFULL MODULE SOURCE:\n```rust\n{}\n```",
            candidate.id,
            candidate.rule(),
            candidate.file_path,
            candidate.start_line,
            candidate.end_line,
            candidate.kind,
            candidate.enclosing_item.as_deref().unwrap_or("unknown"),
            self.module_source(candidate)
        );

        let raw_response = self
            .chat_raw(&format!("hypothesis:{}", candidate.id), &user_prompt, Some(&system_prompt))
            .await?;
        let mut h = Hypothesis::parse_from_json(&extract_json_object(&raw_response))?;
        h.candidate_id = candidate.id.clone();
        Ok(h)
    }

    async fn repair_harness(
        &self,
        candidate: &Candidate,
        failed_hypothesis: &Hypothesis,
        miri_output: &str,
    ) -> Result<Hypothesis, LlmError> {
        // Round accounting is owned by RepairEngine (SPEC.md 7); do not count twice here.
        let system_prompt = format!(
            "You are a Rust soundness auditor. Your previous harness failed verification. Fix it using the verifier output verbatim.\n{}",
            HARNESS_CONTRACT
        );
        let user_prompt = format!(
            "Candidate id: {}\nRule: {}\nUnsafe site lines: {}-{}\n\nFULL MODULE SOURCE:\n```rust\n{}\n```\n\nPREVIOUS harness_code:\n{}\n\nVERIFIER OUTPUT (verbatim):\n{}",
            candidate.id,
            candidate.rule(),
            candidate.start_line,
            candidate.end_line,
            self.module_source(candidate),
            failed_hypothesis.harness_code,
            miri_output
        );

        let raw_response = self
            .chat_raw(&format!("repair:{}", candidate.id), &user_prompt, Some(&system_prompt))
            .await?;
        let mut h = Hypothesis::parse_from_json(&extract_json_object(&raw_response))?;
        h.candidate_id = candidate.id.clone();
        Ok(h)
    }

    async fn propose_patch(
        &self,
        candidate: &Candidate,
        finding: &Finding,
    ) -> Result<String, LlmError> {
        self.cost_tracker.record_patch_round(&candidate.id)?;

        let system_prompt = "You are a Rust systems engineer. Return the complete patched module source only.";
        let user_prompt = format!(
            "Candidate: {}\nModule:\n{}\nFinding: {:?}\nMiri stderr:\n{}",
            candidate.id,
            self.module_source(candidate),
            finding.ub_class,
            finding.miri_stderr
        );

        let text = self
            .chat_raw(&format!("patch:{}", candidate.id), &user_prompt, Some(system_prompt))
            .await?;
        Ok(extract_code_block(&text))
    }

    async fn complete_patch(
        &self,
        candidate: &Candidate,
        _finding: &Finding,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<String, LlmError> {
        // Patch round accounting is owned by PatchEngine (SPEC.md 7: max 3 per candidate).
        let text = self
            .chat_raw(&format!("patch:{}", candidate.id), user_prompt, Some(system_prompt))
            .await?;
        Ok(extract_code_block(&text))
    }

    async fn generate_harness(
        &self,
        target: &ExtractionTarget,
    ) -> Result<CandidateHarness, LlmError> {
        let candidate = Candidate {
            id: format!("cand-{}", target.target_symbol),
            crate_name: "target_crate".to_string(),
            file_path: target.file_path.clone(),
            start_line: target.line_start,
            end_line: target.line_end,
            kind: match target.rule.as_str() {
                "R2" => crate::schema::CandidateKind::UnsafeImpl,
                _ => crate::schema::CandidateKind::UnsafeBlock,
            },
            code_snippet: target.code_snippet.clone(),
            enclosing_item: Some(target.target_symbol.clone()),
            safety_comment: None,
            trait_name: None,
            rule: Some(target.rule.clone()),
            visibility: Some("pub".to_string()),
        };
        let hypothesis = self.propose_hypothesis(&candidate).await?;
        Ok(CandidateHarness {
            rule: target.rule.clone(),
            explanation: hypothesis.explanation,
            harness_code: hypothesis.harness_code,
            estimated_budget_usd: 0.0,
        })
    }

    async fn repair_candidate_harness(
        &self,
        _target: &ExtractionTarget,
        previous_attempt: &CandidateHarness,
        _feedback: &VerificationFeedback,
    ) -> Result<CandidateHarness, LlmError> {
        Ok(previous_attempt.clone())
    }

    fn cumulative_cost(&self) -> f64 {
        self.cost_tracker.total_spent()
    }
}

/// Fallback implementation of AgentBackend when Muse Code CLI is unavailable
#[async_trait]
impl AgentBackend for MetaClient {
    async fn repair_harness(
        &self,
        candidate: &Candidate,
        failed_hypothesis: &Hypothesis,
        compiler_or_miri_output: &str,
    ) -> Result<Hypothesis, LlmError> {
        LlmClient::repair_harness(self, candidate, failed_hypothesis, compiler_or_miri_output).await
    }

    async fn propose_patch(
        &self,
        candidate: &Candidate,
        finding: &Finding,
    ) -> Result<String, LlmError> {
        LlmClient::propose_patch(self, candidate, finding).await
    }
}
