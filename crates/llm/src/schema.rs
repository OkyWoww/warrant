//! # warrant-llm schema
//!
//! Data contracts, canonical schemas, and zero-panic validation logic
//! for project WARRANT (proof-first automated soundness auditor for Rust unsafe code).
//!
//! Strictly adheres to SPEC.md Section 4 and Section 5.2.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::str::FromStr;
use crate::client::LlmError;

/// Mandatory harness header required for all generated test harnesses (SPEC.md Section 5.2 Step 1).
pub const REQUIRED_HARNESS_HEADER: &str = "#![forbid(unsafe_code)]\nextern crate std;\n";

/// Target site context extracted by crates/extract
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractionTarget {
    pub file_path: String,
    pub rule: String, // "R1" | "R2"
    pub target_symbol: String,
    pub line_start: usize,
    pub line_end: usize,
    pub code_snippet: String,
}

/// Structured response output expected from the LLM
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateHarness {
    pub rule: String,
    pub explanation: String,
    /// Harness code without markdown ticks, starting strictly with inner attribute
    pub harness_code: String,
    pub estimated_budget_usd: f64,
}

/// Status of the verifier pipeline for feedback loop
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationFeedback {
    pub compile_passed: bool,
    pub static_filter_passed: bool,
    pub miri_executed: bool,
    pub signature_matched: Option<String>,
    pub stderr_excerpt: String,
}

/// The classification of Unsafe Behavior (UB) targeted by a hypothesis.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UbClass {
    UbOob,
    DataRace,
    UseAfterFree,
    InvalidValue,
    AliasingViolation,
    NullDeref,
    UninitMemory,
    Custom(String),
}

impl fmt::Display for UbClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UbOob => write!(f, "ub_oob"),
            Self::DataRace => write!(f, "data_race"),
            Self::UseAfterFree => write!(f, "use_after_free"),
            Self::InvalidValue => write!(f, "invalid_value"),
            Self::AliasingViolation => write!(f, "aliasing_violation"),
            Self::NullDeref => write!(f, "null_deref"),
            Self::UninitMemory => write!(f, "uninit_memory"),
            Self::Custom(name) => write!(f, "{}", name),
        }
    }
}

impl FromStr for UbClass {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_lowercase().as_str() {
            "ub_oob" | "oob" | "out_of_bounds" => Self::UbOob,
            "data_race" | "race" => Self::DataRace,
            "use_after_free" | "uaf" => Self::UseAfterFree,
            "invalid_value" => Self::InvalidValue,
            "aliasing_violation" | "stacked_borrows" | "tree_borrows" => Self::AliasingViolation,
            "null_deref" => Self::NullDeref,
            "uninit_memory" | "uninit" => Self::UninitMemory,
            other => Self::Custom(other.to_string()),
        })
    }
}

/// Kind of syntactic unsafe construct extracted from Rust AST.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateKind {
    UnsafeBlock,
    UnsafeImpl,
    UnsafeFn,
    RawPtrDeref,
}

/// Candidate unsafe site extracted from target codebase (SPEC.md Section 4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub crate_name: String,
    #[serde(alias = "file")]
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub kind: CandidateKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub code_snippet: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enclosing_item: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_comment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trait_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
}

/// Canonical JSON Candidate representation matching SPEC.md Section 4.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecCandidate {
    pub id: String,
    pub rule: String,
    pub file: String,
    pub line_span: [usize; 2],
    pub enclosing_item: String,
    pub visibility: String,
    pub kind: CandidateKind,
}

impl SpecCandidate {
    /// Zero-panic parsing from JSON string.
    pub fn parse_from_json(json_str: &str) -> Result<Self, LlmError> {
        let spec: Self = serde_json::from_str(json_str)?;
        spec.validate()?;
        Ok(spec)
    }

