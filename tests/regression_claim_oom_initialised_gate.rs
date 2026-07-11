//! Regression (UBFIX-5, findings M-5 + L-9a from
//! `docs/reviews/2026-07-10-ub-audit-final-synthesis.md`): `claim` /
//! `claim_with_config`'s materialisation gate must be
//! `HeapSlot::initialised`, NOT `generation == 1`, and a materialisation
//! failure (OOM) must push the slot back to `free_slots` rather than leaking
//! it as permanently `LIVE`.
//!
//! ## The bugs this covers
//!
//! **M-5 (wrong gate):** the pre-fix `claim` gated `HeapCore` materialisation
//! on `new_gen == 1` (the FIRST bump of `generation` after a `FREE → LIVE`
//! CAS). `generation` is bumped unconditionally by every successful CAS,
//! including one that lands on a slot which was previously claimed-then-OOM'd
//! (materialisation attempted and failed, but the CAS + bump had already run).
//! On a slot's SECOND visit to the claim path after such an OOM, `new_gen`
//! reads `2`, `2 == 1` is false, and the pre-fix code would skip
//! materialisation entirely — handing out a `*mut HeapCore` that still points
//! at `MaybeUninit::uninit()` bytes. `HeapSlot::initialised` (see its doc
//! comment in `src/registry/heap_slot.rs`) is the field explicitly designed
//! to answer "has this slot's `HeapCore` actually been written", independent
//! of how many times `generation` has been bumped; gating on it instead is
//! the fix.
//!
//! **L-9a (unbounded recursion):** `claim`/`claim_with_config` used to retry
//! a lost `FREE → LIVE` CAS race via `return Self::claim()` (self-recursion),
//! with no bound on stack depth under sustained contention. The fix replaces
//! the tail call with a `loop`.
//!
//! ## Why this test cannot force a REAL OOM
//!
//! `HeapCore::new`/`new_with_config` return `None` only when the OS refuses a
//! primordial segment reservation — there is no test-only hook to force that
//! outcome without touching `alloc_core.rs`, which is out of this task's
//! scope (a different agent owns that file in this session — see the task
//! brief). Instead this test drives the EXACT slot-level state transition the
//! OOM branch performs via the `#[doc(hidden)]` test hook
//! `heap_registry::dbg_claim_then_simulate_oom` (claims a slot via the real
//! `pick_slot` + `FREE → LIVE` CAS + `generation` bump prelude, then pushes
//! it back to `FREE` WITHOUT writing `heap`/publishing `initialised` — i.e.
//! "claim, then simulate materialisation failing"), and asserts the two
//! properties that matter: (a) the slot is not leaked (a following `claim()`
//! can reach it again), and (b) that following `claim()` — which lands on a
//! slot whose `generation` already reads `>= 2` — still correctly
//! materialises a fresh, dereferenceable `HeapCore` rather than skipping
//! materialisation.
//!
//! ## Non-vacuousness (counterfactual)
//!
//! If the M-5 fix were reverted (gate back to `new_gen == 1`), test
//! `reclaim_after_simulated_oom_still_materialises` would fail: the
//! `dbg_claim_then_simulate_oom` hook's own `FREE→LIVE` CAS + bump already
//! advances `generation` to `1` on a never-before-claimed slot, so the
//! FOLLOWING real `claim()` on that same index bumps it to `2` — `2 == 1` is
//! false — and the pre-fix code would return a pointer without ever calling
//! `HeapCore::new`, i.e. without ever writing `slot.heap`. This test detects
//! that failure two ways: (1) directly, via `dbg_slot_initialised`, which
//! would read `false` after the "materialising" `claim()` returned (proving
//! materialisation was skipped) instead of `true`; (2) behaviourally, by
//! actually allocating through the returned heap pointer and confirming the
//! allocation succeeds and round-trips (a pointer into skipped-over
//! `MaybeUninit::uninit()` bytes reinterpreted as `HeapCore` would not behave
//! like a valid, freshly-constructed `AllocCore` — in practice this
//! manifests as a crash or corrupted allocation under the old code path, not
//! merely a silently-wrong assertion, which is why property (1) is the
//! primary, deterministic oracle and (2) is corroborating evidence).
//!
//! Test `simulated_oom_does_not_leak_the_slot` is non-vacuous against L-9a's
//! sibling defect (a leaked-forever `LIVE` slot): if `push_back_after_oom`
//! were removed from the OOM branch (the pre-M-5-fix shape), the slot would
//! stay `STATE_LIVE` and OFF `free_slots` forever, so a subsequent `claim()`
//! could never reuse that exact index via the free-list (it would only be
//! reachable again via `bump_count` minting an entirely NEW index) — this
//! test's slot-index re-use assertion would fail.

