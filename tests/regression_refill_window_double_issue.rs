//! Regression PIN for task R1 (retro C1, 2026-07-06) — the **refill-window
//! double-issue** leg of the ring↔magazine cross-thread double-free residual.
//!
//! # What this pins
//!
//! Task #164 (X2) closed the *in-magazine-at-drain* leg: a block P that is
//! simultaneously magazine-resident AND pending in a remote-free ring is
//! detected at drain time and the ring entry is dropped. The campaign believed
//! the remaining residual was a single class — *re-issue-before-drain* (P
//! already popped to the user when the drain runs, information-theoretically
//! indistinguishable from a delayed genuine xfree without per-block
//! generations; pinned RED by `residual_xthread_double_free_no_corruption`).
//!
//! The X-arc adversarial retrospective (2026-07-06, §C1) found a SECOND leg
//! hiding inside the X2 fix's own window: `refill_class_bump_impl` drains the
//! segment freelist into the caller-owned `out[0..filled]` buffer (step 1,
//! `drain_freelist_batch(self.small_cur, …)`) BEFORE it drains the
//! cross-thread remote-free rings (step 2, `find_segment_with_free_checked`).
//! The predicate passed in from `refill_magazine_slow` opens with
//! `if k == c { return false; }` — justified only by borrow-safety
//! (count[c]==0 during its own refill), NOT by semantic coverage of blocks
//! already pulled into `out[0..filled]` during the CURRENT refill call. Those
//! blocks are magazine-destined but invisible to the predicate.
//!
//! Concrete race this test reproduces, single-threaded and deterministic:
//!   - Block P is the ONLY block of class c on `small_cur`'s freelist, and a
//!     stale cross-thread double-free note for P is still in `small_cur`'s
//!     ring.
//!   - A single production refill call (alloc loop → `refill_magazine_slow`)
//!     pulls P from the freelist into `out[0]` (filled = 1 < want), then
//!     reaches the ring-drain step (step 2) in the SAME call because `filled`
//!     is still < `want`. The drain sees P's stale note, bitmap reads
//!     allocated (P was just pulled by step 1), the predicate (blinded to
//!     class c) reports false → the reclaim path relinks P onto the freelist
//!     a SECOND time → a later iteration of the SAME refill loop pulls P into
//!     `out` AGAIN.
//!   Net effect: P is issued twice out of ONE refill call — the positions are
//!   CONSECUTIVE in the issued batch (e.g. [14, 15]), proving the duplicate
//!   arises within a single refill, not across two refills.
//!
//! # Why the existing green test is blind to this
//!
//! `drain_resident_xthread_double_free_no_corruption` drains via
//! `HeapCore::dbg_drain_all_rings`, whose predicate checks ALL classes
//! including c (`tc.slots[class_idx]`) — strictly STRONGER than the production
//! refill predicate. So it can never reproduce this hole, which is specific to
//! the production refill path's `if k == c { return false; }` shortcut.
//!
//! # The fix (R1, variant A)
//!
//! Inside `refill_class_bump_impl`, the predicate passed to
//! `find_segment_with_free_checked` is wrapped so that a pointer is also
//! treated as "still spoken for" if it is present in `out[0..filled]` for the
//! current class. Costs nothing extra when the ring is empty (common case);
//! does not regress the Ir baseline the X2 campaign fought to keep flat.
//!
//! # Counterfactual (non-vacuous — VERIFIED)
//!
//! Reverting ONLY the new out-membership guard (i.e. restoring the bare
//! `find_segment_with_free_checked(class_idx, is_in_magazine)` call) makes this
//! test FAIL: the issued-pointers Vec contains P exactly twice at CONSECUTIVE
//! positions (e.g. [14, 15]), proving the duplicate is within one refill call.
//! This was confirmed by editing the guard out, running the test, observing
//! `p_count=2` with the consecutive positions, then restoring the guard and
//! observing `p_count=1`. Both outputs are quoted in the task R1 final report.

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

