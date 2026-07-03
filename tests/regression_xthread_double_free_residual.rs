//! Regression PIN (task R2 / #154) — the ring↔magazine cross-thread
//! double-free residual limit of M2.
//!
//! # What this pins
//!
//! The magazine (`HeapCore` tcache) and each segment's `RemoteFreeRing` are
//! mutually blind. A block P whose CROSS-THREAD free is still in-flight
//! (packed into its segment's ring, NOT yet drained by the owner) sets NEITHER
//! of the Э6 M2 oracles:
//!   - the in-magazine `slots` scan cannot see it (it is not in `slots`), and
//!   - the BinTable `is_free` bitmap still reads "allocated" (only the owner's
//!     drain → `reclaim_offset` → `mark_free` sets the bit; the ring push
//!     deliberately leaves the bitmap untouched).
//!
//! So a genuine USER cross-thread double-free — an own-thread free of P while
//! P is already queued in the ring — passes both oracles and lands in the
//! magazine. P is then BOTH magazine-resident AND pending in the ring. A later
//! drain's `reclaim_offset` (which passes its own magic/kind/align/off<bump/
//! is_free guards, P being still-carved) does `Node::write_next(P, old_head)`,
//! clobbering P's now-live user bytes once the magazine has re-issued P, and
//! pushes P onto the BinTable + `dec_live` → double-issue + freelist corruption.
//!
//! # Deterministic single-threaded repro
//!
//! No real race is needed — the hazardous interleaving is a SEQUENTIAL one,
//! reproduced with the test-only `dbg_push_to_ring` / `dbg_drain_all_rings`
//! hooks:
//!   1. alloc P (class c).
//!   2. simulate the REMOTE free of P: push (off(P), c) into P's segment ring.
//!   3. own-thread free P → lands in the magazine (both oracles blind → bug).
//!   4. alloc once → pops P from the magazine (LIFO); write a SENTINEL into
//!      P's word0 (P is now a LIVE, user-owned block).
//!   5. drain all rings → `reclaim_offset` fires on the stale ring entry.
//!
//! Then assert the CORRECT (no-corruption) behaviour:
//!   (a) the sentinel in P's word0 survived (no `write_next` clobber), AND
//!   (b) a following alloc batch never returns P twice (no double-issue).
//!
//! # Status — RED today (the bug is real); flips GREEN when task #164 lands
//!
//! This test asserts CORRECTNESS and is `#[ignore]`d until the real fix (#164)
//! lands, so the CI-visible suite stays green (ignored != failed). It FAILS
//! when run with `--ignored`:
//!
//! ```text
//! running 1 test
//! test residual_xthread_double_free_no_corruption ... FAILED
//!
//! failures:
//! ---- residual_xthread_double_free_no_corruption stdout ----
//! thread 'residual_xthread_double_free_no_corruption' panicked at
//!   tests/regression_xthread_double_free_residual.rs:
//! P's sentinel word0 was CLOBBERED by the ring drain's write_next \
//!   (ring↔magazine residual limit, task #164): expected 0x5EFE_5EFE_5EFE_5EFE, \
//!   got <old freelist head>
//! ```
//!
//! (The sentinel-clobber assertion (a) trips first; if a fix only addressed
//! the clobber but not the double-issue, assertion (b) would guard that leg.)
//! When #164 fixes the composition, both assertions pass and the `#[ignore]`
//! is removed.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "fastbin"
))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise: the registry is a process-global static.
static SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

const SENTINEL: usize = 0x5EFE_5EFE_5EFE_5EFE;

#[test]
#[ignore = "known residual M2 limit: cross-thread double-free ring-in-flight — real fix tracked as task #164"]
fn residual_xthread_double_free_no_corruption() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    let layout = Layout::from_size_align(16, 8).unwrap();

    // The class index the magazine keys P under (same classification `alloc`
    // and the ring entry use).
    let c = unsafe { (*heap).dbg_class_for(layout) }.expect("16/8 must be a small class");

    // (1) alloc P.
    let p = unsafe { (*heap).alloc(layout) };
    assert!(!p.is_null());

    // (2) simulate the REMOTE cross-thread free of P: push (off(P), c) into
    // P's segment ring, exactly as `dealloc_routing`'s Variant-2 push does on
    // a foreign thread. The bitmap for P stays "allocated" (ring push does not
    // touch it) and live_count is unchanged — the correct remote-free protocol.
    let pushed = unsafe { (*heap).dbg_push_to_ring(p, c) };
    assert!(pushed, "ring push failed (ring full or P not owned)");

    // (3) the app ALSO frees P on the OWN thread (a user cross-thread
    // double-free — one leg remote (in the ring), one leg own). M2 promises an
    // exact no-op; here both Э6 oracles are blind (P not in `slots`; P's bitmap
    // still reads allocated because the ring push did not set it), so P is
    // (wrongly) pushed into the magazine.
    unsafe { (*heap).dealloc(p, layout) };

    // (4) alloc once → pops P from the magazine (LIFO). P is now a LIVE,
    // user-owned block again. Write a sentinel into its word0 (the exact word
    // `reclaim_offset`'s `write_next` would clobber).
    let p2 = unsafe { (*heap).alloc(layout) };
    assert!(!p2.is_null());
    assert_eq!(
        p2, p,
        "expected the magazine to re-issue P (LIFO); the repro relies on it"
    );
    unsafe { (p2 as *mut usize).write(SENTINEL) };

    // (5) drain all rings → the stale ring entry for P is reclaimed:
    // `reclaim_offset(S, P)` passes magic/kind/align/off<bump/is_free (P is
    // still carved, bitmap still 0), then `write_next(P, old_head)` clobbers
    // P's live word0 and links P onto the BinTable + dec_live.
    unsafe { (*heap).dbg_drain_all_rings() };

    // ── Assert CORRECTNESS (target behaviour; RED today) ──────────────────
    // (a) P's live user bytes must NOT have been clobbered by the drain.
    let word0 = unsafe { (p2 as *mut usize).read() };
    assert_eq!(
        word0, SENTINEL,
        "P's sentinel word0 was CLOBBERED by the ring drain's write_next \
         (ring↔magazine residual limit, task #164): expected {SENTINEL:#018x}, \
         got {word0:#018x}"
    );

    // (b) a following alloc batch must never return P twice (no double-issue).
    // After the (buggy) drain, P sits on the BinTable free list while still
    // being a live user block — a subsequent refill can hand it out again.
    let mut issued: Vec<*mut u8> = Vec::with_capacity(64);
    for _ in 0..64 {
        let q = unsafe { (*heap).alloc(layout) };
        if q.is_null() {
            break;
        }
        issued.push(q);
    }
    let p_count = issued.iter().filter(|&&q| q == p).count();
    assert!(
        p_count <= 1,
        "P was double-issued ({p_count} times) after the ring drain linked a \
         live block onto the BinTable (ring↔magazine residual limit, task #164)"
    );

    // Cleanup (best-effort; the heap state may already be corrupt under the
    // bug — recycle regardless so a later serialized test can claim a slot).
    for &q in &issued {
        if q != p {
            unsafe { (*heap).dealloc(q, layout) };
        }
    }
    unsafe { HeapRegistry::recycle(heap) };
}
