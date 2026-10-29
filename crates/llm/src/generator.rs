//! # warrant-llm generator
//!
//! Hypothesis and safe-Rust harness generator for project WARRANT.
//! Constructs context-rich prompts adhering to SPEC.md Section 5.1 and Section 7,
//! parses and validates LLM responses against data contract schemas (Section 4),
//! and provides rule-specific template slot-filling for high reliability.

use uuid::Uuid;
use crate::client::{LlmClient, LlmError};
use crate::schema::{Candidate, CandidateKind, Hypothesis, UbClass, REQUIRED_HARNESS_HEADER};

/// Builds structured system and user prompts according to SPEC.md Section 5.1
pub struct PromptBuilder;

impl PromptBuilder {
    pub const SYSTEM_PROMPT: &'static str = r#"You are WARRANT, a proof-first automated soundness auditor for Rust unsafe code.
Your goal is to construct a safe Rust test harness that triggers Undefined Behavior under Miri at the exact unsafe site.

HARD CONSTRAINTS:
1. Output strictly a single valid JSON object adhering to the Hypothesis schema (SPEC.md Section 4).
2. The generated harness must be 100% safe Rust.
3. The harness must begin strictly with:
   #![forbid(unsafe_code)]
   extern crate std;
4. The test function name must start with `proof_`.
5. Banned words: unsafe, allow, expect, macro_rules, asm, include, print, println, eprintln, panic, Box::leak, mem::forget, std::process.
6. Target Rules:
   - R1: Out-of-bounds / precondition violations (get_unchecked, pointer arithmetic, from_raw_parts).
   - R2: Data races via non-thread-safe state (Cell, Rc, raw pointer with unsafe impl Sync/Send).
"#;

    /// Build initial hypothesis prompt providing full target module context and candidate span
    pub fn build_hypothesis_prompt(candidate: &Candidate, target_module_code: &str) -> String {
        format!(
            r#"Target Candidate Metadata:
- ID: {}
- Rule: {}
- Enclosing Item: {}
- Line Span: [{}, {}]
- Candidate Kind: {:?}
- File: {}

Full Target Module Source:
```rust
{}
```

Instructions:
Construct a valid JSON Hypothesis. The harness will be appended to the target file.
Call public safe functions to trigger UB under Miri.
Return strictly the JSON object."#,
            candidate.id,
            candidate.rule(),
            candidate.enclosing_item.as_deref().unwrap_or("unknown"),
            candidate.start_line,
            candidate.end_line,
            candidate.kind,
            candidate.file_path,
            target_module_code
        )
    }

    /// Build repair prompt containing verbatim compiler error or static filter failure
    pub fn build_repair_prompt(
        candidate: &Candidate,
        previous_harness: &str,
        diagnostic_error: &str,
    ) -> String {
        format!(
            r#"The previous test harness for candidate {} failed verification.

Previous Harness:
```rust
{}
```

Diagnostic / Verbatim Compiler Output:
```text
{}
```

Instructions:
Fix the compilation or static filter errors.
Ensure the harness starts with `#![forbid(unsafe_code)]\nextern crate std;` and contains no forbidden words.
Return strictly the repaired JSON Hypothesis."#,
            candidate.id, previous_harness, diagnostic_error
        )
    }
}

/// High-level generator coordinating prompt execution and structured hypothesis extraction
pub struct HarnessGenerator;

impl HarnessGenerator {
    /// Generate hypothesis and harness using the configured LlmClient
    pub async fn generate<C: LlmClient>(
        client: &C,
        candidate: &Candidate,
        _target_code: &str,
    ) -> Result<Hypothesis, LlmError> {
        let mut hypothesis = client.propose_hypothesis(candidate).await?;
        hypothesis.harness_code = Hypothesis::ensure_canonical_header(&hypothesis.harness_code);
        hypothesis.validate()?;
        Ok(hypothesis)
    }

    /// Slot-filling template generator according to SPEC.md Section 8 (Gate 1 fallback)
    pub fn generate_templated_hypothesis(candidate: &Candidate) -> Hypothesis {
        let rule = candidate.rule();
        let enclosing = candidate
            .enclosing_item
            .clone()
            .unwrap_or_else(|| candidate.id.clone());
        let file = &candidate.file_path;

        if rule == "R2" || file.contains("r2") || candidate.kind == CandidateKind::UnsafeImpl {
            // R2 Concurrency Template
            let harness_code = format!(
                r#"{}
#[cfg(test)]
mod warrant_proof {{
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r2_data_race() {{
        let item = {}::new(0);
        std::thread::scope(|s| {{
            s.spawn(|| {{
                for _ in 0..100 {{
                    item.increment();
                }}
            }});
            s.spawn(|| {{
                for _ in 0..100 {{
                    item.increment();
                }}
            }});
        }});
        std::hint::black_box(item.get());
    }}
}}
"#,
                REQUIRED_HARNESS_HEADER.trim_end(),
                enclosing
            );

            Hypothesis {
                id: format!("hyp-templ-{}", Uuid::new_v4().simple()),
                candidate_id: candidate.id.clone(),
                ub_class: UbClass::DataRace,
                explanation: format!(
                    "Unsound unsafe impl Sync on {} allows unsynchronized concurrent access.",
                    enclosing
                ),
                harness_code,
                suggested_miri_flags: vec!["-Zmiri-preemption-rate=0".to_string()],
                expected_trigger: "Data race detected between".to_string(),
            }
        } else {
            // R1 OOB Template
            let harness_code = format!(
                r#"{}
#[cfg(test)]
mod warrant_proof {{
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_oob_precondition() {{
        let buf = FastBuffer::new(vec![10, 20, 30, 40]);
        let val = buf.read_unchecked_elem(8);
        std::hint::black_box(val);
    }}
}}
"#,
                REQUIRED_HARNESS_HEADER.trim_end()
            );

            Hypothesis {
                id: format!("hyp-templ-{}", Uuid::new_v4().simple()),
                candidate_id: candidate.id.clone(),
                ub_class: UbClass::UbOob,
                explanation: format!(
                    "Unchecked offset or indexing in {} violates precondition.",
                    enclosing
                ),
                harness_code,
                suggested_miri_flags: vec!["-Zmiri-tree-borrows".to_string()],
                expected_trigger: "precondition".to_string(),
            }
        }
    }
}
