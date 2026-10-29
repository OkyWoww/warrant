# WARRANT SPEC (source of truth)

Version: 1.1 (waiver-aligned). Hackathon: Meta Global AI Developer Hackathon, Indie track, theme Push the Frontier. Build window: 26 Oct - 4 Nov 2026. Submission due 4 Nov 9 PM ET (verify timezone on the submit page).

Normative words: MUST, MUST NOT, SHOULD. Anything marked UNVERIFIED is an assumption and goes to `docs/LIMITS.md`.

## 1. Goal and non-goals

Goal: given a Rust crate or module that compiles on a host target (driver-style code is the flagship case study, not the only target), find `unsafe` soundness bugs, prove each one with a reproducible test, and where possible propose a patch that is itself verified. LLMs propose hypotheses, write harnesses, and draft patches. The compiler, Miri, and a deterministic verifier decide.

Non-goals: auditing real kernel drivers (FFI, `asm!`, MMIO, DMA), exploit generation, scanning code the user does not own or have permission to test, image models (SAM, Muse Image), rule R3 (IRQL), automatic repair as a requirement for a finding.

Allowed claim: on self-contained host-compilable modules, WARRANT converts X% of LLM soundness hypotheses into Miri-verified reproducing tests, with Y false proofs on paired controls (numbers measured, reported with confidence intervals).

### 1.1 Judging alignment and constraints (from the event waiver dated 13 Oct 2026)

Project judging: Impact 30%, Use of Meta AI Tooling 25%, Technical Depth and Execution 20%, Demo 25%. Application review is separate (idea clarity/feasibility and technical skill, equal weight).

| Criterion | What WARRANT MUST show |
|---|---|
| Impact (30%) | Usable by any Rust developer: CLI with standard exit codes, a GitHub Action template, README quickstart. Driver-style code is the flagship case. At least one real-world case: floor = a historical RustSec case reproduced on its original crate version; stretch = a new bug in an active public crate, disclosed to the maintainer first. |
| Meta tooling (25%) | Muse Spark for hypotheses. Muse Code as the agentic harness-repair and patch loop if T2 confirms programmatic use (fallback: the same loops on Muse Spark API calls). A measured comparison across Meta models where access exists. |
| Technical depth (20%) | Deterministic verifier, location attestation, evidence bundles, paired benchmark with intervals. |
| Demo (25%) | Pre-warmed demo mode, honest live latency, recorded backup. The final round watches video demos on 9 Nov, so a clear three-minute video is the main artifact and a runnable product is secondary. |

Constraints from the waiver (not legal advice; Official Rules are still pending):

