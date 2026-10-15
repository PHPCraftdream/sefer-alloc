//! R6-T28 / R6-CR-2: a cross-thread `dealloc` of a 16-aligned INTERIOR
//! offset of a live multi-leaf Small block must be dropped at the producer
//! (counted in `foreign_or_unroutable_frees`), never published — the class
//! leaf carries a code only in an issued block's start granule, so the
//! interior granule reads 0 and `SidecarBitmap::publish` rejects it before
//! the pending-bit RMW.
//!
//! Scenario: one live 6144-byte Small block B (the explicit density-fill
//! class, so B spans more than one 4 KiB leaf). The interior granule used is
//! B's SECOND granule: strictly inside B, 16-aligned, and never an issued
//! start — the only issued start in B's start leaf is B itself, since the
//! bump cursor never enters B's span while B is live. Before R6-T28 that
//! granule reported B's class (uniform leaf) or the class spilled into the
//! leaf by an earlier conflict (spill filled every granule with the old
//! uniform value), so the record published and the owner linked a live
//! block's interior into a free list (`Node::write_next` scribbled over B's
//! payload) and decremented its live credit.
//!
//! `abort()` cannot be caught, so the scenario runs in a child process
//! (this very test binary, selected by `CHILD`). The test thread made all
//! the allocations, so it IS the owner and drains its own ingress.
//!
//! Non-vacuous: without start-granule-only encoding the counter assertion
//! fails (the record publishes, so it is not counted as
//! `foreign_or_unroutable_frees`), and the payload check fails (the owner's
//! `Node::write_next` overwrites the poisoned pattern inside B).

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-stats",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::SeferAlloc;
use std::alloc::{GlobalAlloc, Layout};
use std::thread;

const CHILD: &str = "R6_U21_INTERIOR_CHILD";
const TEST: &str = "foreign_free_of_interior_granule_is_dropped_without_corruption";
const BIG: usize = 6144; // explicit density-fill Small class

fn run_child() {
    let big = Layout::from_size_align(BIG, 16).expect("big layout");
    let small = Layout::from_size_align(16, 16).expect("small layout");
    let allocator = SeferAlloc::new();

    // SAFETY: valid layouts; B stays live for the whole scenario.
    let big_ptr = unsafe { allocator.alloc(big) };
    assert!(!big_ptr.is_null(), "the 6144-class block must allocate");
    let interior = big_ptr as usize + 16;

    // Poison B so any owner-side write into it is observable.
    // SAFETY: B is a live 6144-byte allocation of this thread.
    unsafe { core::slice::from_raw_parts_mut(big_ptr, BIG) }.fill(0xA5);

    let before = SeferAlloc::new().stats().foreign_or_unroutable_frees;
    // This thread never allocated, so `dealloc` resolves ForeignNoBind and
    // goes through GlobalAlloc -> publish_foreign -> route pin -> publish.
    let interior_addr = interior;
    let foreign = thread::spawn(move || {
        // SAFETY: malformed interior pointer on purpose — it must be
        // rejected, never dereferenced.
        unsafe { SeferAlloc::new().dealloc(interior_addr as *mut u8, small) };
    });
    foreign.join().expect("foreign free thread");
    let after = SeferAlloc::new().stats().foreign_or_unroutable_frees;
    assert!(
        after > before,
        "foreign_or_unroutable_frees did not grow: {before} -> {after}"
    );

    // This thread made all the allocations above, so it IS the owner: drain
    // the cross-thread ingress here.
    allocator.trim_current_thread();

    // The dropped interior offset must never have been written by the owner
    // (write_next) nor handed out again (free-list link).
    // SAFETY: B is still live and owned by this thread.
    let poison_ok = unsafe { core::slice::from_raw_parts(big_ptr, BIG) }
        .iter()
        .all(|&byte| byte == 0xA5);
    assert!(poison_ok, "B's payload was mutated by the owner drain");
    // SAFETY: valid layout; the block is freed by the drop of the allocator
    // at thread exit; the check below must not observe the interior offset.
    let again = unsafe { allocator.alloc(small) };
    assert!(!again.is_null());
    assert_ne!(
        again as usize, interior,
        "the interior offset must not come back as a fresh allocation"
    );
    println!("R6_U21_INTERIOR_CHILD_OK");
}

#[test]
fn foreign_free_of_interior_granule_is_dropped_without_corruption() {
    if std::env::var_os(CHILD).is_some() {
        run_child();
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().expect("exe"))
        .arg(TEST)
        .arg("--exact")
        .arg("--nocapture")
        .env(CHILD, "1")
        .output()
        .expect("spawn child");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "child failed ({:?}):\n{stdout}\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("R6_U21_INTERIOR_CHILD_OK"),
        "child omitted the success marker:\n{stdout}"
    );
}
