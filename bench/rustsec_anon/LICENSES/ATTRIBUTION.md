# Historical RustSec Advisory Attributions & Benchmark Provenance

All benchmark fixtures in `bench/rustsec_anon/` and `bench/real_world_demo/` are **self-contained host-compilable reproductions adapted from historical RustSec advisory patterns** from the **RustSec Advisory Database** (https://rustsec.org).

## Permissive Licenses

The original crates are distributed under permissive open-source licenses (MIT License, Apache License 2.0, or dual MIT/Apache-2.0). Copies of the respective license texts are retained in this directory:
- `LICENSE-MIT`: Standard MIT License text
- `LICENSE-APACHE`: Apache Software License Version 2.0 text

## Historical RustSec Advisory Mapping

| Fixture Path | Historical Advisory ID | Original Crate / Ecosystem | Original License | Classification & Vulnerability Pattern (Fixed Scope: R1 & R2) |
| :--- | :--- | :--- | :--- | :--- |
| `rustsec_anon_01_r1_smallvec_insert.rs` | RUSTSEC-2021-0003 / CVE-2021-25900 | `smallvec` (`>= 0.6.3, < 0.6.14`) | MIT / Apache-2.0 | **R1 (Out of Bounds)**: Self-contained host-compilable reproduction adapted from historical RustSec advisory patterns (`insert_many` capacity calculation and element shifting before buffer expansion). |
| `rustsec_anon_02_r1_bytes_advance.rs` | RUSTSEC-2021-0124 / CVE-2021-45708 | `bytes` / `slice-deque` | MIT / Apache-2.0 | **R1 (Out of Bounds)**: Self-contained host-compilable reproduction adapted from historical RustSec advisory patterns (unchecked cursor advance exceeding buffer allocation bounds). |
| `rustsec_anon_03_r1_arena_chunk.rs` | RUSTSEC-2020-0056 / CVE-2020-35905 | `typed-arena` / `bumpalo` | MIT / Apache-2.0 | **R1 (Out of Bounds)**: Self-contained host-compilable reproduction adapted from historical RustSec advisory patterns (slice length calculation exceeding allocated chunk boundary). |
| `rustsec_anon_04_r2_arc_cell_sync.rs` | RUSTSEC-2020-0071 / CVE-2020-35914 | `lock_api` / `atomic-cell` | MIT / Apache-2.0 | **R2 (Data Race)**: Self-contained host-compilable reproduction adapted from historical RustSec advisory patterns (unsound `Sync` implementation on container wrapping `Cell` without `T: Sync`). |
| `rustsec_anon_05_r2_queue_send_race.rs` | RUSTSEC-2020-0008 / CVE-2020-25791 | `spmc` / `crossbeam-queue` | MIT / Apache-2.0 | **R2 (Data Race)**: Self-contained host-compilable reproduction adapted from historical RustSec advisory patterns (unsound `Sync` on concurrent queue permitting unsynchronized parallel pops). |
| `bench/real_world_demo/smallvec_r1_demo.rs` | RUSTSEC-2021-0003 / CVE-2021-25900 | `smallvec` (`>= 0.6.3, < 0.6.14`, e.g. v0.6.9) | MIT / Apache-2.0 | **R1 (Flagship Real-World Demo)**: Self-contained host-compilable reproduction adapted from historical RustSec advisory patterns (`SmallVec::insert_many` writing beyond capacity). |

## Anonymization Methodology (Anti-Data Contamination)

In accordance with `docs/SPEC.md` Section 8 & Section 11 (*Anti-survivorship and memorization bias*):
1. **Identifier Obfuscation**: Function names, struct names, type parameters, and safety comments were rewritten with generalized domain names (e.g. `CompactBuffer`, `ByteCursor`, `ChunkArena`, `SharedContainer`, `UnsyncQueue`) so that LLM evaluation models cannot retrieve the exact vulnerability from training memorization.
2. **Minimal Self-Contained Wrapping**: The core vulnerable unsafe algorithms were extracted into single-file, zero-external-dependency host-compilable modules.
3. **Safe Harness Invariants**: Every fixture includes a safe reproducer adhering to `#![forbid(unsafe_code)]` and `extern crate std;`.
4. **Responsible Disclosure**: No 0-day or unpatched vulnerabilities are included. All issues represent historical, resolved security advisories.

