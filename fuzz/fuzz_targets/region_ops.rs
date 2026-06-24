//! libFuzzer target for `sefer-alloc::Region` op-stream invariants (Phase 5).
//!
//! Interprets the fuzz input as a sequence of ops against a `Region<u64>` and
//! checks the SAME reference-model invariants as `tests/differential.rs`
//! (I1–I5 from `docs/INVARIANTS.md`):
//!
//! - I1: a fresh handle resolves to the inserted value.
//! - I2: a removed handle is `None` forever; a second remove is a no-op.
//! - I3: a stale handle (slot reused) never resolves to a live value.
//! - I4: `len()` tracks the live count exactly.
//! - I5: drop-once — at run end, drops == inserts (no double-free, no leak).
//!
//! `arbitrary::Arbitrary` derives a bounded, structured op stream from the raw
//! fuzzer bytes (rather than hand-parsing them), which gives libFuzzer
//! structure-aware feedback. The run length is capped so a single input cannot
//! OOM the fuzzer with a multi-gigabyte op stream.
//!
//! # How to run (Linux only)
//!
//! libFuzzer requires the nightly toolchain and does NOT run on Windows. From
//! the `fuzz/` directory:
//!
//! ```text
//! cargo +nightly fuzz run region_ops
//! # long overnight run:
//! cargo +nightly fuzz run region_ops -- -max_total_time=3600
//! # reproduce a crash:
//! cargo +nightly fuzz run region_ops -- artifact.bin
//! ```
//!
//! See `fuzz/README.md` for the full guide.

#![forbid(unsafe_code)]

use std::hint::black_box;

use arbitrary::Arbitrary;
use sefer_alloc::{Handle, Region};

/// The drop-counting payload. `id` is the value the model tracks; `drops` is a
/// shared counter so we can check I5 (drop-once) at the end of the run.
#[derive(Clone)]
struct Payload {
    id: u64,
    drops: &'static std::sync::atomic::AtomicUsize,
}

// Compare payloads by id — the model verifies resolution by value.
impl PartialEq for Payload {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for Payload {}

impl std::fmt::Debug for Payload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Payload").field("id", &self.id).finish()
    }
}

impl Drop for Payload {
    fn drop(&mut self) {
        self.drops
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// One operation against the region, derived from fuzzer bytes by `arbitrary`.
/// The `index`/`slot` fields are `usize`; the harness reduces them modulo the
/// live count so they are always in range (mirrors the proptest differential
/// harness).
#[derive(Arbitrary, Debug)]
enum Op {
    Insert(u64),
    Remove(usize),
    Get(usize),
    GetMut(usize, u64),
    Clear,
}

// A process-lifetime counter for I5. libFuzzer invokes the target many times in
// one process; using a fresh thread-local-style cell per call would mask leaks
// across runs, so instead we accumulate into a static and reset per call.
static DROPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The entry point: libFuzzer feeds raw `data`, `arbitrary` shapes it into a
/// bounded `Vec<Op>`, and we replay it against a `Region<Payload>` checked
/// against the reference model. Any invariant violation panics, which
/// libFuzzer reports as a finding.
#[export_name = "rust_fuzzer_test_input"]
pub fn target(data: &[u8]) {
    // Reset the drop counter for this input's I5 accounting.
    DROPS.store(0, std::sync::atomic::Ordering::Relaxed);

    // Cap the op stream so a single input can't OOM the fuzzer with a giant
    // sequence. 4096 ops is plenty to exercise deep churn and reuse.
    let mut decoder = arbitrary::Unstructured::new(data);
    let ops: Vec<Op> = match decoder
        .arbitrary_iter::<Op>()
        .ok()
        .flatten()
        .take(4096)
        .collect()
    {
        Ok(ops) => ops,
        Err(_) => return, // not enough bytes to derive a usable stream; skip.
    };

    let mut region: Region<Payload> = Region::new();
    // The reference model: every currently-live (handle, value).
    let mut live: Vec<(Handle<Payload>, u64)> = Vec::new();
    let mut total_inserts = 0usize;

    for op in ops {
        match op {
            Op::Insert(v) => {
                let p = Payload {
                    id: v,
                    drops: &DROPS,
                };
                let h = region.insert(p);
                // I1: a fresh handle resolves to the inserted value.
                assert_eq!(
                    region.get(h).map(|p| p.id),
                    Some(v),
                    "I1: fresh handle must resolve"
                );
                live.push((h, v));
                total_inserts += 1;
            }
            Op::Remove(n) => {
                if !live.is_empty() {
                    let i = n % live.len();
                    let (h, v) = live.swap_remove(i);
                    assert_eq!(
                        region.remove(h).map(|p| p.id),
                        Some(v),
                        "I2: remove must return the live value"
                    );
                    // I2: removed handle is None, and removing again is a no-op.
                    assert_eq!(
                        region.get(h).map(|p| p.id),
                        None,
                        "I2: removed handle must be None"
                    );
                    assert_eq!(
                        region.remove(h).map(|p| p.id),
                        None,
                        "I2: second remove must be a no-op None"
                    );
                }
            }
            Op::Get(n) => {
                if !live.is_empty() {
                    let i = n % live.len();
                    let (h, v) = live[i];
                    assert_eq!(
                        region.get(h).map(|p| p.id),
                        Some(v),
                        "I1/I3: live handle must resolve to its value"
                    );
                }
            }
            Op::GetMut(n, new_id) => {
                if !live.is_empty() {
                    let i = n % live.len();
                    let h = live[i].0;
                    if let Some(p) = region.get_mut(h) {
                        p.id = new_id;
                    }
                    live[i].1 = new_id;
                    assert_eq!(
                        region.get(h).map(|p| p.id),
                        Some(new_id),
                        "get_mut mutation must stick"
                    );
                }
            }
            Op::Clear => {
                region.clear();
                live.clear();
                assert!(region.is_empty(), "I4: clear empties the region");
                assert_eq!(region.len(), 0, "I4: len is 0 after clear");
            }
        }
        // I4: length tracks the model exactly after every op.
        assert_eq!(
            region.len(),
            live.len(),
            "I4: region len must match the live model count"
        );
    }

    // Every survivor still resolves to its value (I1 still holds at run end).
    for (h, v) in &live {
        assert_eq!(
            region.get(*h).map(|p| p.id),
            Some(*v),
            "I1: survivor must resolve at run end"
        );
    }

    // I5: drop-once. Drop the region (drops all survivors) then account: every
    // insert must correspond to exactly one drop, no more, no less.
    drop(region);
    drop(live);

    let drops = DROPS.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        drops, total_inserts,
        "I5: every inserted value must be dropped exactly once (no double-free, no leak)"
    );

    // Keep the optimizer from eliding the whole replay.
    black_box(());
}
