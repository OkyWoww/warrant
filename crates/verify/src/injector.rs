//! # warrant-verify injector
//!
//! Strict inline harness injector and static filter enforcer.
//!
//! Strictly enforces SPEC.md Section 5.2 Step 1 (append to end of file copy without line shifting)
//! and Step 3 (deterministic static filter rejecting banned words and signature forgeries).

use regex::Regex;
use warrant_llm::REQUIRED_HARNESS_HEADER;
use crate::VerifyError;

/// Strict harness injector ensuring the invariant `#![forbid(unsafe_code)]\nextern crate std;`.
#[derive(Debug, Default, Clone)]
pub struct HarnessInjector;

impl HarnessInjector {
    pub fn new() -> Self {
        Self
    }

    /// Static filter (SPEC.md Section 5.2 Step 3).
    /// Rejects the harness without running it if any of these holds:
    /// - Contains banned words/paths as whole words: unsafe, allow, expect, macro_rules, asm, include,
    ///   print, println, eprintln, panic, Box::leak, mem::forget, std::process.
    /// - Contains signature phrases in string literals: Undefined Behavior, Data race, precondition.
    pub fn validate_static_filter(&self, harness: &str) -> Result<(), VerifyError> {
        // 1. Check banned whole words using regex word boundaries (\b...\b)
        // \bunsafe\b matches 'unsafe', but not 'unsafe_code' because '_' is a word character.
        let banned_patterns = [
            r"\bunsafe\b",
            r"\ballow\b",
            r"\bexpect\b",
            r"\bmacro_rules\b",
            r"\basm\b",
            r"\binclude\b",
            r"\bprint\b",
            r"\bprintln\b",
            r"\beprintln\b",
            r"\bpanic\b",
            r"Box\s*::\s*leak",
            r"mem\s*::\s*forget",
            r"std\s*::\s*process",
        ];

        for pattern in &banned_patterns {
            let re = Regex::new(pattern).map_err(|e| VerifyError::HarnessViolation(e.to_string()))?;
            if let Some(m) = re.find(harness) {
                return Err(VerifyError::HarnessViolation(format!(
                    "Static filter rejected harness: contains banned word/path '{}' at byte {}",
                    m.as_str(),
                    m.start()
                )));
            }
        }

        // 2. Reject signature forgery in string literals
        let signature_phrases = ["Undefined Behavior", "Data race", "precondition"];
        for phrase in &signature_phrases {
            if harness.contains(phrase) {
                return Err(VerifyError::HarnessViolation(format!(
                    "Static filter rejected harness: contains signature phrase '{}' to prevent forged signatures",
                    phrase
                )));
            }
        }

        Ok(())
    }

    /// Validates and standardizes a harness source string before injection.
    pub fn prepare_harness(&self, raw_harness: &str) -> Result<String, VerifyError> {
        let trimmed = raw_harness.trim_start_matches('\u{feff}').trim_start();

        // Hard rule validation
        if !trimmed.starts_with("#![forbid(unsafe_code)]") && !trimmed.starts_with("#[cfg(test)]") {
            let preview: String = trimmed.chars().take(40).collect();
            return Err(VerifyError::HarnessViolation(format!(
                "Harness MUST strictly begin with '#![forbid(unsafe_code)]' or '#[cfg(test)]' (got {:?})",
                preview
            )));
        }

        if !trimmed.contains("#![forbid(unsafe_code)]") {
            return Err(VerifyError::HarnessViolation(
                "Harness MUST contain '#![forbid(unsafe_code)]'".to_string(),
            ));
        }

        if !trimmed.contains("extern crate std;") {
            return Err(VerifyError::HarnessViolation(
                "Harness MUST declare 'extern crate std;' immediately after '#![forbid(unsafe_code)]'".to_string(),
            ));
        }

        // Apply SPEC.md Section 5.2 Step 3 static filter
        self.validate_static_filter(trimmed)?;

        Ok(trimmed.to_string())
    }

    /// Append the harness module to the end of a copy of the target file (SPEC.md Section 5.2 Step 1).
    /// Line numbers of the original target file MUST NOT shift.
    pub fn inject_to_target_copy(&self, target_content: &str, harness_code: &str) -> Result<String, VerifyError> {
        let validated_harness = self.prepare_harness(harness_code)?;

        // Strip any outer preamble before #[cfg(test)] to ensure inner attributes remain inside the module
        let clean_harness = if let Some(idx) = validated_harness.find("#[cfg(test)]") {
            &validated_harness[idx..]
        } else {
            &validated_harness[..]
        };

        // If the validated harness is not already wrapped in a module, wrap it
        let harness_module = if clean_harness.contains("mod warrant_proof") {
            clean_harness.to_string()
        } else {
            format!(
                "\n#[cfg(test)]\nmod warrant_proof {{\n{}\n}}\n",
                clean_harness
            )
        };

        // Strip any existing warrant_proof module at the bottom so injection is idempotent
        let base_target = if let Some(idx) = target_content.find("// ==============================================================================") {
            target_content[..idx].trim_end()
        } else if let Some(idx) = target_content.find("#[cfg(test)]\nmod warrant_proof") {
            target_content[..idx].trim_end()
        } else if let Some(idx) = target_content.find("mod warrant_proof") {
            target_content[..idx].trim_end()
        } else {
            target_content.trim_end()
        };

        // Appending to the very end preserves all original target line numbers exactly
        let mut combined = base_target.to_string();
        combined.push('\n');
        combined.push_str("\n// ==============================================================================\n");
        combined.push_str("// WARRANT Safe Verification Harness (Appended at end of file copy)\n");
        combined.push_str("// ==============================================================================\n");
        combined.push_str(&harness_module);

        Ok(combined)
    }

    /// Combines target item definition and test harness into a standalone test crate main.rs.
    pub fn inject_inline(
        &self,
        target_definition: &str,
        raw_harness: &str,
    ) -> Result<String, VerifyError> {
        let validated_harness = self.prepare_harness(raw_harness)?;

        let body = validated_harness
            .strip_prefix(REQUIRED_HARNESS_HEADER)
            .unwrap_or(&validated_harness);

        let combined = format!(
            "{}\n// Target under audit\n{}\n\n// Safe verification harness\n{}\n",
            REQUIRED_HARNESS_HEADER.trim_end(),
            target_definition,
            body
        );

        Ok(combined)
    }
}
