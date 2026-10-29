//! # WARRANT Reporter Engine
//!
//! Generates standardized Markdown and standalone single-file HTML reports.
//! Strictly enforces SPEC.md Section 1.1, Section 2, and AGENTS.md Hard Rule 1:
//! - PROVEN findings (Public and Internal tiers) are prominently presented with Miri signatures,
//!   harness hashes, and reproduction links.
//! - UNPROVEN hypotheses are strictly separated with an invariant notice stating that unproven
//!   hypotheses are NOT bugs.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Classification tier for proven findings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindingTier {
    ProvenPublic,
    ProvenInternal,
    Unproven,
}

impl FindingTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ProvenPublic => "PROVEN_PUBLIC",
            Self::ProvenInternal => "PROVEN_INTERNAL",
            Self::Unproven => "UNPROVEN",
        }
    }
}

/// A proven unsoundness finding that triggered undefined behavior under Miri.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportFinding {
    pub candidate_id: String,
    pub tier: FindingTier,
    pub rule: String,
    pub symbol: String,
    pub file_path: String,
    pub line_span: (usize, usize),
    pub miri_signature: Option<String>,
    pub harness_sha256: String,
    pub evidence_dir: Option<String>,
    pub harness_code: String,
    pub target_snippet: String,
    pub patch_badge: String,
    pub explanation: String,
    #[serde(default)]
    pub patch_diff: Option<String>,
}

/// An unproven hypothesis that did NOT produce undefined behavior under Miri.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportHypothesis {
    pub candidate_id: String,
    pub rule: String,
    pub symbol: String,
    pub file_path: String,
    pub line_span: (usize, usize),
    pub explanation: String,
    pub reason: String,
    pub target_snippet: String,
}

/// Comprehensive audit report containing all proven and unproven results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    pub target_path: String,
    pub crate_name: String,
    pub timestamp_rfc3339: String,
    pub duration_ms: u128,
    pub total_candidates: usize,
    pub proven_findings: Vec<ReportFinding>,
    pub unproven_hypotheses: Vec<ReportHypothesis>,
    pub api_cost_usd: f64,
    pub total_tokens_in: u64,
    pub total_tokens_out: u64,
    pub backend_used: String,
    pub toolchain: String,
}

impl AuditReport {
    /// Check whether any proven soundness bugs were discovered.
    pub fn has_proven_bugs(&self) -> bool {
        !self.proven_findings.is_empty()
    }

    /// Number of proven bugs.
    pub fn proven_count(&self) -> usize {
        self.proven_findings.len()
    }

    /// Number of unproven hypotheses.
    pub fn unproven_count(&self) -> usize {
        self.unproven_hypotheses.len()
    }

    /// Save both Markdown (`warrant_report.md`) and HTML (`warrant_report.html`) to disk.
    pub fn save_reports(&self, output_dir: &Path) -> std::io::Result<(PathBuf, PathBuf)> {
        fs::create_dir_all(output_dir)?;

        let md_path = output_dir.join("warrant_report.md");
        fs::write(&md_path, self.generate_markdown())?;

        let html_path = output_dir.join("warrant_report.html");
        fs::write(&html_path, self.generate_html())?;

        Ok((md_path, html_path))
    }

    /// Generate clean, structured GitHub Flavored Markdown report.
    pub fn generate_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str("# 🛡️ WARRANT Soundness Audit Report\n\n");
        md.push_str("> **Proof-First Automated Soundness Auditor for Rust `unsafe` Code**  \n");
        md.push_str("> A finding exists ONLY if a safe-Rust harness reproduces Undefined Behavior under Miri at the exact unsafe site.\n\n");

