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
//! # Status
//!
//! Task #164 NARROWED the residual: the **in-magazine leg** (P still resident
//! in `tcache.slots` when the drain runs) is now closed — the drain's
//! `reclaim_offset_checked` consults the magazine predicate and drops the
//! ring entry. See `drain_resident_xthread_double_free_no_corruption` for the
//! GREEN regression test of that fix.
//!
//! This test (`residual_xthread_double_free_no_corruption`) pins the
//! **re-issue-before-drain** leg: P is popped from the magazine (step 4)
//! before the drain runs (step 5). This leg is PROVEN information-
//! theoretically indistinguishable from a delayed remote free of the current
//! lifetime without per-block generations — see
//! `docs/design/RING_MAGAZINE_XTHREAD_DOUBLE_FREE_FIX.md` §8. It remains
//! RED + `#[ignore]`d under non-hardened profiles (the theorem still holds
//! without generations — no distinguishing state exists).
//!
//! **X7 Ф3 (task #191) — the hardened closure.** Under `--features
//! "production hardened"` the per-block generation guard (X7 plan §3-Ф3)
//! makes this interleaving DECIDABLE: the ring note stamped at remote-free
//! time carries the block's THEN-current generation; the drain compares it
//! against the block's NOW-current generation (advanced by `bump_gen` at the
//! issue pop); a mismatch means the note refers to a PAST life and is
//! dropped. The sibling test `residual_xthread_double_free_no_corruption_hardened`
//! (below, `#[cfg(feature = "hardened")]`) runs the SAME A→B→I→D interleaving
//! and asserts NO corruption — it is GREEN under hardened. The original test
//! above stays `#[ignore]`d under ALL profiles (honestly red without
//! generations, the theorem's permanent territory).
//!
//! (The sentinel-clobber assertion (a) trips first; if a fix only addressed
//! the clobber but not the double-issue, assertion (b) would guard that leg.)
//!
//! # R6-MS-1/2 — the `#[ignore]`d residual test was removed (moot)
//!
//! `AllocCore::dealloc` / `HeapCore::dealloc` are now `unsafe fn` (R6-MS-1/2),
//! so a genuine cross-thread DOUBLE-FREE — the setup this file's scenarios
//! rely on (step 3's own-thread `dealloc(P)` after step 2's simulated remote
//! free of the same P) — is **documented caller UB under the `unsafe fn`
//! contract**, not an allocator soundness bug a safe caller can trigger. The
//! former `#[ignore]`d `residual_xthread_double_free_no_corruption` pinned the
//! RED state of that residual as a *soundness* gap; that framing is no longer
//! accurate, so the test was removed (its closure — the per-block generation
//! guard — is still proven GREEN by the hardened sibling
//! `residual_xthread_double_free_no_corruption_hardened` below).
//!
//! The remaining tests here are **defence-in-depth regression tests** for the
//! RETAINED M2/#164/X7 defensive paths: their deliberate double-free setup is
//! caller UB by contract, but the allocator's defensive drain logic must still
//! degrade it benignly (no `write_next` clobber, no double-issue). They are NOT
//! soundness claims — they pin the defensive behaviour the crate still provides
//! on top of the `unsafe fn` contract.

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

// Only used by `residual_xthread_double_free_no_corruption_hardened` below
// (`#[cfg(feature = "hardened")]`) — without `hardened` (e.g. `production`
// alone) this constant is otherwise unused.
#[cfg(feature = "hardened")]
const SENTINEL: usize = 0x5EFE_5EFE_5EFE_5EFE;

// R6-MS-1/2: the former `#[ignore]`d `residual_xthread_double_free_no_corruption`
// test lived here and was removed — see the file-level note above (the
// re-issue-before-drain double-free residual is now caller UB under the
// `unsafe fn` contract; the GREEN hardened sibling below covers the closure).

const SENTINEL2: usize = 0xCAFE_BABE_DEAD_BEEF;

