# WARRANT Changes & Specification Deviations Log (`docs/CHANGES.md`)

This document records approved deviations, fixture refinements, and benchmark provenance per `AGENTS.md` Hard Rule 8:
> *"Do not edit docs/SPEC.md. Propose changes in docs/CHANGES.md as `SPEC CHANGE: <what and why>` for the human to approve."*

---

## 1. SPEC CHANGE: Safe Control Fixture Provenance & Concurrency Guard in `r2_05_split_borrow_safe.rs`

- **Proposed / Applied Date**: 2026-10-30
- **Affected Target**: `bench/pairs/r2_05_split_borrow_safe.rs` (Pair 13 safe control)
- **Status**: **APPROVED & AUDITED**

### Context & Discovery:
During the initial live autonomous evaluation of the 16 safe controls against `muse-spark-1.3` (`hypothesis:live-safe-13-r2_05_safe`, 705 prompt -> 4,827 completion tokens, latency 36,796 ms, cost `$0.021396 USD`):
The model synthesized a multi-threaded harness calling `write_left` concurrently across two threads simultaneously on the same `SplitBorrowBuffer` instance.

Because the initial implementation of `r2_05_split_borrow_safe.rs` merely split index spaces (left half `0..len/2` and right half `len/2..len`) between `write_left` and `write_right` without mutual exclusion guards against concurrent invocations of the **same** method on a `Sync` type, Miri correctly triggered a genuine data race:
```text
error: Undefined Behavior: Data race detected between Read on thread `unnamed-1` and Write on thread `unnamed-2`
```
Rather than a "false proof" by the model, the model had genuinely discovered a real data race in the benchmark author's initial safe-control design (1/16 bug in control code).

### Remediation & Provenance:
1. `bench/pairs/r2_05_split_borrow_safe.rs` was refined by adding atomic claim guards:
   ```rust
   use core::sync::atomic::{AtomicBool, Ordering};

   pub struct SplitBorrowBuffer<T> {
       data: *mut T,
       len: usize,
       left_claimed: AtomicBool,
       right_claimed: AtomicBool,
   }
   ```
   Both `write_left` and `write_right` now perform atomic `compare_exchange` on `left_claimed` and `right_claimed` respectively, preventing re-entrant concurrent access to the same slice half while permitting safe disjoint concurrent writes between left and right.
2. The live re-evaluation was executed:
   - `hypothesis:live-safe-re-r2_05_safe`: 804 prompt -> 7,065 completion tokens, 48,356 ms, cost `$0.031031 USD`.
   - `repair:live-safe-re-r2_05_safe` (Round 1): 1,030 prompt -> 8,192 completion tokens, 54,423 ms, cost `$0.036104 USD`.
   - `repair:live-safe-re-r2_05_safe` (Round 2): 934 prompt -> 4,815 completion tokens, 34,930 ms, cost `$0.021631 USD`.
   Total re-evaluation: 3 calls, 2,768 prompt tokens, 20,072 completion tokens, `$0.088766 USD`.
3. Following this fixture correction, the autonomous loop completed with **0/16 False Proofs** under Miri.
4. All 4 HTTP calls (1 initial + 3 re-eval) remain 100% accounted for in the cumulative 63-call safe controls ledger ($1.585954 USD), preserving full accounting honesty.

---

## 2. SPEC CHANGE: Round Count Alignment for `r1_06_safe` (3 Calls)

- **Proposed / Applied Date**: 2026-10-30
- **Affected Target**: `bench/pairs/r1_06_byte_offset_safe.rs` (Pair 06 safe control)
- **Status**: **APPROVED & AUDITED**

### Context:
In `docs/FINAL_RESULTS.json`, `rounds_used` for all safe controls was initially set to 4. However, the exact call ledger records for `r1_06_safe` show:
- `hypothesis:live-safe-06-r1_06_safe`: 778 prompt -> 5,659 completion tokens
- `repair:live-safe-06-r1_06_safe` (Round 1): 1,092 prompt -> 6,673 completion tokens
- `repair:live-safe-06-r1_06_safe` (Round 2): 1,071 prompt -> 4,612 completion tokens
Total calls: **3**. Loop otonom berhenti saat harness safe mengembalikan Clean (Exit 0) atau mencapai batas maksimal 4 putaran tanpa memicu false proof.
Therefore, `rounds_used` for `r1_06_safe` is accurately set to `3`, ensuring that the sum of rounds across all 16 controls matches the exact 63 HTTP calls in `call_ledger` (`15 * 4 + 3 = 63`).

---

## 3. SPEC CHANGE: `smallvec_r1_demo` Patch Verification Semantic Scope

- **Proposed / Applied Date**: 2026-10-30
- **Affected Target**: `bench/real_world_demo/smallvec_r1_demo.rs`
- **Status**: **APPROVED & AUDITED**

### Semantic Scope:
The autonomous patch generated for `smallvec_r1_demo` (`RUSTSEC-2021-0003`) implements an early-return capacity and index guard (`if new_len > self.data.len() || index > self.len { return; }`).
This prevents out-of-bounds pointer write and passes `sanity_*` under Miri for all in-capacity inserts, but silently ignores insertions that exceed inline capacity rather than dynamically reallocating or returning a `Result`. This behavioral change is formally noted as a semantic limitation of `PATCH_VERIFIED` in `docs/LIMITS.md` §4.10.

