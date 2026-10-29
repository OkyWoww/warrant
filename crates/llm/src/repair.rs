//! # warrant-llm repair
//!
//! Autonomous harness repair loop enforcing SPEC.md Section 5.2 Step 2 and Section 7.
//! Manages iterative repair with verbatim compiler error feedback up to a hard ceiling
//! of 4 rounds per candidate, recording all rounds and token expenditures in CostTracker.

use serde::{Deserialize, Serialize};
use std::future::Future;
use crate::client::{LlmClient, LlmError};
use crate::cost_tracker::CostTracker;
use crate::schema::{Candidate, Hypothesis};

/// Outcome of an individual repair round
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairRoundOutcome {
    pub round: usize,
    pub hypothesis: Hypothesis,
    pub static_filter_passed: bool,
    pub compilation_passed: bool,
    pub miri_executed: bool,
    pub error_message: Option<String>,
}

/// Complete diagnostic history of an autonomous repair session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairSession {
    pub candidate_id: String,
    pub rounds: Vec<RepairRoundOutcome>,
    pub valid_at_1: bool,
    pub valid_at_4: bool,
    pub final_hypothesis: Option<Hypothesis>,
    pub total_rounds_executed: usize,
}

impl RepairSession {
    pub fn new(candidate_id: &str) -> Self {
        Self {
            candidate_id: candidate_id.to_string(),
            rounds: Vec::new(),
            valid_at_1: false,
            valid_at_4: false,
            final_hypothesis: None,
            total_rounds_executed: 0,
        }
    }
}

/// Engine executing the multi-round repair loop with compiler feedback
pub struct RepairEngine {
    pub max_rounds: usize,
}

impl Default for RepairEngine {
    fn default() -> Self {
        Self { max_rounds: 4 }
    }
}

impl RepairEngine {
    pub fn new(max_rounds: usize) -> Self {
        Self {
            max_rounds: max_rounds.clamp(1, 4),
        }
    }

    /// Execute the autonomous repair loop
    pub async fn execute<C, FFilter, FCompile, FutCompile>(
        &self,
        client: &C,
        candidate: &Candidate,
        initial_hypothesis: Hypothesis,
        cost_tracker: &CostTracker,
        static_filter: FFilter,
        mut compiler_check: FCompile,
    ) -> Result<RepairSession, LlmError>
    where
        C: ?Sized + LlmClient,
        FFilter: Fn(&str) -> Result<(), String>,
        FCompile: FnMut(&str) -> FutCompile,
        FutCompile: Future<Output = Result<bool, String>>,
    {
        let mut session = RepairSession::new(&candidate.id);
        let mut current_hyp = initial_hypothesis;

        for round in 1..=self.max_rounds {
            session.total_rounds_executed = round;

            // 1. Static Filter Validation
            let static_res = static_filter(&current_hyp.harness_code);
            if let Err(ref filter_err) = static_res {
                session.rounds.push(RepairRoundOutcome {
                    round,
                    hypothesis: current_hyp.clone(),
                    static_filter_passed: false,
                    compilation_passed: false,
                    miri_executed: false,
                    error_message: Some(filter_err.clone()),
                });

                if round < self.max_rounds {
                    cost_tracker.record_repair_round(&candidate.id)?;
                    match client.repair_harness(candidate, &current_hyp, filter_err).await {
                        Ok(repaired) => current_hyp = repaired,
                        Err(LlmError::Json(e)) => {
                            current_hyp.harness_code = format!("// invalid json: {e}");
                        }
                        Err(LlmError::Validation { field, reason }) => {
                            current_hyp.harness_code = format!("// validation error on {field}: {reason}");
                        }
                        Err(e) => return Err(e),
                    }
                    continue;
                } else {
                    // Exhausted rounds
                    break;
                }
            }

            // 2. Compilation Check (`cargo test --no-run`)
            let compile_res = compiler_check(&current_hyp.harness_code).await;
            match compile_res {
                Ok(true) => {
                    // Compilation succeeded!
                    let is_first_round = round == 1;
                    session.valid_at_1 = is_first_round;
                    session.valid_at_4 = true;
                    session.final_hypothesis = Some(current_hyp.clone());

                    session.rounds.push(RepairRoundOutcome {
                        round,
                        hypothesis: current_hyp,
                        static_filter_passed: true,
                        compilation_passed: true,
                        miri_executed: true,
                        error_message: None,
                    });

                    return Ok(session);
                }
                other => {
                    let err_msg = match other {
                        Err(e) => e,
                        Ok(false) => "Compilation failed with non-zero exit code".to_string(),
                        Ok(true) => unreachable!(),
                    };

                    session.rounds.push(RepairRoundOutcome {
                        round,
                        hypothesis: current_hyp.clone(),
                        static_filter_passed: true,
                        compilation_passed: false,
                        miri_executed: false,
                        error_message: Some(err_msg.clone()),
                    });

                    if round < self.max_rounds {
                        cost_tracker.record_repair_round(&candidate.id)?;
                        match client.repair_harness(candidate, &current_hyp, &err_msg).await {
                            Ok(repaired) => current_hyp = repaired,
                            Err(LlmError::Json(e)) => {
                                current_hyp.harness_code = format!("// invalid json: {e}");
                            }
                            Err(LlmError::Validation { field, reason }) => {
                                current_hyp.harness_code = format!("// validation error on {field}: {reason}");
                            }
                            Err(e) => return Err(e),
                        }
                        continue;
                    } else {
                        break;
                    }
                }
            }
        }

        // Exhausted max_rounds without passing both static filter and compilation
        session.valid_at_1 = false;
        session.valid_at_4 = false;
        Ok(session)
    }
}
