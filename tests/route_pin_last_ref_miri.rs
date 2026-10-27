//! P5 route pins (docs/GHOST_CHECKS_PLAN.md G2, correctness item 186) on the
//! REAL directory under Miri: a pin taken and dropped on another thread races
//! the owner's unlink and owner-reference drop. Whichever reference is last
//! frees the `Entry` and its sidecar (`EntryHandle::drop`), and Miri's
//! data-race detector requires that free to happen-after every other holder's
//! last access to the `Entry` — exactly what the `AcqRel` decrement provides
//! (ghost-lint rule `route-pin/drop-acqrel`). Under the mutant
//! `fetch_sub(1, Relaxed)` the free races the other thread's decrement and
//! Miri reports a data race; under "free before the count check"
//! (`route-pin/last-ref-free`) it reports a double free. The ghost checks in
//! the same code run here too (`cfg(miri)` is a ghost build): no underflow, no
//! pin on a dead entry.
//!
//! The two drops must not be ordered by anything but `refs` itself: the worker
//! announces its pin through a `Relaxed` flag (no happens-before), the owner
//! spins on it with `Relaxed` loads, and then both drop concurrently — the
//! worker's lookup and the owner's unlink do share the shard lock, but each
//! drop happens after its side released it. Roots are numeric segment keys
//! only (`without_provenance_mut`): the directory never dereferences them.
#![cfg(all(feature = "alloc-global", feature = "internals"))]

use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};

const SEGMENT: usize = 1 << 22;
const ROUNDS: usize = 4;

#[test]
fn last_reference_free_happens_after_every_pin_drop() {
    let directory = RouteDirectory::new();
    for round in 0..ROUNDS {
        let root = core::ptr::without_provenance_mut::<u8>(SEGMENT * (16 + round));
        let registration = directory
            .register(root, SEGMENT, root, round + 1, RouteKind::Small)
            .expect("register a Small route");
        let key = root.addr();
        let dir = &directory;
        let pinned = AtomicBool::new(false);
        let flag = &pinned;
        std::thread::scope(|s| {
            s.spawn(move || {
                let pin = dir
                    .lookup(core::ptr::without_provenance_mut::<u8>(key))
                    .expect("linked route pins");
                // Relaxed on purpose: the announcement must not order the drops.
                flag.store(true, Ordering::Relaxed);
                drop(pin);
            });
            while !pinned.load(Ordering::Relaxed) {
                std::thread::yield_now();
            }
            // Owner: unlink + drop the owner reference, racing the worker's drop.
            drop(registration);
        });
        assert!(
            directory.lookup(root).is_none(),
            "round {round}: unlinked route must not pin"
        );
    }
}
