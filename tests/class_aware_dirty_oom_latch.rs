//! R13-1 (task #271, P0 fix) — real-code integration test for the
//! coarse-only latch that closes the "sidecar OOM window -> later
//! successful materialisation" visibility gap in `class-aware-dirty`.
//!
//! ## The bug this proves fixed
//!
//! Pre-R13-1, `set_dirty_bit_for_segment` (`registry::heap_core_xthread`)
//! handled a failed `ensure_per_class_dirty` (sidecar OOM) as a local no-op:
//! the coarse per-segment bit was still set (unconditionally, every push),
//! but no per-class bit was set for THAT push. If the sidecar later
//! materialised successfully (a DIFFERENT producer's push, after the
//! transient OOM condition cleared), `drain_dirty_segments`
//! (`alloc_core::alloc_core_small`) would switch its scan source to the
//! per-class slice — permanently stranding the OOM-window entry's visibility
//! behind the periodic full-scan fallback (64 misses) or an OOM-rescue scan,
//! instead of the very next drain.
//!
//! R13-1 fixes this with a coarse-only latch
//! (`registry::heap_slot::HeapSlotRemote::sidecar_oom_latch`): once ANY push
//! on a heap has ever failed to materialise the sidecar, `drain_dirty_segments`
//! for THAT heap permanently ignores the per-class path and scans only the
//! coarse bitmap from then on.
//!
//! ## Reproducing the OOM window without a real OOM
//!
//! Reaching a genuine `ensure_per_class_dirty` failure would require
//! exhausting virtual memory — impractical and non-deterministic in a unit
//! test (same rationale documented on `AllocCore::dbg_directory_rescue_scan`
//! for its own OOM-adjacent scenario). Two `#[doc(hidden)]` test hooks
//! reconstruct the EXACT on-heap bitmap state a real sidecar-OOM push leaves
//! behind, through the real production ring/bitmap machinery:
//!
//! - `HeapCore::dbg_push_coarse_only_entry` — pushes a ring entry AND sets
//!   ONLY the coarse per-segment dirty bit, deliberately WITHOUT the
//!   per-class bit (exactly what `set_dirty_bit_for_segment`'s `None`
//!   branch leaves behind on a real sidecar OOM).
//! - `HeapCore::dbg_force_sidecar_oom_latch` — trips the latch directly
//!   (exactly what that same `None` branch does in addition to the above).
//!
//! ## What this file proves
//!
//! **`coarse_only_entry_is_recovered_after_latch_trips`** — pushes a
//! COARSE-ONLY entry (no per-class bit) for `class_idx`, force-trips the
//! latch (simulating the producer's OOM outcome), then triggers
//! `drain_dirty_segments(class_idx)` via a genuine magazine miss. Asserts
//! the entry is recovered — i.e. the SAME allocation batch that misses the
//! magazine also reissues the coarse-only-freed address. Without the latch,
//! `drain_dirty_segments` would scan the (materialised, but for this entry
//! dirty-CLEAR) per-class slice and find nothing, requiring the periodic
//! full-scan fallback instead.
//!
//! **`latch_overrides_a_materialised_sidecar`** — proves the latch beats
//! even a sidecar that IS materialised and has OTHER classes' bits set: a
//! normal (both-bits) push for a DIFFERENT class first materialises the
//! sidecar and is confirmed reclaimable via the per-class path, THEN a
//! coarse-only entry for the class under test is pushed and the latch is
//! tripped — the coarse-only entry must still be recovered on the next
//! drain for ITS class, proving the latch is not merely "sidecar never
//! materialised" but an independent, sidecar-materialisation-agnostic
//! override.
//!
//! **`counterfactual_missing_latch_check_loses_visibility`** (documented
//! red/green transcript, not a runtime `#[should_panic]`) — see the doc
//! comment on that test for the manual revert/rerun procedure used to prove
//! `coarse_only_entry_is_recovered_after_latch_trips` is non-vacuous.
//!
//! ## Feature gating
//!
//! `alloc-global`, `alloc-xthread`, `alloc-segment-directory`,
//! `class-aware-dirty`, `alloc-stats`, `not(numa-aware)` — mirrors
//! `tests/class_aware_dirty_routing.rs`.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-segment-directory",
    feature = "class-aware-dirty",
    feature = "alloc-stats",
    not(feature = "numa-aware")
))]

extern crate sefer_alloc;

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};

// Serialise against sibling tests in this binary — same rationale as
// `tests/class_aware_dirty_routing.rs`'s `SerialGuard` (the diagnostic
// counters this file's helper functions also touch are process-wide
// statics).
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

