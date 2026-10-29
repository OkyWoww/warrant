//! # Autonomous Patch Generator & Repair Prompts (SPEC.md Section 5.3 & Task T11)
//!
//! Generates sound safe-Rust remediation patches for proven unsoundness findings.
//! Enforces at most 3 repair rounds per candidate, recording all token usage and costs
//! in `CostTracker` (SPEC.md Section 7).

use serde::{Deserialize, Serialize};
use crate::client::{LlmClient, LlmError};
use crate::cost_tracker::{CostTracker, MAX_PATCH_ROUNDS};
use crate::schema::Candidate;

/// Request payload sent to the LLM/Agent backend to generate or repair a patch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchRequest {
    pub candidate: Candidate,
    pub original_code: String,
    pub harness_code: String,
    pub miri_signature: String,
    pub round: usize,
    pub error_feedback: Option<String>,
}

/// Proposed remediation patch with explanation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchProposal {
    pub candidate_id: String,
    pub round: usize,
    pub patched_code: String,
    pub explanation: String,
}

/// Autonomous patch generator engine with multi-round feedback
pub struct PatchEngine {
    pub max_rounds: usize,
}

impl Default for PatchEngine {
    fn default() -> Self {
        Self { max_rounds: MAX_PATCH_ROUNDS }
    }
}

impl PatchEngine {
    pub fn new(max_rounds: usize) -> Self {
        Self {
            max_rounds: max_rounds.clamp(1, MAX_PATCH_ROUNDS),
        }
    }

    /// Construct structured system prompt for patch remediation
    pub fn build_system_prompt() -> &'static str {
        r#"You are a Senior Rust Systems & Verification Engineer specializing in unsafe soundness repair.
Given a Rust module containing a proven soundness bug under Miri, propose a minimal, sound patch that eliminates the Undefined Behavior.

STRICT INVARIANTS:
1. Public Signatures Preserved: You MUST NOT delete, rename, or change the parameter types, return types, or public visibility of any public function or struct.
2. Anti-Bypass Guard: You MUST NOT insert `todo!()`, `unimplemented!()`, `unreachable!()`, `std::process::exit`, `panic!()`, or empty `Default::default()` bodies.
3. No Cfg Evasion: You MUST NOT use `#[cfg(not(miri))]` or `#[cfg(false)]`.
4. R1 Soundness: For out-of-bounds/unchecked access, enforce explicit bounds checks (`if idx < len`, `.get()`, `checked_add`).
5. R2 Soundness: For Send/Sync data races, replace non-thread-safe primitives (`Cell`, `RefCell`, `Rc`, raw pointer) with thread-safe synchronization (`AtomicUsize`, `Mutex`, `RwLock`, `Arc`).
6. Keep every existing public and private item name; keep method behavior for valid inputs (a sanity test with valid input must still produce the same output).
7. Do NOT include a `warrant_proof` module or any `#[cfg(test)]` module; the verifier appends its own harness.
8. Output: Return ONLY the complete, compilable Rust module code without markdown formatting."#
    }

    /// Construct user prompt for a specific patch round
    pub fn build_user_prompt(req: &PatchRequest) -> String {
        let mut prompt = format!(
            "Target Candidate: {}\nRule: {}\nTarget Item: {}\nLocation: {}:{}-{}\n\n",
            req.candidate.id,
            req.candidate.rule(),
            req.candidate.enclosing_item.as_deref().unwrap_or("unknown"),
            req.candidate.file_path,
            req.candidate.start_line,
            req.candidate.end_line
        );

        prompt.push_str("ORIGINAL SOURCE CODE:\n```rust\n");
        prompt.push_str(&req.original_code);
        prompt.push_str("\n```\n\n");

        prompt.push_str("PROVING SAFE-RUST HARNESS (TRIGGERED UNDEFINED BEHAVIOR):\n```rust\n");
        prompt.push_str(&req.harness_code);
        prompt.push_str("\n```\n\n");

        prompt.push_str(&format!("MIRI FAILURE SIGNATURE:\n{}\n\n", req.miri_signature));

        if let Some(ref feedback) = req.error_feedback {
            prompt.push_str(&format!(
                "ATTENTION: Round {} failed verification with the following error:\n{}\n\nPlease address this failure in your next repair attempt.\n",
                req.round.saturating_sub(1),
                feedback
            ));
        }

        prompt
    }

    /// Execute single patch generation request through client and cost tracker.
    ///
    /// Token usage and cost are NOT estimated here: a real backend (`MetaClient`) records the
    /// `usage` object of each HTTP response into the tracker ledger itself.
    pub async fn request_patch<C: ?Sized + LlmClient>(
        &self,
        client: &C,
        cost_tracker: &CostTracker,
        req: &PatchRequest,
    ) -> Result<String, LlmError> {
        // Enforce per-candidate patch limit (max 3 rounds) and global budget stop
        cost_tracker.record_patch_round(&req.candidate.id)?;
        cost_tracker.check_and_reserve(0.0)?;

        let finding = crate::schema::Finding {
            id: format!("fnd-patch-{}", req.candidate.id),
            candidate_id: req.candidate.id.clone(),
            hypothesis_id: "hyp-patch".to_string(),
            ub_class: if req.candidate.rule() == "R2" {
                crate::schema::UbClass::DataRace
            } else {
                crate::schema::UbClass::UbOob
            },
            status: crate::schema::VerificationStatus::ProvenBug,
            harness_code: req.harness_code.clone(),
            miri_stdout: String::new(),
            miri_stderr: req.miri_signature.clone(),
            exit_code: 1,
            evidence_hash: "n/a (patch request context)".to_string(),
            timestamp_rfc3339: String::new(),
            patch_status: None,
            patch_code: None,
            patch_diff: None,
        };

        let raw_patch = client
            .complete_patch(
                &req.candidate,
                &finding,
                Self::build_system_prompt(),
                &Self::build_user_prompt(req),
            )
            .await?;

        // Clean any accidental markdown fence formatting
        Ok(clean_code_markdown(&raw_patch))
    }
}

/// Strip markdown triple backtick fences if returned by the LLM
pub fn clean_code_markdown(code: &str) -> String {
    let trimmed = code.trim();
    if trimmed.starts_with("```") {
        let lines: Vec<&str> = trimmed.lines().collect();
        if lines.len() >= 2 && lines.last().map(|l| l.trim()).unwrap_or("") == "```" {
            let inner = lines[1..lines.len() - 1].join("\n");
            return inner.trim().to_string();
        }
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_code_markdown() {
        let raw = "```rust\npub fn foo() {}\n```";
        assert_eq!(clean_code_markdown(raw), "pub fn foo() {}");

        let raw_no_lang = "```\npub fn bar() {}\n```";
        assert_eq!(clean_code_markdown(raw_no_lang), "pub fn bar() {}");

        let plain = "pub fn plain() {}";
        assert_eq!(clean_code_markdown(plain), "pub fn plain() {}");
    }

    #[test]
    fn test_patch_engine_max_rounds_clamped() {
        let engine = PatchEngine::new(10);
        assert_eq!(engine.max_rounds, 3);

        let engine_low = PatchEngine::new(0);
        assert_eq!(engine_low.max_rounds, 1);
    }
}
