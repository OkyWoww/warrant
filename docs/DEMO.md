# WARRANT demo video plan

Status: plan only. Nothing in this file may be shown in the video until the real data behind it exists (see "Source of truth"). Numbers in angle brackets are placeholders, never claims.

Constraints (from the participant guide): three minutes at most, shows only what was built during the hackathon, judged on a "working, impressive demo" and whether the video is "genuinely cool to watch". The final round judges watch the video, so the video is the primary artifact. Target length: 2:50.

Reference style (structure only; do not copy designs, branding, or wording): a one-line hook, a short stakes slide, three acts, real captured terminal output with an explicit label, a step bar that highlights the current pipeline stage, tool-call captions, honest speed labels, a benchmark comparison card, an evidence certificate, a tagline end card.

## 1. Storyboard

| Time | Scene | On screen | Narration beat | Source of truth |
|---|---|---|---|---|
| 0:00-0:12 | Hook | The real-world case on its original crate version; the `unsafe` line highlighted; text "This code looks safe."; then the Miri failure | Unsafe Rust can look correct and still be unsound. | `reproduce.sh` log of the real-world case (T9) |
| 0:12-0:30 | Stakes | Advisory title and id with attribution; one line on what an unverified LLM reviewer does (it guesses) | Why this matters, and why guessing is not enough. | The public advisory (paraphrased, cited); baseline result from T8 |
| 0:30-0:50 | How it works | One-screen pipeline with the step chips; roles named: Muse Spark proposes, Muse Code writes and repairs, Miri decides | The model proposes. The compiler decides. | `docs/SPEC.md` section 5; model names as confirmed in T2 |
| 0:50-1:30 | Act 1: What just broke | Real `warrant audit` run; chips light up from events; the safe harness (`forbid(unsafe_code)`); Miri failing at the exact file:line; verdict PROVEN with its tier | Narrate each stage in one short sentence. | `events.jsonl` and the evidence bundle from one real run |
| 1:30-2:00 | Act 2: The fix | Muse Code patch rounds, the diff, the sanity test, PATCH_VERIFIED, then the evidence certificate | The patch is accepted only if the same harness passes and a sanity test still passes. | Evidence bundle (T11), if T11 succeeded; otherwise skip this act and say nothing about patching |
| 2:00-2:20 | Act 3: It stays fixed | CLI exit code 1, then 0 after the fix; the CI gate run | A CI gate with no model in it. | T6 outputs, workflow run |
| 2:20-2:45 | Benchmark | Comparison card: WARRANT vs Clippy vs unverified model vs fuzz/proptest, measured numbers with intervals; one UNPROVEN shown honestly; model comparison if T10 has it | What it finds, what it misses, what it costs. | T10 results table (after Gate 2) |
| 2:45-3:00 | End card | Tagline, public repo URL, "Built during the hackathon", third-party code and licenses | | README provenance section |

Cut order if the video runs long: benchmark detail, Act 3, the stakes slide. Never cut Act 1.

## 2. Make the product recordable: events.jsonl & Ledger-Driven Captions

`warrant audit --events <path>` and `warrant demo --events <path>` write one JSON object per line (schema in `docs/SPEC.md` section 4). The step chips, tool-call captions, elapsed timers, cost counters, and speed labels in the video MUST be generated programmatically from `events.jsonl` (and `docs/DEMO_CACHE.json`), not drawn by hand, typed, or hardcoded.

Every on-screen model call caption is derived directly from the `CallRecord` ledger fields (`prompt_tokens -> completion_tokens`, `latency_ms`, `cost_usd`, and model `muse-spark-1.3-contributor`). Real measured call records from `docs/DEMO_CACHE.json` (captured at `2026-10-30T02:27:22Z`, 9 calls, `8,896` prompt tokens, `37,309` completion tokens, `$0.169685 USD`):
- `c-0001` (flagship SmallVec `RUSTSEC-2021-0003`, 4 calls, 4,832 prompt, 14,941 completion, $0.069540 USD):
  - `hypothesis:c-0001`: `1,056 prompt -> 3,090 completion tokens`, `19,262 ms`, `$0.014453 USD`, model `muse-spark-1.3`.
  - `repair:c-0001` (round 1): `1,304 prompt -> 3,696 completion tokens`, `19,844 ms`, `$0.017338 USD`, model `muse-spark-1.3`.
  - `repair:c-0001` (round 2): `1,291 prompt -> 4,478 completion tokens`, `29,177 ms`, `$0.020645 USD`, model `muse-spark-1.3`.
  - `patch:c-0001` (round 1): `1,181 prompt -> 3,677 completion tokens`, `22,061 ms`, `$0.017104 USD`, model `muse-spark-1.3`.