/// Regression test (task #164, in-magazine leg): a block P that is
/// simultaneously in the magazine AND in the ring is detected by the drain's
/// magazine predicate and DROPPED (not linked to the BinTable). P is never
/// double-issued.
///
/// # Timeline
///
/// 1. `alloc(P)` — P is live (bitmap: allocated).
/// 2. `dbg_push_to_ring(P, c)` — simulate remote cross-thread free; P enters
///    the ring. Bitmap untouched (still "allocated").
/// 3. `dealloc(P)` — own-thread free. Both M2 oracles blind (P not in `slots`;
///    bitmap still "allocated") → P pushed to magazine. P is now BOTH in the
///    magazine AND in the ring — the bug state.
/// 4. `dbg_drain_all_rings()` — the drain finds P's ring entry.
///    `reclaim_offset_checked` sees bitmap "allocated" → consults the magazine
///    predicate → P IS in `slots[c]` → returns false WITHOUT `write_next`.
///    The ring entry is dropped. P stays in the magazine as the sole copy.
/// 5. `alloc()` batch — P is issued exactly ONCE (from the magazine pop).
///    Write a sentinel into P after the single issue.
/// 6. Continue allocating — P must never appear again.
///
/// # Counterfactual (non-vacuous)
///
/// Without the #164 magazine predicate, `reclaim_offset` links P onto the
/// BinTable (`write_next` + `set_head` + `mark_free`). P is then issuable
/// from BOTH the magazine (pop) AND the BinTable (freelist pop). The batch
/// alloc returns P TWICE → assertion (b) fails.
#[test]
fn drain_resident_xthread_double_free_no_corruption() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    let layout = Layout::from_size_align(16, 8).unwrap();

    let c = unsafe { (*heap).dbg_class_for(layout) }.expect("16/8 must be a small class");

    // (1) alloc P.
    let p = unsafe { (*heap).alloc(layout) };
    assert!(!p.is_null());

    // (2) simulate the REMOTE cross-thread free of P.
    // SAFETY (R6-MS-4 + raw-deref): `(*heap)` is the live heap claimed above;
    // `p` is a live allocation it owns; `c` is `p`'s actual class. This push +
    // the dealloc in step (3) DELIBERATELY double-manage `p` to exercise the
    // #164 magazine defensive guard: at drain (4) `reclaim_offset_checked`'s
    // magazine predicate detects `p` is magazine-resident and drops the stale
    // note (no `write_next`/`mark_free`), so `p` is issued exactly once in (5).
    // Sound under this file's `fastbin` gate; a contract-stress of the drain
    // guard, not a contract-honoring single remote free.
    let pushed = unsafe { (*heap).dbg_push_to_ring(p, c) };
    assert!(pushed, "ring push failed (ring full or P not owned)");

    // (3) own-thread free P → lands in the magazine (both oracles blind).
    unsafe { (*heap).dealloc(p, layout) };

    // (4) drain — P is STILL in the magazine (no alloc/pop between steps 3-4).
    // The drain must detect P as magazine-resident and DROP the ring entry.
    unsafe { (*heap).dbg_drain_all_rings() };

    // (5) alloc a batch. P should be issued EXACTLY ONCE (from the magazine).
    let mut issued: Vec<*mut u8> = Vec::with_capacity(64);
    for _ in 0..64 {
        let q = unsafe { (*heap).alloc(layout) };
        if q.is_null() {
            break;
        }
        issued.push(q);
    }

    // (a) P must appear exactly once in the batch.
    let p_count = issued.iter().filter(|&&q| q == p).count();
    assert_eq!(
        p_count, 1,
        "P was issued {p_count} times (expected exactly 1); without the #164 \
         fix the drain links magazine-resident P onto the BinTable → double-issue"
    );

    // (b) write a sentinel into P after the single re-issue and verify
    // subsequent state is consistent (P's word0 was not clobbered by the
    // drain — `write_next` never ran).
    let p_idx = issued.iter().position(|&q| q == p).unwrap();
    let p_ptr = issued[p_idx];
    unsafe { (p_ptr as *mut usize).write(SENTINEL2) };
    let word0 = unsafe { (p_ptr as *mut usize).read() };
    assert_eq!(
        word0, SENTINEL2,
        "P's word0 was clobbered after the drain (expected sentinel {SENTINEL2:#018x}, \
         got {word0:#018x})"
    );

    // Cleanup.
    for &q in &issued {
        unsafe { (*heap).dealloc(q, layout) };
    }
    unsafe { HeapRegistry::recycle(heap) };
}