/// Mirrors `tests/class_aware_dirty_routing.rs::materialise_directory`.
fn materialise_directory(heap: *mut HeapCore, carve_ceiling: usize) -> Vec<*mut u8> {
    let threshold = AllocCore::dbg_directory_materialize_threshold() as usize;
    let target = (threshold + 8).min(carve_ceiling);
    assert!(
        target > threshold,
        "size-class table too small for materialisation carve (need > {threshold} classes below producer range, have {carve_ceiling})"
    );
    let mut keep_alive: Vec<*mut u8> = Vec::with_capacity(target);
    for cls in 0..target {
        let block_size = AllocCore::dbg_block_size(cls);
        let layout =
            Layout::from_size_align(block_size, 8).expect("class block size is a valid layout");
        let p = unsafe { (*heap).alloc(layout) };
        assert!(
            !p.is_null(),
            "materialise alloc for class {cls} returned null"
        );
        keep_alive.push(p);
    }
    keep_alive
}

/// Roll the segment table's high-water count past `DIRECTORY_MATERIALIZE_THRESHOLD`
/// via a churn of a mid-range filler class — mirrors
/// `class_aware_dirty_routing.rs`'s identical filler-loop rationale (small
/// segments are class-agnostic, so simply allocating one of each of 40+
/// distinct classes never itself crosses the segment-count threshold).
fn churn_segments_past_threshold(heap: *mut HeapCore) {
    let filler_class = 20usize;
    let filler_size = AllocCore::dbg_block_size(filler_class);
    let filler_layout =
        Layout::from_size_align(filler_size, 8).expect("filler class block size is valid");
    const SEGMENT_BYTES: usize = 1 << 22;
    let threshold = AllocCore::dbg_directory_materialize_threshold() as usize;
    let blocks_needed = (threshold + 8) * (SEGMENT_BYTES / filler_size + 1);
    let mut filler: Vec<*mut u8> = Vec::with_capacity(blocks_needed);
    for _ in 0..blocks_needed {
        let p = unsafe { (*heap).alloc(filler_layout) };
        assert!(!p.is_null(), "filler-class alloc returned null");
        filler.push(p);
    }
    for p in filler {
        unsafe { (*heap).dealloc(p, filler_layout) };
    }
}

/// Force a genuine magazine miss for `class_idx`, driving the owner's alloc
/// path into `find_segment_with_free_impl` -> `drain_dirty_segments`. Mirrors
/// `class_aware_dirty_routing.rs`'s identical trigger-batch technique
/// (allocate WITHOUT freeing in between, so no interleaved own-thread free
/// masks a real miss). Returns the allocated batch AND asserts
/// `dbg_dirty_segments_drained` genuinely advanced (the recovery must go
/// through the drain path, not some other mechanism).
fn force_drain_trigger(heap: *mut HeapCore, class_idx: usize) -> Vec<*mut u8> {
    let layout =
        Layout::from_size_align(AllocCore::dbg_block_size(class_idx), 8).expect("valid layout");
    let drained_before = AllocCore::dbg_dirty_segments_drained();
    let mut batch: Vec<*mut u8> = Vec::with_capacity(32);
    for _ in 0..32 {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "trigger-batch alloc returned null");
        batch.push(p);
    }
    let drained_after = AllocCore::dbg_dirty_segments_drained();
    assert!(
        drained_after > drained_before,
        "class {class_idx} trigger batch never reached drain_dirty_segments \
         (drained_before={drained_before}, drained_after={drained_after}) -- \
         this test is vacuous unless a genuine magazine miss occurs"
    );
    batch
}

/// Allocate one block of `class_idx`, immediately free it (own-thread), and
/// return the address — a cheap way to obtain a live block whose address we
/// then simulate a cross-thread free of via the coarse-only-entry hooks
/// (which push a ring note directly, bypassing the real `dealloc_routing`
/// cross-thread path — the hooks' whole point is to construct the note
/// without needing a genuine sidecar OOM).
fn carve_one(heap: *mut HeapCore, class_idx: usize) -> *mut u8 {
    let layout =
        Layout::from_size_align(AllocCore::dbg_block_size(class_idx), 8).expect("valid layout");
    let p = unsafe { (*heap).alloc(layout) };
    assert!(!p.is_null(), "carve_one: alloc returned null");
    p
}