/// Regression test (task R1): the refill-window in-out-buffer double-issue
/// leg. A block P — the sole class-c block on `small_cur`'s freelist — with a
/// stale cross-thread double-free note for P still in the ring must NOT be
/// double-issued when the refill that pulls P reaches the ring-drain step
/// within the SAME call.
///
/// # Timeline (deterministic, single-threaded)
///
/// 1. Alloc P (class c). The first refill carves `want` blocks; P is popped
///    and `want - 1` remain in the magazine.
/// 2. Pop the `want - 1` leftovers so the magazine is empty. These blocks are
///    kept live (preventing a decommit) and freed at cleanup.
/// 3. Dealloc P → magazine has EXACTLY P (no other entries).
/// 4. `dbg_push_to_ring(P, c)` — plant the stale cross-thread free note.
///    Bitmap for P UNTOUCHED (still "allocated").
/// 5. `dbg_flush_all()` — flush P onto `small_cur`'s freelist (bitmap → FREE).
///    P is now the ONLY class-c block on the freelist AND pending in the ring.
/// 6. Alloc loop. The first refill pulls P from the freelist into `out[0]`
///    (filled = 1 < want), then reaches the ring-drain step in the SAME call.
///    With R1: the drain sees P's note, the wrapped predicate reports P as
///    "still spoken for" (present in `out[0..filled]`) → entry dropped, P is
///    NOT relinked, P appears exactly once. Without R1: P is relinked and
///    re-pulled in the same refill → P appears at two CONSECUTIVE positions.
/// 7. Assert: P appears at most once across the whole issued batch.
///
/// # Counterfactual
///
/// Without the R1 out-membership guard, step 6's ring drain reclaims P
/// (`write_next` + `mark_free`), relinking P onto the freelist a second time.
/// A later iteration of the SAME refill loop then pulls P into `out` AGAIN.
/// The issued-pointers Vec contains P exactly twice at consecutive positions
/// → assertion fails. Verified by hand (see module doc + task R1 report).
#[test]
fn refill_window_does_not_double_issue_in_out_buffer_resident_block() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    let layout = Layout::from_size_align(16, 8).unwrap();

    let c = unsafe { (*heap).dbg_class_for(layout) }.expect("16/8 must be a small class");
    let want = unsafe { (*heap).dbg_refill_n_for_class(c) };
    assert!(
        want >= 2,
        "this test relies on refill_n_for_class >= 2; got {want}"
    );

    // (1) alloc P. The refill carves `want`; P is popped; `want - 1` remain.
    let p = unsafe { (*heap).alloc(layout) };
    assert!(!p.is_null());

    // (2) pop the `want - 1` magazine leftovers so the magazine is empty.
    //     These blocks are kept live (no decommit) and freed at cleanup.
    let mut leftovers: Vec<*mut u8> = Vec::with_capacity(want - 1);
    for _ in 0..(want - 1) {
        let q = unsafe { (*heap).alloc(layout) };
        assert!(!q.is_null());
        leftovers.push(q);
    }

    // (3) dealloc P → magazine has exactly P.
    unsafe { (*heap).dealloc(p, layout) };

    // (4) plant the stale cross-thread free note for P.
    let pushed = unsafe { (*heap).dbg_push_to_ring(p, c) };
    assert!(pushed, "ring push failed (ring full or P not owned)");

    // (5) flush P onto the freelist. P is the only class-c block the magazine
    //     holds, so P is the only block landing on `small_cur`'s freelist.
    unsafe { (*heap).dbg_flush_all() };

    // (6) alloc loop. The first refill pulls P from the freelist into out[0]
    //     (filled = 1 < want), then reaches the ring-drain step in the SAME
    //     call.
    let mut issued: Vec<*mut u8> = Vec::with_capacity(want * 4);
    for _ in 0..(want * 4) {
        let q = unsafe { (*heap).alloc(layout) };
        if q.is_null() {
            break;
        }
        issued.push(q);
    }

    // (7) P must appear at most once across the whole batch.
    let p_count = issued.iter().filter(|&&q| q == p).count();
    let p_positions: Vec<usize> = issued
        .iter()
        .enumerate()
        .filter(|(_, &q)| q == p)
        .map(|(i, _)| i)
        .collect();
    assert!(
        p_count <= 1,
        "P was double-issued ({p_count} times) at positions {p_positions:?} — \
         the refill-window in-out-buffer leg (task R1 / retro C1) is open: \
         the ring drain relinked a block already pulled into out[0..filled] \
         and the same refill loop pulled it again"
    );

    // Cleanup (best-effort; under the bug the heap state may already be
    // corrupt). Dealloc every distinct pointer at most once.
    let mut seen: Vec<*mut u8> = Vec::with_capacity(issued.len() + leftovers.len());
    for &q in issued.iter().chain(leftovers.iter()) {
        if !q.is_null() && !seen.contains(&q) {
            seen.push(q);
            unsafe { (*heap).dealloc(q, layout) };
        }
    }
    unsafe { HeapRegistry::recycle(heap) };
}
