//! R12-01 (fxx round 12, P2): a cross-thread `dealloc` of a 16-aligned address
//! that is NOT the start of an issued block in a foreign Small segment must be
//! dropped (and counted in `foreign_or_unroutable_frees`) by the producer, not
//! published into the owner's sidecar — where `BitmapCut::pop` finds no class
//! for the granule and `abort()`s the owner.
//!
//! `abort()` cannot be caught, so every case runs in a child process (this very
//! test binary, selected by `CHILD`). Two malformed addresses are covered:
//! (0) a 16-aligned address in the segment metadata, (1) a 16-aligned address in
//! the not-yet-carved tail.
//!
//! Residual (not covered, by design of the producer protocol): an interior
//! pointer into an issued block whose class leaf is uniform still reads a
//! non-zero class for every granule of the leaf, and the producer may not read
//! block geometry from the segment header (the segment of a malformed free may
//! be released concurrently). Such a record is rejected by the owner-side
//! geometry check, which aborts; see `docs/CORRECTNESS_OPEN_ITEMS.md`.
//!
//! Non-vacuous: without the `encoded(granule) == 0` check in
//! `SidecarBitmap::publish` the owner's drain aborts and the child exits with a
//! failure status; the counter assertion also pins the "dropped, not
//! published" outcome.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-stats",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::{SeferAlloc, SegmentLayout};
use std::alloc::{GlobalAlloc, Layout};
use std::sync::mpsc;
use std::thread;

const CHILD: &str = "R12_01_ABORT_CHILD";
const TEST: &str = "foreign_free_of_unissued_granules_is_dropped_without_abort";

fn run_child(case: usize) {
    let layout = Layout::from_size_align(64, 16).expect("layout");
    let (ptr_tx, ptr_rx) = mpsc::channel();
    let (drain_tx, drain_rx) = mpsc::channel();
    let owner = thread::spawn(move || {
        let allocator = SeferAlloc::new();
        // SAFETY: valid layout; the block stays live until after the drain.
        let live = unsafe { allocator.alloc(layout) };
        assert!(!live.is_null());
        let base = live.addr() & !(SegmentLayout::SEGMENT - 1);
        ptr_tx.send(base).expect("send");
        drain_rx.recv().expect("drain signal");
        // Owner-side drain of the cross-thread ingress: reaches `BitmapCut::pop`.
        allocator.trim_current_thread();
        // SAFETY: the only valid allocation of this thread.
        unsafe { allocator.dealloc(live, layout) };
    });
    let base = ptr_rx.recv().expect("owner allocation");
    let address = [base + 16, base + SegmentLayout::SEGMENT - 16][case];
    let before = SeferAlloc::new().stats().foreign_or_unroutable_frees;
    // This thread never allocated, so `dealloc` resolves ForeignNoBind and goes
    // through GlobalAlloc -> publish_foreign -> route pin -> sidecar publish.
    let foreign = thread::spawn(move || {
        // SAFETY: malformed pointer on purpose — it must be rejected, never
        // dereferenced.
        unsafe { SeferAlloc::new().dealloc(address as *mut u8, layout) };
    });
    foreign.join().expect("foreign free thread");
    drain_tx.send(()).expect("drain");
    owner.join().expect("owner thread");
    let after = SeferAlloc::new().stats().foreign_or_unroutable_frees;
    assert!(
        after > before,
        "foreign_or_unroutable_frees did not grow: {before} -> {after}"
    );
    println!("R12_01_CHILD_OK case={case}");
}

#[test]
fn foreign_free_of_unissued_granules_is_dropped_without_abort() {
    if let Some(case) = std::env::var_os(CHILD) {
        run_child(case.to_str().expect("utf8").parse().expect("case index"));
        return;
    }
    for case in 0..2 {
        let output = std::process::Command::new(std::env::current_exe().expect("exe"))
            .arg(TEST)
            .arg("--exact")
            .arg("--nocapture")
            .env(CHILD, case.to_string())
            .output()
            .expect("spawn child");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "case {case} failed ({:?}):\n{stdout}\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains(&format!("R12_01_CHILD_OK case={case}")),
            "case {case} omitted the success marker:\n{stdout}"
        );
    }
}