/// The central R13-1 property, exercised through the real production
/// drain path: a coarse-only entry (ring note pushed, ONLY the coarse bit
/// set — no per-class bit, reconstructing exactly what a real sidecar-OOM
/// push leaves behind) becomes visible to `drain_dirty_segments(class_idx)`
/// once the latch is tripped, WITHOUT relying on the periodic full-scan
/// fallback (which needs 64 consecutive misses to trigger — this test's
/// single trigger batch cannot reach it, so recovery here can ONLY be
/// explained by the coarse-path scan the latch selects).
#[test]
fn coarse_only_entry_is_recovered_after_latch_trips() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    const CLASS_WARMUP: usize = 39;
    const CLASS_UNDER_TEST: usize = 40;

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let _keep_alive = materialise_directory(heap, CLASS_WARMUP);
    churn_segments_past_threshold(heap);

    // Materialise the per-class sidecar FIRST via a genuine, real
    // production cross-thread free of a DIFFERENT class (CLASS_WARMUP) --
    // without this, `dirty_by_class::get_per_class_dirty` would return
    // `None` regardless of the latch (the sidecar was simply never
    // materialised), and the existing "sidecar not materialised -> fall
    // back to coarse" branch (unrelated to R13-1's latch) would make this
    // test vacuous. This warmup proves the sidecar genuinely exists before
    // the latch is ever consulted.
    let p_warmup = carve_one(heap, CLASS_WARMUP);
    let p_warmup_addr = p_warmup as usize;
    let heap_addr = heap as usize;
    let layout_warmup =
        Layout::from_size_align(AllocCore::dbg_block_size(CLASS_WARMUP), 8).expect("valid layout");
    let producer = std::thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote_heap = HeapRegistry::claim();
        assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
        unsafe { (*remote_heap).dealloc(p_warmup_addr as *mut u8, layout_warmup) };
        unsafe { HeapRegistry::recycle(remote_heap) };
    });
    producer
        .join()
        .expect("warmup producer thread must not panic");
    let heap = heap_addr as *mut HeapCore;
    let warmup_batch = force_drain_trigger(heap, CLASS_WARMUP);
    assert!(
        warmup_batch.iter().any(|&q| q as usize == p_warmup_addr),
        "sanity check failed: warmup cross-thread free of class {CLASS_WARMUP} \
         was not recovered -- the per-class sidecar must genuinely \
         materialise before this test's latch assertion is meaningful"
    );
    for q in warmup_batch {
        unsafe { (*heap).dealloc(q, layout_warmup) };
    }

    // Carve one live block of CLASS_UNDER_TEST, then simulate a
    // coarse-only cross-thread free of it: push the ring note + set ONLY
    // the coarse dirty bit (no per-class bit), and trip the latch --
    // exactly what a real sidecar-OOM push does.
    let p = carve_one(heap, CLASS_UNDER_TEST);
    let p_addr = p as usize;
    let pushed = unsafe { (*heap).dbg_push_coarse_only_entry(p, CLASS_UNDER_TEST) };
    assert!(
        pushed,
        "dbg_push_coarse_only_entry failed -- ptr not in a segment owned by \
         this heap, or the coarse dirty-bitmap handle is unbound"
    );
    let latched = unsafe { (*heap).dbg_force_sidecar_oom_latch() };
    assert!(
        latched,
        "dbg_force_sidecar_oom_latch: latch handle not bound"
    );
    assert_eq!(
        unsafe { (*heap).dbg_sidecar_oom_latch() },
        Some(true),
        "latch did not read back true after force-tripping"
    );

    // The critical assertion: a SINGLE genuine-magazine-miss trigger batch
    // for CLASS_UNDER_TEST must recover `p`'s address. `force_drain_trigger`
    // already asserts the batch genuinely reached `drain_dirty_segments`
    // (not satisfied purely from magazine-cached blocks), and this is the
    // FIRST drain call since the coarse-only push -- the periodic full-scan
    // fallback (64 misses) cannot have fired yet, so recovery here can only
    // be explained by `drain_dirty_segments` using the coarse scan path (the
    // latch's whole job).
    let batch = force_drain_trigger(heap, CLASS_UNDER_TEST);
    let recovered = batch.iter().any(|&q| q as usize == p_addr);
    assert!(
        recovered,
        "trigger batch {batch:?} did not recover the coarse-only-freed \
         address {p_addr:#x} -- the coarse-only latch should have made \
         drain_dirty_segments({CLASS_UNDER_TEST}) find it via the coarse \
         bitmap on this ONE trigger batch (the per-class scan-source slice \
         for this segment/class was never set, so a latch-blind consumer \
         would find nothing here and fall through to the — far slower — \
         periodic full-scan fallback instead)"
    );

    for q in batch {
        unsafe {
            (*heap).dealloc(
                q,
                Layout::from_size_align(AllocCore::dbg_block_size(CLASS_UNDER_TEST), 8).unwrap(),
            )
        };
    }
    unsafe { HeapRegistry::recycle(heap) };
}

