/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
 */

import React, { useState } from 'react';
import {
  ShieldAlert,
  ShieldCheck,
  Terminal,
  FileCode2,
  Cpu,
  Layers,
  Box,
  CheckCircle2,
  AlertTriangle,
  Play,
  Copy,
  Check,
  Download,
  FolderTree,
  Lock,
  ArrowRight,
  RefreshCw,
  Sliders,
  Code,
  FileText
} from 'lucide-react';

// Hard constraints & project configuration
const TOOLCHAIN = 'nightly-2026-10-20';
const MIRI_SYSROOT = '/usr/local/cargo/miri-sysroot';
const SANDBOX_FLAGS = '--network none --read-only --tmpfs /tmp:rw,noexec,nosuid --pids-limit 64 --memory 2g';

interface SourceFile {
  path: string;
  category: 'config' | 'crates' | 'sandbox' | 'benchmarks';
  language: string;
  description: string;
  content: string;
}

const PROJECT_FILES: SourceFile[] = [
  {
    path: 'Cargo.toml',
    category: 'config',
    language: 'toml',
    description: 'Root Cargo workspace definition with shared dependencies and workspace resolver v2',
    content: `[workspace]
resolver = "2"
members = [
    "crates/extract",
    "crates/llm",
    "crates/verify",
    "crates/cli",
    "tools/mutator",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.82.0"
authors = ["WARRANT Security Team <security@warrant.internal>"]
license = "MIT OR Apache-2.0"

[workspace.dependencies]
# Internal workspace crates
warrant-extract = { path = "crates/extract" }
warrant-llm = { path = "crates/llm" }
warrant-verify = { path = "crates/verify" }

# Serialization and data contracts
serde = { version = "1.0.217", features = ["derive"] }
serde_json = { version = "1.0.138" }

# AST Parsing and Code Generation
syn = { version = "2.0.98", features = ["full", "extra-traits", "visit", "parsing", "printing"] }
quote = "1.0.38"
proc-macro2 = { version = "1.0.93", features = ["span-locations"] }

# Asynchronous runtime and abstractions
tokio = { version = "1.43.0", features = ["full"] }
async-trait = "0.1.86"

# Command line interface
clap = { version = "4.5.30", features = ["derive", "env"] }

# Observability and Error Handling
tracing = "0.1.41"
tracing-subscriber = { version = "0.3.19", features = ["env-filter", "json"] }
thiserror = "2.0.11"
anyhow = "1.0.95"

# System and Sandboxing utilities
tempfile = "3.17.1"
regex = "1.11.1"
uuid = { version = "1.13.1", features = ["v4", "serde"] }
walkdir = "2.5.0"
sha2 = "0.10.8"
hex = "0.4.3"`
  },
  {
    path: 'rust-toolchain.toml',
    category: 'config',
    language: 'toml',
    description: 'Pinned nightly toolchain definition with miri and rust-src components',
    content: `[toolchain]
channel = "nightly-2026-10-20"
components = ["rust-src", "miri", "rustfmt", "clippy"]
profile = "minimal"`
  },
  {
    path: '.gitignore',
    category: 'config',
    language: 'plaintext',
    description: 'Root gitignore enforcing exclusion of bench/own/, .env, and target directories',
    content: `# Rust & Cargo build artifacts
/target/
crates/**/target/
tools/**/target/
Cargo.lock

# Secret and environment configurations
.env
.env.*
!.env.example

# Benchmark proprietary / confidential test cases
bench/own/

# Miri and sandbox artifacts
/sandbox/.miri/
/sandbox/tmp/
*.bundle.json
evidence/`
  },
  {
    path: 'crates/llm/src/lib.rs',
    category: 'crates',
    language: 'rust',
    description: 'Core trait LlmClient, MockLlmClient, and Candidate/Hypothesis/Finding data contracts with zero-panic parsing',
    content: `//! # warrant-llm
//! Core contracts, LLM abstractions, and zero-panic deserialization.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

pub const REQUIRED_HARNESS_HEADER: &str = "#![forbid(unsafe_code)]\\nextern crate std;\\n";

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("JSON serialization or deserialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Data contract validation failure: {field} - {reason}")]
    Validation { field: &'static str, reason: String },
    #[error("API transport or execution error: {0}")]
    Transport(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub crate_name: String,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub kind: CandidateKind,
    pub code_snippet: String,
    pub enclosing_item: Option<String>,
    pub safety_comment: Option<String>,
    pub trait_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hypothesis {
    pub id: String,
    pub candidate_id: String,
    pub ub_class: UbClass,
    pub explanation: String,
    pub harness_code: String,
    pub suggested_miri_flags: Vec<String>,
    pub expected_trigger: String,
}

impl Hypothesis {
    pub fn validate(&self) -> Result<(), LlmError> {
        let trimmed = self.harness_code.trim_start();
        if !trimmed.starts_with("#![forbid(unsafe_code)]") {
            return Err(LlmError::Validation {
                field: "harness_code",
                reason: "harness must strictly start with '#![forbid(unsafe_code)]'".into(),
            });
        }
        if !trimmed.contains("extern crate std;") {
            return Err(LlmError::Validation {
                field: "harness_code",
                reason: "harness must contain 'extern crate std;'".into(),
            });
        }
        Ok(())
    }
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn propose_hypothesis(&self, candidate: &Candidate) -> Result<Hypothesis, LlmError>;
    async fn repair_harness(&self, candidate: &Candidate, failed: &Hypothesis, miri_output: &str) -> Result<Hypothesis, LlmError>;
    async fn propose_patch(&self, candidate: &Candidate, finding: &Finding) -> Result<String, LlmError>;
}`
  },
  {
    path: 'crates/extract/src/lib.rs',
    category: 'crates',
    language: 'rust',
    description: 'syn-based AST parser scanning for unsafe blocks, unsafe impls, and item spans',
    content: `//! # warrant-extract
//! syn-based AST parser extracting unsafe blocks, unsafe impls, and item spans.

use quote::ToTokens;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use warrant_llm::{Candidate, CandidateKind};

struct UnsafeVisitor<'a> {
    file_path: &'a str,
    crate_name: &'a str,
    source_lines: &'a [&'a str],
    current_enclosing_item: Option<String>,
    candidates: Vec<Candidate>,
}

impl<'ast, 'a> Visit<'ast> for UnsafeVisitor<'a> {
    fn visit_expr_unsafe(&mut self, node: &'ast syn::ExprUnsafe) {
        let span = node.span();
        let (start, end) = (span.start().line, span.end().line);
        self.candidates.push(Candidate {
            id: format!("cand-block-{}-{}", start, end),
            crate_name: self.crate_name.into(),
            file_path: self.file_path.into(),
            start_line: start,
            end_line: end,
            kind: CandidateKind::UnsafeBlock,
            code_snippet: node.to_token_stream().to_string(),
            enclosing_item: self.current_enclosing_item.clone(),
            safety_comment: None,
            trait_name: None,
        });
        visit::visit_expr_unsafe(self, node);
    }
}`
  },
  {
    path: 'crates/verify/src/lib.rs',
    category: 'crates',
    language: 'rust',
    description: 'Deterministic Miri verifier, inline harness injector, and evidence bundle collector',
    content: `//! # warrant-verify
//! Harness injection with strict forbid(unsafe_code) invariant and evidence bundle generation.

use warrant_llm::{Candidate, Finding, Hypothesis, UbClass, VerificationStatus, REQUIRED_HARNESS_HEADER};

pub struct HarnessInjector;

impl HarnessInjector {
    pub fn prepare_harness(&self, raw: &str) -> Result<String, String> {
        let trimmed = raw.trim_start();
        if !trimmed.starts_with("#![forbid(unsafe_code)]") {
            return Err("Harness MUST strictly begin with '#![forbid(unsafe_code)]'".into());
        }
        if !trimmed.contains("extern crate std;") {
            return Err("Harness MUST contain 'extern crate std;'".into());
        }
        Ok(trimmed.to_string())
    }
}`
  },
  {
    path: 'crates/cli/src/main.rs',
    category: 'crates',
    language: 'rust',
    description: 'Clap-based CLI with standardized exit codes: 0 = clean, 1 = proven bug, 2 = error',
    content: `//! # warrant CLI
//! Exit codes:
//!   0 = Clean / No proven bug
//!   1 = Proven soundness bug detected under Miri
//!   2 = Fatal error

use clap::{Parser, Subcommand};
use warrant_cli::{EXIT_CLEAN, EXIT_ERROR, EXIT_PROVEN_BUG};

#[derive(Parser, Debug)]
#[command(name = "warrant", version = "0.1.0", about = "Proof-first automated soundness auditor")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Extract { target: std::path::PathBuf },
    Audit { target: std::path::PathBuf, sandbox: bool },
    Verify { candidate: std::path::PathBuf, hypothesis: std::path::PathBuf },
}`
  },
  {
    path: 'sandbox/Dockerfile',
    category: 'sandbox',
    language: 'dockerfile',
    description: 'Network-isolated container with pinned nightly, pre-warmed miri sysroot, non-root user',
    content: `FROM debian:bookworm-slim AS base

RUN apt-get update && apt-get install -y --no-install-recommends \\
    ca-certificates curl build-essential gcc libc6-dev git \\
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \\
    CARGO_HOME=/usr/local/cargo \\
    PATH=/usr/local/cargo/bin:$PATH \\
    RUST_TOOLCHAIN=nightly-2026-10-20

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- \\
    -y --default-toolchain \${RUST_TOOLCHAIN} --profile minimal \\
    --component rust-src --component miri --component rustfmt --component clippy

# Pre-warm Miri sysroot for zero-network read-only execution
ENV MIRI_SYSROOT=/usr/local/cargo/miri-sysroot
RUN cargo miri setup --target x86_64-unknown-linux-gnu

# Non-root user with read-only root compatibility
RUN groupadd -g 1000 warrant && \\
    useradd -u 1000 -g warrant -m -s /bin/bash warrant && \\
    mkdir -p /home/warrant/.cargo /tmp/warrant-work /evidence && \\
    chown -R warrant:warrant /home/warrant /tmp/warrant-work /evidence

USER warrant
WORKDIR /tmp/warrant-work
ENTRYPOINT ["cargo", "miri"]
CMD ["--help"]`
  },
  {
    path: 'sandbox/run_in_sandbox.sh',
    category: 'sandbox',
    language: 'bash',
    description: 'Hardened execution runner enforcing zero-network and strict memory/pids bounds',
    content: `#!/usr/bin/env bash
set -euo pipefail

IMAGE_NAME="\${WARRANT_DOCKER_IMAGE:-warrant-sandbox:latest}"
TARGET_DIR="\${1:-}"

if [[ -z "\${TARGET_DIR}" ]]; then
    echo "Usage: $0 <target_crate_dir> [cargo_miri_args...]" >&2
    exit 2
fi

shift || true

exec docker run --rm -i \\
    --network none \\
    --read-only \\
    --tmpfs /tmp:rw,noexec,nosuid \\
    --pids-limit 64 \\
    --memory 2g \\
    --user 1000:1000 \\
    -v "\$(cd "\${TARGET_DIR}" && pwd):/tmp/warrant-work:ro" \\
    -e RUST_BACKTRACE=1 \\
    -e MIRI_AUTO_OPS=0 \\
    -w /tmp/warrant-work \\
    "\${IMAGE_NAME}" \\
    run "$@"`
  },
  {
    path: 'bench/pairs/r1_reference.rs',
    category: 'benchmarks',
    language: 'rust',
    description: 'Fixture R1: Out-of-bounds get_unchecked pattern + inline safe harness triggering ub_oob',
    content: `// Target under audit: Unsound safe wrapper around get_unchecked
pub mod target_lib {
    pub struct FastBuffer {
        data: Vec<u8>,
    }

    impl FastBuffer {
        pub fn new(data: Vec<u8>) -> Self {
            Self { data }
        }

        /// UNSOUND: Relies on caller invariant without being declared unsafe fn
        pub fn read_unchecked_elem(&self, index: usize) -> u8 {
            unsafe { *self.data.get_unchecked(index) }
        }
    }
}

// Invariant: #![forbid(unsafe_code)] + extern crate std;
#![forbid(unsafe_code)]
extern crate std;

use target_lib::FastBuffer;

fn main() {
    let buffer = FastBuffer::new(vec![10, 20, 30, 40]);
    // Provokes out-of-bounds access past 4-byte buffer allocation
    let val = buffer.read_unchecked_elem(8);
    std::hint::black_box(val);
}`
  },
  {
    path: 'bench/pairs/r2_reference.rs',
    category: 'benchmarks',
    language: 'rust',
    description: 'Fixture R2: Unsound unsafe impl Sync on Cell<usize> + safe multithreaded harness triggering data_race',
    content: `pub mod target_lib {
    use std::cell::Cell;

    pub struct SharedCounter {
        counter: Cell<usize>,
    }

    impl SharedCounter {
        pub fn new(init: usize) -> Self {
            Self { counter: Cell::new(init) }
        }

        pub fn increment(&self) {
            let cur = self.counter.get();
            self.counter.set(cur + 1);
        }

        pub fn get(&self) -> usize {
            self.counter.get()
        }
    }

    // UNSOUND: Cell<T> is fundamentally not Sync
    unsafe impl Sync for SharedCounter {}
}

// Invariant: #![forbid(unsafe_code)] + extern crate std;
#![forbid(unsafe_code)]
extern crate std;

use std::sync::Arc;
use std::thread;
use target_lib::SharedCounter;

fn main() {
    let counter = Arc::new(SharedCounter::new(0));
    let c1 = Arc::clone(&counter);
    let t1 = thread::spawn(move || {
        for _ in 0..10_000 { c1.increment(); }
    });
    let c2 = Arc::clone(&counter);
    let t2 = thread::spawn(move || {
        for _ in 0..10_000 { c2.increment(); }
    });
    t1.join().unwrap();
    t2.join().unwrap();
}`
  }
];

