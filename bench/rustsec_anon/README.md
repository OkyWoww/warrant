# RustSec Anonymized Benchmark Corpus

This directory contains anonymized, minimized reproductions derived from real-world historical RustSec advisories (e.g., RUSTSEC-2020-0071, RUSTSEC-2021-0119, RUSTSEC-2022-0048).

Each benchmark fixture contains:
1. Target minimal crate module exhibiting the unsound API design.
2. Verified safe harness adhering to `#![forbid(unsafe_code)]` + `extern crate std;`.
3. Expected Miri UB classification (e.g. `aliasing_violation`, `uninit_memory`, `use_after_free`).