/// Regression test (task #164, realloc drain path): the magazine predicate
/// must also fire when the drain runs via the REALLOC slow path
/// (`HeapCore::realloc` → `try_realloc_inplace` miss → `HeapCore::alloc` →
/// magazine refill → `find_segment_with_free_checked` drain).
///
/// # Timeline
///
/// 1. `alloc(P, 16/8)` — P is live, class c0.
/// 2. `dbg_push_to_ring(P, c0)` — simulate remote cross-thread free.
/// 3. `dealloc(P, 16/8)` — own-thread free, P enters magazine for c0.
///    P is now BOTH magazine-resident AND in the ring.
/// 4. `alloc(Q, 64/8)` — a DIFFERENT class c1. Q is live.
/// 5. `realloc(Q, 64/8, 256)` — cross-class realloc. The in-place fast path
///    fails (class changes). The alloc leg needs a block of the new class.
///    If the new class's freelist is empty, the refill drains ALL owned
///    segments' rings via `find_segment_with_free_checked`. P's ring entry
///    is encountered; the magazine predicate detects P in `slots[c0]` and
///    drops the entry.
/// 6. Alloc a batch of class c0 — P must appear exactly once (magazine pop).
///
/// # Counterfactual (non-vacuous)
///
/// Without the #164 realloc routing fix (if the realloc slow path used
/// `AllocCore::realloc` → `alloc_small` → unchecked drain), P's ring entry
/// would be reclaimed blind (`write_next` + `mark_free`), double-issuing P.
/// The batch alloc would return P TWICE → assertion fails.
#[test]
fn realloc_path_drain_respects_magazine() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());

    let layout_small = Layout::from_size_align(16, 8).unwrap();
    let layout_med = Layout::from_size_align(64, 8).unwrap();

    let c0 = unsafe { (*heap).dbg_class_for(layout_small) }.expect("16/8 must be small");

    // (1) alloc P (class c0).
    let p = unsafe { (*heap).alloc(layout_small) };
    assert!(!p.is_null());

    // (2) simulate remote cross-thread free of P.
    // SAFETY (R6-MS-4 + raw-deref): `(*heap)` is the live heap claimed above;
    // `p` is a live allocation it owns; `c0` is `p`'s actual class. This push +
    // the dealloc in step (3) DELIBERATELY double-manage `p` to exercise the
    // #164 magazine defensive guard (sound under this file's `fastbin` gate): at
    // drain the magazine predicate drops the stale note, so `p` is issued
    // exactly once in (6). A contract-stress of the drain guard, not a
    // contract-honoring single remote free.
    let pushed = unsafe { (*heap).dbg_push_to_ring(p, c0) };
    assert!(pushed, "ring push failed");

    // (3) own-thread free P → magazine (both oracles blind).
    unsafe { (*heap).dealloc(p, layout_small) };

    // (4) alloc Q in a different class.
    let q = unsafe { (*heap).alloc(layout_med) };
    assert!(!q.is_null());

    // (5) cross-class realloc of Q. The alloc leg may drain rings.
    let q2 = unsafe { (*heap).realloc(q, layout_med, 256) };
    assert!(!q2.is_null());

    // (6) alloc a batch of class c0 — P must appear exactly once.
    let mut issued: Vec<*mut u8> = Vec::with_capacity(64);
    for _ in 0..64 {
        let r = unsafe { (*heap).alloc(layout_small) };
        if r.is_null() {
            break;
        }
        issued.push(r);
    }

    let p_count = issued.iter().filter(|&&r| r == p).count();
    assert_eq!(
        p_count, 1,
        "P was issued {p_count} times (expected 1); the realloc drain path \
         linked magazine-resident P onto the BinTable → double-issue"
    );

    // Cleanup.
    for &r in &issued {
        unsafe { (*heap).dealloc(r, layout_small) };
    }
    if !q2.is_null() {
        unsafe { (*heap).dealloc(q2, Layout::from_size_align(256, 8).unwrap()) };
    }
    unsafe { HeapRegistry::recycle(heap) };
}

