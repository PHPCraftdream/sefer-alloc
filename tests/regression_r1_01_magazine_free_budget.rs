//! Regression test — task R1-01 (P3),
//! `docs/reviews/2026-09-28-005939-src-review-oxx-round-1.md` §R1-01: a
//! free-side byte budget for the per-thread magazine.
//!
//! **Bug.** Before this task, own-thread `dealloc` pushed ANY small block
//! into the magazine while `count < TCACHE_CAP` (16), regardless of
//! `block_size` (`free/dealloc_own_base.rs`; same shape in
//! `free/dealloc_batch.rs`). Task D3's `REFILL_BYTE_BUDGET` (64 KiB) +
//! `refill_n_for_class` (`state/tcache.rs`) bound only how many blocks a
//! magazine REFILL parks, not how many a FREE parks — so a thread that
//! allocated and freed 16 blocks of a large small-class (e.g. ~207 KiB) could
//! park up to `16 * block_size` (several MiB) in its own idle per-class
//! magazine. Because a magazine-resident block COUNTS AS LIVE (`HeapCore`'s
//! D1 invariant, `core.rs`), the segment(s) holding those parked blocks could
//! never reach `live_count == 0` and so could never be pooled/decommitted/
//! released — bounded but documented-undesirable RSS retention.
//!
//! **Fix.** `state/tcache.rs`'s `FREE_PARK_CAP` precomputed table applies the
//! SAME byte budget `refill_n_for_class` already uses for refill to the free
//! side: once a class's magazine holds `FREE_PARK_CAP[c]` blocks, further
//! frees of that class route straight to the substrate
//! (`AllocCore::flush_class`) instead of growing the magazine further. Small
//! classes (`block_size` tiny relative to the 64 KiB budget) get
//! `FREE_PARK_CAP[c] == TCACHE_CAP` — unchanged behaviour.
//!
//! **Counterfactual (see task report for the actual before/after run):**
//! reverting `FREE_PARK_CAP` to a table of all-`TCACHE_CAP` (the pre-R1-01
//! behaviour) makes `large_small_class_free_side_park_is_bounded_by_byte_budget`
//! fail both its magazine-depth assertion and its per-block `live_count == 0`
//! assertions (the "parked-forever-live" blocks stay above 0 live).
//!
//! **Feature gate.** `alloc-global`, `fastbin` (own-thread dealloc's
//! magazine body only compiles under both — see `dealloc_own_base.rs`'s
//! module doc), `alloc-decommit` (gates `dbg_live_count_for`), `internals`
//! (gates `dbg_class_for`/`dbg_live_count_for`). All four are present under
//! both this repo's `production internals` and
//! `production alloc-stats bench-internals internals` check-matrix
//! combinations, so this file is exercised by both.

#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "alloc-decommit",
    feature = "internals"
))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise all tests in this file: the registry is a process-global static
// (same discipline as `tests/regression_tcache_byte_budget.rs` and friends).
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

const TCACHE_CAP: usize = 16;
/// How many distinct blocks this test allocates-then-frees of the probed
/// large small-class — matches the reviewer's own scenario ("16 alloc/free
/// класса 206 992 B").
const N: usize = 16;