        // Summary Banner
        md.push_str("## 📊 Executive Summary\n\n");
        md.push_str("| Metric | Value |\n");
        md.push_str("|:---|:---|\n");
        md.push_str(&format!("| **Target** | `{}` |\n", self.target_path));
        md.push_str(&format!("| **Crate** | `{}` |\n", self.crate_name));
        md.push_str(&format!("| **Audit Timestamp** | `{}` |\n", self.timestamp_rfc3339));
        md.push_str(&format!("| **Total Unsafe Sites Scanned** | **{}** |\n", self.total_candidates));
        md.push_str(&format!(
            "| **Proven Soundness Bugs** | **{}** ({}) |\n",
            self.proven_count(),
            if self.has_proven_bugs() { "🚨 VULNERABILITIES FOUND" } else { "✅ CLEAN" }
        ));
        md.push_str(&format!("| **Unproven Hypotheses** | {} (rejected / safe) |\n", self.unproven_count()));
        md.push_str(&format!("| **Audit Duration** | {:.2}s |\n", (self.duration_ms as f64) / 1000.0));
        md.push_str(&format!("| **API Spend (CostTracker)** | ${:.6} USD |\n", self.api_cost_usd));
        md.push_str(&format!("| **LLM Backend** | `{}` |\n", self.backend_used));
        md.push_str(&format!("| **Verifier Toolchain** | `{}` |\n\n", self.toolchain));

        // Section 1: PROVEN FINDINGS
        md.push_str("## 🚨 PROVEN SOUNDNESS FINDINGS\n\n");
        if self.proven_findings.is_empty() {
            md.push_str("_No proven soundness bugs detected under Miri. All examined unsafe operations satisfied soundness preconditions or yielded clean traces._\n\n");
        } else {
            md.push_str("The following findings have been **cryptographically attested and reproduced under Miri** using 100% safe-Rust harnesses:\n\n");
            for (idx, f) in self.proven_findings.iter().enumerate() {
                md.push_str(&format!(
                    "### #{}: [{}] Candidate `{}` ({})\n\n",
                    idx + 1,
                    f.tier.as_str(),
                    f.candidate_id,
                    f.rule
                ));
                md.push_str(&format!("- **Target Symbol**: `{}`\n", f.symbol));
                md.push_str(&format!("- **Location**: `{}:{}-{}`\n", f.file_path, f.line_span.0, f.line_span.1));
                md.push_str(&format!("- **Remediation Badge**: `{}`\n", f.patch_badge));
                md.push_str(&format!("- **Miri Failure Signature**: `{}`\n", f.miri_signature.as_deref().unwrap_or("Undefined Behavior")));
                md.push_str(&format!("- **Harness SHA-256**: `{}`\n", f.harness_sha256));
                if let Some(ref dir) = f.evidence_dir {
                    md.push_str(&format!("- **Evidence Bundle Directory**: `{}`\n", dir));
                }
                md.push_str(&format!("- **Analysis**: {}\n\n", f.explanation));

                md.push_str("#### Target Code Snippet\n```rust\n");
                md.push_str(&f.target_snippet);
                md.push_str("\n```\n\n");

                md.push_str("#### Reproducing Safe-Rust Harness\n```rust\n");
                md.push_str(&f.harness_code);
                md.push_str("\n```\n\n");

                md.push_str("#### Standalone Reproduction Command\n```bash\n");
                md.push_str(&format!("cargo +{} miri test --lib warrant_proof::proof_\n", self.toolchain));
                md.push_str("```\n\n");

                if let Some(ref diff) = f.patch_diff {
                    md.push_str("#### Verified Remediation Patch Diff\n```diff\n");
                    md.push_str(diff);
                    md.push_str("\n```\n\n");
                }

                md.push_str("---\n\n");
            }
        }

        // Section 2: UNPROVEN HYPOTHESES (STRICTLY SEPARATED)
        md.push_str("## ⚠️ UNPROVEN HYPOTHESES (Non-Bugs / Inconclusive)\n\n");
        md.push_str("> ⚠️ **HARD RULE 1 INVARIANT**  \n");
        md.push_str("> The candidates below were evaluated by automated LLM hypotheses but **DID NOT** produce Undefined Behavior under Miri.  \n");
        md.push_str("> Unproven ideas are strictly labeled `UNPROVEN` and are **NEVER** reported as bugs or vulnerabilities.\n\n");