#![cfg(feature = "alloc-global")]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::heap_registry::{dbg_claim_then_simulate_oom, dbg_slot_initialised};
use sefer_alloc::registry::{bootstrap, heap_slot::STATE_FREE, HeapRegistry};

// Serialise against the other registry-touching test files in this crate
// (matches the discipline used throughout `tests/` — see `registry_basic.rs`).
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

/// Read a slot's `state` atomically (test helper, mirrors `registry_basic.rs`).
fn slot_state(idx: usize) -> u8 {
    let reg = bootstrap::ensure();
    reg.slots[idx].state.load(Ordering::Acquire)
}

/// Read a slot's `generation` atomically (test helper, mirrors `registry_basic.rs`).
fn slot_generation(idx: usize) -> u64 {
    let reg = bootstrap::ensure();
    reg.slots[idx].generation.load(Ordering::Acquire)
}

/// Drain `free_slots` by claiming (and deliberately NOT recycling) every
/// currently-recycled slot. `HeapSlot::initialised` sticks `true` forever
/// once a slot has been materialised once (see its doc comment: a slot's
/// `HeapCore` is reused as-is across every later `recycle` -> `claim`
/// cycle, never reset) -- so a test that needs to observe
/// `initialised == false` on the slot `dbg_claim_then_simulate_oom` touches
/// MUST make sure that hook mints a genuinely FRESH slot (via `bump_count`)
/// rather than popping a previously-materialised one back off `free_slots`
/// (`pick_slot` prefers the free list). Draining the free list first forces
/// exactly that. The claimed-and-leaked heaps are intentionally never
/// recycled in THIS helper (recycling would just refill the free list this
/// function exists to empty); each test that calls this is expected to be
/// the only thing touching the registry under the `SERIAL` guard, so the
/// leaked slots are harmless test-process-lifetime bookkeeping, not a
/// real leak (the registry itself is never torn down, per every other
/// `tests/registry_*` file's documented isolation model).
fn drain_free_slots_by_claiming() {
    loop {
        let before = HeapRegistry::claim();
        assert!(
            !before.is_null(),
            "registry must not be exhausted in a fresh test process"
        );
        // SAFETY: just claimed, not yet recycled.
        let idx = unsafe { (*before).id() };
        // If this claim minted a BRAND NEW slot (generation == 1 AND it is
        // the high-water index), free_slots was already empty -- stop here,
        // this heap is now the guaranteed-fresh boundary marker and we leave
        // it LIVE (not recycled) so free_slots stays empty for the caller.
        if slot_generation(idx as usize) == 1 {
            return;
        }
        // Otherwise this was a recycled slot popped off free_slots -- keep
        // draining (deliberately not recycling it back).
    }
}

/// (a) The `initialised` gate is correct: a slot that has been claimed-then-
/// simulated-OOM'd (so `generation >= 1` but `initialised == false`) is NOT
/// treated as "already materialised" merely because `generation != 1` on its
/// next visit. This is the M-5 defect's exact shape.
#[test]
fn simulated_oom_leaves_initialised_false() {
    let _serial = SerialGuard::acquire();
    drain_free_slots_by_claiming();

    let idx = dbg_claim_then_simulate_oom()
        .expect("registry must not be exhausted in a fresh test process");

    assert!(
        !dbg_slot_initialised(idx),
        "a slot whose materialisation was simulated to fail must read \
         initialised == false -- HeapCore::new was never called, so \
         `heap` was never written and `initialised` was never published"
    );
    assert_eq!(
        slot_state(idx as usize),
        STATE_FREE,
        "the OOM push-back must CAS the slot back to FREE (mirrors recycle)"
    );
    assert_eq!(
        slot_generation(idx as usize),
        1,
        "the simulated-OOM prelude bumps generation exactly once (the same \
         FREE->LIVE CAS + fetch_add claim performs), even though \
         materialisation never happened -- this is the exact state that \
         defeats a `generation == 1` gate on the NEXT claim of this slot"
    );
}

