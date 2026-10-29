//! # warrant-verify classifier
//!
//! Substring signature classification and location attestation engine for Miri outputs.
//!
//! Strictly adheres to SPEC.md Section 3 (Rules R1 and R2 signatures),
//! Section 5.2 Step 5 (Signature verification), and Step 6 (Location attestation).

use warrant_llm::{Candidate, UbClass, VerificationStatus};
use crate::runner::MiriOutput;

/// Classify the Miri execution output against the expected audit rule (R1 or R2).
pub fn classify_ub_signature(output: &MiriOutput, expected_rule: &str) -> (VerificationStatus, UbClass) {
    let combined = format!("{}\n{}", output.stdout, output.stderr);
    let exit_code = output.exit_code.unwrap_or(-1);

    // 1. REJECT CONDITIONS (SPEC.md Section 3 & Section 5.2 Step 5)
    // Memory leaks must NEVER count as proof
    if combined.contains("memory leaked") || combined.contains("error: memory leak") {
        return (
            VerificationStatus::Inconclusive,
            UbClass::Custom("memory_leak_rejected".to_string()),
        );
    }

    // Compilation errors
    if combined.contains("error[E") || combined.contains("could not compile") {
        return (
            VerificationStatus::CompilationError,
            UbClass::Custom("compilation_error".to_string()),
        );
    }

    // 2. RULE-SPECIFIC SIGNATURE MATCHING
    match expected_rule.to_uppercase().as_str() {
        "R1" => {
            // R1: Out of bounds / unchecked access / precondition violation
            if combined.contains("unsafe precondition(s) violated:")
                || combined.contains("error: Undefined Behavior: dereferencing pointer failed:")
                || combined.contains("dereferencing pointer failed: alloc has size")
                || combined.contains("pointer to 1 byte starting at offset")
                || combined.contains("in-bounds pointer arithmetic failed")
                || combined.contains("memory access failed")
                || combined.contains("going beyond the bounds of its allocation")
                || (combined.contains("constructing invalid value") && combined.contains("Undefined Behavior"))
                || (combined.contains("pointer arithmetic failed") && combined.contains("Undefined Behavior"))
                || (combined.contains("out-of-bounds") && combined.contains("Undefined Behavior"))
            {
                (VerificationStatus::ProvenBug, UbClass::UbOob)
            } else if combined.contains("panicked at") {
                // Assertion panics without precondition violation are rejected as wrong reason
                (
                    VerificationStatus::Inconclusive,
                    UbClass::Custom("assertion_panic_rejected".to_string()),
                )
            } else if exit_code == 0 {
                (VerificationStatus::SoundSafe, UbClass::Custom("clean".to_string()))
            } else {
                (
                    VerificationStatus::Inconclusive,
                    UbClass::Custom("unclassified_failure".to_string()),
                )
            }
        }
        "R2" => {
            // R2: Multithreaded data race on non-Sync types
            if combined.contains("error: Undefined Behavior: Data race detected between")
                || combined.contains("Data race detected between")
            {
                (VerificationStatus::ProvenBug, UbClass::DataRace)
            } else if combined.contains("panicked at") {
                (
                    VerificationStatus::Inconclusive,
                    UbClass::Custom("assertion_panic_rejected".to_string()),
                )
            } else if exit_code == 0 {
                (VerificationStatus::SoundSafe, UbClass::Custom("clean".to_string()))
            } else {
                (
                    VerificationStatus::Inconclusive,
                    UbClass::Custom("unclassified_failure".to_string()),
                )
            }
        }
        _ => {
            // Generic matcher fallback
            if combined.contains("Data race detected") {
                (VerificationStatus::ProvenBug, UbClass::DataRace)
            } else if combined.contains("dereferencing pointer failed") || combined.contains("out-of-bounds") {
                (VerificationStatus::ProvenBug, UbClass::UbOob)
            } else if combined.contains("use-after-free") || combined.contains("dangling pointer") {
                (VerificationStatus::ProvenBug, UbClass::UseAfterFree)
            } else if combined.contains("Undefined Behavior") {
                (VerificationStatus::ProvenBug, UbClass::AliasingViolation)
            } else if exit_code == 0 {
                (VerificationStatus::SoundSafe, UbClass::Custom("clean".to_string()))
            } else {
                (
                    VerificationStatus::Inconclusive,
                    UbClass::Custom("unclassified_failure".to_string()),
                )
            }
        }
    }
}

