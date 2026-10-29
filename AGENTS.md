# AGENTS.md

WARRANT is a proof-first auditor for Rust `unsafe` soundness. A finding exists only if a safe-Rust harness fails under Miri at the exact `unsafe` site.

Source of truth: `docs/SPEC.md`. Read it before any task. If this file and SPEC.md disagree, SPEC.md wins: stop and report the conflict, do not guess.

## Session protocol

1. Read `docs/SPEC.md` and `docs/PROGRESS.md` (create PROGRESS.md in task T0 if missing).
2. Do exactly ONE task from SPEC.md section 10 per session. Do not start the next task.
3. Order inside a task: Red (write the failing test or check) -> Green (make it pass) -> verify.
4. Before ending, append to `docs/PROGRESS.md`: task id, evidence (pasted command output), cumulative API cost from the cost tracker, what is next, blockers.

## Hard rules

1. Never report a bug without a failing harness that the verifier accepted. Unproven ideas are labeled `UNPROVEN`.
2. Generated harnesses are 100% safe Rust and follow SPEC.md section 5.2 steps 1 and 3: appended at the END of a copy of the target file (line numbers must not shift), module starts with `#![forbid(unsafe_code)]` then `extern crate std;`, and none of the banned words (`unsafe`, `allow`, `expect`, `Box::leak`, `mem::forget`, `std::process`, and the others listed there) appear. Never weaken the filter to get a harness through.
3. Deterministic stages (extractor, verifier, reporter) contain no LLM calls. LLM output is never trusted without verification.
4. Run generated or target code only inside the sandbox (`sandbox/run_in_sandbox.sh`, no network, read-only rootfs). Never execute target crates on the host.
5. A task is done only when real command output is pasted into PROGRESS.md: `cargo check --workspace` and `cargo test --workspace` at minimum, plus the task's own "Done when" command. A narrative summary is not evidence. If you cannot run a command, say so and mark the task NOT DONE.
6. Scope is fixed: rules R1 and R2 only. No IRQL/R3, no image models, no new rules, no new crates beyond SPEC.md section 9.
7. Do not invent facts about Meta APIs or Miri behavior. Record unknowns in `docs/LIMITS.md` as `UNVERIFIED` and ask the human.
8. Do not edit `docs/SPEC.md`. Propose changes in `docs/CHANGES.md` as `SPEC CHANGE: <what and why>` for the human to approve.
9. After Gate 2 (benchmark freeze), never change benchmark labels or the metrics definition.
10. Never commit secrets or API keys. Everything submitted is non-confidential and licensed to the organizers. Do not copy private code into the repo. NEVER stage or commit anything from `bench/own/`.
11. Every model call goes through the cost tracker. Respect the limits in SPEC.md section 7; when the global budget stop is reached, stop and report.
12. Third-party fixtures: permissive licenses only, keep license text and attribution. Never publish or describe an unreported vulnerability in a public crate; the human decides disclosure.
13. Provenance: commit with real timestamps; never backdate or rewrite history to hide when work was done. List every file that existed before kickoff (26 Oct 2026, 12:00 PM ET) in the README under "Prepared before the event". Do not copy code from pre-existing projects into this repo; the human decides what is allowed.
14. Privacy: while free credits are in use, prompts may be used by Meta for product improvement (Billing page). Never send private or non-public code, secrets, or anything from `bench/own/` to a model API.

## Commands

- Build and test: `cargo check --workspace`, `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`
- Miri (sandbox only), harness only: `sandbox/run_in_sandbox.sh cargo +$WARRANT_NIGHTLY miri test --lib warrant_proof::proof_`. Never run the full crate test suite under Miri to reach a verdict.
- Miri (sandbox only), patch sanity check: `sandbox/run_in_sandbox.sh cargo +$WARRANT_NIGHTLY miri test --lib warrant_proof::sanity_`
- Pinned nightly: `WARRANT_NIGHTLY=<fill in task T0>`
- Language: Rust for all crates. Prefer small, explicit code. No macros unless needed.

## Style

- Error handling with `Result` and explicit error types; no `unwrap` in non-test code.
- Every module that parses model output validates it against the JSON schema in SPEC.md section 4.
- Commit small and often, message format: `T<id>: <what>`.