/// **X7 Ф3 (task #191) — the hardened closure of the re-issue-before-drain
/// residual (leg 3).** This is the SUCCESS CRITERION of the entire X7 arc:
/// the SAME A→B→I→D interleaving that is permanently RED without generations
/// (pinned by `residual_xthread_double_free_no_corruption` above) becomes
/// GREEN under `--features "production hardened"` — the per-block generation
/// guard makes the previously-undecidable state decidable.
///
/// # Why this is GREEN under hardened (and the sibling stays RED)
///
/// The three X7 Ф3 touches (see `docs/design/X7_GENERATIONAL_RING_PLAN.md`
/// §3-Ф3) close the window:
///
/// 1. **Touch (a) — bump at ISSUE:** when step (4) pops P from the magazine
///    and hands it to the caller, `bump_gen(P)` advances P's generation
///    counter from N to N+1. P is now in its (N+1)-th life.
/// 2. **Touch (b) — stamp at REMOTE FREE:** when step (2) pushed P's offset
///    into the ring (simulating the remote free), `pack_entry_hardened`
///    stamped P's THEN-current generation (N) into the ring note.
/// 3. **Touch (c) — compare at DRAIN:** when step (5) drains the stale ring
///    entry, `reclaim_offset_checked` reads P's CURRENT generation (N+1) and
///    compares it against the stamped generation (N). They MISMATCH → the
///    note refers to a PAST life → it is DROPPED (return false, no
///    `write_next`, no `mark_free`, no `dec_live`). P's live word0 (the
///    sentinel) survives, and P is never linked onto the BinTable → no
///    double-issue.
///
/// # Counterfactual (non-vacuous)
///
/// Without touch (c) — if the gen-comparison were removed — the drain would
/// `write_next(P, old_head)`, clobbering P's sentinel word0, and link P onto
/// the BinTable → P double-issuable. Assertion (a) (sentinel survives) AND
/// assertion (b) (P issued ≤ 1 time) would both fail. The counterfactual
/// proof (break touch (c), confirm RED, restore, confirm GREEN) is in the
/// Ф3 final report.
///
/// # cfg gate
///
/// `#[cfg(feature = "hardened")]`: compiled ONLY under hardened. Under
/// non-hardened the generation table does not exist, the ring entry is not
/// stamped, and the drain has no gen to compare — the test would fail
/// (correctly: the theorem holds without generations). The sibling test above
/// pins that permanent RED state.
#[cfg(feature = "hardened")]
#[test]
fn residual_xthread_double_free_no_corruption_hardened() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    let layout = Layout::from_size_align(16, 8).unwrap();

    let c = unsafe { (*heap).dbg_class_for(layout) }.expect("16/8 must be a small class");

    // (1) alloc P. Under hardened, touch (a) bumps P's generation to 1
    //     (0 → 1 at the issue pop).
    let p = unsafe { (*heap).alloc(layout) };
    assert!(!p.is_null());

    // (2) simulate the REMOTE cross-thread free of P. Under hardened, touch
    //     (b) stamps P's CURRENT generation (1) into the ring note.
    // SAFETY (R6-MS-4 + raw-deref): `(*heap)` is the live heap claimed above;
    // `p` is a live allocation it owns; `c` is `p`'s actual class. This push +
    // the dealloc (3) + alloc-reissue (4) DELIBERATELY construct the stale-note
    // hazard to exercise the generation defensive guard (this test is
    // `#[cfg(feature = "hardened")]`): at drain (5) the stamped gen (1) != P's
    // current gen (2) → the note is dropped (no `write_next`), so P's sentinel
    // survives. Sound under `hardened`; a contract-stress of the drain guard,
    // not a contract-honoring single remote free.
    let pushed = unsafe { (*heap).dbg_push_to_ring(p, c) };
    assert!(pushed, "ring push failed (ring full or P not owned)");

    // (3) own-thread free P → lands in the magazine (both oracles blind).
    unsafe { (*heap).dealloc(p, layout) };

    // (4) alloc once → pops P from the magazine (LIFO). Under hardened, touch
    //     (a) bumps P's generation again (1 → 2). P is now in its 2nd life.
    //     Write a sentinel into its word0.
    let p2 = unsafe { (*heap).alloc(layout) };
    assert!(!p2.is_null());
    assert_eq!(
        p2, p,
        "expected the magazine to re-issue P (LIFO); the repro relies on it"
    );
    unsafe { (p2 as *mut usize).write(SENTINEL) };

    // (5) drain all rings → the stale ring entry (stamped gen=1) is compared
    //     against P's current gen (=2). MISMATCH → the note is DROPPED.
    //     write_next NEVER runs. P's sentinel survives.
    unsafe { (*heap).dbg_drain_all_rings() };

    // ── Assert CORRECTNESS (GREEN under hardened) ────────────────────────
    // (a) P's live user bytes must NOT have been clobbered by the drain.
    let word0 = unsafe { (p2 as *mut usize).read() };
    assert_eq!(
        word0, SENTINEL,
        "P's sentinel word0 was CLOBBERED by the ring drain's write_next \
         (X7 hardened gen-guard failed to drop the stale note): expected \
         {SENTINEL:#018x}, got {word0:#018x}"
    );

    // (b) a following alloc batch must never return P twice (no double-issue).
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
        "P was double-issued ({p_count} times) after the ring drain — the \
         X7 hardened gen-guard failed to drop the stale note and linked a \
         live block onto the BinTable"
    );

    // Cleanup.
    for &q in &issued {
        if q != p {
            unsafe { (*heap).dealloc(q, layout) };
        }
    }
    unsafe { HeapRegistry::recycle(heap) };
}
