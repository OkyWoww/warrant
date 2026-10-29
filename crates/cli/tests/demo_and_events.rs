//! Integration tests for Task T12: `--events <path>` JSONL telemetry & `warrant demo --offline`.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;
use warrant_cli::EventStreamWriter;
use warrant_llm::{AuditEvent, EventStage, EventStatus};

fn warrant_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_warrant"))
}

fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn test_audit_event_schema_validation() {
    let sample = r#"{ "ts": "2026-10-29T10:00:00Z", "run_id": "r-0001", "candidate_id": "c-0001", "stage": "miri_red", "status": "ok", "elapsed_ms": 4120, "model": "muse-spark-1.3", "cost_usd": 0.0, "detail": "Miri UB confirmed at line 42" }"#;
    let parsed = AuditEvent::parse_from_json(sample).expect("canonical SPEC.md Section 4 event must parse");
    assert_eq!(parsed.stage, EventStage::MiriRed);
    assert_eq!(parsed.status, EventStatus::Ok);
    assert_eq!(parsed.elapsed_ms, 4120);
    assert_eq!(parsed.model, "muse-spark-1.3");
}

#[test]
fn test_warrant_audit_emits_valid_events_jsonl() {
    let root = workspace_root();
    let temp = tempfile::tempdir().expect("tempdir");
    let events_path = temp.path().join("audit_events.jsonl");
    let out_dir = temp.path().join("audit_out");

    let target_file = root.join("bench/pairs/r1_reference.rs");
    let output = Command::new(warrant_bin())
        .current_dir(&root)
        .arg("audit")
        .arg(&target_file)
        .arg("--backend")
        .arg("mock")
        .arg("--output-dir")
        .arg(&out_dir)
        .arg("--events")
        .arg(&events_path)
        .output()
        .expect("failed to execute warrant audit");

    // Proven bug in r1_reference.rs returns exit code 1 (EXIT_PROVEN_BUG)
    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit code 1 (PROVEN BUG), stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(events_path.exists(), "audit_events.jsonl must be created");
    let events = EventStreamWriter::load_and_validate_file(&events_path)
        .expect("all JSONL lines must validate against AuditEvent schema");
    assert!(
        events.len() >= 11,
        "Expected all 11 canonical stages to be recorded, got {}",
        events.len()
    );

    let stages: HashSet<EventStage> = events.iter().map(|e| e.stage).collect();
    for expected_stage in EventStage::all() {
        assert!(
            stages.contains(&expected_stage),
            "Missing canonical stage {:?} in audit_events.jsonl",
            expected_stage
        );
    }
}

#[test]
fn test_warrant_demo_offline_replay_and_telemetry() {
    let root = workspace_root();
    let temp = tempfile::tempdir().expect("tempdir");
    let events_path = temp.path().join("demo_events.jsonl");
    let out_dir = temp.path().join("demo_out");

    let start = Instant::now();
    let output = Command::new(warrant_bin())
        .current_dir(&root)
        .arg("demo")
        .arg("--offline")
        .arg("--events")
        .arg(&events_path)
        .arg("--output-dir")
        .arg(&out_dir)
        .output()
        .expect("failed to execute warrant demo --offline");
    let elapsed = start.elapsed();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(0),
        "warrant demo --offline must exit 0.\nSTDOUT:\n{}\nSTDERR:\n{}",
        stdout,
        stderr
    );

    // Verify required honesty & offline replay headers and step-chip bar
    assert!(
        stdout.contains("REAL OUTPUT - captured on"),
        "Missing honesty header in stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("[OFFLINE REPLAY]"),
        "Missing [OFFLINE REPLAY] banner in stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("[✓ extract] -> [✓ hypothesis] -> [✓ harness] -> [✓ compile] -> [✓ miri_red] -> [✓ signature] -> [✓ location] -> [✓ tier] -> [✓ patch] -> [✓ sanity] -> [✓ evidence]"),
        "Missing 11-stage step-chip progress bar in stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("PROVEN_PUBLIC (Miri-Verified Operational Proof)"),
        "Missing PROVEN_PUBLIC verdict in stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("PATCH_VERIFIED"),
        "Missing PATCH_VERIFIED badge in stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("UNPROVEN"),
        "Missing UNPROVEN control case in stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Measured elapsed time:"),
        "Missing measured elapsed time in stdout:\n{}",
        stdout
    );

    // Verify reports and events.jsonl exist and validate 100%
    assert!(out_dir.join("warrant_report.md").exists(), "warrant_report.md must exist");
    assert!(out_dir.join("warrant_report.html").exists(), "warrant_report.html must exist");
    assert!(events_path.exists(), "demo_events.jsonl must exist");

    let events = EventStreamWriter::load_and_validate_file(&events_path)
        .expect("all demo_events.jsonl lines must validate against AuditEvent schema");
    assert_eq!(
        events.len(),
        33,
        "Expected 3 candidates * 11 canonical stages = 33 events"
    );

    // Verify elapsed time < 60s (target < 30s once Miri cache is warm)
    assert!(
        elapsed.as_secs_f64() < 60.0,
        "Demo took too long: {:.2}s",
        elapsed.as_secs_f64()
    );
}