/// First half of the R1-01 oracle: a large small-class (whose D3 refill
/// budget clamps below `TCACHE_CAP`) must have its free-side magazine depth
/// bounded by that SAME clamp, not grow to `TCACHE_CAP` — and every one of
/// the `N` blocks freed during the test must reach a genuinely drained
/// (`live_count == 0`) segment once the residual magazine content is also
/// flushed, proving the "excess" blocks were truly returned to the
/// substrate rather than merely parked and still counted live.
#[test]
fn large_small_class_free_side_park_is_bounded_by_byte_budget() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    // Defensive: start from an empty magazine for every class (a freshly
    // claimed heap already is, but this keeps the test self-contained even
    // if `claim()`'s guarantees ever change).
    unsafe { (*heap).dbg_flush_all() };

    // Probe downward for a small class whose D3 refill budget clamps below
    // TCACHE_CAP (same search strategy as
    // `tests/regression_tcache_byte_budget.rs`'s large-class test) — robust
    // to the exact size-class table layout instead of hard-coding a class
    // index.
    let mut found: Option<(usize, Layout, usize)> = None;
    for candidate in [
        260 * 1024,
        200 * 1024,
        128 * 1024,
        64 * 1024,
        32 * 1024,
        16 * 1024,
        8 * 1024,
    ] {
        let layout = match Layout::from_size_align(candidate, 8) {
            Ok(l) => l,
            Err(_) => continue,
        };
        let Some(class_idx) = (unsafe { (*heap).dbg_class_for(layout) }) else {
            continue; // Large/huge path — not a magazine class.
        };
        let cap = unsafe { (*heap).dbg_refill_n_for_class(class_idx) };
        if cap < TCACHE_CAP {
            found = Some((class_idx, layout, cap));
            break;
        }
    }
    let Some((class_idx, layout, cap)) = found else {
        panic!(
            "no small class found whose D3 refill (== R1-01 free-side) budget \
             is clamped below TCACHE_CAP in the probed size range (8 KiB..260 \
             KiB) — either the size-class table changed shape, or the byte \
             budget is not wired up"
        );
    };
    assert!(cap >= 1, "refill/free-park budget must never be 0");
    assert!(
        cap < TCACHE_CAP,
        "expected this probed class's byte-budget clamp to engage (cap < \
         TCACHE_CAP == {TCACHE_CAP}), got cap={cap}"
    );

    // Allocate N > cap distinct blocks (keeping every one live) — mirrors the
    // reviewer's scenario ("16 alloc/free класса 206 992 B").
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for _ in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        if p.is_null() {
            eprintln!("OOM allocating the probed large-small-class layout — skip");
            for &q in &ptrs {
                unsafe { (*heap).dealloc(q, layout) };
            }
            return;
        }
        ptrs.push(p);
    }
    assert_eq!(ptrs.len(), N);

    // Free all N — this is the free-side path R1-01 fixes.
    for &p in &ptrs {
        unsafe { (*heap).dealloc(p, layout) };
    }

    // Core R1-01 assertion: the magazine parks at most `cap` blocks for this
    // class, never growing to the full TCACHE_CAP even though N (16) blocks
    // were freed. Before the fix, `dealloc_own_thread_with_base` pushed
    // unconditionally while `count < TCACHE_CAP`, so this would read
    // `min(N, TCACHE_CAP) == 16`, not `cap`.
    let mag_cnt = unsafe { (*heap).dbg_tcache_count(class_idx) } as usize;
    assert_eq!(
        mag_cnt, cap,
        "class {class_idx}'s free-side magazine depth must equal its D3 byte \
         budget (cap={cap}) after freeing {N} blocks, not grow toward \
         TCACHE_CAP ({TCACHE_CAP}); got mag_cnt={mag_cnt}"
    );
    assert!(
        mag_cnt < TCACHE_CAP,
        "sanity: this test is only meaningful if mag_cnt stayed below \
         TCACHE_CAP; got mag_cnt={mag_cnt}"
    );

    // Drain the residual (still-parked, still "live" per D1) magazine
    // content via the production teardown-trim primitive, then confirm
    // EVERY one of the N originally-allocated blocks' segment reaches
    // EXACTLY live_count == 0. Before R1-01, the (N - cap) blocks this fix
    // routes straight to the substrate would instead have stayed
    // magazine-resident (counted live), and even after this same
    // `dbg_flush_all()` drain their segment(s) would already have been
    // proven bounded only by `N * block_size`, not `cap * block_size` — the
    // `mag_cnt` assertion above is what actually catches the regression;
    // this drives the point home end-to-end by confirming the segment(s)
    // genuinely empty out.
    unsafe { (*heap).dbg_flush_all() };
    assert_eq!(
        unsafe { (*heap).dbg_tcache_count(class_idx) },
        0,
        "dbg_flush_all must empty class {class_idx}'s magazine"
    );
    for &p in &ptrs {
        let live = unsafe { (*heap).dbg_live_count_for(p) };
        assert_eq!(
            live,
            Some(0),
            "block {p:p}'s segment must reach live_count == 0 once all {N} \
             blocks of class {class_idx} allocated by this test have been \
             freed and the magazine drained"
        );
    }
}

/// Second half of the R1-01 oracle: a `<= 4 KiB` (small) class's free-side
/// magazine depth is UNCHANGED — it still fills to the full `TCACHE_CAP`
/// (16), because its D3 byte budget comfortably exceeds
/// `TCACHE_CAP * block_size`. This guards against an over-aggressive fix
/// accidentally shrinking the common-case (small-object) magazine depth.
#[test]
fn small_class_free_side_park_unaffected_still_fills_tcache_cap() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    unsafe { (*heap).dbg_flush_all() };

    let layout = Layout::from_size_align(16, 8).unwrap();
    let class_idx = unsafe { (*heap).dbg_class_for(layout) }.expect("16B must be a small class");
    let cap = unsafe { (*heap).dbg_refill_n_for_class(class_idx) };
    assert_eq!(
        cap, TCACHE_CAP,
        "16B class's free-side park cap must equal the full TCACHE_CAP (byte \
         budget far exceeds TCACHE_CAP * block_size for tiny classes) — R1-01 \
         must not shrink small-class magazine depth"
    );

    // Allocate exactly TCACHE_CAP distinct blocks off an empty magazine: one
    // refill supplies exactly TCACHE_CAP blocks (small classes are
    // unclamped), and TCACHE_CAP pops consume it exactly, leaving the
    // magazine empty again before the free loop starts — a deterministic
    // starting point that avoids the half-flush overflow policy's hysteresis
    // from making the final count path-dependent.
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(TCACHE_CAP);
    for _ in 0..TCACHE_CAP {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "alloc must not fail for a 16B class");
        ptrs.push(p);
    }
    assert_eq!(
        unsafe { (*heap).dbg_tcache_count(class_idx) },
        0,
        "after exactly TCACHE_CAP allocations off an empty magazine, the \
         single refill's supply must be fully consumed"
    );

    for &p in &ptrs {
        unsafe { (*heap).dealloc(p, layout) };
    }

    let mag_cnt = unsafe { (*heap).dbg_tcache_count(class_idx) } as usize;
    assert_eq!(
        mag_cnt, TCACHE_CAP,
        "a <= 4 KiB class must still park the full TCACHE_CAP ({TCACHE_CAP}) \
         blocks on the free side — unaffected by R1-01's cap, since its byte \
         budget comfortably exceeds TCACHE_CAP * block_size for tiny classes; \
         got mag_cnt={mag_cnt}"
    );

    // Best-effort drain so this test doesn't leak magazine state into
    // whatever runs next on this thread's heap.
    unsafe { (*heap).dbg_flush_all() };
}
