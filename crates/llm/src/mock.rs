//! # warrant-llm mock
//!
//! Deterministic mock LLM client and AgentBackend providing schema-compliant
//! canned hypotheses and repairs. Tailored for zero-network testing and Task T1/T5 validation.

use async_trait::async_trait;
use uuid::Uuid;
use crate::client::{AgentBackend, LlmClient, LlmError};
use crate::schema::{
    Candidate, CandidateHarness, ExtractionTarget, Finding, Hypothesis, UbClass,
    VerificationFeedback, REQUIRED_HARNESS_HEADER,
};

/// Deterministic mock LLM client providing schema-compliant canned hypotheses.
#[derive(Debug, Clone)]
pub struct MockLlmClient {
    pub mock_r1_harness: String,
    pub mock_r2_harness: String,
    pub cached_patches: std::collections::BTreeMap<String, String>,
}

/// Type alias for ergonomic client usage
pub type MockClient = MockLlmClient;

impl Default for MockLlmClient {
    fn default() -> Self {
        Self {
            mock_r1_harness: format!(
                r#"{}
#[cfg(test)]
mod warrant_proof {{
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_next_frame_oob() {{
        let buffer = FastBuffer::new(vec![10, 20, 30, 40]);
        let val = buffer.read_unchecked_elem(8);
        std::hint::black_box(val);
    }}
}}
"#,
                REQUIRED_HARNESS_HEADER.trim_end()
            ),
            mock_r2_harness: format!(
                r#"{}
#[cfg(test)]
mod warrant_proof {{
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r2_sync_data_race() {{
        let counter = SharedCounter::new(0);
        std::thread::scope(|s| {{
            s.spawn(|| {{
                for _ in 0..100 {{
                    counter.increment();
                }}
            }});
            s.spawn(|| {{
                for _ in 0..100 {{
                    counter.increment();
                }}
            }});
        }});
        std::hint::black_box(counter.get());
    }}
}}
"#,
                REQUIRED_HARNESS_HEADER.trim_end()
            ),
            cached_patches: std::collections::BTreeMap::new(),
        }
    }
}

impl MockLlmClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cached_patches(
        mut self,
        patches: std::collections::BTreeMap<String, String>,
    ) -> Self {
        self.cached_patches = patches;
        self
    }
}

