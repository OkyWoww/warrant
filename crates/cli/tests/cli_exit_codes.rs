//! # Integration Test: CLI Exit Code and Reporting Protocol
//!
//! Validates the standardized exit codes:
//!   - Exit Code 0: Clean / No proven bug discovered
//!   - Exit Code 1: Soundness bug proven under Miri
//!   - Exit Code 2: Error (invalid arguments, missing target path, or IO error)
//!
//! Validates that generated Markdown and HTML reports strictly separate
//! PROVEN and UNPROVEN sections per SPEC.md Section 10 (Task T6) and AGENTS.md Hard Rule 1.

use std::fs;
use std::path::Path;
use std::process::Command;

fn workspace_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

#[test]
fn test_exit_code_0_on_safe_control() {
    let warrant_bin = env!("CARGO_BIN_EXE_warrant");
    let temp_dir = tempfile::tempdir().expect("tempdir");

    let status = Command::new(warrant_bin)
        .current_dir(workspace_dir())
        .arg("audit")
        .arg("bench/pairs/r1_reference_safe.rs")
        .arg("--output-dir")
        .arg(temp_dir.path())
        .status()
        .expect("failed to execute warrant binary");

    assert_eq!(
        status.code(),
        Some(0),
        "Safe control fixture MUST yield Exit Code 0 (CLEAN)"
    );

    let md_report = temp_dir.path().join("warrant_report.md");
    let html_report = temp_dir.path().join("warrant_report.html");

    assert!(md_report.exists(), "Markdown report must be generated");
    assert!(html_report.exists(), "HTML report must be generated");

    let md_content = fs::read_to_string(&md_report).expect("read md");
    assert!(md_content.contains("No proven soundness bugs detected"));
    assert!(md_content.contains("UNPROVEN HYPOTHESES"));
}

#[test]
fn test_exit_code_1_on_proven_mutant() {
    let warrant_bin = env!("CARGO_BIN_EXE_warrant");
    let temp_dir = tempfile::tempdir().expect("tempdir");

    let status = Command::new(warrant_bin)
        .current_dir(workspace_dir())
        .arg("audit")
        .arg("bench/pairs/r1_reference.rs")
        .arg("--output-dir")
        .arg(temp_dir.path())
        .status()
        .expect("failed to execute warrant binary");

    assert_eq!(
        status.code(),
        Some(1),
        "Mutant with reproducible UB MUST yield Exit Code 1 (PROVEN SOUNDNESS BUG)"
    );

    let md_report = temp_dir.path().join("warrant_report.md");
    let html_report = temp_dir.path().join("warrant_report.html");

    assert!(md_report.exists(), "Markdown report must be generated");
    assert!(html_report.exists(), "HTML report must be generated");

    let md_content = fs::read_to_string(&md_report).expect("read md");
    assert!(md_content.contains("PROVEN SOUNDNESS FINDINGS"));
    assert!(md_content.contains("UNPROVEN HYPOTHESES"));
    assert!(md_content.contains("c-0001"));
    assert!(md_content.contains("HARD RULE 1 INVARIANT"));

    let html_content = fs::read_to_string(&html_report).expect("read html");
    assert!(html_content.contains("PROVEN SOUNDNESS BUGS DETECTED"));
    assert!(html_content.contains("c-0001"));
    assert!(html_content.contains("Miri Failure Signature"));
}

#[test]
fn test_exit_code_2_on_invalid_arguments_or_path() {
    let warrant_bin = env!("CARGO_BIN_EXE_warrant");

    // Case A: Missing / Non-existent target file
    let status_missing = Command::new(warrant_bin)
        .current_dir(workspace_dir())
        .arg("audit")
        .arg("non_existent_fixture_path_xyz.rs")
        .status()
        .expect("failed to execute warrant binary");

    assert_eq!(
        status_missing.code(),
        Some(2),
        "Non-existent target path MUST yield Exit Code 2 (ERROR)"
    );

    // Case B: Invalid flag / clap parse failure
    let status_invalid_flag = Command::new(warrant_bin)
        .current_dir(workspace_dir())
        .arg("audit")
        .arg("--unrecognized-invalid-flag-12345")
        .status()
        .expect("failed to execute warrant binary");

    assert_eq!(
        status_invalid_flag.code(),
        Some(2),
        "Unrecognized flag / CLI argument failure MUST yield Exit Code 2 (ERROR)"
    );
}

