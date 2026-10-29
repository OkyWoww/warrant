//! # warrant-llm
//!
//! Language model interaction layer, hypothesis generation, data contracts,
//! and deterministic mock backends for project WARRANT.
//!
//! Strictly adheres to SPEC.md Section 4, Section 7, and Section 10 (Task T1).

pub mod client;
pub mod cost_tracker;
pub mod generator;
pub mod meta_client;
pub mod mock;
pub mod open_weights;
pub mod patcher;
pub mod repair;
pub mod schema;

pub use client::*;
pub use cost_tracker::*;
pub use generator::*;
pub use meta_client::*;
pub use mock::*;
pub use open_weights::*;
pub use patcher::*;
pub use repair::*;
pub use schema::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_zero_panic_parsing_valid_candidate() {
        let json = r#"{
            "id": "cand-001",
            "crate_name": "sample",
            "file_path": "src/lib.rs",
            "start_line": 10,
            "end_line": 15,
            "kind": "unsafe_block",
            "code_snippet": "unsafe { *ptr }"
        }"#;
        let parsed = Candidate::parse_from_json(json).expect("should parse valid candidate");
        assert_eq!(parsed.id, "cand-001");
        assert_eq!(parsed.kind, CandidateKind::UnsafeBlock);
    }

    #[test]
    fn test_zero_panic_parsing_invalid_harness_header() {
        let json = r#"{
            "id": "hyp-001",
            "candidate_id": "cand-001",
            "ub_class": "ub_oob",
            "explanation": "Out of bounds access",
            "harness_code": "fn main() { println!(\"no forbid header\"); }",
            "suggested_miri_flags": [],
            "expected_trigger": "error"
        }"#;
        let result = Hypothesis::parse_from_json(json);
        assert!(result.is_err(), "harness without forbid header must fail validation");
    }

    #[test]
    fn test_canonical_header_enforcement() {
        let raw = "fn main() { std::hint::black_box(1); }";
        let canonical = Hypothesis::ensure_canonical_header(raw);
        assert!(canonical.starts_with("#![forbid(unsafe_code)]\nextern crate std;"));
    }

    #[tokio::test]
    async fn test_mock_returns_canned_hypothesis_validating_against_schema() {
        let client = MockLlmClient::new();
        let candidate = Candidate {
            id: "c-0001".to_string(),
            crate_name: "test_crate".to_string(),
            file_path: "src/buffer.rs".to_string(),
            start_line: 14,
            end_line: 14,
            kind: CandidateKind::UnsafeBlock,
            code_snippet: "unsafe { *self.data.get_unchecked(index) }".to_string(),
            enclosing_item: Some("FastBuffer::read_unchecked_elem".to_string()),
            safety_comment: None,
            trait_name: None,
            rule: Some("R1".to_string()),
            visibility: Some("pub".to_string()),
        };

        let hypothesis = client
            .propose_hypothesis(&candidate)
            .await
            .expect("Mock must return hypothesis");

        // Done when: canned Hypothesis validates against the schema
        assert!(hypothesis.validate().is_ok());
        assert_eq!(hypothesis.candidate_id, "c-0001");
        assert_eq!(hypothesis.ub_class, UbClass::UbOob);
        assert!(hypothesis.harness_code.contains("#![forbid(unsafe_code)]"));
        assert!(hypothesis.harness_code.contains("extern crate std;"));
    }

    #[tokio::test]
    async fn test_mock_agent_backend_repair_and_patch() {
        let backend = MockLlmClient::new();
        let candidate = Candidate {
            id: "c-0002".to_string(),
            crate_name: "test_crate".to_string(),
            file_path: "src/sync.rs".to_string(),
            start_line: 20,
            end_line: 20,
            kind: CandidateKind::UnsafeImpl,
            code_snippet: "unsafe impl Sync for SharedCounter {}".to_string(),
            enclosing_item: None,
            safety_comment: None,
            trait_name: Some("Sync".to_string()),
            rule: Some("R2".to_string()),
            visibility: Some("pub".to_string()),
        };

        let initial_hyp = backend.propose_hypothesis(&candidate).await.unwrap();
        assert_eq!(initial_hyp.ub_class, UbClass::DataRace);

        // Test AgentBackend repair loop
        let repaired = AgentBackend::repair_harness(
            &backend,
            &candidate,
            &initial_hyp,
            "error: compile failed on line 12",
        )
        .await
        .expect("repair should succeed");
        assert!(repaired.validate().is_ok());

        // Test AgentBackend patch loop
        let finding = Finding {
            id: "fnd-001".to_string(),
            candidate_id: candidate.id.clone(),
            hypothesis_id: initial_hyp.id.clone(),
            ub_class: UbClass::DataRace,
            status: VerificationStatus::ProvenBug,
            harness_code: initial_hyp.harness_code.clone(),
            miri_stdout: "".to_string(),
            miri_stderr: "Data race detected".to_string(),
            exit_code: 1,
            evidence_hash: "abcd1234".to_string(),
            timestamp_rfc3339: "2026-10-27T00:00:00Z".to_string(),
            patch_status: None,
            patch_code: None,
            patch_diff: None,
        };

        let patch = AgentBackend::propose_patch(&backend, &candidate, &finding)
            .await
            .expect("patch should succeed");
        assert!(patch.contains("AtomicUsize") || patch.contains("SoundShared"));
    }

    #[test]
    fn test_cost_tracker_budget_stop() {
        let tracker = CostTracker::new(10.0); // $10 limit
        assert_eq!(tracker.total_spent(), 0.0);
        assert_eq!(tracker.remaining_budget(), 10.0);

        // Within budget
        assert!(tracker.check_and_reserve(5.0).is_ok());
        tracker.record_spend(5.0);
        assert_eq!(tracker.total_spent(), 5.0);
        assert_eq!(tracker.remaining_budget(), 5.0);

        // Exceeding budget should trigger circuit breaker
        let res = tracker.check_and_reserve(5.01);
        assert!(res.is_err());
        match res.unwrap_err() {
            LlmError::BudgetExceeded { spent, limit } => {
                assert_eq!(spent, 5.0);
                assert_eq!(limit, 10.0);
            }
            other => panic!("Unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_cost_tracker_round_limits() {
        let tracker = CostTracker::default();
        let cand = "c-123";

        // Harness repair allowed up to 4 rounds
        for round in 1..=4 {
            let res = tracker.record_repair_round(cand);
            assert_eq!(res.unwrap(), round);
        }
        // 5th round must fail
        assert!(tracker.record_repair_round(cand).is_err());

        // Patch allowed up to 3 rounds
        for round in 1..=3 {
            let res = tracker.record_patch_round(cand);
            assert_eq!(res.unwrap(), round);
        }
        // 4th round must fail
        assert!(tracker.record_patch_round(cand).is_err());
    }

    #[test]
    fn test_cost_tracker_token_calculation() {
        let tracker = CostTracker::default();
        // 1000 prompt tokens, 500 completion tokens, 0 cached, standard mode
        // 1000 * 1.25/1M = $0.00125
        // 500 * 4.25/1M = $0.002125
        // Total = $0.003375
        let cost = tracker.record_tokens(1000, 500, 0, false);
        assert!((cost - 0.003375).abs() < 1e-6);
        assert!((tracker.total_spent() - 0.003375).abs() < 1e-4);
    }

    #[tokio::test]
    async fn test_open_weights_stub_compiles() {
        let tracker = Arc::new(CostTracker::default());
        let config = OpenWeightsConfig::default();
        let client = OpenWeightsClient::new(config, tracker).expect("open weights stub should initialize");
        let candidate = Candidate {
            id: "c-open-01".to_string(),
            crate_name: "test".to_string(),
            file_path: "src/lib.rs".to_string(),
            start_line: 1,
            end_line: 1,
            kind: CandidateKind::UnsafeBlock,
            code_snippet: "unsafe {}".to_string(),
            enclosing_item: None,
            safety_comment: None,
            trait_name: None,
            rule: Some("R1".to_string()),
            visibility: Some("pub".to_string()),
        };
        let hyp = client.propose_hypothesis(&candidate).await.expect("stub hypothesis");
        assert!(hyp.harness_code.contains("#![forbid(unsafe_code)]"));
    }
}

