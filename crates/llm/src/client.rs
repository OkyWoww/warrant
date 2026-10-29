//! # warrant-llm client
//!
//! Trait definitions for LLM backends and agentic loops (`LlmClient` and `AgentBackend`),
//! error representations, and provider configurations.
//!
//! Adheres strictly to SPEC.md Section 7 and Section 10 (Task T1).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use crate::schema::{
    Candidate, CandidateHarness, ExtractionTarget, Finding, Hypothesis, VerificationFeedback,
};

/// Errors originating from LLM interaction, schema serialization, or validation.
#[derive(Debug, Error)]
pub enum LlmError {
    #[error("JSON serialization or deserialization failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Data contract validation failure: {field} - {reason}")]
    Validation {
        field: &'static str,
        reason: String,
    },

    #[error("Local budget exceeded: spent ${spent:.4}, limit is ${limit:.4}")]
    BudgetExceeded { spent: f64, limit: f64 },

    #[error("Exceeded maximum repair rounds ({current}/{max}) for candidate {candidate_id}")]
    MaxRoundsExceeded {
        candidate_id: String,
        current: usize,
        max: usize,
    },

    #[error("API transport or execution error: {0}")]
    Transport(String),

    #[error("Model refused or produced unparseable output: {0}")]
    UnparseableOutput(String),

    #[error("Parsing error: {0}")]
    ParseError(String),

    #[error("Candidate with ID {0} not found in mock store")]
    MockNotFound(String),

    #[error("Upstream HTTP {status}: {body}")]
    ApiStatus { status: u16, body: String },
}

/// Core asynchronous trait defining LLM capabilities for Project WARRANT (SPEC.md Section 7).
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Propose a concrete unsoundness hypothesis and safe harness for an unsafe candidate.
    async fn propose_hypothesis(&self, candidate: &Candidate) -> Result<Hypothesis, LlmError>;

    /// Iteratively repair a harness that failed to trigger UB or failed compilation.
    async fn repair_harness(
        &self,
        candidate: &Candidate,
        failed_hypothesis: &Hypothesis,
        miri_output: &str,
    ) -> Result<Hypothesis, LlmError>;

    /// Propose a sound Rust patch eliminating the verified UB while preserving functionality.
    async fn propose_patch(
        &self,
        candidate: &Candidate,
        finding: &Finding,
    ) -> Result<String, LlmError>;

    /// Patch generation with caller-built prompts (full module, harness, Miri signature, prior-round
    /// verifier feedback). Backends without a real model (mock) fall back to `propose_patch`.
    async fn complete_patch(
        &self,
        candidate: &Candidate,
        finding: &Finding,
        _system_prompt: &str,
        _user_prompt: &str,
    ) -> Result<String, LlmError> {
        self.propose_patch(candidate, finding).await
    }

    /// Initial attempt to generate a harness proof for an extracted site (pipeline helper)
    async fn generate_harness(
        &self,
        target: &ExtractionTarget,
    ) -> Result<CandidateHarness, LlmError>;

    /// Feedback iteration when verifier rejects or fails to trigger UB (pipeline helper)
    async fn repair_candidate_harness(
        &self,
        target: &ExtractionTarget,
        previous_attempt: &CandidateHarness,
        feedback: &VerificationFeedback,
    ) -> Result<CandidateHarness, LlmError>;

    /// Return cumulative tracked cost in USD
    fn cumulative_cost(&self) -> f64 {
        0.0
    }
}

/// Agentic backend abstraction for Muse Code or programmatic agent loops (SPEC.md Section 7 & Task T1).
#[async_trait]
pub trait AgentBackend: Send + Sync {
    /// Agentic harness repair loop
    async fn repair_harness(
        &self,
        candidate: &Candidate,
        failed_hypothesis: &Hypothesis,
        compiler_or_miri_output: &str,
    ) -> Result<Hypothesis, LlmError>;

    /// Agentic patch proposal loop
    async fn propose_patch(
        &self,
        candidate: &Candidate,
        finding: &Finding,
    ) -> Result<String, LlmError>;
}

/// Production Meta API / Enterprise LLM backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaApiConfig {
    pub endpoint: String,
    pub model_id: String,
    pub api_key_env: String,
    pub temperature: f32,
    pub max_tokens: usize,
}

impl Default for MetaApiConfig {
    fn default() -> Self {
        Self {
            endpoint: "https://api.meta.ai/v1/chat/completions".to_string(),
            model_id: "muse-spark-1.3".to_string(),
            api_key_env: "META_MODEL_API_KEY".to_string(),
            temperature: 0.1,
            max_tokens: 8192,
        }
    }
}

/// Open Weights (vLLM / Ollama / Self-hosted) client backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenWeightsConfig {
    pub base_url: String,
    pub model_name: String,
    pub context_window: usize,
    pub timeout_seconds: u64,
}

impl Default for OpenWeightsConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:8000/v1".to_string(),
            model_name: "meta-models/muse-spark-open-1026".to_string(),
            context_window: 16384,
            timeout_seconds: 60,
        }
    }
}