- Submissions are non-confidential and licensed to the organizers perpetually and irrevocably. MUST NOT commit secrets or API keys. MUST NOT include private code in the submitted repo.
- Submissions MUST be original work. Open source is allowed if licenses are followed and the submission builds on it.
- Indie track allows teams of up to four. One track only.
- Participant guide (Oct 2026): repositories MUST be public. What is presented MUST have been built during the event (the guide lets only Startups build on existing codebases), and the demo MUST show only features and code built during the hackathon. Pre-event material (planning documents, generic sandbox scripts) MUST be listed in the README under "Prepared before the event", and all project code MUST be committed with honest timestamps. Read the Official Rules for the exact wording.
- Submission needs a three-minute demo video, the public repo, and a live product or runnable instructions (UNVERIFIED what counts as the live product; ask in #questions). Stage One is pass/fail: the project reasonably fits the theme and reasonably uses the Meta Model API and/or Meta open-weight models. The top 5 per track go to a final round whose judges watch the video demos, so the video is the primary artifact.
- Meta models named in the guide: Muse Spark series, Muse Wonder, Muse Image, Muse Voice Transcribe, SAM 3.1 (via Meta Model API); open weights Muse Glimmer and Muse Spark Open; Muse Code coding agent. On Baseten use `meta-models/muse-spark-open-1026`, not the id shown in the docs.
- Bonus prize: Best Build on Meta Open Weight Models ($10,000) for projects built with Muse Glimmer or Muse Spark Open on own hardware or partner inference.
- The waiver states that selected participants receive $150 in Meta Model API credits for development over a ten-day period. Confirm the actual balance and any expiry date in T2. If credits expire before the 9 Nov finals, the live demo MUST run from cached outputs. Partner credits (Baseten, Modal, Fireworks) are for select participants only (UNVERIFIED). Budget rules are in section 7.
- Third-party fixtures: permissive licenses only (MIT, Apache-2.0, BSD). Keep license text and attribution.

## 2. Definitions

- **Soundness bug:** safe code can trigger undefined behavior through an API whose implementation uses `unsafe`, or through an incorrect `unsafe impl Send/Sync`.
- **Not a finding:** calling an `unsafe fn` with arguments that violate its documented contract. That is caller error.
- **Candidate:** a located `unsafe` block, `unsafe impl`, or `unsafe fn` flagged by a rule heuristic.
- **Harness:** a test module written in 100% safe Rust, appended at the end of a copy of the target file.
- **PROVEN (public API):** harness calls only items reachable as `pub` from outside the crate, and all verifier steps 2-7 pass.
- **PROVEN (internal):** harness uses private items via the child module (`use super::*`), and steps 2-7 pass. Weaker; MUST be labeled so.
- **UNPROVEN:** anything else. Reported separately as a hypothesis, never mixed with PROVEN.
- **PATCH_VERIFIED / PATCH_FAILED:** remediation badge from section 5.3. Does not affect PROVEN.

## 3. Rules

| Rule | Pattern | Expected failure |
|---|---|---|
| R1 | Unchecked access whose bounds depend on input-controlled length/offset: `get_unchecked`, `from_raw_parts`, pointer arithmetic | Miri reports Undefined Behavior, or the std precondition check panics |
| R2 | `unsafe impl Send` / `unsafe impl Sync` on a type holding non-thread-safe state (`Cell`, `Rc`, raw pointer without synchronization) | Miri reports a data race in a two-thread safe harness |

Signature families (MUST be confirmed on the pinned nightly in T0 and stored in `docs/LIMITS.md`):

- `ub_oob` / `precondition_panic`: output contains `Undefined Behavior` or `unsafe precondition(s) violated`.
- `data_race`: output contains `Data race detected`.
- Always rejected as wrong reason: memory leak report, `unwrap`/`expect` panic unrelated to the candidate, timeout, compile error.

## 4. Data contracts (JSON, validate on parse)

Candidate:

```json
{ "id": "c-0001", "rule": "R1", "file": "src/frame.rs", "line_span": [14, 14],
  "enclosing_item": "FrameReader::next_frame", "visibility": "pub",
  "kind": "unsafe_block" }
```

Hypothesis (model output):

```json
{ "candidate_id": "c-0001", "skip": false,
  "precondition": "start + len <= buf.len()",
  "safe_entry_points": ["FrameReader::new", "FrameReader::feed", "FrameReader::next_frame"],
  "violating_sequence": "feed([200,1,2,3]); next_frame()",
  "expected_signature": "ub_oob", "confidence": "medium" }
```

Finding (verifier output):

```json
{ "candidate_id": "c-0001",
  "verdict": "PROVEN_PUBLIC | PROVEN_INTERNAL | UNPROVEN",
  "reason": "string", "patch_badge": "NONE | PATCH_VERIFIED | PATCH_FAILED",
  "toolchain": "nightly-YYYY-MM-DD", "miri_flags": "string",
  "harness_sha256": "hex", "evidence_dir": "path" }
```

Event (written by `warrant audit --events <path>`, JSONL, one object per line; used to render the demo video, see `docs/DEMO.md`):

```json
{ "ts": "2026-10-30T10:15:03Z", "run_id": "r-0001", "candidate_id": "c-0001",
  "stage": "miri_red", "status": "ok",
  "elapsed_ms": 4120, "model": "string or null", "cost_usd": 0.0, "detail": "short string" }
```

`stage` is one of: `extract`, `hypothesis`, `harness`, `compile`, `miri_red`, `signature`, `location`, `tier`, `patch`, `sanity`, `evidence`. `status` is one of: `start`, `ok`, `fail`, `skip`. `elapsed_ms` is measured, never estimated.

## 5. Verifier (normative)

### 5.1 Context for the model

The model receives the full text of the module file plus the candidate's line span. No call graph, no type resolver. The extractor MUST use `syn` only to find spans and enclosing items.

### 5.2 Decision procedure

1. Copy the target crate to a workdir. Append the harness module to the end of the target file (original line numbers MUST NOT shift). The module MUST start with `#![forbid(unsafe_code)]` followed by `extern crate std;`. That line is valid inside a child module and makes `std::thread` available even when the parent file is `#![no_std]`, without touching the top of the file. Test names MUST start with `proof_` (red harness) or `sanity_` (patch badge only).
2. Compile with `cargo test --no-run`. On failure, send the compiler output verbatim to the repair step. Maximum 4 repair rounds.
3. Static filter. Reject the harness without running it if any of these holds (all deterministic):
   - its text contains, as whole words (regex word boundary, so `unsafe_code` in the mandatory header does not match): `unsafe`, `allow`, `expect`, `macro_rules`, `asm`, `include`, `print`, `println`, `eprintln`, `panic`, or the paths `Box::leak`, `mem::forget`, `std::process`;
   - any string literal contains a signature phrase from section 3 (`Undefined Behavior`, `Data race`, `precondition`), to prevent forged signatures;
   - it defines an item (fn, struct, enum, trait, impl, static, const) whose name matches any item name in the original file (`syn` check), to prevent shadowing the target. Only `proof_*` / `sanity_*` test functions and uniquely named helpers are allowed.
   Note: on current rustc, `allow` should not be able to override `forbid` for `unsafe_code` (error E0453). This is UNVERIFIED; confirm in T0. The filter is defense in depth.
4. **Red:** run only the harness under Miri on the pinned nightly: `cargo +$WARRANT_NIGHTLY miri test --lib warrant_proof::proof_`. The filter keeps the crate's own tests (which may be Miri-incompatible) and doctests out of the verdict. The test MUST fail.
5. **Signature:** failure text MUST match the expected family (section 3).
6. **Location attestation:** the Miri stack trace MUST contain the candidate's `file:line` in the original file region. A harness that copies the function into itself fails here because its lines lie in the harness region.
7. **Tier:** determine PUBLIC vs INTERNAL from the visibility of the items the harness calls. Record the heuristic in the report.
8. Verdict is `PROVEN_PUBLIC` or `PROVEN_INTERNAL` if steps 2-7 pass. Otherwise `UNPROVEN`.

For R2, run several Miri seeds (`-Zmiri-many-seeds`). Choose and record one aliasing model (default or Tree Borrows) and the exact flags.

### 5.3 Patch verification (headline demo feature; built after Gate 2 because it needs PROVEN findings)

The model proposes a minimal patch. Badge `PATCH_VERIFIED` requires all of: the same harness passes and Miri is clean; the crate's existing tests pass natively (`cargo test --lib`, not under Miri); a sanity test (valid input gives the expected output, written before patching and passing on the original code) still passes (run with `cargo +$WARRANT_NIGHTLY miri test --lib warrant_proof::sanity_`). This blocks trivial patches such as always returning an empty slice. Failure is recorded as `PATCH_FAILED` and MUST NOT downgrade the finding.

### 5.4 Evidence bundle (per finding)

Target commit, toolchain version, Miri flags, exact commands, stdout/stderr, harness file and its sha256, patch diff if any, and `reproduce.sh` that re-runs the verdict from a clean checkout.

## 6. Sandbox requirements

- Container with no network, read-only root filesystem, non-root user, CPU/memory/PID/time limits.
- Miri sysroot built at image build time (`cargo miri setup`). Dependencies vendored (`cargo vendor`), builds run offline.
- Warm `CARGO_TARGET_DIR` shared across candidates. Only the source copy lives in tmpfs.
- Primary path: local Docker. Modal is optional for benchmark parallelism and MUST NOT be the only path.

## 7. LLM layer requirements

- `trait LlmClient` with backends: `meta_api` (Muse Spark), `open_weights` (Muse Spark Open or Muse Glimmer via a partner endpoint or own hardware), `mock` (deterministic).
- Model output MUST be parsed against the section 4 schemas. Invalid output counts as a failed attempt.
- Muse Code is the development agent for this repo and, if T2 confirms programmatic use (third-party sources mention `muse serve` and an SDK; UNVERIFIED), the agentic loop for harness repair and patch proposals. Put it behind an `AgentBackend` trait. Fallback if the probe fails: the same loops on Muse Spark API calls. The product MUST still work without Muse Code; it MUST NOT be a single point of failure.
- Cost control: every model call goes through a cost tracker that records tokens and cost per candidate from the API usage fields (which fields exist is UNVERIFIED). Hard limits: at most 4 harness repair rounds and at most 3 patch rounds per candidate; a global stop at 70% of the credit, after which the agent stops and reports to the human. The remaining 30% is reserved for the final benchmark runs (T13). If credits expire before the finals, plan the demo on cached outputs. Reuse cached model outputs on benchmark re-runs where valid.
- The waiver states $150 in credits for selected participants. Structured output, rate limits, context size, usage fields, the actual balance, and credit expiry are UNVERIFIED until T2 records the probe result.
- Privacy: the Billing page states that while free credits are in use, content may be used for product improvement. MUST NOT send private code, secrets, or `bench/own/` content to any model API. Send only public, permissively licensed fixtures and this project's own new code.

## 8. Benchmark, metrics, gates

Sets:

| Set | Content |
|---|---|
| pairs | about 16 pairs (8 R1, 8 R2): a safe version and a version with one mutation each (for example `checked_add` to `+`, `AtomicUsize` to `Cell<usize>`, bounds check removed). Every mutant manually verified to be a real bug. |
| rustsec_anon | about 4-6 historical soundness bugs, identifiers and comments anonymized, wrapped as host-compilable fixtures. Chosen and verified by the human. Permissive licenses only; license text and attribution kept in `bench/rustsec_anon/LICENSES/`. |
| own (optional, local only) | the human's own driver-style Rust modules made host-compilable. Kept out of the submitted repo by default because the submission license to the organizers is perpetual and irrevocable; the human decides case by case. Labeled `own`. |

At least 4 cases MUST need staged state or concurrency.

Real-world floor (T9): at least one historical RustSec case reproduced on its original crate version for the demo (separate from the anonymized benchmark copy). Stretch: a new bug in an active public crate. Responsible disclosure applies: notify the maintainer first, and do not publish or describe it in the demo or repo until the maintainer has had a reasonable chance to respond. The human decides. Seeded cases MUST be labeled seeded in every report.

Baselines on the same sets: Clippy, the model without verification, and fuzz/proptest under Miri with a minimal auto-generated harness. Report cases where a baseline wins.

Model comparison (Use of Meta tooling): run the benchmark with Muse Spark and with Muse Spark Open (and Muse Glimmer if time allows) where access exists (partner credits are not guaranteed; UNVERIFIED). Report proof rate, valid@4, patch verification rate (if T11 is built), latency, and cost per candidate for each, with intervals. Do not claim a ranking beyond what the sample supports.

Metrics: proof rate per tier; false proof on the safe half of pairs (target 0, reported with a Clopper-Pearson interval); harness valid@1 and valid@4; patch badge rate; measured time per candidate; three runs for variance.

Gates (internal go/no-go thresholds, adjustable after T2 by the human only):

- Gate 1 (end of day 3): harness valid@4 at least 50% on 8 cases, and false proof 0. If it fails: switch to template slot-filling (model fills parameters only), reduce to R1 only, adjust claims. Do not add rules.
- Gate 2 (end of day 6): proof rate at least 50% on the mutant half. Then freeze the benchmark.

## 9. Repo layout

```
warrant/
  AGENTS.md  README.md  LICENSE
  .github/workflows/warrant.yml
  docs/ SPEC.md PROGRESS.md LIMITS.md CHANGES.md BENCH.md DEMO.md
  crates/extract/   crates/llm/   crates/verify/   crates/cli/
  tools/mutator/
  bench/pairs/  bench/rustsec_anon/  (bench/own/ is local only and gitignored)
  sandbox/  Dockerfile  run_in_sandbox.sh
  reproduce/
```

## 10. Tasks (one per agent session; each has a verifiable "Done when")

**T0 Pre-flight.** Read the Official Rules and record any constraint in `docs/LIMITS.md`; stop if they forbid the plan. Verify `bench/own/` is in `.gitignore`. Pin the nightly. Build the Docker image with sysroot and vendored deps. Create `docs/PROGRESS.md`. Agent-instruction canary: confirm the agent reports hard rules 1-3 from `AGENTS.md` when asked.
Done when: offline `docker run --network none` Miri run shows the R1 reference fixture failing with an accepted signature and the R2 fixture showing a data race; outputs pasted in PROGRESS.md; signature strings stored in LIMITS.md. Also confirm and paste output for: `ls AGENTS.md docs/SPEC.md` succeeds from the repo root; the R2 fixture compiles inside a `#![no_std]` parent file using `extern crate std;` in the harness module; the filtered command `cargo +$WARRANT_NIGHTLY miri test --lib warrant_proof::proof_` runs only the harness tests (show the test list); the static filter in 5.2 step 3 accepts the mandatory header and rejects a sample with `#[allow(...)]`. Record any deviation in LIMITS.md.

**T1 Workspace skeleton.** Crates from section 9, `LlmClient` and `AgentBackend` traits, `mock` backend.
Done when: `cargo check --workspace` and `cargo test --workspace` pass; mock returns a canned Hypothesis that validates against the schema.

**T2 Model and agent probe.** Small program that calls the Meta API and records what is available: structured output, context size, rate limits, remaining credit and any expiry date, and the usage fields the cost tracker needs. Check whether Muse Code can be driven programmatically (for example `muse serve` or an SDK; UNVERIFIED). Implement the cost tracker and the hard limits from section 7. Open-weights backend stub compiles.
Done when: probe output (or the exact failure) for the API and for Muse Code is pasted in PROGRESS.md, the cost tracker prints cost per call in a test run with the mock backend and one real call, and LIMITS.md records the decision: Muse Code in the loop, or fallback.

**T3 Verifier core with hand-written harnesses.** Sandbox runner, inline injection, steps 4-8, evidence bundle, `reproduce.sh`.
Done when: 3 hand-written gold cases yield PROVEN with bundles, `reproduce.sh` reproduces the verdict, and 1 safe control yields UNPROVEN. Output pasted.

**T4 Extractor.** `syn` span finder for R1 and R2 candidates, schema output.
Done when: unit tests on fixtures find all `unsafe` blocks/impls with correct line spans; `cargo test` output pasted.

**T5 Hypothesis, harness generation, repair loop (max 4).**
Done when: a script prints valid@1 and valid@4 on 8 seeded cases. This is the Gate 1 measurement. Output pasted.

**T6 Reporter and CLI/CI packaging (Impact).** Markdown and HTML report separating PROVEN (with tier and patch badge when present) from UNPROVEN. CLI exit codes: 0 = no PROVEN finding, 1 = at least one PROVEN finding, 2 = tool or usage error. Sample `.github/workflows/warrant.yml` that runs the CLI on a fixture crate. README quickstart. `warrant audit --events <path>` writes the JSONL events defined in section 4 (stage, status, measured elapsed time, model, cost).
Done when: a report is generated from T3/T5 results with PROVEN and UNPROVEN in separate sections; the three exit codes are demonstrated by pasted command output; a sample `events.jsonl` from a real run validates against the section 4 event schema and contains every stage the run executed; the workflow file exists and its commands run locally from a clean clone.

**T7 Mutator and pairs benchmark.** About 16 pairs; `docs/BENCH.md` lists each mutant and the manual bug verification.
Done when: every pair has a safe version that passes under Miri and a mutant that fails under Miri when given a hand-written reproducer.

**T8 Baselines.** Clippy, model without verification, fuzz/proptest under Miri.
Done when: one script outputs baseline results on the pairs set as a table.

**T9 RustSec fixtures and real-world case.** Human selects cases (permissive licenses only); agent wraps and anonymizes, and keeps license text and attribution. Also prepare one non-anonymized historical RustSec reproduction on its original crate version for the demo. `own` fixtures stay local and out of the submitted repo unless the human decides otherwise.
Done when: each fixture compiles on host, a hand-written reproducer fails under Miri (for bug cases), `bench/rustsec_anon/LICENSES/` exists, and the demo reproduction runs end to end via `reproduce.sh`. Output pasted.

**T10 Metrics and model comparison script (Meta tooling).** All metrics from section 8 including intervals, three-run variance, and the head-to-head comparison between Muse Spark and a Meta open-weights model where access exists.
Done when: running it on current results prints the full table with cost per candidate. Then the human declares Gate 2.

**T11 Patch verification** (starts after Gate 2 is declared, because it needs PROVEN findings). Section 5.3. The patch loop uses Muse Code if T2 allowed it, with at most 3 patch rounds per candidate.
Done when: badge computed on all PROVEN pairs; `PATCH_FAILED` cases do not change verdicts. Output pasted.

**T12 Demo mode (Demo; lower priority than the video, because the final round watches video demos).** `warrant demo` with pre-warmed caches (Miri sysroot, vendored deps, warm target dir, cached model outputs for the demo crate). Live progress with real elapsed timers, driven by the same `events.jsonl` events. If the network or API fails, fall back to a clearly labeled `[OFFLINE REPLAY]`.
Done when: `warrant demo` completes from a cold terminal in a measured time (report it; under 30 seconds is a goal, not a claim) and runs with the network disabled in replay mode.

**T13 Freeze and final runs.** Three full benchmark runs, checked against the credit reserve from section 7; commit the results table.
Done when: the results file is committed with toolchain, model versions, and total API spend from the cost tracker.

**T14 Demo video, docs, submission.** README quickstart with a "Prepared before the event" provenance section, a LICENSE file (confirm the required license in the Official Rules), final `docs/LIMITS.md`, a three-minute demo video following `docs/DEMO.md` (the guide's limit; real-world RustSec reproduction, live Miri proof, patch diff if T11 succeeded; only what was built during the hackathon), clean-clone test. Check Official Rules compliance (including AI-assistance and IP terms), no secrets or `bench/own/` files in git history, license attribution intact. Submit at https://cerebralvalley.ai/e/meta-ai-developer-hackathon/hackathon/submit before the deadline (4 Nov, 9 PM ET; after US DST ends on 1 Nov that is about 09:00 WIB on 5 Nov).
Done when: a fresh clone reproduces one finding via `reproduce.sh` (output pasted), the demo video file (three minutes at most) is saved, and the human confirms submission.

## 11. Risk evaluation

**Anti-survivorship bias.** Tools like this often look good on clean functions with primitive arguments and fail on stateful structs, lifetimes, and generics. Author-written bugs drift toward what the tool detects; hence mutator pairs and anonymized historical cases. Historical bugs may be in model training data; hence anonymization. A plain fuzzer can win on simple functions; report it.

**Single point of failure.** The rate at which the model produces a safe, compiling, correctly failing inline harness on stateful and concurrent code. Reinforce first: rule templates, verbatim compiler errors, full-module context, Gate 1 on day 3. Second: Meta API/Muse Code access (T2, open-weights fallback).

**Cut order if time runs short (first to last):** open-weights model comparison, patch verification beyond the demo cases, GitHub Action polish. Never cut: verifier core, paired benchmark, CLI exit codes, the three-minute demo video. Demo mode may shrink to a replay script.

**Pre-mortem (how this fails completely).**

- Scope creep beyond R1/R2: blocked by hard rule 6 and the day-8 freeze.
- Fake proofs: blocked by safe-only harness, signature, location attestation, leak rejection.
- Real code does not compile on host: only extractable modules; document the rest in LIMITS.md.
- Miri message formats change: pinned nightly, signatures recorded in T0.
- Offline sandbox fails: sysroot and vendor baked in T0.
- Agent drifts or claims completion without evidence: rule 5, one task per session, PROGRESS.md evidence.
- Late submission or rule violation: T0 and T14 read the Official Rules; freeze on day 8.
- Judged as a niche verifier with an LLM bolted on: position as a general Rust tool (CLI, CI), Muse Code in the loop with fallback, model comparison, a real-world case.
- Credits exhausted or expired before judging: cost tracker, hard limits, 70% global stop, 30% reserve, and a demo that works from cached outputs.
- Live demo fails: pre-warmed demo mode, labeled replay fallback, backup video.

## 12. Unverified assumptions (verify, do not assume)

- Muse Code behavior: headless mode, authentication, which instruction files it loads, how it handles worktrees and sessions.
- Meta Model API: structured output, rate limits, context size, credits, whether a full module fits in context.
- Miri on the pinned nightly: exact messages, Stacked vs Tree Borrows choice, data race coverage across seeds. Miri's aliasing rules are not official Rust semantics, so results are operational evidence, not mathematical proof.
- That `#![forbid(unsafe_code)]` inside a child module behaves as expected on the pinned toolchain.
- That driver-style memory bugs concentrate in parsing and data-structure logic.
- Official Rules: pre-existing code, licensing, AI-assisted code, prize eligibility.

## 13. Reference examples (bugs are intentional)

R1 target and harness:

```rust
pub struct FrameReader { buf: Vec<u8>, pos: usize }
impl FrameReader {
    pub fn new() -> Self { Self { buf: Vec::new(), pos: 0 } }
    pub fn feed(&mut self, data: &[u8]) { self.buf.extend_from_slice(data); }
    pub fn next_frame(&mut self) -> Option<&[u8]> {
        let len = *self.buf.get(self.pos)? as usize;
        let start = self.pos + 1;
        // BUG: start + len is not checked against buf.len()
        let frame = unsafe { self.buf.get_unchecked(start..start + len) };
        self.pos = start + len;
        Some(frame)
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    use super::*;
    #[test]
    fn proof_r1_next_frame_oob() {
        let mut r = FrameReader::new();
        r.feed(&[200, 1, 2, 3]);
        let _ = r.next_frame();
    }
}
```

Patch idea: `let end = start.checked_add(len)?; let frame = self.buf.get(start..end)?;`. Sanity test: feeding `[2, 7, 8]` gives `Some(&[7, 8])`.

R2 target and harness (the `extern crate std;` line inside the module is what lets a `#![no_std]` parent file use `std::thread`):

```rust
pub struct Ring { head: core::cell::Cell<usize> }
unsafe impl Sync for Ring {}
impl Ring {
    pub fn new() -> Self { Self { head: core::cell::Cell::new(0) } }
    pub fn push_one(&self) { self.head.set(self.head.get() + 1); }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;
    #[test]
    fn proof_r2_ring_sync_race() {
        let r = Ring::new();
        std::thread::scope(|s| {
            s.spawn(|| r.push_one());
            s.spawn(|| r.push_one());
        });
    }
}
```

These examples have not been run yet. T0 runs them and records the real Miri output.