/// Convenience helper returning only the VerificationStatus
pub fn classify_status(output: &MiriOutput, expected_rule: &str) -> VerificationStatus {
    classify_ub_signature(output, expected_rule).0
}

/// Extract the exact matched signature substring for reporting and evidence
pub fn extract_matched_signature(combined: &str, expected_rule: &str) -> Option<String> {
    match expected_rule.to_uppercase().as_str() {
        "R1" => {
            if combined.contains("unsafe precondition(s) violated: slice::get_unchecked requires that the index is within the slice") {
                Some("unsafe precondition(s) violated: slice::get_unchecked requires that the index is within the slice".to_string())
            } else if combined.contains("unsafe precondition(s) violated: slice::get_unchecked_mut requires that the index is within the slice") {
                Some("unsafe precondition(s) violated: slice::get_unchecked_mut requires that the index is within the slice".to_string())
            } else if combined.contains("unsafe precondition(s) violated: slice::get_unchecked requires that the range is within the slice") {
                Some("unsafe precondition(s) violated: slice::get_unchecked requires that the range is within the slice".to_string())
            } else if combined.contains("unsafe precondition(s) violated:") {
                Some("unsafe precondition(s) violated".to_string())
            } else if combined.contains("error: Undefined Behavior: dereferencing pointer failed:") {
                Some("error: Undefined Behavior: dereferencing pointer failed:".to_string())
            } else if combined.contains("dereferencing pointer failed: alloc has size") {
                Some("dereferencing pointer failed: alloc has size".to_string())
            } else if combined.contains("pointer to 1 byte starting at offset") {
                Some("pointer to 1 byte starting at offset".to_string())
            } else if combined.contains("in-bounds pointer arithmetic failed") {
                Some("error: Undefined Behavior: in-bounds pointer arithmetic failed".to_string())
            } else if combined.contains("memory access failed") {
                Some("error: Undefined Behavior: memory access failed".to_string())
            } else if combined.contains("going beyond the bounds of its allocation") {
                Some("error: Undefined Behavior: dangling reference (going beyond the bounds of its allocation)".to_string())
            } else if combined.contains("constructing invalid value") && combined.contains("Undefined Behavior") {
                Some("error: Undefined Behavior: constructing invalid value".to_string())
            } else {
                None
            }
        }
        "R2" => {
            if combined.contains("error: Undefined Behavior: Data race detected between") {
                Some("error: Undefined Behavior: Data race detected between".to_string())
            } else if combined.contains("Data race detected between") {
                Some("Undefined Behavior: Data race detected between".to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Verify location attestation: The Miri stack backtrace MUST contain the candidate's
/// original item or file location (SPEC.md Section 5.2 Step 6).
pub fn verify_location_attestation(stderr: &str, candidate: &Candidate) -> bool {
    // 1. Check enclosing item symbol if available (e.g. `FastBuffer::read_unchecked_elem` or `SharedCounter::increment`)
    if let Some(enclosing) = &candidate.enclosing_item {
        if stderr.contains(enclosing) {
            return true;
        }
        // Also check stripped generic base type (e.g. `SharedContainer<T>` -> `SharedContainer`)
        let base_type = enclosing.split('<').next().unwrap_or(enclosing).trim();
        if !base_type.is_empty() && stderr.contains(base_type) {
            return true;
        }
        // Also check short function name (e.g. `read_unchecked_elem` or `increment`)
        if let Some(short_fn) = enclosing.split("::").last() {
            let stripped_short = short_fn.split('<').next().unwrap_or(short_fn).trim();
            if !stripped_short.is_empty() && stderr.contains(stripped_short) {
                return true;
            }
        }
    }

    // 2. Check file path or filename
    let file_name = std::path::Path::new(&candidate.file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&candidate.file_path);

    if stderr.contains(file_name) {
        // Also verify line number is within or adjacent to candidate span
        for line in candidate.start_line..=candidate.end_line {
            let line_marker = format!("{}:{}", file_name, line);
            if stderr.contains(&line_marker) {
                return true;
            }
        }
    }

    // 3. In isolated temporary packages, target source is copied to src/lib.rs
    for line in candidate.start_line..=candidate.end_line {
        let marker1 = format!("src/lib.rs:{}", line);
        let marker2 = format!(r"src\lib.rs:{}", line);
        let marker3 = format!("lib.rs:{}", line);
        if stderr.contains(&marker1) || stderr.contains(&marker2) || stderr.contains(&marker3) {
            return true;
        }
    }

    false
}
