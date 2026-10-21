//! R12-01 (fxx round 12, P2): a cross-thread `dealloc` of a 16-aligned address
//! that is NOT the start of an issued block in a foreign Small segment must be
//! dropped (and counted in `foreign_or_unroutable_frees`) by the producer, not
//! published into the owner's sidecar — where `BitmapCut::pop` finds no class
//! for the granule and `abort()`s the owner.
//!
//! `abort()` cannot be caught, so every case runs in a child process (this very
//! test binary, selected by `CHILD`). Three malformed addresses are covered:
//! (0) a 16-aligned address in the segment metadata (rejected by the payload
//! filter), (1) a 16-aligned address in the not-yet-carved tail (rejected by
//! the zero start bit), and — D1' phase-1b, closing CORRECTNESS item 166 —
//! (2) `live + 32`: a 16-aligned INTERIOR pointer into a LIVE issued 64-byte
//! block. Its granule was never issued as a block START, so the start-bit
//! gate rejects the publication BEFORE any sidecar mutation (no RMW), the
//! rejection is counted in `foreign_or_unroutable_frees`, and the owner's
//! drain finds nothing: the live block is untouched and never aborts.
//!
//! Non-vacuous: without the start-bit gate in `SidecarBitmap::publish` (the
//! former leaf-class gate was its 1a predecessor) the stale pending bit would
//! survive into the owner's drain and the child exits with a failure status;
//! the counter assertion also pins the "rejected, not published" outcome.

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
        ptr_tx.send((base, live.addr())).expect("send");
        drain_rx.recv().expect("drain signal");
        // Owner-side drain of the cross-thread ingress: reaches
        // `BitmapScan::next_cut`/`BitmapCut::pop`. For the interior case the
        // publication was rejected BEFORE any RMW, so the drain must find
        // NOTHING: the live block stays live (proved by the clean re-dealloc
        // below — a wrongly reclaimed block would corrupt the owner's state).
        allocator.trim_current_thread();
        // SAFETY: the only valid allocation of this thread.
        unsafe { allocator.dealloc(live, layout) };
    });
    let (base, live_addr) = ptr_rx.recv().expect("owner allocation");
    let address = [
        base + 16,
        base + SegmentLayout::SEGMENT - 16,
        // Case 2 (item 166): interior of the LIVE 64-byte block allocated in
        // the owner thread — `live` itself is `base + off` for some
        // payload-start-aligned offset; `live + 32` is a 16-aligned interior
        // granule of that live block. The owner sends the segment base only,
        // so probe live's own granule + 2 granules: still inside the live
        // block (64 bytes), never issued as a start.
        live_addr + 32,
    ][case];
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
    for case in 0..3 {
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