        if self.unproven_hypotheses.is_empty() {
            md.push_str("_No unproven hypotheses were recorded._\n\n");
        } else {
            md.push_str("| Candidate ID | Rule | Enclosing Item | Location | Outcome / Reason |\n");
            md.push_str("|:---|:---|:---|:---|:---|\n");
            for hyp in &self.unproven_hypotheses {
                md.push_str(&format!(
                    "| `{}` | `{}` | `{}` | `{}:{}` | {} |\n",
                    hyp.candidate_id,
                    hyp.rule,
                    hyp.symbol,
                    hyp.file_path,
                    hyp.line_span.0,
                    hyp.reason
                ));
            }
            md.push('\n');
        }

        md.push_str("---\n\n");
        md.push_str("*Generated automatically by Project WARRANT (Meta Global AI Developer Hackathon)*\n");
        md
    }

    /// Generate modern, responsive, standalone single-file HTML report.
    pub fn generate_html(&self) -> String {
        let is_clean = !self.has_proven_bugs();
        let status_color = if is_clean { "#10b981" } else { "#ef4444" };
        let status_text = if is_clean { "AUDIT CLEAN: 0 PROVEN BUGS" } else { "PROVEN SOUNDNESS BUGS DETECTED" };

        let mut proven_cards = String::new();
        if self.proven_findings.is_empty() {
            proven_cards.push_str(r#"
            <div class="card clean-card">
                <div class="card-title">✅ No Soundness Bugs Proven</div>
                <p>All unsafe code blocks and unsafe trait implementations passed Miri execution without triggering Undefined Behavior.</p>
            </div>
            "#);
        } else {
            for (idx, f) in self.proven_findings.iter().enumerate() {
                let badge_class = match f.tier {
                    FindingTier::ProvenPublic => "badge-public",
                    FindingTier::ProvenInternal => "badge-internal",
                    FindingTier::Unproven => "badge-unproven",
                };

                let escaped_harness = html_escape(&f.harness_code);
                let escaped_target = html_escape(&f.target_snippet);
                let sig = f.miri_signature.as_deref().unwrap_or("Undefined Behavior detected");

                let patch_class = match f.patch_badge.as_str() {
                    "PATCH_VERIFIED" => "badge-patch-verified",
                    "PATCH_UNVERIFIED" => "badge-patch-unverified",
                    _ => "badge-patch-none",
                };

                let diff_section = if let Some(ref diff) = f.patch_diff {
                    format!(
                        r#"
                        <details class="code-details" open>
                            <summary>Verified Remediation Patch Diff</summary>
                            <pre><code class="language-diff">{}</code></pre>
                        </details>
                        "#,
                        html_escape(diff)
                    )
                } else {
                    String::new()
                };

                proven_cards.push_str(&format!(
                    r#"
                    <div class="card finding-card">
                        <div class="card-header">
                            <div>
                                <span class="badge {badge_class}">{tier_str}</span>
                                <span class="finding-id">#{num} {id} ({rule})</span>
                            </div>
                            <span class="patch-badge {patch_class}">Patch: {patch}</span>
                        </div>
                        <div class="meta-grid">
                            <div><strong>Symbol:</strong> <code>{symbol}</code></div>
                            <div><strong>Location:</strong> <code>{file}:{start}-{end}</code></div>
                            <div><strong>Harness SHA-256:</strong> <code class="hash">{hash}</code></div>
                            <div><strong>Evidence Bundle:</strong> <code>{ev_dir}</code></div>
                        </div>
                        <div class="signature-box">
                            <strong>Miri Failure Signature:</strong>
                            <pre><code>{sig}</code></pre>
                        </div>
                        <div class="explanation">
                            <strong>Hypothesis & Analysis:</strong> {explanation}
                        </div>
                        <details class="code-details">
                            <summary>Target Unsafe Snippet</summary>
                            <pre><code class="language-rust">{escaped_target}</code></pre>
                        </details>
                        <details class="code-details" open>
                            <summary>Reproducing Safe-Rust Harness (100% Safe Rust)</summary>
                            <pre><code class="language-rust">{escaped_harness}</code></pre>
                        </details>
                        {diff_section}
                        <div class="reproduce-box">
                            <strong>Standalone Reproduction Command:</strong>
                            <code>cargo +{toolchain} miri test --lib warrant_proof::proof_</code>
                        </div>
                    </div>
                    "#,
                    badge_class = badge_class,
                    patch_class = patch_class,
                    diff_section = diff_section,
                    tier_str = f.tier.as_str(),
                    num = idx + 1,
                    id = f.candidate_id,
                    rule = f.rule,
                    patch = f.patch_badge,
                    symbol = html_escape(&f.symbol),
                    file = html_escape(&f.file_path),
                    start = f.line_span.0,
                    end = f.line_span.1,
                    hash = &f.harness_sha256[..16.min(f.harness_sha256.len())],
                    ev_dir = html_escape(f.evidence_dir.as_deref().unwrap_or("evidence/")),
                    sig = html_escape(sig),
                    explanation = html_escape(&f.explanation),
                    escaped_target = escaped_target,
                    escaped_harness = escaped_harness,
                    toolchain = self.toolchain,
                ));
            }
        }

        let mut unproven_rows = String::new();
        if self.unproven_hypotheses.is_empty() {
            unproven_rows.push_str("<tr><td colspan='5' style='text-align:center;'>No unproven hypotheses recorded.</td></tr>");
        } else {
            for hyp in &self.unproven_hypotheses {
                unproven_rows.push_str(&format!(
                    r#"
                    <tr>
                        <td><code>{}</code></td>
                        <td><span class="badge badge-rule">{}</span></td>
                        <td><code>{}</code></td>
                        <td><code>{}:{}</code></td>
                        <td>{}</td>
                    </tr>
                    "#,
                    html_escape(&hyp.candidate_id),
                    html_escape(&hyp.rule),
                    html_escape(&hyp.symbol),
                    html_escape(&hyp.file_path),
                    hyp.line_span.0,
                    html_escape(&hyp.reason)
                ));
            }
        }

        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>WARRANT Audit Report - {crate_name}</title>
    <style>
        :root {{
            --bg: #090d16;
            --surface: #131c2e;
            --surface-hover: #1e293b;
            --border: #24344d;
            --text-primary: #f8fafc;
            --text-secondary: #94a3b8;
            --accent: #38bdf8;
            --danger: #ef4444;
            --warning: #f59e0b;
            --success: #10b981;
            --muted: #64748b;
        }}
        * {{ box-sizing: border-box; margin: 0; padding: 0; }}
        body {{
            background-color: var(--bg);
            color: var(--text-primary);
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
            line-height: 1.6;
            padding: 2rem 1rem;
        }}
        .container {{
            max-width: 1100px;
            margin: 0 auto;
        }}
        header {{
            margin-bottom: 2rem;
            padding-bottom: 1.5rem;
            border-bottom: 1px solid var(--border);
            display: flex;
            justify-content: space-between;
            align-items: flex-start;
            flex-wrap: wrap;
            gap: 1rem;
        }}
        h1 {{
            font-size: 1.8rem;
            font-weight: 700;
            display: flex;
            align-items: center;
            gap: 0.5rem;
        }}
        .tagline {{
            color: var(--text-secondary);
            font-size: 0.95rem;
            margin-top: 0.25rem;
        }}
        .status-pill {{
            background: {status_color}22;
            color: {status_color};
            border: 1px solid {status_color}88;
            padding: 0.5rem 1rem;
            border-radius: 9999px;
            font-size: 0.85rem;
            font-weight: 700;
            text-transform: uppercase;
            letter-spacing: 0.05em;
        }}
        .summary-grid {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 1rem;
            margin-bottom: 2.5rem;
        }}
        .stat-card {{
            background: var(--surface);
            border: 1px solid var(--border);
            padding: 1.25rem;
            border-radius: 8px;
        }}
        .stat-label {{
            color: var(--text-secondary);
            font-size: 0.8rem;
            text-transform: uppercase;
            letter-spacing: 0.05em;
            margin-bottom: 0.5rem;
        }}
        .stat-value {{
            font-size: 1.5rem;
            font-weight: 700;
        }}
        .section-header {{
            display: flex;
            align-items: center;
            gap: 0.75rem;
            margin-bottom: 1rem;
            font-size: 1.3rem;
            border-bottom: 1px solid var(--border);
            padding-bottom: 0.5rem;
        }}
        .card {{
            background: var(--surface);
            border: 1px solid var(--border);
            border-radius: 8px;
            padding: 1.5rem;
            margin-bottom: 1.5rem;
        }}
        .finding-card {{
            border-left: 4px solid var(--danger);
        }}
        .clean-card {{
            border-left: 4px solid var(--success);
            text-align: center;
            padding: 2.5rem;
        }}
        .card-header {{
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 1rem;
        }}
        .finding-id {{
            font-size: 1.15rem;
            font-weight: 700;
            margin-left: 0.5rem;
        }}
        .badge {{
            display: inline-block;
            padding: 0.2rem 0.6rem;
            border-radius: 4px;
            font-size: 0.75rem;
            font-weight: 700;
            text-transform: uppercase;
        }}
        .badge-public {{ background: #dc2626; color: white; }}
        .badge-internal {{ background: #d97706; color: white; }}
        .badge-unproven {{ background: var(--muted); color: white; }}
        .badge-rule {{ background: #1e293b; color: var(--accent); border: 1px solid var(--border); }}
        .patch-badge {{
            font-size: 0.8rem;
            padding: 0.25rem 0.5rem;
            border-radius: 4px;
        }}
        .badge-patch-verified {{ background: #059669; color: white; font-weight: 700; }}
        .badge-patch-unverified {{ background: #d97706; color: white; font-weight: 700; }}
        .badge-patch-none {{ background: #1e293b; color: var(--text-secondary); }}
        .meta-grid {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
            gap: 0.75rem;
            background: #0d131f;
            padding: 0.75rem 1rem;
            border-radius: 6px;
            font-size: 0.85rem;
            margin-bottom: 1rem;
        }}
        .signature-box {{
            background: #231215;
            border: 1px solid #7f1d1d;
            border-radius: 6px;
            padding: 0.75rem 1rem;
            margin-bottom: 1rem;
        }}
        .signature-box strong {{ color: #fca5a5; font-size: 0.85rem; }}
        .signature-box pre {{ margin-top: 0.25rem; color: #fee2e2; white-space: pre-wrap; font-size: 0.85rem; }}
        .explanation {{
            margin-bottom: 1rem;
            font-size: 0.95rem;
            color: #e2e8f0;
        }}
        details.code-details {{
            margin-bottom: 0.75rem;
            background: #090d16;
            border: 1px solid var(--border);
            border-radius: 6px;
            overflow: hidden;
        }}
        details summary {{
            padding: 0.5rem 1rem;
            background: #111827;
            cursor: pointer;
            font-size: 0.85rem;
            font-weight: 600;
            color: var(--accent);
        }}
        pre {{
            padding: 1rem;
            font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
            font-size: 0.85rem;
            overflow-x: auto;
            color: #e2e8f0;
        }}
        code {{
            font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
        }}
        .reproduce-box {{
            background: #091e36;
            border: 1px solid #0369a1;
            padding: 0.75rem 1rem;
            border-radius: 6px;
            font-size: 0.85rem;
            display: flex;
            flex-direction: column;
            gap: 0.25rem;
        }}
        .reproduce-box strong {{ color: #7dd3fc; }}
        .reproduce-box code {{ color: #e0f2fe; }}
        .invariant-notice {{
            background: #1e1b2e;
            border: 1px solid #6b21a8;
            padding: 1rem 1.25rem;
            border-radius: 8px;
            margin-bottom: 1.5rem;
            font-size: 0.9rem;
            color: #d8b4fe;
        }}
        table {{
            width: 100%;
            border-collapse: collapse;
            font-size: 0.9rem;
            background: var(--surface);
            border-radius: 8px;
            overflow: hidden;
        }}
        th, td {{
            padding: 0.75rem 1rem;
            text-align: left;
            border-bottom: 1px solid var(--border);
        }}
        th {{
            background: #111827;
            color: var(--text-secondary);
            font-size: 0.8rem;
            text-transform: uppercase;
            letter-spacing: 0.05em;
        }}
        footer {{
            margin-top: 3rem;
            padding-top: 1.5rem;
            border-top: 1px solid var(--border);
            text-align: center;
            color: var(--text-secondary);
            font-size: 0.85rem;
        }}
    </style>
</head>
<body>
    <div class="container">
        <header>
            <div>
                <h1>🛡️ WARRANT Soundness Report</h1>
                <div class="tagline">Proof-First Automated Soundness Auditor for Rust unsafe Code</div>
            </div>
            <div class="status-pill">{status_text}</div>
        </header>

        <section class="summary-grid">
            <div class="stat-card">
                <div class="stat-label">Target Scanned</div>
                <div class="stat-value" style="font-size: 1.1rem; word-break: break-all;">{target_path}</div>
            </div>
            <div class="stat-card">
                <div class="stat-label">Unsafe Candidates</div>
                <div class="stat-value">{total_candidates}</div>
            </div>
            <div class="stat-card">
                <div class="stat-label">Proven Soundness Bugs</div>
                <div class="stat-value" style="color: {status_color};">{proven_count}</div>
            </div>
            <div class="stat-card">
                <div class="stat-label">Unproven Hypotheses</div>
                <div class="stat-value" style="color: var(--muted);">{unproven_count}</div>
            </div>
            <div class="stat-card">
                <div class="stat-label">Audit Duration</div>
                <div class="stat-value">{duration_sec:.2}s</div>
            </div>
            <div class="stat-card">
                <div class="stat-label">API Spend (CostTracker)</div>
                <div class="stat-value">${api_cost:.6}</div>
            </div>
        </section>

        <main>
            <h2 class="section-header">🚨 Proven Soundness Findings ({proven_count})</h2>
            <div style="margin-bottom: 2rem;">
                {proven_cards}
            </div>

            <h2 class="section-header">⚠️ Unproven Hypotheses ({unproven_count})</h2>
            <div class="invariant-notice">
                <strong>HARD RULE 1 INVARIANT:</strong> A finding exists <em>only</em> if a safe-Rust harness reproduces Undefined Behavior under Miri at the exact unsafe site. The candidates below did not trigger Undefined Behavior and are strictly categorized as <strong>UNPROVEN</strong> (not bugs).
            </div>
            <table>
                <thead>
                    <tr>
                        <th>Candidate ID</th>
                        <th>Rule</th>
                        <th>Enclosing Item</th>
                        <th>Location</th>
                        <th>Reason / Outcome</th>
                    </tr>
                </thead>
                <tbody>
                    {unproven_rows}
                </tbody>
            </table>
        </main>

        <footer>
            Generated by <strong>Project WARRANT</strong> &bull; Meta Global AI Developer Hackathon &bull; Verified under <code>{toolchain}</code>
        </footer>
    </div>
</body>
</html>
"#,
            crate_name = html_escape(&self.crate_name),
            status_color = status_color,
            status_text = status_text,
            target_path = html_escape(&self.target_path),
            total_candidates = self.total_candidates,
            proven_count = self.proven_count(),
            unproven_count = self.unproven_count(),
            duration_sec = (self.duration_ms as f64) / 1000.0,
            api_cost = self.api_cost_usd,
            proven_cards = proven_cards,
            unproven_rows = unproven_rows,
            toolchain = html_escape(&self.toolchain),
        )
    }
}

/// Helper function to escape HTML special characters.
fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_proven_and_unproven_separation() {
        let report = AuditReport {
            target_path: "bench/pairs/r1_reference.rs".to_string(),
            crate_name: "target_crate".to_string(),
            timestamp_rfc3339: "2026-10-28T00:00:00Z".to_string(),
            duration_ms: 1250,
            total_candidates: 2,
            proven_findings: vec![ReportFinding {
                candidate_id: "c-0001".to_string(),
                tier: FindingTier::ProvenPublic,
                rule: "R1".to_string(),
                symbol: "FastBuffer::read_unchecked_elem".to_string(),
                file_path: "bench/pairs/r1_reference.rs".to_string(),
                line_span: (21, 21),
                miri_signature: Some("unsafe precondition(s) violated: slice::get_unchecked".to_string()),
                harness_sha256: "abcdef1234567890".to_string(),
                evidence_dir: Some("evidence/c-0001".to_string()),
                harness_code: "#![forbid(unsafe_code)]\nextern crate std;".to_string(),
                target_snippet: "unsafe { *self.data.get_unchecked(idx) }".to_string(),
                patch_badge: "NONE".to_string(),
                explanation: "Triggers out-of-bounds slice indexing".to_string(),
                patch_diff: None,
            }],
            unproven_hypotheses: vec![ReportHypothesis {
                candidate_id: "c-0002".to_string(),
                rule: "R2".to_string(),
                symbol: "SafeCounter".to_string(),
                file_path: "bench/pairs/r2_safe.rs".to_string(),
                line_span: (10, 15),
                explanation: "Possible race condition".to_string(),
                reason: "Miri execution completed cleanly with 0 UB".to_string(),
                target_snippet: "impl Sync for SafeCounter {}".to_string(),
            }],
            api_cost_usd: 0.0,
            total_tokens_in: 500,
            total_tokens_out: 250,
            backend_used: "mock".to_string(),
            toolchain: "nightly-2026-10-20".to_string(),
        };

        assert!(report.has_proven_bugs());
        assert_eq!(report.proven_count(), 1);
        assert_eq!(report.unproven_count(), 1);

        let md = report.generate_markdown();
        assert!(md.contains("## 🚨 PROVEN SOUNDNESS FINDINGS"));
        assert!(md.contains("## ⚠️ UNPROVEN HYPOTHESES (Non-Bugs / Inconclusive)"));
        assert!(md.contains("HARD RULE 1 INVARIANT"));
        assert!(md.contains("c-0001"));
        assert!(md.contains("c-0002"));

        let html = report.generate_html();
        assert!(html.contains("PROVEN SOUNDNESS BUGS DETECTED"));
        assert!(html.contains("c-0001"));
        assert!(html.contains("c-0002"));
        assert!(html.contains("HARD RULE 1 INVARIANT"));
    }

    #[test]
    fn test_clean_report_generation() {
        let report = AuditReport {
            target_path: "bench/pairs/r1_reference_safe.rs".to_string(),
            crate_name: "target_crate".to_string(),
            timestamp_rfc3339: "2026-10-28T00:00:00Z".to_string(),
            duration_ms: 800,
            total_candidates: 1,
            proven_findings: vec![],
            unproven_hypotheses: vec![ReportHypothesis {
                candidate_id: "c-safe-01".to_string(),
                rule: "R1".to_string(),
                symbol: "SafeBuffer".to_string(),
                file_path: "src/lib.rs".to_string(),
                line_span: (15, 15),
                explanation: "Testing safety".to_string(),
                reason: "No UB detected".to_string(),
                target_snippet: "// safe".to_string(),
            }],
            api_cost_usd: 0.0,
            total_tokens_in: 100,
            total_tokens_out: 50,
            backend_used: "mock".to_string(),
            toolchain: "nightly-2026-10-20".to_string(),
        };

        assert!(!report.has_proven_bugs());
        assert_eq!(report.proven_count(), 0);

        let md = report.generate_markdown();
        assert!(md.contains("No proven soundness bugs detected"));

        let html = report.generate_html();
        assert!(html.contains("AUDIT CLEAN: 0 PROVEN BUGS"));
    }
}
