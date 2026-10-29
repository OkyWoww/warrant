//! # Event Stream Telemetry (`events.jsonl`, SPEC.md Section 4 & Task T12)
//!
//! Real-time JSONL telemetry emitter and terminal step-chip progress renderer
//! for `warrant audit --events <path>` and `warrant demo`.

use anyhow::Result;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use warrant_llm::{AuditEvent, EventStage, EventStatus};

/// Convert current `SystemTime` into an RFC3339 UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`).
pub fn now_rfc3339_utc() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (year, month, day, hour, min, sec) = unix_secs_to_ymdhms(secs);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, min, sec
    )
}

/// Convert current `SystemTime` into a `YYYY-MM-DD` date string.
pub fn now_date_ymd() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (year, month, day, _, _, _) = unix_secs_to_ymdhms(secs);
    format!("{:04}-{:02}-{:02}", year, month, day)
}

/// Howard Hinnant's civil_from_days algorithm for exact UTC calendar conversion.
fn unix_secs_to_ymdhms(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = (secs % 86_400) as u32;
    let hour = rem / 3600;
    let min = (rem % 3600) / 60;
    let sec = rem % 60;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d, hour, min, sec)
}

/// Real-time JSONL writer that streams `AuditEvent` records to one or more file paths.
pub struct EventStreamWriter {
    run_id: String,
    model_name: String,
    files: Vec<(PathBuf, File)>,
    events: Vec<AuditEvent>,
}

impl EventStreamWriter {
    /// Create a new `EventStreamWriter` writing to all provided `paths` (truncating any existing file at start of run).
    pub fn new(run_id: impl Into<String>, model_name: impl Into<String>, paths: &[PathBuf]) -> Result<Self> {
        let mut files = Vec::new();
        for p in paths {
            if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)?;
                }
            }
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(p)?;
            files.push((p.clone(), file));
        }
        Ok(Self {
            run_id: run_id.into(),
            model_name: model_name.into(),
            files,
            events: Vec::new(),
        })
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn set_model_name(&mut self, model_name: impl Into<String>) {
        self.model_name = model_name.into();
    }

    pub fn events(&self) -> &[AuditEvent] {
        &self.events
    }

    /// Emit and flush a single `AuditEvent` record in real-time.
    pub fn emit(
        &mut self,
        candidate_id: &str,
        stage: EventStage,
        status: EventStatus,
        elapsed_ms: u64,
        cost_usd: f64,
        detail: impl Into<String>,
    ) -> Result<AuditEvent> {
        let event = AuditEvent {
            ts: now_rfc3339_utc(),
            run_id: self.run_id.clone(),
            candidate_id: candidate_id.to_string(),
            stage,
            status,
            elapsed_ms,
            model: self.model_name.clone(),
            cost_usd,
            detail: detail.into(),
        };
        event.validate()?;

        let line = serde_json::to_string(&event)?;
        for (_, f) in &mut self.files {
            writeln!(f, "{}", line)?;
            f.flush()?;
        }
        self.events.push(event.clone());
        Ok(event)
    }

    /// Render the canonical 11-stage terminal step-chip progress bar for a candidate:
    /// `[✓ extract] -> [✓ hypothesis] -> [✓ harness] -> [✓ compile] -> [✓ miri_red] -> [✓ signature] -> [✓ location] -> [✓ tier] -> [✓ patch] -> [✓ sanity] -> [✓ evidence]`
    pub fn format_step_chip_bar(&self, candidate_id: &str) -> String {
        let mut latest_status: HashMap<EventStage, EventStatus> = HashMap::new();
        for ev in &self.events {
            if ev.candidate_id == candidate_id {
                latest_status.insert(ev.stage, ev.status);
            }
        }

        let chips: Vec<String> = EventStage::all()
            .iter()
            .map(|stage| {
                let symbol = match latest_status.get(stage) {
                    Some(EventStatus::Ok) => "✓",
                    Some(EventStatus::Fail) => "✗",
                    Some(EventStatus::Skip) => "–",
                    Some(EventStatus::Start) => "…",
                    None => "·",
                };
                format!("[{} {}]", symbol, stage.as_str())
            })
            .collect();

        chips.join(" -> ")
    }

    /// Parse and validate all JSONL lines in a file on disk.
    pub fn load_and_validate_file(path: &Path) -> Result<Vec<AuditEvent>> {
        let content = fs::read_to_string(path)?;
        let mut parsed = Vec::new();
        for (idx, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let ev = AuditEvent::parse_from_json(trimmed)
                .map_err(|e| anyhow::anyhow!("Invalid AuditEvent at line {}: {}", idx + 1, e))?;
            parsed.push(ev);
        }
        Ok(parsed)
    }
}