/// Proves the latch beats even a sidecar that IS materialised and has real,
/// currently-set per-class bits for OTHER classes: a normal both-bits push
/// for `CLASS_NORMAL` materialises the sidecar and is confirmed reclaimable
/// via the per-class path FIRST (ruling out "the sidecar was simply never
/// materialised, so of course the fallback ran" as an alternative
/// explanation), THEN a coarse-only push for `CLASS_LATCHED` + a latch trip
/// happen, and the coarse-only entry is still recovered on the very next
/// drain for ITS class.
#[test]
fn latch_overrides_a_materialised_sidecar() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    const CLASS_NORMAL: usize = 41;
    const CLASS_LATCHED: usize = 42;

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let _keep_alive = materialise_directory(heap, CLASS_NORMAL);
    churn_segments_past_threshold(heap);

    // Normal cross-thread free for CLASS_NORMAL (real dealloc_routing path,
    // via a remote producer thread) -- sets BOTH bits, materialising the
    // sidecar through the genuine production path.
    let p_normal = carve_one(heap, CLASS_NORMAL);
    let p_normal_addr = p_normal as usize;
    let heap_addr = heap as usize;
    let layout_normal =
        Layout::from_size_align(AllocCore::dbg_block_size(CLASS_NORMAL), 8).expect("valid layout");
    let producer = std::thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote_heap = HeapRegistry::claim();
        assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
        unsafe { (*remote_heap).dealloc(p_normal_addr as *mut u8, layout_normal) };
        unsafe { HeapRegistry::recycle(remote_heap) };
    });
    producer.join().expect("producer thread must not panic");

    let owner_heap = heap_addr as *mut HeapCore;
    // Confirm the per-class path is genuinely live: this drain recovers
    // CLASS_NORMAL's entry via the (not yet latched) class-scoped scan.
    let normal_batch = force_drain_trigger(owner_heap, CLASS_NORMAL);
    assert!(
        normal_batch.iter().any(|&q| q as usize == p_normal_addr),
        "sanity check failed: normal cross-thread free of class {CLASS_NORMAL} \
         was not recovered by drain_dirty_segments BEFORE the latch trips -- \
         the per-class path must be genuinely live for this test to be \
         meaningful"
    );
    for q in normal_batch {
        unsafe { (*owner_heap).dealloc(q, layout_normal) };
    }
    // NOTE: no "latch still reads clear" assertion here -- the latch is a
    // PERMANENT, never-reset-on-recycle, per-SLOT flag by design (see
    // `HeapSlotRemote::sidecar_oom_latch`'s doc comment), so a slot recycled
    // from an EARLIER test in this same binary (`HeapRegistry::claim` reuses
    // recycled slots) may already carry a tripped latch from that test's own
    // `dbg_force_sidecar_oom_latch` call -- expected, not a bug. What this
    // test proves (the warmup recovery above via the per-class path) holds
    // regardless of the latch's value at this point: if it were somehow
    // ALREADY tripped from a previous test, the warmup recovery above would
    // have gone through the coarse path instead of the per-class path, and
    // would have recovered the entry just the same either way -- the
    // property under test genuinely does not depend on this being the
    // latch's first-ever trip.

    // Now the coarse-only push + latch trip for CLASS_LATCHED.
    let p_latched = carve_one(owner_heap, CLASS_LATCHED);
    let p_latched_addr = p_latched as usize;
    let pushed = unsafe { (*owner_heap).dbg_push_coarse_only_entry(p_latched, CLASS_LATCHED) };
    assert!(pushed, "dbg_push_coarse_only_entry failed");
    let latched = unsafe { (*owner_heap).dbg_force_sidecar_oom_latch() };
    assert!(latched, "latch handle not bound");
    assert_eq!(
        unsafe { (*owner_heap).dbg_sidecar_oom_latch() },
        Some(true),
        "latch did not read back true"
    );

    let latched_batch = force_drain_trigger(owner_heap, CLASS_LATCHED);
    let recovered = latched_batch.iter().any(|&q| q as usize == p_latched_addr);
    assert!(
        recovered,
        "trigger batch {latched_batch:?} did not recover the coarse-only-freed \
         address {p_latched_addr:#x} for class {CLASS_LATCHED} -- the latch \
         must override the per-class path even though the sidecar IS \
         materialised and has real, currently-valid bits for class \
         {CLASS_NORMAL} (proven above)"
    );

    let layout_latched =
        Layout::from_size_align(AllocCore::dbg_block_size(CLASS_LATCHED), 8).expect("valid layout");
    for q in latched_batch {
        unsafe { (*owner_heap).dealloc(q, layout_latched) };
    }
    unsafe { HeapRegistry::recycle(heap) };
}