- `c-0002` (concurrency `r2_01_cell_sync_mutant`, 4 calls, 3,313 prompt, 19,031 completion, $0.085024 USD):
  - `hypothesis:c-0002`: `675 prompt -> 4,415 completion tokens`, `24,093 ms`, `$0.019608 USD`, model `muse-spark-1.3`.
  - `repair:c-0002` (round 1): `917 prompt -> 5,774 completion tokens`, `28,960 ms`, `$0.025686 USD`, model `muse-spark-1.3`.
  - `repair:c-0002` (round 2): `914 prompt -> 7,157 completion tokens`, `44,049 ms`, `$0.031560 USD`, model `muse-spark-1.3`.
  - `patch:c-0002` (round 1): `807 prompt -> 1,685 completion tokens`, `10,258 ms`, `$0.008170 USD`, model `muse-spark-1.3`.
- `c-0003` (sound safe control `r1_01_slice_get_safe`, 1 call, 751 prompt, 3,337 completion, $0.015121 USD):
  - `hypothesis:c-0003`: `751 prompt -> 3,337 completion tokens`, `16,238 ms`, `$0.015121 USD`, model `muse-spark-1.3` (held back as `UNPROVEN`).

Reference video style is followed for structure only; never copy branding, trademarks, or visual assets of reference materials.

Stage names for the chips (same as the event `stage` field): `extract`, `hypothesis`, `harness`, `compile`, `miri_red`, `signature`, `location`, `tier`, `patch`, `sanity`, `evidence`. A final chip `ci_gate` is shown for the CLI exit-code scene.

## 3. Honesty and label rules

1. Real output carries the label `REAL OUTPUT - captured on <OS>, <date>`. Never show typed or mocked output under that label.
2. If a recording is sped up, show `<N> s real, shown faster` on screen, using the elapsed time from `events.jsonl`.
3. If a scene is a cached replay, label it `[OFFLINE REPLAY]`.
4. Seeded benchmark cases are labeled `seeded`. Real-world cases are labeled with their source and license.
5. Every number on screen comes from a results file committed in the repo. No rounding up, no hand-picked runs.
6. Show one UNPROVEN finding and say what it means.
7. **Sandbox Honesty**: Sandbox Docker (`sandbox/Dockerfile`) disediakan di repositori, namun seluruh evaluasi benchmark dan demo pada fixture publik ini dieksekusi menggunakan `LocalMiriRunner` langsung pada host Windows.

## 4. Must not appear in the video or repo

- Files or tools that existed before kickoff, unless listed in the README under "Prepared before the event".
- Private code, `bench/own/` content, secrets, API keys, card or account pages.
- An unreported vulnerability in a public crate.
- Other people's branding, designs, or assets without the right to use them.
- Any claim about a feature that did not work in the recorded run.

## 5. Production notes

- Source of truth for every scene is a file in the repo (`reproduce/`, evidence bundles, the results table, `events.jsonl`).
- Record the terminal at 1920x1080. Keep raw recordings and logs.
- Overlays (chips, captions, cards) are rendered from the data files, for example with a small Python and ffmpeg script. This pipeline is not built yet.
- Narration: record your own voice or use text-to-speech. Write the script after the numbers are final.
- Subtitles on, because judges may watch muted.

## 6. Before recording

- [ ] Gate 2 declared and benchmark frozen; results table committed.
- [ ] Two real-world candidates prepared (a primary and a backup); the primary reproduces from a clean clone.
- [ ] `events.jsonl` produced by a real run and validated against the schema.
- [ ] Label audit done (section 3).
- [ ] Provenance audit done (section 4).
- [ ] Total length at most 2:55, checked by playing the exported file.
- [ ] Backup recording saved in case the primary export fails.

## 7. Risks

**Anti-survivorship bias.** Polished demos usually show only the curated success. Judges have seen many; the benchmark card, the UNPROVEN item, and the speed labels are what make this one credible.

**Single point of failure.** The demo depends on one real-world case that reproduces cleanly. Reinforce first: prepare a second candidate and record both early.

**Pre-mortem.** The video fails if it runs over three minutes, shows pre-event material, or is recorded in the last hour before the deadline. Mitigations: lock this storyboard, measure length, run the provenance audit, and schedule recording at least one day before the deadline (4 Nov, 9 PM ET, about 09:00 WIB on 5 Nov).

## 8. Optional Future Model Version Exploration (Post-Freeze)

If time permits after all priority audit closure and recording freeze steps are verified, an optional evaluation comparing `muse-spark-1.3` against `muse-spark-1.2` can be conducted on the 8 Gate-1 candidates to measure version-over-version repair iteration efficiency and latency differences.