/// (b) OOM-on-materialisation does not leak the slot: after
/// `dbg_claim_then_simulate_oom`, the slot is back on `free_slots` and a
/// following `claim()` reuses that EXACT index (LIFO free-list behaviour),
/// not a freshly-minted one. This is the M-5 "push back" fix's own
/// postcondition, and the counterfactual for a reverted fix (see module doc).
#[test]
fn simulated_oom_does_not_leak_the_slot() {
    let _serial = SerialGuard::acquire();

    let freed_idx = dbg_claim_then_simulate_oom()
        .expect("registry must not be exhausted in a fresh test process");
    assert_eq!(slot_state(freed_idx as usize), STATE_FREE);

    // The next real claim must reuse this exact slot via the free_slots
    // LIFO stack -- if the OOM branch failed to push it back, this claim
    // would mint a BRAND NEW index via `bump_count` instead (the freed slot
    // would sit LIVE-but-unreachable forever).
    let heap = HeapRegistry::claim();
    assert!(
        !heap.is_null(),
        "claim after simulated OOM must not return null"
    );
    // SAFETY: `heap` was just returned by `claim`.
    let reclaimed_idx = unsafe { (*heap).id() };
    assert_eq!(
        reclaimed_idx, freed_idx,
        "claim() after a simulated OOM must reuse the SAME slot index via \
         free_slots (LIFO) -- if the slot were leaked (not pushed back), \
         this claim would mint an unrelated fresh index instead"
    );

    // SAFETY: `heap` was returned by `claim` and not yet recycled.
    unsafe { HeapRegistry::recycle(heap) };
}

/// (a)+(b) combined, end to end: after a simulated OOM, the following real
/// `claim()` on the SAME slot (whose `generation` already reads `>= 1` from
/// the simulated-OOM prelude, so the reclaiming `claim()` bumps it to `>= 2`)
/// must still fully materialise a working `HeapCore` -- both via the
/// `initialised` flag (direct oracle) and via an actual allocation through
/// the returned pointer (behavioural oracle). This is the primary
/// non-vacuous counterfactual for M-5: see the module doc comment for the
/// exact mechanics of why a reverted `new_gen == 1` gate fails this test.
#[test]
fn reclaim_after_simulated_oom_still_materialises() {
    let _serial = SerialGuard::acquire();
    drain_free_slots_by_claiming();

    let freed_idx = dbg_claim_then_simulate_oom()
        .expect("registry must not be exhausted in a fresh test process");
    assert!(!dbg_slot_initialised(freed_idx));
    let gen_before_reclaim = slot_generation(freed_idx as usize);
    assert_eq!(
        gen_before_reclaim, 1,
        "sanity: the simulated-OOM prelude bumped generation to 1 already"
    );

    let heap = HeapRegistry::claim();
    assert!(
        !heap.is_null(),
        "reclaim after simulated OOM must not return null"
    );
    // SAFETY: `heap` was just returned by `claim`.
    let reclaimed_idx = unsafe { (*heap).id() };
    assert_eq!(
        reclaimed_idx, freed_idx,
        "must reuse the same slot (free_slots LIFO)"
    );

    let gen_after_reclaim = slot_generation(reclaimed_idx as usize);
    assert_eq!(
        gen_after_reclaim,
        gen_before_reclaim + 1,
        "the reclaiming claim must bump generation again (now >= 2) -- this \
         is exactly the value that would defeat a `new_gen == 1` gate"
    );

    // Direct oracle: initialised must now be true -- materialisation MUST
    // have run on this reclaim (the old buggy gate would have skipped it,
    // since generation is 2, not 1).
    assert!(
        dbg_slot_initialised(reclaimed_idx),
        "claim() must materialise the HeapCore on this reclaim (initialised \
         must become true) even though generation is now >= 2 -- a gate on \
         `generation == 1` would wrongly skip materialisation here (M-5)"
    );

    // Behavioural oracle: the returned heap must be a genuinely usable,
    // freshly-constructed HeapCore -- allocate and round-trip through it.
    let layout = Layout::from_size_align(64, 8).unwrap();
    // SAFETY: `heap` is live, materialised (just proven above), and not yet
    // recycled; we are its sole writer.
    let ptr = unsafe { (*heap).alloc(layout) };
    assert!(
        !ptr.is_null(),
        "alloc through the reclaimed heap must succeed -- a HeapCore that \
         was never actually constructed (materialisation skipped) could not \
         serve a real allocation without either crashing or corrupting \
         memory; observing a valid non-null pointer here corroborates that \
         `HeapCore::new` genuinely ran"
    );
    unsafe {
        core::ptr::write_bytes(ptr, 0xAB, layout.size());
        (*heap).dealloc(ptr, layout);
    }

    // SAFETY: `heap` was returned by `claim` and not yet recycled.
    unsafe { HeapRegistry::recycle(heap) };
}
