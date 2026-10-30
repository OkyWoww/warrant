# LIMITS.md

Known limitations, verified signatures, and unverified assumptions for project WARRANT.

## 1. Verified Signature Strings (Pinned nightly: `nightly-2026-10-20`)

Confirmed empirically during Task T0:

| Signature Family | Exact Substring Matched | Verified In Fixture | Status |
|---|---|---|---|
| `ub_oob` / `precondition_panic` | `unsafe precondition(s) violated: slice::get_unchecked requires that the index is within the slice` | `bench/pairs/r1_reference.rs` | **VERIFIED** |
| `ub_oob` (deref) | `error: Undefined Behavior: dereferencing pointer failed:` | Gold fixtures | **VERIFIED** |
| `data_race` | `error: Undefined Behavior: Data race detected between` | `bench/pairs/r2_reference.rs` | **VERIFIED** |

Reject list (never classified as proof):
- Memory leak reports (`memory leaked`)
- Standard assertion/unwrap panics (`explicit panic`, `called \`Option::unwrap()\` on a \`None\` value`, etc.)
- Cargo/rustc compile errors (`error[E...`)
- Execution timeouts

## 2. Invariant & Compiler Behavior Confirmations

- **`#![no_std]` harness compatibility**: A child module `#[cfg(test)] mod warrant_proof` starting with `#![forbid(unsafe_code)]` followed immediately by `extern crate std;` successfully makes standard library threading (`std::thread::scope`) and allocators available without modifying the parent crate's `#![no_std]` declaration.
- **Inner attribute rules**: `#![forbid(unsafe_code)]` is strictly valid as an inner attribute at the module root (`mod warrant_proof { #![forbid(unsafe_code)] ... }`), but invalid in file scope after items.
- **Static Filter**: Deterministic regex filter rejects banned words (`unsafe`, `allow`, `expect`, `panic`, `Box::leak`, etc.) and signature strings in string literals while preserving `#![forbid(unsafe_code)]`.
- **Miri execution scope**: Direct execution of standalone files via `miri.exe` is unsupported by rustup sysroot; all execution must go through `cargo-miri` or containerized runner.

## 3. Verified Meta Model API & Agent Findings (Confirmed in Task T2)

- **Meta Model API Endpoint**: `https://api.meta.ai/v1/chat/completions` (authenticated via `META_MODEL_API_KEY` or `META_API_KEY` Bearer token).
- **Gateway Architecture**: Response header `x-route: model-api-rust` was observed in HTTP responses from the endpoint.
- **Available Models Verified**:
  * `muse-spark-1.3` (Primary target for hypotheses and repair)
  * `muse-spark-1.3-contributor` (Discounted training contributor mode)
  * `muse-spark-1.2` / `muse-spark-1.2-contributor` / `muse-spark-1.1`
  * `muse-wonder-1.0`
  * `sam-3.1`, `muse-voice-transcribe-1.0`, `muse-image-1.0`
- **Response Token Usage Structure**: Verified JSON fields:
  ```json
  "usage": {
    "prompt_tokens": <u32>,
    "completion_tokens": <u32>,
    "total_tokens": <u32>,
    "prompt_tokens_details": { "cached_tokens": <u32> },
    "completion_tokens_details": { "reasoning_tokens": <u32> }
  }
  ```
- **Live Smoke Test Metrics**:
  * HTTP Status: `200 OK`
  * Round-Trip Latency: `1870 ms`
  * Token Count: `14 prompt + 16 completion = 30 total tokens`
  * Measured Call Cost: `$0.000086 USD` recorded in `CostTracker`
- **Muse Code Programmatic Driver Decision**:
  * Status: Local `muse` CLI binary was **NOT FOUND** in host environment.
  * Decision: **FALLBACK ENGAGED** (per SPEC.md Section 7).
  * Implementation: Agentic harness repair and patch loops utilize live HTTP API calls against `muse-spark-1.3` via `MetaClient` (`https://api.meta.ai/v1/chat/completions`), recording real response `usage` token counts and wall-clock latency in `CostTracker`.
  * Assurance: Muse Code is not a single point of failure; product functions deterministically in headless CI and developer workstations.