    /// Enforce data contract invariants.
    pub fn validate(&self) -> Result<(), LlmError> {
        if self.id.trim().is_empty() {
            return Err(LlmError::Validation {
                field: "id",
                reason: "candidate ID must not be empty".to_string(),
            });
        }
        if self.rule != "R1" && self.rule != "R2" {
            return Err(LlmError::Validation {
                field: "rule",
                reason: format!("rule must be 'R1' or 'R2', got '{}'", self.rule),
            });
        }
        if self.file.trim().is_empty() {
            return Err(LlmError::Validation {
                field: "file",
                reason: "file must not be empty".to_string(),
            });
        }
        if self.line_span[0] == 0 || self.line_span[1] < self.line_span[0] {
            return Err(LlmError::Validation {
                field: "line_span",
                reason: format!("invalid line span {:?}", self.line_span),
            });
        }
        Ok(())
    }
}

impl From<SpecCandidate> for Candidate {
    fn from(spec: SpecCandidate) -> Self {
        Candidate {
            id: spec.id,
            crate_name: String::new(),
            file_path: spec.file,
            start_line: spec.line_span[0],
            end_line: spec.line_span[1],
            kind: spec.kind,
            code_snippet: String::new(),
            enclosing_item: Some(spec.enclosing_item),
            safety_comment: None,
            trait_name: None,
            rule: Some(spec.rule),
            visibility: Some(spec.visibility),
        }
    }
}

impl From<&Candidate> for SpecCandidate {
    fn from(candidate: &Candidate) -> Self {
        candidate.to_spec_candidate()
    }
}

impl Candidate {
    /// Zero-panic parsing from JSON string (supports both extended Candidate and SPEC SpecCandidate format).
    pub fn parse_from_json(json_str: &str) -> Result<Self, LlmError> {
        if let Ok(spec) = serde_json::from_str::<SpecCandidate>(json_str) {
            if spec.validate().is_ok() {
                return Ok(spec.into());
            }
        }
        let candidate: Self = serde_json::from_str(json_str)?;
        candidate.validate()?;
        Ok(candidate)
    }

    /// Enforce data contract invariants.
    pub fn validate(&self) -> Result<(), LlmError> {
        if self.id.trim().is_empty() {
            return Err(LlmError::Validation {
                field: "id",
                reason: "candidate ID must not be empty".to_string(),
            });
        }
        if self.file_path.trim().is_empty() {
            return Err(LlmError::Validation {
                field: "file_path",
                reason: "file path must not be empty".to_string(),
            });
        }
        if self.start_line == 0 || self.end_line < self.start_line {
            return Err(LlmError::Validation {
                field: "lines",
                reason: format!(
                    "invalid line span [start={}, end={}]",
                    self.start_line, self.end_line
                ),
            });
        }
        if self.code_snippet.trim().is_empty() && self.rule.is_none() {
            return Err(LlmError::Validation {
                field: "code_snippet",
                reason: "code snippet must not be empty".to_string(),
            });
        }
        Ok(())
    }

    /// 1-indexed line span [start, end].
    pub fn line_span(&self) -> [usize; 2] {
        [self.start_line, self.end_line]
    }

    /// Assigned rule ("R1" or "R2").
    pub fn rule(&self) -> &str {
        self.rule.as_deref().unwrap_or(match self.kind {
            CandidateKind::UnsafeImpl => "R2",
            _ => "R1",
        })
    }

    /// Visibility string ("pub" or "private").
    pub fn visibility(&self) -> &str {
        self.visibility.as_deref().unwrap_or("pub")
    }

    /// Export to canonical SpecCandidate according to SPEC.md Section 4.
    pub fn to_spec_candidate(&self) -> SpecCandidate {
        SpecCandidate {
            id: self.id.clone(),
            rule: self.rule().to_string(),
            file: self.file_path.clone(),
            line_span: self.line_span(),
            enclosing_item: self.enclosing_item.clone().unwrap_or_else(|| self.id.clone()),
            visibility: self.visibility().to_string(),
            kind: self.kind.clone(),
        }
    }

    /// Convert to ExtractionTarget for downstream pipeline processing.
    pub fn to_extraction_target(&self) -> ExtractionTarget {
        ExtractionTarget {
            file_path: self.file_path.clone(),
            rule: self.rule().to_string(),
            target_symbol: self.enclosing_item.clone().unwrap_or_else(|| self.id.clone()),
            line_start: self.start_line,
            line_end: self.end_line,
            code_snippet: self.code_snippet.clone(),
        }
    }
}

