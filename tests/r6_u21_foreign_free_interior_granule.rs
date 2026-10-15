//! R6-T28 / R6-CR-2: a cross-thread `dealloc` of a 16-aligned INTERIOR
//! offset of a live multi-leaf Small block must be dropped at the producer
//! (counted in `foreign_or_unroutable_frees`), never published — the class
//! leaf carries a code only in an issued block's start granule, so the
//! interior granule reads 0 and `SidecarBitmap::publish` rejects it before
//! the pending-bit RMW.
//!
//! Scenario (CR-2): a 6144-byte Small block B whose start is leaf-aligned
//! (so B's tail leaf ends mid-leaf), then a 16-byte allocation whose start
//! granule lands in B's tail leaf, then a foreign free of the tail leaf's
//! first granule (strictly inside B). Before R6-T28 that granule reported
//! the neighbor's class, the record published, and the owner linked a live
//! block's interior into the class-0 free list (the next 16-byte carve
//! handed it out again and `Node::write_next` scribbled over B's payload).
//!
//! `abort()` cannot be caught, so the scenario runs in a child process
//! (this very test binary, selected by `CHILD`). No separate owner thread
//! is needed: the test thread made all the allocations, so it IS the owner.
//!
//! Non-vacuous: without start-granule-only encoding the child's payload
//! check fails (the owner's `Node::write_next` overwrites the poisoned
//! pattern inside B) and/or the follow-up 16-byte allocation returns the
//! interior offset from the corrupted free list.

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
const LEAF: usize = 4096;

fn align_up(value: usize, align: usize) -> usize {
    value.div_ceil(align) * align
}

fn run_child() {
    let big = Layout::from_size_align(BIG, 16).expect("big layout");
    let small = Layout::from_size_align(16, 16).expect("small layout");
    let allocator = SeferAlloc::new();

    // SAFETY: valid layouts; every block below stays live until after the
    // owner-side drain.
    let first = unsafe { allocator.alloc(small) };
    assert!(!first.is_null());
    let mut warm = vec![first];
    // Advance the bump so the 6144-class carve lands on a 12288-aligned
    // start: its tail leaf then ends mid-leaf and the follow-up 16-byte
    // carve PLANTS an issued start inside B's tail leaf (the CR-2
    // precondition for the old uniform encoding).
    let mut bump = first as usize + 16;
    while align_up(bump, BIG) % (2 * LEAF) != 0 {
        // SAFETY: valid layout; the walker stays live.
        let ptr = unsafe { allocator.alloc(small) };
        assert!(!ptr.is_null(), "bump walk must not OOM");
        warm.push(ptr);
        bump = ptr as usize + 16;
    }
    // SAFETY: valid layout; B stays live for the whole scenario.
    let big_ptr = unsafe { allocator.alloc(big) };
    assert_eq!(
        big_ptr as usize,
        align_up(bump, BIG),
        "B must carve at the aligned bump"
    );
    let start = big_ptr as usize;
    assert_eq!(start % LEAF, 0, "B must start leaf-aligned");
    // SAFETY: valid layout; the neighbor stays live.
    let neighbor = unsafe { allocator.alloc(small) };
    assert_eq!(
        neighbor as usize,
        start + BIG,
        "the neighbor must carve right after B"
    );
    assert_eq!(
        neighbor as usize / LEAF,
        (start + BIG - 1) / LEAF,
        "the neighbor's start granule must sit in B's tail leaf"
    );
    let interior = start + LEAF; // tail leaf's first granule, strictly inside B
    assert!(interior < start + BIG && interior != neighbor as usize);

    // Poison B so any owner-side write into it is observable.
    // SAFETY: B is a live 6144-byte allocation of this thread.
    unsafe { core::slice::from_raw_parts_mut(big_ptr, BIG) }.fill(0xA5);

    let before = SeferAlloc::new().stats().foreign_or_unroutable_frees;
    // This thread never allocated, so `dealloc` resolves ForeignNoBind and
    // goes through GlobalAlloc -> publish_foreign -> route pin -> publish.
    let foreign = thread::spawn(move || {
        // SAFETY: malformed interior pointer on purpose — it must be
        // rejected, never dereferenced.
        unsafe { SeferAlloc::new().dealloc(interior as *mut u8, small) };
    });
    foreign.join().expect("foreign free thread");
    let after = SeferAlloc::new().stats().foreign_or_unroutable_frees;
    assert!(
        after > before,
        "foreign_or_unroutable_frees did not grow: {before} -> {after}"
    );
    // This thread made all the allocations above, so it IS the owner:
    // drain the cross-thread ingress here.
    allocator.trim_current_thread();

    // The dropped interior offset must never have been written by the owner
    // (write_next) nor handed out again (free-list link).
    // SAFETY: B is still live and owned by this thread.
    let poison_ok = unsafe { core::slice::from_raw_parts(big_ptr, BIG) }
        .iter()
        .all(|&byte| byte == 0xA5);
    assert!(poison_ok, "B's payload was mutated by the owner drain");
    // SAFETY: valid layout; the block is freed right after the check.
    let again = unsafe { allocator.alloc(small) };
    assert!(!again.is_null());
    assert_ne!(
        again as usize, interior,
        "the interior offset must not come back as a fresh allocation"
    );
    drop(warm);
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