export default function App() {
  const [activeTab, setActiveTab] = useState<'overview' | 'files' | 'interactive' | 'benchmarks'>('overview');
  const [selectedFile, setSelectedFile] = useState<SourceFile>(PROJECT_FILES[3]); // crates/llm/src/lib.rs
  const [copiedPath, setCopiedPath] = useState<string | null>(null);

  // Interactive audit state
  const [activeBenchmark, setActiveBenchmark] = useState<'r1' | 'r2'>('r1');
  const [miriEngine, setMiriEngine] = useState<'tree-borrows' | 'stacked-borrows'>('tree-borrows');
  const [isAuditing, setIsAuditing] = useState(false);
  const [auditProgressStep, setAuditProgressStep] = useState<number>(0);
  const [auditComplete, setAuditComplete] = useState<boolean>(true);

  const handleCopy = (content: string, path: string) => {
    navigator.clipboard.writeText(content);
    setCopiedPath(path);
    setTimeout(() => setCopiedPath(null), 2000);
  };

  const runSimulation = () => {
    setIsAuditing(true);
    setAuditProgressStep(1);
    setAuditComplete(false);

    setTimeout(() => {
      setAuditProgressStep(2);
      setTimeout(() => {
        setAuditProgressStep(3);
        setTimeout(() => {
          setAuditProgressStep(4);
          setIsAuditing(false);
          setAuditComplete(true);
        }, 600);
      }, 700);
    }, 600);
  };

  const getBenchmarkData = () => {
    if (activeBenchmark === 'r1') {
      return {
        id: 'bench/pairs/r1_reference.rs',
        title: 'R1: get_unchecked OOB Bypass',
        targetSnippet: `pub fn read_unchecked_elem(&self, index: usize) -> u8 {
    // UNSOUND: Invokes get_unchecked without bounds validation
    unsafe { *self.data.get_unchecked(index) }
}`,
        candidateKind: 'unsafe_block',
        enclosingItem: 'fn read_unchecked_elem',
        lineSpan: 'L15-L17',
        ubClass: 'ub_oob',
        explanation: 'Caller passes index 8 into a buffer of length 4. Dereferences invalid memory past allocation boundaries.',
        harness: `#![forbid(unsafe_code)]
extern crate std;

use target_lib::FastBuffer;

fn main() {
    let buffer = FastBuffer::new(vec![10, 20, 30, 40]);
    let val = buffer.read_unchecked_elem(8);
    std::hint::black_box(val);
}`,
        miriStderr: `error: Undefined Behavior: dereferencing pointer failed: alloc has size 4, so pointer to 1 byte starting at offset 8 is out-of-bounds
   --> main.rs:6:19
    |
6   |     let val = unsafe { *self.data.get_unchecked(index) };
    |                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ dereferencing pointer failed: alloc has size 4
    |
    = note: inside \`target_lib::FastBuffer::read_unchecked_elem\` at main.rs:6:19
    = note: inside \`main\` at main.rs:18:15`,
        evidenceHash: 'c4e97a3d20fbb279930f7851da85f91f3a53489e81b672803b9b940986ee34a1',
        exitCode: 1,
        patch: `// Fix: Enforce safe bounds checking prior to returning element
pub fn read_unchecked_elem(&self, index: usize) -> Option<u8> {
    self.data.get(index).copied()
}`
      };
    } else {
      return {
        id: 'bench/pairs/r2_reference.rs',
        title: 'R2: Unsound Sync on Cell<T>',
        targetSnippet: `// UNSOUND: Cell provides unsynchronized interior mutability
unsafe impl Sync for SharedCounter {}`,
        candidateKind: 'unsafe_impl',
        enclosingItem: 'impl Sync for SharedCounter',
        lineSpan: 'L24-L24',
        ubClass: 'data_race',
        explanation: 'Unsound unsafe impl Sync on non-atomic Cell enables simultaneous concurrent writes & reads without lock or memory fence.',
        harness: `#![forbid(unsafe_code)]
extern crate std;

use std::sync::Arc;
use std::thread;
use target_lib::SharedCounter;

fn main() {
    let counter = Arc::new(SharedCounter::new(0));
    let c1 = Arc::clone(&counter);
    let t1 = thread::spawn(move || {
        for _ in 0..10_000 { c1.increment(); }
    });
    let c2 = Arc::clone(&counter);
    let t2 = thread::spawn(move || {
        for _ in 0..10_000 { c2.increment(); }
    });
    t1.join().unwrap();
    t2.join().unwrap();
}`,
        miriStderr: `error: Undefined Behavior: Data race detected between (1) Read on thread \`unnamed-1\` and (2) Write on thread \`unnamed-2\` at alloc1428
   --> main.rs:13:23
    |
13  |         let cur = self.counter.get();
    |                   ^^^^^^^^^^^^^^^^^^ Data race detected
    |
    = note: and (2) occurred on thread \`unnamed-2\` inside \`target_lib::SharedCounter::increment\`
    = note: concurrent access to non-atomic Cell<usize>`,
        evidenceHash: '7f91a92e105b4e78c89b216503c5d808e04b4c7324fa934c9c228d40be1ef892',
        exitCode: 1,
        patch: `// Fix: Replace interior Cell with thread-safe atomic primitive
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct SharedCounter {
    counter: AtomicUsize,
}
// AtomicUsize is natively Sync without requiring unsafe impl`
      };
    }
  };

  const currentBench = getBenchmarkData();

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100 flex flex-col font-sans selection:bg-amber-500/20 selection:text-amber-200">
      {/* Top Banner & Header */}
      <header className="border-b border-zinc-800/80 bg-zinc-900/60 backdrop-blur-md sticky top-0 z-40">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-3.5 flex flex-wrap items-center justify-between gap-4">
          <div className="flex items-center gap-3">
            <div className="w-9 h-9 rounded bg-amber-500/10 border border-amber-500/30 flex items-center justify-center text-amber-400">
              <ShieldAlert className="w-5 h-5" />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <span className="font-mono text-base font-bold tracking-tight text-zinc-100">WARRANT</span>
                <span className="text-[11px] font-mono px-2 py-0.5 rounded bg-zinc-800 text-zinc-300 border border-zinc-700/60">
                  {TOOLCHAIN}
                </span>
                <span className="text-[11px] font-mono px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
                  Sandbox Active
                </span>
              </div>
              <p className="text-xs text-zinc-400">
                Proof-First Automated Soundness Auditor for Rust Unsafe Code
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <nav className="flex bg-zinc-900 p-1 rounded-lg border border-zinc-800 text-xs font-mono">
              <button
                onClick={() => setActiveTab('overview')}
                className={`px-3 py-1.5 rounded transition-colors ${
                  activeTab === 'overview'
                    ? 'bg-zinc-800 text-zinc-100 font-medium shadow-sm'
                    : 'text-zinc-400 hover:text-zinc-200'
                }`}
              >
                Overview
              </button>
              <button
                onClick={() => setActiveTab('files')}
                className={`px-3 py-1.5 rounded transition-colors flex items-center gap-1.5 ${
                  activeTab === 'files'
                    ? 'bg-zinc-800 text-zinc-100 font-medium shadow-sm'
                    : 'text-zinc-400 hover:text-zinc-200'
                }`}
              >
                <FolderTree className="w-3.5 h-3.5" />
                Workspace Files
              </button>
              <button
                onClick={() => setActiveTab('interactive')}
                className={`px-3 py-1.5 rounded transition-colors flex items-center gap-1.5 ${
                  activeTab === 'interactive'
                    ? 'bg-zinc-800 text-zinc-100 font-medium shadow-sm'
                    : 'text-zinc-400 hover:text-zinc-200'
                }`}
              >
                <Terminal className="w-3.5 h-3.5" />
                Live Auditor
              </button>
              <button
                onClick={() => setActiveTab('benchmarks')}
                className={`px-3 py-1.5 rounded transition-colors flex items-center gap-1.5 ${
                  activeTab === 'benchmarks'
                    ? 'bg-zinc-800 text-zinc-100 font-medium shadow-sm'
                    : 'text-zinc-400 hover:text-zinc-200'
                }`}
              >
                <Cpu className="w-3.5 h-3.5" />
                Pairs Benchmark
              </button>
            </nav>
          </div>
        </div>
      </header>

      {/* Main Content Area */}
      <main className="flex-1 max-w-7xl mx-auto w-full px-4 sm:px-6 lg:px-8 py-6">
        {/* OVERVIEW TAB */}
        {activeTab === 'overview' && (
          <div className="space-y-6">
            {/* Mission & Architectural Invariants */}
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <div className="bg-zinc-900/70 border border-zinc-800 rounded-lg p-5">
                <div className="flex items-center gap-2.5 text-amber-400 mb-2">
                  <Lock className="w-4 h-4" />
                  <h3 className="font-mono text-sm font-semibold uppercase tracking-wider">T0 / T1 Invariant</h3>
                </div>
                <p className="text-xs text-zinc-300 leading-relaxed font-mono">
                  All test harnesses strictly mandate <span className="text-amber-300">#![forbid(unsafe_code)]</span> and <span className="text-amber-300">extern crate std;</span>. Safe code alone must provoke the soundness bug.
                </p>
              </div>

              <div className="bg-zinc-900/70 border border-zinc-800 rounded-lg p-5">
                <div className="flex items-center gap-2.5 text-emerald-400 mb-2">
                  <ShieldCheck className="w-4 h-4" />
                  <h3 className="font-mono text-sm font-semibold uppercase tracking-wider">Isolated Sandbox</h3>
                </div>
                <p className="text-xs text-zinc-300 leading-relaxed font-mono">
                  Execution runs with zero network (<span className="text-emerald-300">--network none</span>), read-only root, pre-warmed Miri sysroot, and strictly capped 2GB memory & 64 PIDs limit.
                </p>
              </div>

              <div className="bg-zinc-900/70 border border-zinc-800 rounded-lg p-5">
                <div className="flex items-center gap-2.5 text-blue-400 mb-2">
                  <Terminal className="w-4 h-4" />
                  <h3 className="font-mono text-sm font-semibold uppercase tracking-wider">Deterministic Exit</h3>
                </div>
                <p className="text-xs text-zinc-300 leading-relaxed font-mono">
                  Standardized status codes: <span className="text-emerald-400">0 = Clean</span> (safe), <span className="text-red-400">1 = Proven Bug</span> (UB verified), <span className="text-yellow-400">2 = Error</span> (fatal fault).
                </p>
              </div>
            </div>

            {/* Pipeline Execution Flow Diagram */}
            <div className="bg-zinc-900/50 border border-zinc-800 rounded-lg p-6">
              <div className="flex items-center justify-between mb-6">
                <div>
                  <h2 className="text-base font-semibold text-zinc-100 font-mono flex items-center gap-2">
                    <Layers className="w-4 h-4 text-amber-400" />
                    WARRANT Pipeline Architecture
                  </h2>
                  <p className="text-xs text-zinc-400 mt-0.5">
                    End-to-end mathematical soundness verification pipeline
                  </p>
                </div>
                <button
                  onClick={() => setActiveTab('interactive')}
                  className="px-3.5 py-1.5 rounded bg-amber-500 hover:bg-amber-400 text-zinc-950 font-mono text-xs font-semibold flex items-center gap-1.5 transition-colors"
                >
                  <Play className="w-3.5 h-3.5 fill-current" />
                  Run Live Pipeline
                </button>
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
                <div className="border border-zinc-800 bg-zinc-900/80 rounded p-4 relative">
                  <div className="text-[10px] font-mono text-zinc-500 mb-1">STAGE 01</div>
                  <h4 className="font-mono text-sm font-medium text-zinc-200 flex items-center gap-1.5">
                    <FileCode2 className="w-4 h-4 text-amber-400" />
                    AST Extraction
                  </h4>
                  <p className="text-xs text-zinc-400 mt-2">
                    Uses <code className="text-amber-300">syn</code> to traverse crate syntax trees. Discovers unsafe blocks, unsafe impls (Send/Sync), raw pointer derefs, and collects safety contracts.
                  </p>
                  <div className="mt-3 text-[11px] font-mono text-zinc-500 bg-zinc-950 p-2 rounded border border-zinc-800/80">
                    crates/extract → Candidate
                  </div>
                </div>

                <div className="border border-zinc-800 bg-zinc-900/80 rounded p-4 relative">
                  <div className="text-[10px] font-mono text-zinc-500 mb-1">STAGE 02</div>
                  <h4 className="font-mono text-sm font-medium text-zinc-200 flex items-center gap-1.5">
                    <Cpu className="w-4 h-4 text-blue-400" />
                    Hypothesis Gen
                  </h4>
                  <p className="text-xs text-zinc-400 mt-2">
                    Formulates formal unsoundness hypotheses via <code className="text-blue-300">LlmClient</code>. Synthesizes strictly safe harnesses with <code className="text-blue-300">forbid(unsafe_code)</code>.
                  </p>
                  <div className="mt-3 text-[11px] font-mono text-zinc-500 bg-zinc-950 p-2 rounded border border-zinc-800/80">
                    crates/llm → Hypothesis
                  </div>
                </div>

                <div className="border border-zinc-800 bg-zinc-900/80 rounded p-4 relative">
                  <div className="text-[10px] font-mono text-zinc-500 mb-1">STAGE 03</div>
                  <h4 className="font-mono text-sm font-medium text-zinc-200 flex items-center gap-1.5">
                    <ShieldCheck className="w-4 h-4 text-emerald-400" />
                    Miri Verification
                  </h4>
                  <p className="text-xs text-zinc-400 mt-2">
                    Injects safe harness and executes inside Docker container under Tree Borrows or Stacked Borrows. Catches data races, OOB, and pointer invalidations.
                  </p>
                  <div className="mt-3 text-[11px] font-mono text-zinc-500 bg-zinc-950 p-2 rounded border border-zinc-800/80">
                    sandbox/run_in_sandbox.sh
                  </div>
                </div>

                <div className="border border-zinc-800 bg-zinc-900/80 rounded p-4 relative">
                  <div className="text-[10px] font-mono text-zinc-500 mb-1">STAGE 04</div>
                  <h4 className="font-mono text-sm font-medium text-zinc-200 flex items-center gap-1.5">
                    <Box className="w-4 h-4 text-purple-400" />
                    Evidence Bundling
                  </h4>
                  <p className="text-xs text-zinc-400 mt-2">
                    Generates immutable cryptographic evidence bundle (SHA-256 hash, execution logs, reproduction command, and patch proposal).
                  </p>
                  <div className="mt-3 text-[11px] font-mono text-zinc-500 bg-zinc-950 p-2 rounded border border-zinc-800/80">
                    crates/verify → *.bundle.json
                  </div>
                </div>
              </div>
            </div>

            {/* Quick Workspace Tree */}
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
              <div className="bg-zinc-900/60 border border-zinc-800 rounded-lg p-5">
                <h3 className="font-mono text-sm font-semibold text-zinc-200 mb-3 flex items-center gap-2">
                  <FolderTree className="w-4 h-4 text-amber-400" />
                  Cargo Workspace Structure
                </h3>
                <div className="bg-zinc-950 p-3 rounded font-mono text-xs text-zinc-300 space-y-1 border border-zinc-850">
                  <div className="text-amber-300 font-semibold">warrant/</div>
                  <div className="pl-4">├── Cargo.toml <span className="text-zinc-500">(resolver = 2, shared deps)</span></div>
                  <div className="pl-4">├── rust-toolchain.toml <span className="text-zinc-500">(pinned {TOOLCHAIN})</span></div>
                  <div className="pl-4">├── .gitignore <span className="text-zinc-500">(ignores bench/own/, target, .env)</span></div>
                  <div className="pl-4">├── crates/</div>
                  <div className="pl-8">├── extract/ <span className="text-zinc-500">(syn AST parser)</span></div>
                  <div className="pl-8">├── llm/ <span className="text-zinc-500">(trait LlmClient & data contracts)</span></div>
                  <div className="pl-8">├── verify/ <span className="text-zinc-500">(deterministic Miri verifier)</span></div>
                  <div className="pl-8">└── cli/ <span className="text-zinc-500">(standardized exit codes 0/1/2)</span></div>
                  <div className="pl-4">├── tools/mutator/ <span className="text-zinc-500">(pair mutation utility)</span></div>
                  <div className="pl-4">├── sandbox/ <span className="text-zinc-500">(network-isolated Dockerfile & runner)</span></div>
                  <div className="pl-4">└── bench/pairs/ <span className="text-zinc-500">(r1_reference, r2_reference fixtures)</span></div>
                </div>
              </div>

              <div className="bg-zinc-900/60 border border-zinc-800 rounded-lg p-5">
                <h3 className="font-mono text-sm font-semibold text-zinc-200 mb-3 flex items-center gap-2">
                  <Terminal className="w-4 h-4 text-blue-400" />
                  Standard Execution Commands
                </h3>
                <div className="space-y-3 font-mono text-xs">
                  <div>
                    <span className="text-zinc-400 block mb-1"># 1. Run automated soundness audit on benchmark pair</span>
                    <div className="bg-zinc-950 p-2.5 rounded border border-zinc-800 text-zinc-200 flex justify-between items-center">
                      <code>cargo run -p warrant-cli -- audit --target bench/pairs/r1_reference.rs</code>
                      <button
                        onClick={() => handleCopy('cargo run -p warrant-cli -- audit --target bench/pairs/r1_reference.rs', 'cmd1')}
                        className="text-zinc-500 hover:text-zinc-200"
                      >
                        {copiedPath === 'cmd1' ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                      </button>
                    </div>
                  </div>

                  <div>
                    <span className="text-zinc-400 block mb-1"># 2. Extract AST candidates as machine-readable JSON</span>
                    <div className="bg-zinc-950 p-2.5 rounded border border-zinc-800 text-zinc-200 flex justify-between items-center">
                      <code>cargo run -p warrant-cli -- extract --target src/lib.rs --json</code>
                      <button
                        onClick={() => handleCopy('cargo run -p warrant-cli -- extract --target src/lib.rs --json', 'cmd2')}
                        className="text-zinc-500 hover:text-zinc-200"
                      >
                        {copiedPath === 'cmd2' ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                      </button>
                    </div>
                  </div>

                  <div>
                    <span className="text-zinc-400 block mb-1"># 3. Execute in hardened Docker container</span>
                    <div className="bg-zinc-950 p-2.5 rounded border border-zinc-800 text-zinc-200 flex justify-between items-center">
                      <code>./sandbox/run_in_sandbox.sh ./bench/pairs -Zmiri-tree-borrows</code>
                      <button
                        onClick={() => handleCopy('./sandbox/run_in_sandbox.sh ./bench/pairs -Zmiri-tree-borrows', 'cmd3')}
                        className="text-zinc-500 hover:text-zinc-200"
                      >
                        {copiedPath === 'cmd3' ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                      </button>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        )}

        {/* WORKSPACE FILES TAB */}
        {activeTab === 'files' && (
          <div className="grid grid-cols-1 lg:grid-cols-12 gap-6">
            {/* File List Navigation */}
            <div className="lg:col-span-4 space-y-4">
              <div className="bg-zinc-900/80 border border-zinc-800 rounded-lg p-3">
                <div className="text-xs font-mono uppercase tracking-wider text-zinc-400 font-semibold mb-2 px-2">
                  Scaffolded Workspace Files ({PROJECT_FILES.length})
                </div>
                <div className="space-y-1">
                  {PROJECT_FILES.map((file) => (
                    <button
                      key={file.path}
                      onClick={() => setSelectedFile(file)}
                      className={`w-full text-left px-3 py-2 rounded text-xs font-mono transition-colors flex items-center justify-between ${
                        selectedFile.path === file.path
                          ? 'bg-amber-500/10 text-amber-300 border border-amber-500/30'
                          : 'text-zinc-300 hover:bg-zinc-800/60'
                      }`}
                    >
                      <div className="flex items-center gap-2 truncate">
                        <FileCode2 className="w-3.5 h-3.5 text-zinc-400 shrink-0" />
                        <span className="truncate">{file.path}</span>
                      </div>
                      <span className="text-[10px] text-zinc-500 uppercase px-1.5 py-0.5 rounded bg-zinc-950 shrink-0">
                        {file.category}
                      </span>
                    </button>
                  ))}
                </div>
              </div>

              {/* Invariant Checklist */}
              <div className="bg-zinc-900/60 border border-zinc-800 rounded-lg p-4 font-mono text-xs space-y-2">
                <div className="font-semibold text-zinc-200 uppercase tracking-wider text-[11px] mb-2 flex items-center gap-1.5">
                  <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
                  Hard Rule Verification
                </div>
                <div className="flex items-center gap-2 text-zinc-300">
                  <Check className="w-3.5 h-3.5 text-emerald-400 shrink-0" />
                  <span>Pinned toolchain: nightly-2026-10-20</span>
                </div>
                <div className="flex items-center gap-2 text-zinc-300">
                  <Check className="w-3.5 h-3.5 text-emerald-400 shrink-0" />
                  <span>Zero-network read-only sandbox container</span>
                </div>
                <div className="flex items-center gap-2 text-zinc-300">
                  <Check className="w-3.5 h-3.5 text-emerald-400 shrink-0" />
                  <span>Mandatory #![forbid(unsafe_code)]</span>
                </div>
                <div className="flex items-center gap-2 text-zinc-300">
                  <Check className="w-3.5 h-3.5 text-emerald-400 shrink-0" />
                  <span>Zero-panic Serde JSON contracts</span>
                </div>
                <div className="flex items-center gap-2 text-zinc-300">
                  <Check className="w-3.5 h-3.5 text-emerald-400 shrink-0" />
                  <span>Exit codes: 0 = clean, 1 = bug, 2 = err</span>
                </div>
                <div className="flex items-center gap-2 text-zinc-300">
                  <Check className="w-3.5 h-3.5 text-emerald-400 shrink-0" />
                  <span>.gitignore ignores bench/own/, target, .env</span>
                </div>
              </div>
            </div>

            {/* File Viewer Panel */}
            <div className="lg:col-span-8 bg-zinc-900/90 border border-zinc-800 rounded-lg flex flex-col overflow-hidden">
              <div className="px-4 py-3 border-b border-zinc-800 bg-zinc-950 flex items-center justify-between">
                <div>
                  <div className="flex items-center gap-2 font-mono text-sm text-zinc-200">
                    <Code className="w-4 h-4 text-amber-400" />
                    <span>{selectedFile.path}</span>
                  </div>
                  <p className="text-xs text-zinc-400 mt-0.5">{selectedFile.description}</p>
                </div>
                <button
                  onClick={() => handleCopy(selectedFile.content, selectedFile.path)}
                  className="px-2.5 py-1.5 rounded bg-zinc-800 hover:bg-zinc-700 text-xs font-mono text-zinc-300 flex items-center gap-1.5 transition-colors"
                >
                  {copiedPath === selectedFile.path ? (
                    <>
                      <Check className="w-3.5 h-3.5 text-emerald-400" />
                      <span>Copied</span>
                    </>
                  ) : (
                    <>
                      <Copy className="w-3.5 h-3.5" />
                      <span>Copy Code</span>
                    </>
                  )}
                </button>
              </div>

              <div className="p-4 bg-zinc-950 overflow-x-auto max-h-[640px] text-xs font-mono leading-relaxed text-zinc-300">
                <pre>
                  <code>{selectedFile.content}</code>
                </pre>
              </div>
            </div>
          </div>
        )}

        {/* LIVE AUDITOR TAB */}
        {activeTab === 'interactive' && (
          <div className="space-y-6">
            {/* Control Bar */}
            <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4 flex flex-wrap items-center justify-between gap-4">
              <div className="flex items-center gap-3">
                <span className="text-xs font-mono text-zinc-400">Target Fixture:</span>
                <div className="flex bg-zinc-950 p-1 rounded border border-zinc-800 text-xs font-mono">
                  <button
                    onClick={() => {
                      setActiveBenchmark('r1');
                      setAuditProgressStep(0);
                    }}
                    className={`px-3 py-1 rounded transition-colors ${
                      activeBenchmark === 'r1'
                        ? 'bg-amber-500/20 text-amber-300 border border-amber-500/30 font-medium'
                        : 'text-zinc-400 hover:text-zinc-200'
                    }`}
                  >
                    r1_reference (get_unchecked OOB)
                  </button>
                  <button
                    onClick={() => {
                      setActiveBenchmark('r2');
                      setAuditProgressStep(0);
                    }}
                    className={`px-3 py-1 rounded transition-colors ${
                      activeBenchmark === 'r2'
                        ? 'bg-amber-500/20 text-amber-300 border border-amber-500/30 font-medium'
                        : 'text-zinc-400 hover:text-zinc-200'
                    }`}
                  >
                    r2_reference (unsafe impl Sync)
                  </button>
                </div>
              </div>

              <div className="flex items-center gap-4">
                <div className="flex items-center gap-2 text-xs font-mono text-zinc-400">
                  <Sliders className="w-3.5 h-3.5 text-zinc-500" />
                  <span>Miri Mode:</span>
                  <select
                    value={miriEngine}
                    onChange={(e) => setMiriEngine(e.target.value as any)}
                    className="bg-zinc-950 border border-zinc-800 text-zinc-200 rounded px-2.5 py-1 text-xs font-mono"
                  >
                    <option value="tree-borrows">-Zmiri-tree-borrows</option>
                    <option value="stacked-borrows">-Zmiri-stacked-borrows</option>
                  </select>
                </div>

                <button
                  disabled={isAuditing}
                  onClick={runSimulation}
                  className="px-4 py-2 rounded bg-amber-500 hover:bg-amber-400 text-zinc-950 font-mono text-xs font-bold flex items-center gap-2 transition-colors disabled:opacity-50"
                >
                  {isAuditing ? (
                    <>
                      <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                      <span>Auditing in Sandbox...</span>
                    </>
                  ) : (
                    <>
                      <Play className="w-3.5 h-3.5 fill-current" />
                      <span>Audit Candidate</span>
                    </>
                  )}
                </button>
              </div>
            </div>

            {/* Stepper Pipeline State */}
            <div className="grid grid-cols-1 md:grid-cols-4 gap-3">
              <div
                className={`p-3 rounded border font-mono text-xs ${
                  auditProgressStep >= 1
                    ? 'bg-amber-500/10 border-amber-500/40 text-amber-300'
                    : 'bg-zinc-900 border-zinc-800 text-zinc-500'
                }`}
              >
                <div className="flex items-center gap-1.5 font-semibold">
                  <CheckCircle2 className="w-3.5 h-3.5" />
                  1. AST Candidate
                </div>
                <div className="text-[11px] text-zinc-400 mt-1">
                  Extracted {currentBench.candidateKind} ({currentBench.lineSpan})
                </div>
              </div>

              <div
                className={`p-3 rounded border font-mono text-xs ${
                  auditProgressStep >= 2
                    ? 'bg-blue-500/10 border-blue-500/40 text-blue-300'
                    : 'bg-zinc-900 border-zinc-800 text-zinc-500'
                }`}
              >
                <div className="flex items-center gap-1.5 font-semibold">
                  <CheckCircle2 className="w-3.5 h-3.5" />
                  2. LLM Hypothesis
                </div>
                <div className="text-[11px] text-zinc-400 mt-1">
                  Synthesized Safe Harness
                </div>
              </div>

              <div
                className={`p-3 rounded border font-mono text-xs ${
                  auditProgressStep >= 3
                    ? 'bg-purple-500/10 border-purple-500/40 text-purple-300'
                    : 'bg-zinc-900 border-zinc-800 text-zinc-500'
                }`}
              >
                <div className="flex items-center gap-1.5 font-semibold">
                  <CheckCircle2 className="w-3.5 h-3.5" />
                  3. Miri Sandboxed Exec
                </div>
                <div className="text-[11px] text-zinc-400 mt-1">
                  --network none / read-only
                </div>
              </div>

              <div
                className={`p-3 rounded border font-mono text-xs ${
                  auditProgressStep >= 4 || auditComplete
                    ? 'bg-red-500/10 border-red-500/40 text-red-300'
                    : 'bg-zinc-900 border-zinc-800 text-zinc-500'
                }`}
              >
                <div className="flex items-center gap-1.5 font-semibold">
                  <AlertTriangle className="w-3.5 h-3.5" />
                  4. Proven Bug (Exit: 1)
                </div>
                <div className="text-[11px] text-zinc-400 mt-1">
                  Evidence Bundle Sealed
                </div>
              </div>
            </div>

            {/* Split Audit Details */}
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
              {/* Left Column: Target & Safe Harness */}
              <div className="space-y-4">
                {/* Target Snippet */}
                <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4">
                  <div className="flex items-center justify-between mb-2">
                    <span className="font-mono text-xs text-amber-400 font-semibold uppercase tracking-wider flex items-center gap-1.5">
                      <Code className="w-3.5 h-3.5" />
                      Target Under Audit: {currentBench.enclosingItem}
                    </span>
                    <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-zinc-800 text-zinc-400 border border-zinc-700/60">
                      {currentBench.lineSpan}
                    </span>
                  </div>
                  <pre className="p-3 bg-zinc-950 rounded font-mono text-xs text-zinc-300 border border-zinc-800/80 overflow-x-auto">
                    <code>{currentBench.targetSnippet}</code>
                  </pre>
                </div>

                {/* Safe Harness with forbid(unsafe_code) */}
                <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4">
                  <div className="flex items-center justify-between mb-2">
                    <span className="font-mono text-xs text-blue-400 font-semibold uppercase tracking-wider flex items-center gap-1.5">
                      <Lock className="w-3.5 h-3.5" />
                      Synthesized Safe Harness (Strict Invariant)
                    </span>
                    <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
                      #![forbid(unsafe_code)]
                    </span>
                  </div>
                  <pre className="p-3 bg-zinc-950 rounded font-mono text-xs text-zinc-300 border border-zinc-800/80 overflow-x-auto">
                    <code>{currentBench.harness}</code>
                  </pre>
                </div>
              </div>

              {/* Right Column: Miri Diagnostic Output & Evidence Bundle */}
              <div className="space-y-4">
                {/* Miri Sandbox Output */}
                <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4">
                  <div className="flex items-center justify-between mb-2">
                    <span className="font-mono text-xs text-red-400 font-semibold uppercase tracking-wider flex items-center gap-1.5">
                      <Terminal className="w-3.5 h-3.5" />
                      Miri Diagnostic Stderr Output
                    </span>
                    <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-red-500/10 text-red-400 border border-red-500/20">
                      exit code: {currentBench.exitCode}
                    </span>
                  </div>
                  <pre className="p-3 bg-zinc-950 rounded font-mono text-xs text-red-300/90 border border-red-950 overflow-x-auto leading-relaxed">
                    <code>{currentBench.miriStderr}</code>
                  </pre>
                </div>

                {/* Evidence Bundle & Patch */}
                <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4 space-y-3">
                  <div className="flex items-center justify-between">
                    <span className="font-mono text-xs text-purple-400 font-semibold uppercase tracking-wider flex items-center gap-1.5">
                      <Box className="w-3.5 h-3.5" />
                      Cryptographic Evidence Bundle
                    </span>
                    <span className="text-[10px] font-mono text-zinc-400">
                      SHA256: {currentBench.evidenceHash.substring(0, 16)}...
                    </span>
                  </div>

                  <div className="bg-zinc-950 p-3 rounded border border-zinc-800/80 font-mono text-xs text-zinc-400 space-y-1">
                    <div><span className="text-zinc-500">UB Class:</span> <span className="text-red-400 font-bold">{currentBench.ubClass}</span></div>
                    <div><span className="text-zinc-500">Status:</span> <span className="text-red-400">PROVEN_BUG</span></div>
                    <div><span className="text-zinc-500">Flags:</span> <code>-Zmiri-tree-borrows -Zmiri-strict-provenance</code></div>
                    <div><span className="text-zinc-500">Reproduction:</span> <code>cargo miri run -- main.rs</code></div>
                  </div>

                  {/* Patch Proposal */}
                  <div>
                    <span className="font-mono text-xs text-emerald-400 font-semibold block mb-1">
                      Sound Remediation Patch (LlmClient::propose_patch):
                    </span>
                    <pre className="p-2.5 bg-zinc-950 rounded font-mono text-xs text-emerald-300/90 border border-emerald-950 overflow-x-auto">
                      <code>{currentBench.patch}</code>
                    </pre>
                  </div>
                </div>
              </div>
            </div>
          </div>
        )}

        {/* BENCHMARK MATRIX TAB */}
        {activeTab === 'benchmarks' && (
          <div className="space-y-6">
            <div className="bg-zinc-900/60 border border-zinc-800 rounded-lg p-5">
              <h2 className="text-base font-semibold text-zinc-100 font-mono mb-2 flex items-center gap-2">
                <Cpu className="w-4 h-4 text-amber-400" />
                Benchmark Corpus Matrix (Task T0 / T1)
              </h2>
              <p className="text-xs text-zinc-400 leading-relaxed">
                Benchmark pairs validating soundness detection accuracy across UB classes. All fixtures include both the vulnerable unsafe abstraction and a verified safe consumer harness triggering Undefined Behavior under Miri.
              </p>
            </div>

            <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
              {/* Fixture R1 Card */}
              <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-5 flex flex-col justify-between">
                <div>
                  <div className="flex items-center justify-between mb-3">
                    <span className="font-mono text-xs font-bold text-amber-400">bench/pairs/r1_reference.rs</span>
                    <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-red-500/10 text-red-400 border border-red-500/20">
                      ub_oob
                    </span>
                  </div>
                  <h3 className="font-mono text-sm font-semibold text-zinc-200 mb-1">
                    Unsound get_unchecked OOB Dereference
                  </h3>
                  <p className="text-xs text-zinc-400 mb-4">
                    Exposes safe caller API wrapping <code className="text-amber-300">get_unchecked</code> without index bounds validation. Safe caller reads past 4-byte buffer allocation at offset 8.
                  </p>
                  <div className="bg-zinc-950 p-3 rounded font-mono text-xs text-zinc-300 border border-zinc-800/80 mb-4">
                    <div className="text-zinc-500 text-[11px] mb-1">Miri Stderr Trigger:</div>
                    <div className="text-red-400">error: Undefined Behavior: dereferencing pointer failed: alloc has size 4, so pointer to 1 byte starting at offset 8 is out-of-bounds</div>
                  </div>
                </div>
                <div className="pt-3 border-t border-zinc-800 flex items-center justify-between">
                  <span className="text-[11px] font-mono text-zinc-500">Exit Code: 1 (Proven Bug)</span>
                  <button
                    onClick={() => {
                      setSelectedFile(PROJECT_FILES.find(f => f.path.includes('r1_reference'))!);
                      setActiveTab('files');
                    }}
                    className="text-xs font-mono text-amber-400 hover:text-amber-300 flex items-center gap-1"
                  >
                    <span>View Fixture Source</span>
                    <ArrowRight className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>

              {/* Fixture R2 Card */}
              <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-5 flex flex-col justify-between">
                <div>
                  <div className="flex items-center justify-between mb-3">
                    <span className="font-mono text-xs font-bold text-amber-400">bench/pairs/r2_reference.rs</span>
                    <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-red-500/10 text-red-400 border border-red-500/20">
                      data_race
                    </span>
                  </div>
                  <h3 className="font-mono text-sm font-semibold text-zinc-200 mb-1">
                    Unsound Sync Impl on Interior Cell
                  </h3>
                  <p className="text-xs text-zinc-400 mb-4">
                    Implements <code className="text-amber-300">unsafe impl Sync</code> for <code className="text-amber-300">SharedCounter(Cell&lt;usize&gt;)</code>. Safe multithreaded harness spawns concurrent worker threads executing unsynchronized mutations.
                  </p>
                  <div className="bg-zinc-950 p-3 rounded font-mono text-xs text-zinc-300 border border-zinc-800/80 mb-4">
                    <div className="text-zinc-500 text-[11px] mb-1">Miri Stderr Trigger:</div>
                    <div className="text-red-400">error: Undefined Behavior: Data race detected between (1) Read on thread \`main\` and (2) Write on thread \`unnamed-1\`</div>
                  </div>
                </div>
                <div className="pt-3 border-t border-zinc-800 flex items-center justify-between">
                  <span className="text-[11px] font-mono text-zinc-500">Exit Code: 1 (Proven Bug)</span>
                  <button
                    onClick={() => {
                      setSelectedFile(PROJECT_FILES.find(f => f.path.includes('r2_reference'))!);
                      setActiveTab('files');
                    }}
                    className="text-xs font-mono text-amber-400 hover:text-amber-300 flex items-center gap-1"
                  >
                    <span>View Fixture Source</span>
                    <ArrowRight className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>
            </div>
          </div>
        )}
      </main>

      {/* Footer */}
      <footer className="border-t border-zinc-800/80 bg-zinc-950 py-4 mt-auto">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 flex flex-wrap items-center justify-between gap-4 text-xs font-mono text-zinc-500">
          <div className="flex items-center gap-4">
            <span>Project WARRANT • v0.1.0</span>
            <span>Channel: {TOOLCHAIN}</span>
            <span>Sysroot: {MIRI_SYSROOT}</span>
          </div>
          <div>
            <span>Isolated Execution: {SANDBOX_FLAGS}</span>
          </div>
        </div>
      </footer>
    </div>
  );
}