/// Formal hypothesis of unsoundness accompanied by an executable safe harness (SPEC.md Section 4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hypothesis {
    pub id: String,
    pub candidate_id: String,
    pub ub_class: UbClass,
    pub explanation: String,
    pub harness_code: String,
    #[serde(default)]
    pub suggested_miri_flags: Vec<String>,
    pub expected_trigger: String,
}

impl Hypothesis {
    /// Zero-panic parsing from JSON string.
    pub fn parse_from_json(json_str: &str) -> Result<Self, LlmError> {
        let hypothesis: Self = serde_json::from_str(json_str)?;
        hypothesis.validate()?;
        Ok(hypothesis)
    }

    /// Ensure the harness satisfies mandatory `#![forbid(unsafe_code)]` followed by `extern crate std;`.
    pub fn validate(&self) -> Result<(), LlmError> {
        if self.id.trim().is_empty() {
            return Err(LlmError::Validation {
                field: "id",
                reason: "hypothesis ID must not be empty".to_string(),
            });
        }
        if self.candidate_id.trim().is_empty() {
            return Err(LlmError::Validation {
                field: "candidate_id",
                reason: "candidate ID must not be empty".to_string(),
            });
        }
        let trimmed = self.harness_code.trim_start();
        if !trimmed.starts_with("#![forbid(unsafe_code)]") && !trimmed.starts_with("#[cfg(test)]") {
            return Err(LlmError::Validation {
                field: "harness_code",
                reason: "harness must strictly start with '#![forbid(unsafe_code)]' or '#[cfg(test)]'".to_string(),
            });
        }
        if !trimmed.contains("extern crate std;") {
            return Err(LlmError::Validation {
                field: "harness_code",
                reason: "harness must contain 'extern crate std;' following the forbid attribute".to_string(),
            });
        }
        Ok(())
    }

    /// Prepend the mandatory header if absent, producing a compliant harness.
    pub fn ensure_canonical_header(raw_code: &str) -> String {
        let trimmed = raw_code.trim();
        if trimmed.starts_with("#![forbid(unsafe_code)]") && trimmed.contains("extern crate std;") {
            trimmed.to_string()
        } else {
            format!("{}{}", REQUIRED_HARNESS_HEADER, trimmed)
        }
    }
}

/// Status of verification under Miri.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    ProvenBug,
    Inconclusive,
    SoundSafe,
    CompilationError,
    Timeout,
}

#[allow(non_upper_case_globals)]
impl VerificationStatus {
    pub const Verified: Self = Self::ProvenBug;
    pub const Unproven: Self = Self::Inconclusive;
    pub const Reject: Self = Self::Inconclusive;
}

/// Remediation badge status for automated program repair (SPEC.md Section 5.3 & Task T11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchStatus {
    PatchVerified,
    PatchUnverified,
    NotApplicable,
}

impl PatchStatus {
    pub fn badge_str(&self) -> &'static str {
        match self {
            Self::PatchVerified => "PATCH_VERIFIED",
            Self::PatchUnverified => "PATCH_UNVERIFIED",
            Self::NotApplicable => "NOT_APPLICABLE",
        }
    }
}

impl fmt::Display for PatchStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.badge_str())
    }
}

/// Verified finding containing full reproduction evidence (SPEC.md Section 4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub candidate_id: String,
    pub hypothesis_id: String,
    pub ub_class: UbClass,
    pub status: VerificationStatus,
    pub harness_code: String,
    pub miri_stdout: String,
    pub miri_stderr: String,
    pub exit_code: i32,
    pub evidence_hash: String,
    pub timestamp_rfc3339: String,
    #[serde(default)]
    pub patch_status: Option<PatchStatus>,
    #[serde(default)]
    pub patch_code: Option<String>,
    #[serde(default)]
    pub patch_diff: Option<String>,
}

impl Finding {
    /// Zero-panic parsing from JSON string.
    pub fn parse_from_json(json_str: &str) -> Result<Self, LlmError> {
        let finding: Self = serde_json::from_str(json_str)?;
        finding.validate()?;
        Ok(finding)
    }

