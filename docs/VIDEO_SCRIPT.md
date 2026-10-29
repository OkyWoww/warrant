# WARRANT — 3-Minute Demo Video Script (target 2:50)

Storyboard follows `docs/DEMO.md`. Every number on screen comes from `docs/FINAL_RESULTS.json` or `docs/DEMO_CACHE.json`. Do not round up or invent numbers.

## Recording setup

- Terminal: dark theme, font ≥ 18pt, window 120×35. Run from the repo root.
- Pre-build once before recording: `cargo build --release -p warrant-cli`
- Pinned toolchain: `nightly-2026-10-20` with `miri` and `rust-src`.
- Demo uses `--offline` (replay from `docs/DEMO_CACHE.json`, \$0 spend, labeled `[OFFLINE REPLAY]`). Say this out loud on camera.

---

## 0:00 – 0:30 · Hook & Stakes

**Screen:** `bench/real_world_demo/smallvec_r1_demo.rs`, highlight the `ptr::copy` in `insert_many`.

**Voice-over:**
> "This is a self-contained reproduction of RUSTSEC-2021-0003, a buffer overflow pattern from smallvec. It compiles. Clippy is quiet. An LLM can *guess* it's buggy — but a guess is not a finding. WARRANT only reports a bug when a 100% safe-Rust harness makes Miri fail at the exact unsafe line."

**Terminal (optional, 5 s):**
```bash
cargo clippy --manifest-path <scratch crate> -- -D warnings   # no warning on the UB
```

## 0:30 – 1:30 · Act 1 — The Proof

**Terminal:**
```bash
./target/release/warrant demo --offline --events target/warrant/demo/events.jsonl --output-dir target/warrant/demo
```

**Show:**
1. Stage chips going green: `extract → hypothesis → harness → compile → miri_red → signature → location → tier`.
2. Open the generated harness: first lines are `#![forbid(unsafe_code)]` and `extern crate std;`.
3. Miri output: `Undefined Behavior: in-bounds pointer arithmetic failed` pointing at the `ptr.add(...)` line → verdict `PROVEN_PUBLIC`.
4. Case 3 (safe control) ends `UNPROVEN` — "held back, never reported without proof".

**Voice-over:**
> "The model proposes, Miri disposes. The harness is forbidden from using unsafe, and the verdict is attested to the exact source line."

**Independent check (10 s):**
```bash
reproduce/real_world_rustsec/reproduce.sh     # Windows: reproduce\real_world_rustsec\reproduce.bat
```
> "The evidence bundle reproduces without WARRANT installed."

## 1:30 – 2:00 · Act 2 — Verified Patch

**Show:**
1. AST anti-bypass guard rejecting the three adversarial patches (from `eval_patches` output): removed public fn, `#[cfg(not(miri))]`, `todo!()` → `REJECTED`.
2. The diff of the accepted patch (bounds check before `ptr::copy`).
3. `proof_*` now clean under Miri, `sanity_*` clean under Miri → `PATCH_VERIFIED`.

**Voice-over:**
> "A patch is only accepted if the proof harness goes green, native tests pass, and behavior on valid inputs is unchanged under Miri. Patches that cheat are rejected before they run."

## 2:00 – 2:20 · Act 3 — It Stays Fixed

**Terminal:**
```bash
./target/release/warrant audit bench/pairs/r1_reference.rs --backend mock --output-dir evidence ; echo "exit=$?"        # exit=1
./target/release/warrant audit bench/pairs/r1_reference_safe.rs --backend mock --output-dir evidence ; echo "exit=$?"   # exit=0
```
> "Exit 1 on a proven bug, exit 0 when clean — drop it into CI with `.github/workflows/warrant.yml`."

## 2:20 – 2:45 · Benchmark & Meta Tooling

**Screen:** `docs/BENCH.md` sections 4 and 8 (or the table in `README.md`).

| Shown on screen | Value |
| :--- | :--- |
| Clippy / Fuzz-under-Miri / Unverified LLM / WARRANT bugs found (16 pairs) | 0/16 · 1/16 · 12/16 (0 proofs) · **16/16** |
| False proofs on safe controls | **0/16** |
| Live `muse-spark-1.3` valid@1 → valid@4 | **79.54% → 100%** |
| PATCH_VERIFIED | **17/17** |
| Total Meta API spend | **\$1.61** across **121** live calls |

> "Built on Meta's muse-spark-1.3. Every token and every cent comes from the API's own usage field."

## 2:45 – 3:00 · End Card

- Tagline: **"No proof, no finding."**
- Repo URL: `<public GitHub URL — fill in before recording>`
- "Dual-licensed MIT / Apache-2.0. RustSec reproductions keep their original MIT/Apache-2.0 attribution."

---

## Honesty checklist before publishing

- [ ] `[OFFLINE REPLAY]` label visible when using `--offline`.
- [ ] Numbers match `docs/FINAL_RESULTS.json` exactly.
- [ ] Say "Miri-verified operational proof", never "mathematically proven".
- [ ] No `.env`, API key, or `bench/own/` content on screen.