#[async_trait]
impl LlmClient for MockLlmClient {
    async fn propose_hypothesis(&self, candidate: &Candidate) -> Result<Hypothesis, LlmError> {
        let code = &candidate.code_snippet;
        let file = &candidate.file_path;
        let enclosing = candidate.enclosing_item.as_deref().unwrap_or("");

        // Case 0: Flagship RustSec SmallVec insert_many (smallvec_r1_demo.rs, RUSTSEC-2021-0003)
        if file.contains("smallvec") || enclosing.contains("SmallVec") || code.contains("insert_many") {
            let harness = r#"#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_smallvec_insert_many_oob() {
        let mut sv: SmallVec<u64> = SmallVec::new();
        sv.push(111);
        sv.push(222);
        sv.insert_many(1, &[333, 444, 555, 666, 777, 888]);
        std::hint::black_box(sv.get(0));
    }
}"#
            .to_string();
            let hypothesis = Hypothesis {
                id: format!("hyp-cached-{}", Uuid::new_v4().simple()),
                candidate_id: candidate.id.clone(),
                ub_class: UbClass::UbOob,
                explanation: "Calling SmallVec::insert_many shifts inline elements via ptr::copy without verifying that self.len + elements.len() <= 2 (RUSTSEC-2021-0003).".to_string(),
                harness_code: harness,
                suggested_miri_flags: vec![
                    "-Zmiri-tree-borrows".to_string(),
                    "-Zmiri-strict-provenance".to_string(),
                ],
                expected_trigger: "in-bounds pointer arithmetic failed".to_string(),
            };
            hypothesis.validate()?;
            return Ok(hypothesis);
        }

        // Case 1a: SliceBuffer read_at_offset (r1_01_slice_get_mutant.rs / r1_01_slice_get_safe.rs)
        if file.contains("r1_01") || enclosing.contains("SliceBuffer") || code.contains("SliceBuffer") {
            let harness = r#"#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_01_slice_get() {
        let mut buf = SliceBuffer::new();
        buf.push_frame(&[10, 20, 30, 40]);
        buf.advance(2);
        let val = buf.read_at_offset(6);
        std::hint::black_box(val);
    }
}"#
            .to_string();
            let hypothesis = Hypothesis {
                id: format!("hyp-cached-{}", Uuid::new_v4().simple()),
                candidate_id: candidate.id.clone(),
                ub_class: UbClass::UbOob,
                explanation: "Advancing SliceBuffer cursor and calling read_at_offset(6) tests whether idx = cursor + offset is bounds-checked before slice::get_unchecked(idx).".to_string(),
                harness_code: harness,
                suggested_miri_flags: vec!["-Zmiri-tree-borrows".to_string()],
                expected_trigger: "unsafe precondition(s) violated: slice::get_unchecked".to_string(),
            };
            hypothesis.validate()?;
            return Ok(hypothesis);
        }

        // Case 1b: TelemetryCounter Cell Sync (r2_01_cell_sync_mutant.rs / r2_01_cell_sync_safe.rs)
        if file.contains("r2_01") || enclosing.contains("TelemetryCounter") || code.contains("TelemetryCounter") {
            let harness = r#"#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r2_01_cell_sync() {
        let counter = TelemetryCounter::new(0);
        std::thread::scope(|s| {
            s.spawn(|| {
                for _ in 0..10 {
                    counter.increment();
                }
            });
            s.spawn(|| {
                for _ in 0..10 {
                    counter.increment();
                }
            });
        });
        std::hint::black_box(counter.get());
    }
}"#
            .to_string();
            let hypothesis = Hypothesis {
                id: format!("hyp-cached-{}", Uuid::new_v4().simple()),
                candidate_id: candidate.id.clone(),
                ub_class: UbClass::DataRace,
                explanation: "Unsound `unsafe impl Sync for TelemetryCounter` exposes non-atomic `Cell<usize>` to concurrent unsynchronized thread mutation.".to_string(),
                harness_code: harness,
                suggested_miri_flags: vec!["-Zmiri-preemption-rate=0".to_string()],
                expected_trigger: "Data race detected between".to_string(),
            };
            hypothesis.validate()?;
            return Ok(hypothesis);
        }

        // Case 1: FastBuffer get_unchecked (r1_reference.rs)
        if file.contains("r1_reference") || enclosing.contains("FastBuffer") {
            let hypothesis = Hypothesis {
                id: format!("hyp-mock-{}", Uuid::new_v4().simple()),
                candidate_id: candidate.id.clone(),
                ub_class: UbClass::UbOob,
                explanation: "Caller passes index exceeding allocated slice length, dereferencing invalid memory via get_unchecked."
                    .to_string(),
                harness_code: self.mock_r1_harness.clone(),
                suggested_miri_flags: vec!["-Zmiri-tree-borrows".to_string()],
                expected_trigger: "unsafe precondition(s) violated: slice::get_unchecked".to_string(),
            };
            hypothesis.validate()?;
            return Ok(hypothesis);
        }

        // Case 2: RawBuffer pointer add (r1_ptr_deref.rs)
        if file.contains("r1_ptr_deref") || enclosing.contains("RawBuffer") {
            let harness = format!(
                r#"{}
#[cfg(test)]
mod warrant_proof {{
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_raw_ptr_oob() {{
        let data = vec![10u8, 20, 30, 40];
        let buf = RawBuffer::new(&data);
        let val = buf.read_offset(100);
        std::hint::black_box(val);
    }}
}}
"#,
                REQUIRED_HARNESS_HEADER.trim_end()
            );

            let hypothesis = Hypothesis {
                id: format!("hyp-mock-{}", Uuid::new_v4().simple()),
                candidate_id: candidate.id.clone(),
                ub_class: UbClass::UbOob,
                explanation: "Unchecked pointer arithmetic and dereference beyond slice allocation.".to_string(),
                harness_code: harness,
                suggested_miri_flags: vec!["-Zmiri-strict-provenance".to_string()],
                expected_trigger: "in-bounds pointer arithmetic failed".to_string(),
            };
            hypothesis.validate()?;
            return Ok(hypothesis);
        }

        // Case 3: SliceWindow from_raw_parts (r1_from_raw_parts.rs)
        if file.contains("from_raw_parts") || enclosing.contains("SliceWindow") {
            let harness = format!(
                r#"{}
#[cfg(test)]
mod warrant_proof {{
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_from_raw_parts_oob() {{
        let sw = SliceWindow::new(vec![1, 2, 3, 4]);
        let s = sw.window(128);
        std::hint::black_box(s[100]);
    }}
}}
"#,
                REQUIRED_HARNESS_HEADER.trim_end()
            );

            let hypothesis = Hypothesis {
                id: format!("hyp-mock-{}", Uuid::new_v4().simple()),
                candidate_id: candidate.id.clone(),
                ub_class: UbClass::UbOob,
                explanation: "Reconstructed slice length exceeds allocated buffer boundary.".to_string(),
                harness_code: harness,
                suggested_miri_flags: vec!["-Zmiri-tree-borrows".to_string()],
                expected_trigger: "dereferencing pointer failed".to_string(),
            };
            hypothesis.validate()?;
            return Ok(hypothesis);
        }

        // Case 4: FastQueue peek_unchecked (r1_vec_queue.rs)
        // Note: Simulates an initial compile error on Round 1 to exercise the Autonomous Repair Loop!
        if file.contains("vec_queue") || enclosing.contains("FastQueue") {
            let initial_flawed_harness = format!(
                r#"{}
#[cfg(test)]
mod warrant_proof {{
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_vec_queue_oob() {{
        let mut q = FastQueue::new(vec![100, 200]);
        // Intentional compile error for Round 1: missing call argument
        let val = q.peek_unchecked;
        std::hint::black_box(val);
    }}
}}
"#,
                REQUIRED_HARNESS_HEADER.trim_end()
            );

            let hypothesis = Hypothesis {
                id: format!("hyp-mock-{}", Uuid::new_v4().simple()),
                candidate_id: candidate.id.clone(),
                ub_class: UbClass::UbOob,
                explanation: "Out-of-bounds indexing in FastQueue storage buffer.".to_string(),
                harness_code: initial_flawed_harness,
                suggested_miri_flags: vec!["-Zmiri-tree-borrows".to_string()],
                expected_trigger: "unsafe precondition(s) violated: slice::get_unchecked_mut".to_string(),
            };
            hypothesis.validate()?;
            return Ok(hypothesis);
        }

        // Case 5: SharedCounter Cell Sync (r2_reference.rs)
        if file.contains("r2_reference") || enclosing.contains("SharedCounter") {
            let hypothesis = Hypothesis {
                id: format!("hyp-mock-{}", Uuid::new_v4().simple()),
                candidate_id: candidate.id.clone(),
                ub_class: UbClass::DataRace,
                explanation: "Unsound `unsafe impl Sync` on `Cell<usize>` enables concurrent unsynchronized reads and writes across threads."
                    .to_string(),
                harness_code: self.mock_r2_harness.clone(),
                suggested_miri_flags: vec!["-Zmiri-preemption-rate=0".to_string()],
                expected_trigger: "Data race detected between".to_string(),
            };
            hypothesis.validate()?;
            return Ok(hypothesis);
        }

        // Case 6: SharedRc Send (r2_rc_sync.rs)
        if file.contains("rc_sync") || enclosing.contains("SharedRc") {
            let harness = format!(
                r#"{}
#[cfg(test)]
mod warrant_proof {{
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r2_rc_data_race() {{
        let rc = SharedRc::new("hello".to_string());
        std::thread::spawn(move || {{
            let _ = rc.inner.clone();
        }});
    }}
}}
"#,
                REQUIRED_HARNESS_HEADER.trim_end()
            );

            let hypothesis = Hypothesis {
                id: format!("hyp-mock-{}", Uuid::new_v4().simple()),
                candidate_id: candidate.id.clone(),
                ub_class: UbClass::DataRace,
                explanation: "Unsound `unsafe impl Send` on non-atomic Rc allows concurrent refcount mutation.".to_string(),
                harness_code: harness,
                suggested_miri_flags: vec!["-Zmiri-preemption-rate=0".to_string()],
                expected_trigger: "Data race detected between".to_string(),
            };
            hypothesis.validate()?;
            return Ok(hypothesis);
        }

        // Case 7: UnsyncBuffer raw pointer Sync (r2_unsync_buffer.rs)
        if file.contains("unsync_buffer") || enclosing.contains("UnsyncBuffer") {
            let harness = format!(
                r#"{}
#[cfg(test)]
mod warrant_proof {{
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r2_unsync_buffer_data_race() {{
        let mut data = vec![0u8; 10];
        let buf = UnsyncBuffer::new(&mut data);

        std::thread::scope(|s| {{
            s.spawn(|| {{
                for _ in 0..100 {{
                    buf.write_at(0, 42);
                }}
            }});
            s.spawn(|| {{
                for _ in 0..100 {{
                    buf.write_at(0, 99);
                }}
            }});
        }});
    }}
}}
"#,
                REQUIRED_HARNESS_HEADER.trim_end()
            );

            let hypothesis = Hypothesis {
                id: format!("hyp-mock-{}", Uuid::new_v4().simple()),
                candidate_id: candidate.id.clone(),
                ub_class: UbClass::DataRace,
                explanation: "Unsynchronized concurrent writes to raw pointer buffer via unsound Sync impl.".to_string(),
                harness_code: harness,
                suggested_miri_flags: vec!["-Zmiri-preemption-rate=0".to_string()],
                expected_trigger: "Data race detected between".to_string(),
            };
            hypothesis.validate()?;
            return Ok(hypothesis);
        }

        // Case 8: SharedRefCell Sync (r2_refcell_sync.rs)
        if file.contains("refcell_sync") || enclosing.contains("SharedRefCell") {
            let harness = format!(
                r#"{}
#[cfg(test)]
mod warrant_proof {{
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r2_refcell_data_race() {{
        let sc = SharedRefCell::new(0);

        std::thread::scope(|s| {{
            s.spawn(|| {{
                for _ in 0..100 {{
                    sc.add(1);
                }}
            }});
            s.spawn(|| {{
                for _ in 0..100 {{
                    sc.add(1);
                }}
            }});
        }});

        std::hint::black_box(sc.value());
    }}
}}
"#,
                REQUIRED_HARNESS_HEADER.trim_end()
            );

            let hypothesis = Hypothesis {
                id: format!("hyp-mock-{}", Uuid::new_v4().simple()),
                candidate_id: candidate.id.clone(),
                ub_class: UbClass::DataRace,
                explanation: "Unsound `unsafe impl Sync` on RefCell allows concurrent borrow counter mutation.".to_string(),
                harness_code: harness,
                suggested_miri_flags: vec!["-Zmiri-preemption-rate=0".to_string()],
                expected_trigger: "Data race detected between".to_string(),
            };
            hypothesis.validate()?;
            return Ok(hypothesis);
        }

        // Generic fallback hypothesis
        let harness = if candidate.rule() == "R2"
            || (code.contains("unsafe impl") && code.contains("Sync"))
            || candidate.trait_name.as_deref() == Some("Sync")
        {
            self.mock_r2_harness.clone()
        } else {
            self.mock_r1_harness.clone()
        };

        let hypothesis = Hypothesis {
            id: format!("hyp-mock-{}", Uuid::new_v4().simple()),
            candidate_id: candidate.id.clone(),
            ub_class: if candidate.rule() == "R2" { UbClass::DataRace } else { UbClass::UbOob },
            explanation: "Candidate exhibits unsafe construct without explicit invariant verification.".to_string(),
            harness_code: harness,
            suggested_miri_flags: vec!["-Zmiri-tree-borrows".to_string()],
            expected_trigger: "Undefined Behavior".to_string(),
        };
        hypothesis.validate()?;
        Ok(hypothesis)
    }

    async fn repair_harness(
        &self,
        candidate: &Candidate,
        failed_hypothesis: &Hypothesis,
        compiler_or_miri_output: &str,
    ) -> Result<Hypothesis, LlmError> {
        let mut repaired = failed_hypothesis.clone();
        repaired.id = format!("hyp-repair-{}", Uuid::new_v4().simple());
        repaired.explanation = format!(
            "Repaired hypothesis targeting diagnostic: {}",
            compiler_or_miri_output.lines().next().unwrap_or("unknown failure")
        );

        // Case 4 repair: fix the deliberate missing argument in r1_vec_queue
        if candidate.file_path.contains("vec_queue") || failed_hypothesis.harness_code.contains("FastQueue") {
            repaired.harness_code = format!(
                r#"{}
#[cfg(test)]
mod warrant_proof {{
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_vec_queue_oob() {{
        let mut q = FastQueue::new(vec![100, 200]);
        let val = q.peek_unchecked(50);
        std::hint::black_box(val);
    }}
}}
"#,
                REQUIRED_HARNESS_HEADER.trim_end()
            );
        } else {
            repaired.harness_code = Hypothesis::ensure_canonical_header(&failed_hypothesis.harness_code);
        }

        repaired.suggested_miri_flags.push("-Zmiri-strict-provenance".to_string());
        repaired.suggested_miri_flags.dedup();
        repaired.validate()?;
        tracing::debug!(candidate_id = %candidate.id, "Mock repaired harness successfully");
        Ok(repaired)
    }

    async fn propose_patch(
        &self,
        candidate: &Candidate,
        finding: &Finding,
    ) -> Result<String, LlmError> {
        if let Some(cached) = self.cached_patches.get(&candidate.id) {
            return Ok(cached.clone());
        }
        let file = &candidate.file_path;
        let enclosing = candidate.enclosing_item.as_deref().unwrap_or("");
        let snippet = &candidate.code_snippet;

        // Case 0: Flagship RustSec SmallVec (smallvec_r1_demo.rs, RUSTSEC-2021-0003)
        if file.contains("smallvec") || enclosing.contains("SmallVec") || snippet.contains("insert_many") {
            return Ok(r#"//! # Real-World Historical RustSec Benchmark: `smallvec` (RUSTSEC-2021-0003 / CVE-2021-25900)
//!
//! Self-contained host-compilable reproduction adapted from historical RustSec advisory patterns.
//! Original Crate: `smallvec` (affected `>= 0.6.3, < 0.6.14`, e.g. v0.6.9)
//! Original License: MIT / Apache-2.0 (Dual License)
//! Original Author: The Servo Project Developers
//! Advisory: https://rustsec.org/advisories/RUSTSEC-2021-0003.html
//!
//! In affected versions of `smallvec`, calling `insert_many` or inserting elements
//! into an inline `SmallVec` calculated the copy offset before verifying that the
//! inline storage buffer had sufficient capacity to hold the additional elements,
//! leading to an out-of-bounds memory write via `ptr::copy`.

pub struct SmallVec<T> {
    data: [T; 2],
    len: usize,
}

impl<T: Copy + Default> SmallVec<T> {
    pub fn new() -> Self {
        Self {
            data: [T::default(); 2],
            len: 0,
        }
    }

    pub fn push(&mut self, val: T) {
        if self.len < 2 {
            self.data[self.len] = val;
            self.len += 1;
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// REMEDIATED (RUSTSEC-2021-0003):
    /// Verifies remaining inline capacity (`self.data.len() - self.len`) before `ptr::copy`.
    pub fn insert_many(&mut self, index: usize, elements: &[T]) {
        assert!(index <= self.len);
        let available = self.data.len().saturating_sub(self.len);
        let num_items = elements.len().min(available);
        if num_items == 0 {
            return;
        }
        unsafe {
            let ptr = self.data.as_mut_ptr();
            std::ptr::copy(ptr.add(index), ptr.add(index + num_items), self.len - index);
            std::ptr::copy_nonoverlapping(elements.as_ptr(), ptr.add(index), num_items);
        }
        self.len += num_items;
    }

    pub fn get(&self, index: usize) -> Option<T> {
        if index < self.len {
            Some(self.data[index])
        } else {
            None
        }
    }
}

impl<T: Copy + Default> Default for SmallVec<T> {
    fn default() -> Self {
        Self::new()
    }
}"#.trim().to_string());
        }

        // Case 1: SliceBuffer (r1_01_slice_get)
        if file.contains("r1_01") || enclosing.contains("SliceBuffer") || snippet.contains("SliceBuffer") {
            return Ok(r#"
pub struct SliceBuffer {
    data: Vec<u8>,
    cursor: usize,
}

impl SliceBuffer {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            cursor: 0,
        }
    }

    pub fn push_frame(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }

    pub fn advance(&mut self, steps: usize) {
        self.cursor = self.cursor.saturating_add(steps);
    }

    /// SOUND: Validates `idx < self.data.len()` before `get_unchecked`.
    pub fn read_at_offset(&self, offset: usize) -> Option<u8> {
        let idx = self.cursor.checked_add(offset)?;
        if idx < self.data.len() {
            Some(unsafe { *self.data.get_unchecked(idx) })
        } else {
            None
        }
    }
}
"#.trim().to_string());
        }

        // Case 2: RawCursor (r1_02_ptr_arithmetic)
        if file.contains("r1_02") || enclosing.contains("RawCursor") || snippet.contains("RawCursor") {
            return Ok(r#"
pub struct RawCursor {
    buf: Vec<u32>,
    pos: usize,
}

impl RawCursor {
    pub fn new(buf: Vec<u32>) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn seek(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// SOUND: Verifies `target < self.buf.len()` prior to pointer offset and read.
    pub fn peek_offset(&self, offset: usize) -> Option<u32> {
        let target = self.pos.checked_add(offset)?;
        if target < self.buf.len() {
            Some(unsafe { *self.buf.as_ptr().add(target) })
        } else {
            None
        }
    }
}
"#.trim().to_string());
        }

        // Case 3: FrameView (r1_03_from_raw_parts)
        if file.contains("r1_03") || enclosing.contains("FrameView") || snippet.contains("FrameView") {
            return Ok(r#"
pub struct FrameView {
    buf: Vec<u8>,
}

impl FrameView {
    pub fn new(buf: Vec<u8>) -> Self {
        Self { buf }
    }

    /// SOUND: Verifies `start + len <= self.buf.len()` before `from_raw_parts`.
    pub fn slice_window(&self, start: usize, len: usize) -> Option<&[u8]> {
        let end = start.checked_add(len)?;
        if end <= self.buf.len() {
            Some(unsafe { std::slice::from_raw_parts(self.buf.as_ptr().add(start), len) })
        } else {
            None
        }
    }
}
"#.trim().to_string());
        }

        // Case 4: RingQueue (r1_04_vec_queue)
        if file.contains("r1_04") || enclosing.contains("RingQueue") || snippet.contains("RingQueue") {
            return Ok(r#"
pub struct RingQueue {
    buf: Vec<u64>,
    head: usize,
}

impl RingQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: vec![0; capacity.max(1)],
            head: 0,
        }
    }

    pub fn advance_head(&mut self, steps: usize) {
        self.head = (self.head + steps) % self.buf.len();
    }

    /// SOUND: Modulo wrap guarantees `idx < self.buf.len()`.
    pub fn write_at_offset(&mut self, offset: usize, val: u64) {
        let idx = (self.head + offset) % self.buf.len();
        unsafe {
            *self.buf.get_unchecked_mut(idx) = val;
        }
    }
}
"#.trim().to_string());
        }

        // Case 5: TelemetryCounter (r2_01_cell_sync)
        if file.contains("r2_01") || enclosing.contains("TelemetryCounter") || snippet.contains("TelemetryCounter") {
            return Ok(r#"
use core::sync::atomic::{AtomicUsize, Ordering};

pub struct TelemetryCounter {
    count: AtomicUsize,
}

unsafe impl Sync for TelemetryCounter {}

impl TelemetryCounter {
    pub fn new(init: usize) -> Self {
        Self {
            count: AtomicUsize::new(init),
        }
    }

    pub fn increment(&self) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn get(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}
"#.trim().to_string());
        }

        // Case 6: SharedHandle (r2_02_rc_send)
        if file.contains("r2_02") || enclosing.contains("SharedHandle") || snippet.contains("SharedHandle") {
            return Ok(r#"
use std::sync::Arc;

pub struct SharedHandle {
    inner: Arc<usize>,
}

unsafe impl Send for SharedHandle {}
unsafe impl Sync for SharedHandle {}

impl SharedHandle {
    pub fn new(val: usize) -> Self {
        Self {
            inner: Arc::new(val),
        }
    }

    pub fn clone_handle(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }

    pub fn read(&self) -> usize {
        *self.inner
    }
}
"#.trim().to_string());
        }

        // Case 7: SyncSlotBuffer (r2_03_unsync_buffer)
        if file.contains("r2_03") || enclosing.contains("SyncSlotBuffer") || snippet.contains("SyncSlotBuffer") {
            return Ok(r#"
use std::sync::Mutex;

pub struct SyncSlotBuffer {
    slots: Mutex<[u8; 8]>,
}

unsafe impl Sync for SyncSlotBuffer {}

impl SyncSlotBuffer {
    pub fn new() -> Self {
        Self {
            slots: Mutex::new([0u8; 8]),
        }
    }

    pub fn write_slot(&self, index: usize, val: u8) {
        if index < 8 {
            if let Ok(mut guard) = self.slots.lock() {
                guard[index] = val;
            }
        }
    }
}
"#.trim().to_string());
        }

        // Case 8: ConfigCache (r2_04_refcell_sync)
        if file.contains("r2_04") || enclosing.contains("ConfigCache") || snippet.contains("ConfigCache") {
            return Ok(r#"
use std::sync::RwLock;

pub struct ConfigCache {
    cell: RwLock<u64>,
}

unsafe impl Sync for ConfigCache {}

impl ConfigCache {
    pub fn new(initial: u64) -> Self {
        Self {
            cell: RwLock::new(initial),
        }
    }

    pub fn update(&self, delta: u64) {
        if let Ok(mut guard) = self.cell.write() {
            *guard += delta;
        }
    }

    pub fn read(&self) -> u64 {
        self.cell.read().map(|g| *g).unwrap_or(0)
    }
}
"#.trim().to_string());
        }

        // Case 9: PacketAssembler (r1_05_copy_nonoverlapping)
        if file.contains("r1_05") || enclosing.contains("PacketAssembler") || snippet.contains("PacketAssembler") {
            return Ok(r#"
pub struct PacketAssembler {
    dst: [u8; 8],
    write_pos: usize,
}

impl PacketAssembler {
    pub fn new() -> Self {
        Self {
            dst: [0u8; 8],
            write_pos: 0,
        }
    }

    pub fn append_payload(&mut self, src: &[u8]) -> bool {
        let count = src.len();
        let end = match self.write_pos.checked_add(count) {
            Some(e) if e <= self.dst.len() => e,
            _ => return false,
        };
        unsafe {
            let dst_ptr = self.dst.as_mut_ptr().add(self.write_pos);
            core::ptr::copy_nonoverlapping(src.as_ptr(), dst_ptr, count);
        }
        self.write_pos = end;
        true
    }
}

impl Default for PacketAssembler {
    fn default() -> Self {
        Self::new()
    }
}
"#.trim().to_string());
        }

        // Case 10: HeaderInspector (r1_06_byte_offset)
        if file.contains("r1_06") || enclosing.contains("HeaderInspector") || snippet.contains("HeaderInspector") {
            return Ok(r#"
#[repr(C)]
pub struct RawHeader {
    pub magic: u32,
    pub version: u16,
    pub flags: u16,
}

pub struct HeaderInspector {
    header: RawHeader,
}

impl HeaderInspector {
    pub fn new(magic: u32, version: u16, flags: u16) -> Self {
        Self {
            header: RawHeader {
                magic,
                version,
                flags,
            },
        }
    }

    pub fn read_header_byte(&self, byte_offset: usize) -> Option<u8> {
        if byte_offset >= core::mem::size_of::<RawHeader>() {
            return None;
        }
        unsafe {
            let base = (&self.header as *const RawHeader).cast::<u8>();
            Some(*base.add(byte_offset))
        }
    }
}
"#.trim().to_string());
        }

        // Case 11: ChunkReader (r1_07_chunk_reader)
        if file.contains("r1_07") || enclosing.contains("ChunkReader") || snippet.contains("ChunkReader") {
            return Ok(r#"
pub struct ChunkReader {
    buf: Vec<u8>,
    pos: usize,
}

impl ChunkReader {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            pos: 0,
        }
    }

    pub fn feed(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    pub fn next_chunk(&mut self, chunk_size: usize) -> Option<&[u8]> {
        let start = self.pos;
        let end = start.checked_add(chunk_size)?;
        if end > self.buf.len() {
            return None;
        }
        self.pos = end;
        unsafe { Some(self.buf.get_unchecked(start..end)) }
    }
}

impl Default for ChunkReader {
    fn default() -> Self {
        Self::new()
    }
}
"#.trim().to_string());
        }

        // Case 12: NibbleLookup (r1_08_lookup_table)
        if file.contains("r1_08") || enclosing.contains("NibbleLookup") || snippet.contains("NibbleLookup") {
            return Ok(r#"
pub struct NibbleLookup {
    table: [u32; 16],
}

impl NibbleLookup {
    pub fn new() -> Self {
        let mut table = [0u32; 16];
        let mut i = 0;
        while i < 16 {
            table[i] = (i as u32) * 17;
            i += 1;
        }
        Self { table }
    }

    pub fn lookup(&self, raw_index: usize) -> u32 {
        unsafe { *self.table.get_unchecked(raw_index & 0x0F) }
    }
}

impl Default for NibbleLookup {
    fn default() -> Self {
        Self::new()
    }
}
"#.trim().to_string());
        }

        // Case 13: ParallelPair (r2_05_split_borrow)
        if file.contains("r2_05") || enclosing.contains("ParallelPair") || snippet.contains("ParallelPair") {
            return Ok(r#"
use core::sync::atomic::{AtomicU64, Ordering};

pub struct ParallelPair {
    pair: [AtomicU64; 2],
}

unsafe impl Sync for ParallelPair {}

impl ParallelPair {
    pub const fn new(a: u64, b: u64) -> Self {
        Self {
            pair: [AtomicU64::new(a), AtomicU64::new(b)],
        }
    }

    pub fn write_left(&self, val: u64) {
        self.pair[0].store(val, Ordering::SeqCst);
    }

    pub fn write_right(&self, val: u64) {
        self.pair[1].store(val, Ordering::SeqCst);
    }
}
"#.trim().to_string());
        }

        // Case 14: StatusChannel (r2_06_atomic_relaxed_race)
        if file.contains("r2_06") || enclosing.contains("StatusChannel") || snippet.contains("StatusChannel") {
            return Ok(r#"
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub struct StatusChannel {
    payload: AtomicU64,
    ready: AtomicBool,
}

unsafe impl Sync for StatusChannel {}

impl StatusChannel {
    pub const fn new(initial: u64) -> Self {
        Self {
            payload: AtomicU64::new(initial),
            ready: AtomicBool::new(false),
        }
    }

    pub fn publish(&self, val: u64) {
        self.payload.store(val, Ordering::Release);
        self.ready.store(true, Ordering::Release);
    }
}
"#.trim().to_string());
        }

        // Case 15: LazyValue (r2_07_lazy_init)
        if file.contains("r2_07") || enclosing.contains("LazyValue") || snippet.contains("LazyValue") {
            return Ok(r#"
use std::sync::Mutex;

pub struct LazyValue {
    slot: Mutex<Option<u64>>,
}

unsafe impl Sync for LazyValue {}

impl LazyValue {
    pub const fn new() -> Self {
        Self {
            slot: Mutex::new(None),
        }
    }

    pub fn init_or_set(&self, val: u64) {
        if let Ok(mut guard) = self.slot.lock() {
            *guard = Some(val);
        }
    }
}

impl Default for LazyValue {
    fn default() -> Self {
        Self::new()
    }
}
"#.trim().to_string());
        }

        // Case 16: SharedAccumulator (r2_08_alias_mut)
        if file.contains("r2_08") || enclosing.contains("SharedAccumulator") || snippet.contains("SharedAccumulator") {
            return Ok(r#"
use core::sync::atomic::{AtomicU64, Ordering};

pub struct SharedAccumulator {
    val: AtomicU64,
}

unsafe impl Sync for SharedAccumulator {}

impl SharedAccumulator {
    pub const fn new(init: u64) -> Self {
        Self {
            val: AtomicU64::new(init),
        }
    }

    pub fn add(&self, delta: u64) {
        self.val.fetch_add(delta, Ordering::SeqCst);
    }
}
"#.trim().to_string());
        }

        match finding.ub_class {
            UbClass::UbOob => Ok(
                "// Replace get_unchecked with bounds-checked index or get().ok_or(OutOfBounds)?\npub fn safe_access(slice: &[u8], idx: usize) -> Option<&u8> {\n    slice.get(idx)\n}".to_string()
            ),
            UbClass::DataRace => Ok(
                "// Remove unsound `unsafe impl Sync` or replace interior mutability with `std::sync::atomic::AtomicUsize` or `Mutex<T>`.\nuse std::sync::atomic::{AtomicUsize, Ordering};\npub struct SoundShared(AtomicUsize);\n// AtomicUsize is natively Sync without unsafe impl".to_string()
            ),
            _ => Ok(format!(
                "// Remediate unsafe block in {}:{}-{}\n// Enforce preconditions prior to entering unsafe block.",
                candidate.file_path, candidate.start_line, candidate.end_line
            )),
        }
    }

    async fn generate_harness(
        &self,
        target: &ExtractionTarget,
    ) -> Result<CandidateHarness, LlmError> {
        let harness = match target.rule.as_str() {
            "R1" => self.mock_r1_harness.clone(),
            "R2" => self.mock_r2_harness.clone(),
            other => return Err(LlmError::ParseError(format!("Unsupported rule: {other}"))),
        };

        Ok(CandidateHarness {
            rule: target.rule.clone(),
            explanation: "Deterministic mock harness replaying reference proof".to_string(),
            harness_code: harness,
            estimated_budget_usd: 0.0,
        })
    }

    async fn repair_candidate_harness(
        &self,
        _target: &ExtractionTarget,
        previous_attempt: &CandidateHarness,
        _feedback: &VerificationFeedback,
    ) -> Result<CandidateHarness, LlmError> {
        Ok(previous_attempt.clone())
    }

    fn cumulative_cost(&self) -> f64 {
        0.0
    }
}

#[async_trait]
impl AgentBackend for MockLlmClient {
    async fn repair_harness(
        &self,
        candidate: &Candidate,
        failed_hypothesis: &Hypothesis,
        compiler_or_miri_output: &str,
    ) -> Result<Hypothesis, LlmError> {
        LlmClient::repair_harness(self, candidate, failed_hypothesis, compiler_or_miri_output).await
    }

    async fn propose_patch(
        &self,
        candidate: &Candidate,
        finding: &Finding,
    ) -> Result<String, LlmError> {
        LlmClient::propose_patch(self, candidate, finding).await
    }
}