    /// Enforce data contract invariants.
    pub fn validate(&self) -> Result<(), LlmError> {
        if self.id.trim().is_empty() {
            return Err(LlmError::Validation {
                field: "id",
                reason: "finding ID must not be empty".to_string(),
            });
        }
        if self.evidence_hash.trim().is_empty() {
            return Err(LlmError::Validation {
                field: "evidence_hash",
                reason: "evidence hash must not be empty".to_string(),
            });
        }
        Ok(())
    }

    /// Compute cryptographic hash of the evidence bundle.
    pub fn compute_evidence_hash(harness: &str, stderr: &str, exit_code: i32) -> String {
        let mut hasher = Sha256::new();
        hasher.update(harness.as_bytes());
        hasher.update(b"\n---STDERR---\n");
        hasher.update(stderr.as_bytes());
        hasher.update(b"\n---EXIT---\n");
        hasher.update(exit_code.to_string().as_bytes());
        hex::encode(hasher.finalize())
    }
}

/// Canonical 11 pipeline telemetry stages (SPEC.md Section 4 & Task T12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventStage {
    Extract,
    Hypothesis,
    Harness,
    Compile,
    MiriRed,
    Signature,
    Location,
    Tier,
    Patch,
    Sanity,
    Evidence,
}

impl EventStage {
    /// Returns all 11 canonical pipeline stages in execution order.
    pub const fn all() -> [Self; 11] {
        [
            Self::Extract,
            Self::Hypothesis,
            Self::Harness,
            Self::Compile,
            Self::MiriRed,
            Self::Signature,
            Self::Location,
            Self::Tier,
            Self::Patch,
            Self::Sanity,
            Self::Evidence,
        ]
    }

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Extract => "extract",
            Self::Hypothesis => "hypothesis",
            Self::Harness => "harness",
            Self::Compile => "compile",
            Self::MiriRed => "miri_red",
            Self::Signature => "signature",
            Self::Location => "location",
            Self::Tier => "tier",
            Self::Patch => "patch",
            Self::Sanity => "sanity",
            Self::Evidence => "evidence",
        }
    }
}

impl fmt::Display for EventStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Status of an `AuditEvent` stage (SPEC.md Section 4 & Task T12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventStatus {
    Start,
    Ok,
    Fail,
    Skip,
}

impl EventStatus {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Ok => "ok",
            Self::Fail => "fail",
            Self::Skip => "skip",
        }
    }
}

impl fmt::Display for EventStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Real-time JSONL telemetry event record (`events.jsonl`, SPEC.md Section 4 & Task T12).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub ts: String,
    pub run_id: String,
    pub candidate_id: String,
    pub stage: EventStage,
    pub status: EventStatus,
    pub elapsed_ms: u64,
    pub model: String,
    pub cost_usd: f64,
    pub detail: String,
}

impl AuditEvent {
    /// Zero-panic parsing and validation from a single JSONL line.
    pub fn parse_from_json(json_str: &str) -> Result<Self, LlmError> {
        let ev: Self = serde_json::from_str(json_str)?;
        ev.validate()?;
        Ok(ev)
    }

    /// Validate event fields against the SPEC.md Section 4 contract.
    pub fn validate(&self) -> Result<(), LlmError> {
        if self.ts.trim().is_empty() {
            return Err(LlmError::Validation {
                field: "ts",
                reason: "timestamp must not be empty".to_string(),
            });
        }
        if self.run_id.trim().is_empty() {
            return Err(LlmError::Validation {
                field: "run_id",
                reason: "run_id must not be empty".to_string(),
            });
        }
        if self.candidate_id.trim().is_empty() {
            return Err(LlmError::Validation {
                field: "candidate_id",
                reason: "candidate_id must not be empty".to_string(),
            });
        }
        if self.model.trim().is_empty() {
            return Err(LlmError::Validation {
                field: "model",
                reason: "model must not be empty".to_string(),
            });
        }
        if !self.cost_usd.is_finite() || self.cost_usd < 0.0 {
            return Err(LlmError::Validation {
                field: "cost_usd",
                reason: "cost_usd must be finite and non-negative".to_string(),
            });
        }
        Ok(())
    }
}
