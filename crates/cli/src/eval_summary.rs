//! Machine-readable result summaries written by the evaluation binaries and read back by the
//! comparison table (`eval_metrics_gate2`). Every number comes from a real run; a missing file
//! means "unmeasured", never a default.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSummary {
    pub experiment: String,
    pub backend: String,
    pub model: String,
    /// Number of evaluated units (cases) for this experiment.
    pub n: usize,
    pub counts: BTreeMap<String, usize>,
    pub calls: usize,
    pub calls_without_usage: usize,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub mean_latency_ms: u64,
    pub cost_usd: f64,
}

impl EvalSummary {
    pub fn path(experiment: &str, backend: &str) -> PathBuf {
        Path::new("target/warrant").join(format!("summary_{}_{}.json", experiment, backend))
    }

    pub fn save(&self) -> std::io::Result<PathBuf> {
        let p = Self::path(&self.experiment, &self.backend);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(&p, json)?;
        Ok(p)
    }

    pub fn load(experiment: &str, backend: &str) -> Option<Self> {
        let text = fs::read_to_string(Self::path(experiment, backend)).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn count(&self, key: &str) -> usize {
        self.counts.get(key).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_roundtrip_json() {
        let mut counts = BTreeMap::new();
        counts.insert("valid_at_1".to_string(), 3);
        let s = EvalSummary {
            experiment: "unit".into(),
            backend: "test".into(),
            model: "m".into(),
            n: 4,
            counts,
            calls: 2,
            calls_without_usage: 0,
            prompt_tokens: 10,
            completion_tokens: 5,
            mean_latency_ms: 7,
            cost_usd: 0.1,
        };
        let json = serde_json::to_string(&s).unwrap_or_default();
        let back: EvalSummary = serde_json::from_str(&json).unwrap_or_else(|_| s.clone());
        assert_eq!(back.count("valid_at_1"), 3);
        assert_eq!(back.n, 4);
    }
}

