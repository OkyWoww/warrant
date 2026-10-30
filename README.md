# 🛡️ WARRANT: Proof-First Automated Soundness Auditor for Rust `unsafe`

![Miri](https://img.shields.io/badge/Miri-nightly--2026--10--20%20pinned-orange) ![License](https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue) ![Meta Model API](https://img.shields.io/badge/Meta%20Model%20API-muse--spark--1.3-0467DF)

> **A finding exists ONLY if a safe-Rust harness reproduces Undefined Behavior under Miri at the exact `unsafe` site.**

WARRANT is an automated, proof-first soundness auditor built for the **Meta Global AI Developer Hackathon (Push the Frontier theme)**. Rather than reporting speculative AI warnings or relying solely on heuristics, WARRANT combines the generative reasoning of LLMs (Meta Muse Spark / Muse Code) with a deterministic Miri operational verification pipeline to produce irrefutable, cryptographically attested proof of Rust `unsafe` soundness violations.

> [!NOTE]
> Sandbox Docker (`sandbox/Dockerfile`) disediakan di repositori, namun seluruh evaluasi benchmark dan demo pada fixture publik ini dieksekusi menggunakan `LocalMiriRunner` langsung pada host Windows.

---

## ⚡ 1-Minute Quickstart

### Prerequisites
- Rust nightly toolchain with `miri` and `rust-src`:
  ```bash
  rustup toolchain install nightly-2026-10-20 --component miri,rust-src
  ```

### Build the CLI
```bash
cargo build --release -p warrant-cli
```
The compiled binary will be placed at `./target/release/warrant` (`warrant.exe` on Windows).

### Run an Audit or the Flagship Pre-Warmed Demo
Audit any Rust file or crate directory, or launch the pre-warmed 3-case flagship demo:
```bash
# Run the flagship 3-case demo (RUSTSEC-2021-0003 smallvec R1 + R2 concurrency + safe control UNPROVEN)
./target/release/warrant demo --offline --events target/warrant/demo/events.jsonl --output-dir target/warrant/demo

# Run the flagship demo live against Meta Model API (muse-spark-1.3)
./target/release/warrant demo --backend meta-api --output-dir target/warrant/demo_live

# Audit a specific module with real-time JSONL event stream telemetry
./target/release/warrant audit bench/pairs/r1_reference.rs --events target/warrant/events.jsonl --output-dir evidence

# Audit an entire crate directory with offline deterministic mock backend
./target/release/warrant audit . --backend mock --output-dir evidence
```

---

## 📊 Final Empirical Results (`docs/FINAL_RESULTS.json` & `docs/BENCH.md`)

All benchmark numbers are reproducible via `cargo run --bin final_benchmark_runs -p warrant-cli` and recorded in [`docs/FINAL_RESULTS.json`](file:///c:/Users/ADMIN/Documents/warrant/docs/FINAL_RESULTS.json):

| Metric (official, `docs/FINAL_RESULTS.json`) | Value |
| :--- | :--- |
| Verifier proof rate, 22 frozen bug targets × 3 runs | **22/22 (100%)**, variance 0 |
| False proofs on 16 safe controls (reference & autonomous loop) | **0/16 (0%)**, 95% CI upper 20.59% |
| Live `muse-spark-1.3` valid@1 → valid@4 (2 passes) | **79.54% → 100.0%** (+20.46% from repair loop) |
| `PATCH_VERIFIED` (16 mutants + `smallvec`) | **17/17 (100%)** |
| Total live API spend | **\$3.30 USD** ($3.295901 USD across 188 HTTP calls) |

### Table A — Verifier Core Determinism (on Reference Harnesses, 3 Runs across 38 Frozen Targets)
- **Bug Targets Proven (`PROVEN_PUBLIC`)**: **22 / 22 (100.0%)** across all 3 runs (`16/16` `bench/pairs/` mutants, `5/5` `bench/rustsec_anon/`, `1/1` `smallvec` `RUSTSEC-2021-0003`) — Clopper-Pearson 95% CI `[84.56%, 100.00%]` (`> 87.27%` one-sided lower), **3-Run Variance = `0.0000`**.
- **False Proofs on Safe Controls (`SoundSafe`)**: **0 / 16 (0.0%)** across all 3 runs — Clopper-Pearson 95% CI `[0.00%, 20.59%]` (`< 17.07%` one-sided upper), **3-Run Variance = `0.0000`**.

### Table B — End-to-End Autonomous Model Performance (`muse-spark-1.3` Live API)
- **Full Frozen Bug Corpus Live Synthesis (2 Passes × 22 Bug Targets = 44 Evaluations, 57 HTTP Calls)**:
  - **Live Pass #1**: `valid@1 = 17/22 (77.27%)`, `valid@4 = 22/22 (100.0%)`, `Model-Generated Proof Rate = 22/22 (100.0%)` (17 proven on Round 1; 7 repair calls across Rounds 2–4 on 5 targets), `False Proofs = 0/16 (0.0% on 16 live safe controls evaluated under autonomous loop)`, 29 HTTP calls, `$0.531060 USD`.
  - **Live Pass #2**: `valid@1 = 18/22 (81.82%)`, `valid@4 = 22/22 (100.0%)`, `Model-Generated Proof Rate = 22/22 (100.0%)` (18 proven on Round 1; 6 repair calls across Rounds 2–4 on 4 targets), `False Proofs = 0/16 (0.0% on 16 live safe controls evaluated under autonomous loop)`, 28 HTTP calls, `$0.507838 USD`.
  - **Autonomous Repair Loop Impact**: **+20.46% proof rate gain** (Round 1 `valid@1`: Mean `79.54%`, StdDev `3.22%` -> Round 4 `valid@4`: Mean `100.00%`, StdDev `0.00%` through verbatim compiler/Miri diagnostic feedback).
- **Live Safe Controls Autonomous Loop Evaluation (`final_benchmark_runs`, $n = 16$)**: **0 / 16 False Proofs (0.0%)** across up to 4 autonomous hypothesis and repair rounds under Miri — Clopper-Pearson 95% CI `[0.00%, 20.59%]` (`< 17.07%` one-sided upper), 63 HTTP calls, `$1.585954 USD`.
- **Gate 1 Seeded Cohort (`eval_gate1 --backend meta`, $n = 8$)**: `valid@1 = 7/8 (87.5%)`, `valid@4 = 8/8 (100.0%)`, `Proof Rate = 6/8 (75.0%)`, `False Proofs = 0/8 (0.0%)`, 9 HTTP calls, `$0.163977 USD`.
- **Autonomous Patch Verification Loop (`eval_patches --backend meta`, 17 Targets = 16 Mutants + `smallvec`)**: **17 / 17 `PATCH_VERIFIED` (100.0%)** in Round 1/3 (AST anti-bypass/signature guard + exact `warrant_proof::proof_*` clean under Miri + native `cargo test --lib` + `warrant_proof::sanity_*` clean under Miri; for `smallvec_r1_demo`, the patch adds an early-return capacity & index guard preventing OOB pointer write and preserving elements within capacity limits per `sanity_*` without silent truncation, dengan catatan batasan semantik `PATCH_VERIFIED` di [`docs/LIMITS.md`](file:///c:/Users/ADMIN/Documents/warrant/docs/LIMITS.md)) — Clopper-Pearson 95% CI `[80.49%, 100.00%]`, 17 HTTP calls, `$0.214093 USD`.
- **Comparative Baselines (`eval_baselines --backend meta`, 16 Pairs = 32 Targets)**:
  - *Cargo Clippy*: `0/16 (0.0%)` bugs found, `12/16 (75.0%)` lint noise on safe controls, `0/16` proofs.
  - *Fuzzing under Miri*: `1/16 (6.2%)` bugs found, `0/16 (0.0%)` false alarms, `1/16` proofs (blind to staged state & multi-threaded data races).
  - *Unverified Model (`muse-spark-1.3`)*: *Unverified LLM mampu mengidentifikasi pola kode mencurigakan (75% recall / 12/16 mutan, 0% false alarm pada pairs), namun hanya menghasilkan opini teks (0% Miri-Verified Operational Proofs). Nilai utama WARRANT bukan pada menebak lokasi `unsafe`, melainkan mengubah hipotesis menjadi bukti reproduksi Miri yang deterministik, teratestasi lokasinya, dan memverifikasi patch secara otomatis untuk gerbang CI.*
- **Cumulative Live API Ledger (`CostTracker`)**: **188 HTTP calls** (`0` missing usage), **151,959 prompt tokens**, **735,096 completion tokens**, **`$3.295901 USD`** total spend out of `$150.00` promotional credit (`$146.704099 USD` / `97.80%` credit remaining, circuit breaker remaining: `$101.704099 USD`).

---

## 🔐 Privacy & Promotional Credit Disclosure (`muse-spark-1.3-contributor`)

Per `SPEC.md` Section 11 and `AGENTS.md` Hard Rule 14: while promotional free credits are active on Meta's API gateway (`https://api.meta.ai/v1/chat/completions`), requests to `muse-spark-1.3` resolve in the HTTP response to `muse-spark-1.3-contributor`, where prompts and completions may be used by Meta for product improvement. Accordingly:
1. Only public, permissively licensed (MIT / Apache-2.0) benchmark fixtures in `bench/pairs/`, `bench/rustsec_anon/`, and `bench/real_world_demo/` were ever transmitted to the Meta Model API.
2. Zero private code, zero secrets (`.env`), and nothing from `bench/own/` was ever sent to a model API or staged in Git.

---

## 🚦 Standardized Exit Codes (CI/CD Protocol)

WARRANT strictly adheres to standard exit codes, making it seamless to integrate into automated GitHub Actions and continuous integration pipelines:

| Exit Code | Classification | Condition |
|:---:|:---|:---|
| **`0`** | **CLEAN** | Audit succeeded. Zero proven soundness bugs discovered. |
| **`1`** | **PROVEN BUG** | Audit succeeded. At least one (1) bug was proven to trigger Undefined Behavior under Miri. |
| **`2`** | **ERROR** | Tool or execution error (invalid arguments, missing target path, compiler toolchain error). |

---

## 📦 GitHub Actions CI Integration

Add `.github/workflows/warrant.yml` to your repository for automated soundness verification on every Pull Request:

```yaml
name: WARRANT Unsafe Soundness Audit

on:
  push:
    branches: [ "main" ]
  pull_request:
    branches: [ "main" ]

jobs:
  soundness-audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@nightly
        with:
          toolchain: nightly-2026-10-20
          components: miri, rust-src

      - name: Build WARRANT CLI
        run: cargo build --release -p warrant-cli

      - name: Run Soundness Audit
        run: ./target/release/warrant audit . --backend mock --output-dir evidence

      - name: Archive Audit Reports & Evidence Bundles
        uses: actions/upload-artifact@v4
        if: always()
        with:
          name: warrant-audit-report
          path: |
            evidence/warrant_report.md
            evidence/warrant_report.html
            evidence/
```

---

## 🔒 Soundness Invariants & Hard Rules

WARRANT guarantees zero false proofs through strict invariants enforced at the compiler and type-system level:

1. **Hard Rule 1 (Zero Speculative Bugs)**: Unproven ideas remain labeled `UNPROVEN`. A finding is declared **ONLY** if safe code reproduces Undefined Behavior under Miri at the target line span.
2. **100% Safe-Rust Test Harnesses**: Every generated harness begins with an inner `#![forbid(unsafe_code)]` attribute followed by `extern crate std;`.
3. **Deterministic Static Filter**: Automatically rejects harnesses containing banned evasion tokens (`unsafe`, `allow`, `panic!`, `Box::leak`, `mem::forget`, `std::process`) or signature phrase forgeries in string literals.
4. **Autonomous Compiler Repair Loop**: Automatically feeds verbatim `rustc` compiler diagnostics back into the model for up to 4 autonomous iterations to ensure 100% syntactically valid safe Rust.
5. **Lossless Evidence Bundles**: Every finding generates an isolated reproducer folder containing Miri traces, the synthesized harness, SHA-256 integrity hashes, and standalone `reproduce.sh` / `reproduce.bat` scripts.

---

## 🎯 Target Rule Scope

WARRANT focuses on high-consequence unsoundness patterns in systems and driver-style Rust:

- **Rule R1 (Unchecked In-Memory Access)**:
  - `slice::get_unchecked`, `from_raw_parts`, unchecked pointer arithmetic.
  - Expected Miri trigger: `Undefined Behavior: dereferencing pointer failed`, `in-bounds pointer arithmetic failed`, or `unsafe precondition(s) violated`.
- **Rule R2 (Unsound Concurrency Implementations)**:
  - Incorrect `unsafe impl Send` or `unsafe impl Sync` on types containing unsynchronized state (`Cell`, `Rc`, raw pointers without memory barriers).
  - Expected Miri trigger: `Undefined Behavior: Data race detected between`.

---

## 📜 Prepared Before the Event

In strict compliance with the **Meta Global AI Developer Hackathon Participant Guide** (Provenance & Transparency, SPEC.md Section 1.1 and AGENTS.md Rule 13), the following foundational materials were prepared prior to the official event kickoff (26 Oct 2026, 12:00 PM ET):

1. **`docs/SPEC.md`**: Architectural specification, evaluation metrics, and normative data contracts.
2. **`sandbox/Dockerfile` & `sandbox/run_in_sandbox.sh`**: Generic offline container runner scripts for Miri isolation (Sandbox Docker disediakan di repositori, namun seluruh evaluasi benchmark dan demo pada fixture publik ini dieksekusi menggunakan `LocalMiriRunner` langsung pada host Windows).
3. **`Cargo.toml` (Workspace root)**: Initial workspace layout definition.
4. **`AGENTS.md`**: Agent session protocol and hard rules (also archived with `SPEC.md` in `docs/files(9).zip`).
5. **Architecture design**: the proof-first pipeline design (extract → hypothesize → safe harness → Miri → attest → patch), as captured in `docs/SPEC.md`.

*All subsequent components—including AST candidate extractors, prompt builders, autonomous 4-round repair engine, Miri execution pipeline, benchmark pairs, CLI binaries, test suites, and report generators—were implemented during the hackathon build window.*

---

## 📄 License

Dual-licensed under either ([LICENSE](file:///c:/Users/ADMIN/Documents/warrant/LICENSE)):
- Apache License, Version 2.0 ([bench/rustsec_anon/LICENSES/LICENSE-APACHE](file:///c:/Users/ADMIN/Documents/warrant/bench/rustsec_anon/LICENSES/LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([bench/rustsec_anon/LICENSES/LICENSE-MIT](file:///c:/Users/ADMIN/Documents/warrant/bench/rustsec_anon/LICENSES/LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