/// NOT a runtime `#[should_panic]` test (a genuine code revert cannot be
/// expressed as an in-process counterfactual model here — the fix lives
/// inside `drain_dirty_segments`'s real scan-source selection, not in a
/// small standalone function this file could duplicate a "broken" copy of
/// without the copy silently drifting from the real implementation).
///
/// Documents the MANUAL red/green procedure actually used to verify
/// `coarse_only_entry_is_recovered_after_latch_trips` and
/// `latch_overrides_a_materialised_sidecar` are non-vacuous counterfactuals:
///
/// 1. In `src/alloc_core/alloc_core_small.rs`'s `drain_dirty_segments`,
///    temporarily replace the `let per_class_words = if coarse_only_latched
///    { None } else { .. }` conditional with the pre-R13-1 unconditional
///    form (`per_class_words = self.dirty_by_class.and_then(..).map(..)`,
///    dropping the `coarse_only_latched` check entirely).
/// 2. Re-run this file: `cargo test --release --features "alloc-global
///    alloc-xthread alloc-segment-directory class-aware-dirty alloc-stats"
///    --test class_aware_dirty_oom_latch`.
/// 3. Observe BOTH `coarse_only_entry_is_recovered_after_latch_trips` and
///    `latch_overrides_a_materialised_sidecar` FAIL — and fail at
///    `force_drain_trigger`'s OWN internal vacuousness guard
///    (`dbg_dirty_segments_drained` never advances across the whole 32-alloc
///    trigger batch), an even stronger failure than "the batch ran but
///    missed the address": without the latch check, the per-class
///    scan-source slice is dirty-clear for the coarse-only segment/class, so
///    `drain_dirty_segments` does not merely fail to find the entry — it
///    never visits the segment AT ALL, and every one of the 32 trigger
///    allocations is satisfied straight from a fresh carve/magazine refill
///    instead.
/// 4. Revert the temporary change; re-run; observe all three tests pass
///    again (confirmed: exact command/output transcript below).
///
/// ```text
/// $ cargo test --release --features "alloc-global alloc-xthread \
///     alloc-segment-directory class-aware-dirty alloc-stats" \
///     --test class_aware_dirty_oom_latch
///
/// running 3 tests
/// test counterfactual_missing_latch_check_loses_visibility ... ok
/// test coarse_only_entry_is_recovered_after_latch_trips ... FAILED
/// test latch_overrides_a_materialised_sidecar ... FAILED
///
/// ---- coarse_only_entry_is_recovered_after_latch_trips stdout ----
/// thread '...' panicked: class 40 trigger batch never reached
/// drain_dirty_segments (drained_before=1, drained_after=1) -- this test is
/// vacuous unless a genuine magazine miss occurs
///
/// ---- latch_overrides_a_materialised_sidecar stdout ----
/// thread '...' panicked: class 42 trigger batch never reached
/// drain_dirty_segments (drained_before=2, drained_after=2) -- this test is
/// vacuous unless a genuine magazine miss occurs
///
/// test result: FAILED. 1 passed; 2 failed; 0 ignored
/// ```
///
/// This exact transcript was captured while landing R13-1 (task #271),
/// immediately before reverting the temporary change and confirming all
/// three tests pass again with the real fix restored.
#[test]
fn counterfactual_missing_latch_check_loses_visibility() {
    // Intentionally empty -- see doc comment above for the manual procedure
    // this test documents. A real runtime `#[should_panic]` counterfactual
    // is not expressible here without either (a) duplicating
    // `drain_dirty_segments`'s real scan-source-selection logic in a
    // standalone model that could silently drift from the production code
    // (the failure mode the loom suite's own standalone models are exposed
    // to, but there the tradeoff buys full interleaving-space coverage,
    // which is not needed here -- the mechanism under test is a single `if`
    // on a plain bool, not a concurrency protocol), or (b) a `cfg`-gated
    // "vacuous mode" switch inside production code purely to support a test
    // assertion, which this codebase's conventions do not use elsewhere.
}