- **Live `muse-spark-1.3` Benchmark Evaluation Ledger (`--backend meta`)**:
  * **T2 API Smoke Probe (`probe`)**: 1 HTTP call, `16` prompt tokens, `15` completion tokens, latency `1,957 ms`, cost `$0.000086 USD`.
  * **T5/T10 Gate 1 Seeded Synthesis (`eval_gate1 --backend meta`)**: 9 HTTP calls, `6,955` prompt tokens, `36,956` completion tokens, mean latency `15,219 ms`, cost `$0.163977 USD`. Measured `valid@1 = 7/8 (87.5%)`, `valid@4 = 8/8 (100.0%)`, `Proof Rate = 6/8 (75.0%)`, `False Proofs = 0/8 (0.0%)`.
  * **T8 Baselines Unverified LLM (`eval_baselines --backend meta`)**: 32 HTTP calls, `6,909` prompt tokens, `26,958` completion tokens, mean latency `4,235 ms`, cost `$0.123208 USD`. *Unverified LLM mampu mengidentifikasi pola kode mencurigakan (75% recall / 12/16, 0% false alarm pada pairs), namun hanya menghasilkan opini teks (0% Miri-Verified Operational Proofs). Nilai utama WARRANT bukan pada menebak lokasi `unsafe`, melainkan mengubah hipotesis menjadi bukti reproduksi Miri yang deterministik, teratestasi lokasinya, dan memverifikasi patch secara otomatis untuk gerbang CI.*
  * **T11 Full Patch Verification Loop (`eval_patches --backend meta`, 17 targets = 16 mutants + `smallvec_r1_demo`)**: 17 HTTP calls, `26,154` prompt tokens, `42,869` completion tokens, mean latency `11,563 ms`, cost `$0.214093 USD`. Measured `17/17 (100.0%)` `PATCH_VERIFIED` on Round 1/3 across all 4 verification steps (AST signature/anti-bypass guard, exact `warrant_proof::proof_*` under Miri, native `cargo test --lib`, and `warrant_proof::sanity_*` under Miri; for `smallvec_r1_demo`, the patch adds an early-return capacity & index guard preventing OOB pointer write and preserving elements within capacity limits per `sanity_*`). *(Catatan kaki: Run batch 17 target ini dieksekusi sebelum penambahan asersi elemen pada `sanity_smallvec_r1_demo`, sedangkan patch dan `sanity_*` terbaru untuk `smallvec_r1_demo` telah diverifikasi secara live pada capture `warrant demo --backend meta-api` di `docs/DEMO_CACHE.json`, `patch:c-0001`, `$0.017104 USD`)*.
  * **T12 Flagship Demo Live Capture (`warrant demo --backend meta-api` -> `docs/DEMO_CACHE.json`)**: 9 HTTP calls, `8,896` prompt tokens, `37,309` completion tokens, mean latency `23,771 ms`, cost `$0.169685 USD`. Measured `2/2 PROVEN_PUBLIC` + `2/2 PATCH_VERIFIED` (`smallvec` & `r2_01_cell_sync`, where `smallvec` adds early-return capacity & index guard) + `1/1 UNPROVEN` (`r1_01_slice_get_safe`).
  * **T13 Multi-Pass Live Benchmark Synthesis (`final_benchmark_runs`, 2 passes × 22 frozen bug targets = 44 evaluations)**: 57 HTTP calls (`29` in Pass #1, `28` in Pass #2), `46,420` prompt tokens, `232,798` completion tokens, mean latency `15,958 ms`, cost `$1.038898 USD`. Measured per-target from `rounds_used`: Pass #1 `valid@1 = 17/22 (77.27%)`, Pass #2 `valid@1 = 18/22 (81.82%)`; `valid@4 = 22/22 (100.0%)` and `Model-Generated Proof Rate = 22/22 (100.0%)` in both passes.
  * **T13 Live Safe Controls Autonomous Loop (`final_benchmark_runs`, 16 safe controls × up to 4 repair rounds)**: 63 HTTP calls, `56,047` prompt tokens, `358,536` completion tokens, mean latency `37,684 ms`, cost `$1.585954 USD`. Measured `0/16 False Proofs (0.0%)` (Clopper-Pearson 95% CI: `[0.00%, 20.59%]`).
  * **Cumulative Live Project Spend**: **`$3.295901 USD`** across **188 HTTP calls** (`151,397` prompt tokens, `735,441` completion tokens) out of `$150.00` promotional credit (`$146.704099 USD` / `97.80%` remaining, circuit breaker remaining: `$101.704099 USD`).
- **Promotional Credit & Contributor Routing Privacy Note (`muse-spark-1.3-contributor`)**:
  * While free promotional credits are in use on `https://api.meta.ai/v1/chat/completions`, requests for `muse-spark-1.3` resolve to `muse-spark-1.3-contributor`, under which prompts and completions may be used by Meta for product improvement (SPEC.md Section 11, AGENTS.md Hard Rule 14). Only public MIT/Apache-2.0 benchmark fixtures (`bench/pairs/`, `bench/rustsec_anon/`, `bench/real_world_demo/`) were transmitted to the API; `bench/own/` and `.env` were never sent or staged.
- **Open-Weights Backend (`meta-models/muse-spark-open-1026`)**:
  * Explicitly labeled `Stub / Unmeasured (No partner endpoint credits)` in all comparison tables (`eval_metrics_gate2`, `docs/FINAL_RESULTS.json`, and `docs/BENCH.md`); no synthetic or mocked accuracy percentages are claimed for unmeasured backends.

## 4. Operational Proof & Benchmark Scope Limitations (SPEC.md Section 12)

1. **Miri-Verified Operational Proof (Not Formal Verification)**: A `PROVEN` verdict establishes a concrete, deterministic **Miri-Verified Operational Proof** of Undefined Behavior or a concurrency data race on the executed code path and thread schedule under Miri (`-Zmiri-tree-borrows -Zmiri-strict-provenance -Zmiri-preemption-rate=0`). Conversely, `UNPROVEN` or `PATCH_VERIFIED` does not constitute a formal mathematical proof of absence of bugs across all possible inputs.
2. **Historical RustSec Reproductions**: The 5 fixtures in `bench/rustsec_anon/` and 1 fixture in `bench/real_world_demo/smallvec_r1_demo.rs` (`RUSTSEC-2021-0003`, `smallvec >= 0.6.3, < 0.6.14`) are **self-contained host-compilable reproductions adapted from historical RustSec advisory patterns** with permissive MIT/Apache-2.0 attribution retained in `bench/rustsec_anon/LICENSES/`.
3. **Write-Only Target Sanity Limitation (`r2_03_unsync_buffer`)**: `SyncSlotBuffer` in `r2_03_unsync_buffer_mutant.rs` exposes no public getter method (`new` and `write_slot` only), so `sanity_r2_03_unsync_buffer` verifies non-aborting completion on valid and out-of-range writes under Miri without modifying the public struct signature.
4. **Rule Scope**: Only R1 (unchecked in-memory access) and R2 (unsound `Send`/`Sync` impls / data races) are covered. No IRQL/R3, FFI, inline asm, or other UB classes.
5. **Host-Compilable Targets Only**: Targets must compile as a single-crate lib under the pinned nightly with Miri. Code that depends on FFI, platform syscalls Miri does not model, or unavailable external crates cannot be proven and stays `UNPROVEN`.
6. **Schedule Dependence (R2)**: Data-race proofs hold for the explored schedule (`-Zmiri-preemption-rate=0`, scoped threads). A missing race on other schedules does not mean the code is race-free.
7. **Sandbox on Windows Host**: Sandbox Docker (`sandbox/Dockerfile`) disediakan di repositori, namun seluruh evaluasi benchmark dan demo pada fixture publik ini dieksekusi menggunakan `LocalMiriRunner` langsung pada host Windows. Seluruh kode yang dieksekusi merupakan fixture benchmark publik MIT/Apache-2.0.
8. **Small Sample Sizes**: n = 22 bug targets and 16 safe controls. Exact Clopper-Pearson intervals are wide (e.g. 0/16 false proofs → upper bound 20.59%).
9. **Transparansi Prompt Retry/Repair Loop & Ketahanan Terhadap False Proof**:
   Pada loop otonom `repair_harness` (`crates/llm/src/client.rs`), umpan balik yang dikirimkan ke model adalah hasil deterministik dari compiler atau Miri (`"Harness compiled and ran cleanly under Miri (exit code 0) without triggering UB at the target unsafe site. Drive inputs out-of-bounds or concurrently across threads to trigger UB."`).
   Meskipun model diberikan hingga 4 putaran percobaan perbaikan dengan instruksi eksplisit untuk memicu UB, pada sampel 16 kontrol aman ini (setelah koreksi bug re-entrant pada `r2_05_safe`), loop otonom **menghasilkan 0/16 false proofs (0.0%)**.
   *(Catatan Provenance & Kejujuran Kontrol `r2_05_safe`)*: Pada evaluasi live pertama (`hypothesis:live-safe-13-r2_05_safe`, 705 prompt -> 4,827 completion tokens, latency 36,796 ms, cost `$0.021396 USD`), model `muse-spark-1.3` memanggil `write_left` secara konkuren dari 2 thread dan berhasil membuktikan adanya data race nyata (1/16) pada desain awal `r2_05_split_borrow_safe.rs` (karena belum ada guard terhadap pemanggilan re-entrant metode yang sama pada tipe `Sync`). Setelah bug pada fixture kontrol diperbaiki dengan guard klaim atomik (`left_claimed` / `right_claimed: AtomicBool`), evaluasi ulang live (`hypothesis:live-safe-re-r2_05_safe` + 2 `repair`, 3 calls) tidak lagi memicu UB (0/16 False Proofs pasca-koreksi kontrol, sementara keempat panggilan HTTP tetap tercatat utuh di dalam 63 calls ledger; lihat `docs/CHANGES.md`). Vonis `UNPROVEN` pada `r2_05_safe` didasarkan pada verifikasi independen Miri (`test_e2e_r2_05_split_borrow_safe_miri_clean`) yang membuktikan ketiadaan UB/data race pada harness multi-thread setelah penambahan guard klaim atomik.
10. **Early-Return Capacity & Index Guard Semantics (`smallvec_r1_demo`)**:
    Pada patch yang disintesis untuk `bench/real_world_demo/smallvec_r1_demo.rs` (dan diverifikasi di `bench/patch_sanity/smallvec_r1_demo.rs`), model menerapkan *early-return capacity and index guard*:
    ```rust
    let new_len = match self.len.checked_add(num_items) {
        Some(n) => n,
        None => return,
    };
    if new_len > self.data.len() {
        return;
    }
    if index > self.len {
        return;
    }
    ```
    Patch menambahkan early-return guard (`if new_len > self.data.len() { return; }`) yang mencegah OOB pointer write dan lulus `sanity_*` untuk sisipan dalam kapasitas, namun mengabaikan penyisipan yang melebihi kapasitas inline tanpa mengembalikan sinyal error ke pemanggil (perubahan perilaku / batasan semantik `PATCH_VERIFIED`). Patch tidak mengimplementasikan dynamic heap reallocation (`SmallVec::grow`) karena fixture ini merupakan reproduksi self-contained buffer inline; operasi yang melebihi kapasitas inline diabaikan secara awal (*early return*) daripada memicu memory corruption atau Undefined Behavior.

## 5. Remaining Unverified Assumptions (UNVERIFIED)

1. `UNVERIFIED`: Exact credit balance and expiry date on the hackathon Meta API credits ($150 waiver allocation; web dashboard check required).
2. `UNVERIFIED`: Partner inference availability (Baseten/Modal/Fireworks) for open-weights models (`meta-models/muse-spark-open-1026`).
3. `UNVERIFIED`: Hackathon Official Rules confirmation regarding pre-event utility scripts and exact submission guidelines.

