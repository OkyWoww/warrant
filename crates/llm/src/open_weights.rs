//! # Open-Weights Backend Stub
//!
//! Stub implementation for self-hosted or partner inference (Baseten/Modal/Fireworks)
//! running Muse Spark Open (`meta-models/muse-spark-open-1026`) or Muse Glimmer.
//!
//! Adheres strictly to SPEC.md Section 7 and Section 10 (Task T2).

use std::sync::Arc;
use std::time::Duration;
use async_trait::async_trait;

use crate::client::{LlmClient, LlmError, OpenWeightsConfig};
use crate::cost_tracker::CostTracker;
use crate::schema::{
    Candidate, CandidateHarness, ExtractionTarget, Finding, Hypothesis, UbClass, VerificationFeedback,
};

/// Open weights model client stub
pub struct OpenWeightsClient {
    _http_client: reqwest::Client,
    config: OpenWeightsConfig,
    cost_tracker: Arc<CostTracker>,
}

impl OpenWeightsClient {
    pub fn new(config: OpenWeightsConfig, cost_tracker: Arc<CostTracker>) -> Result<Self, LlmError> {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .map_err(|e| LlmError::Transport(format!("Failed to build client: {e}")))?;

        Ok(Self {
            _http_client: http_client,
            config,
            cost_tracker,
        })
    }

    pub fn config(&self) -> &OpenWeightsConfig {
        &self.config
    }
}

#[async_trait]
impl LlmClient for OpenWeightsClient {
    async fn propose_hypothesis(&self, candidate: &Candidate) -> Result<Hypothesis, LlmError> {
        // Stub implementation for compilation requirement
        Ok(Hypothesis {
            id: format!("open-hyp-{}", candidate.id),
            candidate_id: candidate.id.clone(),
            ub_class: match candidate.kind {
                crate::schema::CandidateKind::UnsafeBlock => UbClass::UbOob,
                crate::schema::CandidateKind::UnsafeImpl => UbClass::DataRace,
                _ => UbClass::UbOob,
            },
            explanation: format!("Generated via open-weights model {}", self.config.model_name),
            harness_code: "#![forbid(unsafe_code)]\nextern crate std;\n#[test]\nfn proof_stub() {}".to_string(),
            suggested_miri_flags: vec![],
            expected_trigger: "Undefined Behavior".to_string(),
        })
    }

    async fn repair_harness(
        &self,
        candidate: &Candidate,
        failed_hypothesis: &Hypothesis,
        _miri_output: &str,
    ) -> Result<Hypothesis, LlmError> {
        self.cost_tracker.record_repair_round(&candidate.id)?;
        Ok(failed_hypothesis.clone())
    }

    async fn propose_patch(
        &self,
        candidate: &Candidate,
        _finding: &Finding,
    ) -> Result<String, LlmError> {
        self.cost_tracker.record_patch_round(&candidate.id)?;
        Ok(format!("// Open-weights patch for {}", candidate.id))
    }

    async fn generate_harness(
        &self,
        target: &ExtractionTarget,
    ) -> Result<CandidateHarness, LlmError> {
        Ok(CandidateHarness {
            rule: target.rule.clone(),
            explanation: "Open-weights candidate harness".to_string(),
            harness_code: "#![forbid(unsafe_code)]\nextern crate std;\n#[test]\nfn proof_open() {}".to_string(),
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
