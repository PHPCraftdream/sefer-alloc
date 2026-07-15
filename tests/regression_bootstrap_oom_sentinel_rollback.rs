//! Regression (task #131; re-scoped to the chunk level by R6-OPT-P0-2
//! round 1): a chunk-materialisation OOM bailout must roll that chunk's
//! `AtomicPtr<RegistryChunk>` back from `SENTINEL_INITIALIZING` to `null`
//! BEFORE it aborts the process, instead of leaving the sentinel stuck
//! forever.
//!
//! ## The bug this covers (original, whole-registry form)
//!
//! `ensure_slow`'s CAS winner used to reserve virtual memory for the WHOLE
//! `Registry` via `aligned_vmem::reserve_aligned`. Before the original fix, a
//! `None` result there hit `.expect(..)`, which panics -- but `REGISTRY_PTR`
//! had ALREADY been CASed to `SENTINEL_INITIALIZING` and the panic path never
//! rolled it back. Two failure modes followed: every loser thread already
//! spinning in `ensure_slow`'s `Err` branch spun FOREVER (the sentinel never
//! became a real pointer), and every FUTURE `ensure()` call (from any thread)
//! saw the non-null sentinel, fell into `ensure_slow`, failed
//! `compare_exchange(null, SENTINEL)` (current value was SENTINEL, not
//! null), and ALSO spun forever -- the whole process livelocked on the next
//! registry touch.
//!
//! ## R6-OPT-P0-2 round 1 — same bug class, narrower scope
//!
//! The slot array is now split into lazily-materialised chunks
//! (`src/registry/registry_chunk.rs`); `Registry` itself is a plain `static`
//! with NO lazy init of its own (see `bootstrap.rs`'s module doc). The
//! CAS-then-publish-then-rollback-on-OOM protocol this test exercises moved
//! DOWN a level, from "the one `REGISTRY_PTR`" to "one
//! `AtomicPtr<RegistryChunk>` per chunk" (`Registry::chunks[chunk_idx]`,
//! `ensure_chunk_slow` in `bootstrap.rs`). The identical livelock hazard
//! applies at chunk granularity: an un-rolled-back sentinel on chunk N would
//! permanently wedge every `slot()` call whose index falls in chunk N's
//! 64-slot range (while chunks materialised elsewhere stay completely
//! unaffected -- a narrower blast radius than the old whole-registry
//! version, by design; see `bootstrap.rs`'s module doc for the full
//! reasoning behind treating a chunk OOM this way).
//!
//! ## The fix under test
//!
//! The chunk-materialisation OOM branch in `ensure_chunk_slow` calls
//! `rollback_chunk_sentinel(chunk_ptr)` (store the chunk's `AtomicPtr` back
//! to `null` with `Release`) BEFORE `std::process::abort()`. `abort` cannot
//! be observed from within a test process (it terminates it), so this test
//! does not attempt to trigger the real OOM path. Instead it exercises
//! `rollback_chunk_sentinel` THROUGH the exact same function call the fix
//! uses -- via the `#[doc(hidden)]` test hook
//! `bootstrap::dbg_rollback_chunk_sentinel_reenterable(chunk_idx)`, which
//! drives the LIVE `Registry::chunks[chunk_idx]` through the
//! sentinel -> rollback -> postcondition-CAS sequence and restores it
//! afterward. See that function's doc comment in `src/registry/bootstrap.rs`
//! for the full safety argument (it only acts when the target chunk pointer
//! is observed as `null`, so it never disturbs an already-materialised
//! chunk, and it always restores `null` on exit).
//!
//! ## Race safety
//!
//! This test targets `dbg_num_chunks() - 1` -- the LAST chunk index. No
//! other test in this suite claims anywhere near `MAX_HEAPS` (4096) slots
//! (the whole suite's cumulative `claim()` traffic across every test file
//! stays in the low hundreds at most), so the last chunk is never
//! materialised by ordinary test traffic, making a collision with another
//! test's `claim()` calls effectively impossible. The hook is ALSO
//! defensive on its own terms: if some other caller has (contrary to that
//! expectation) already materialised or is concurrently materialising this
//! exact chunk index, the hook's own internal CAS(null, SENTINEL) simply
//! fails and it returns `None` rather than touching a live/contended chunk
//! -- this test treats `None` as inconclusive (skips the assertion) rather
//! than as a failure, so it can never falsely fail (or corrupt shared state).
//!
//! ## Non-vacuousness (counterfactual)
//!
//! If `rollback_chunk_sentinel` is broken (e.g. its `store` is removed or it
//! stores the sentinel back instead of `null`), the hook's postcondition
//! CAS(null, SENTINEL) -- performed immediately after the rollback --
//! observes the sentinel still in place and fails, so the hook returns
//! `Some(false)` and this test's `assert!(rolled_back_cleanly)` fails. This
//! mirrors the original task #131 test's verified counterfactual (manually
//! confirmed during that task by temporarily commenting out the rollback
//! store and re-running); the chunk-scoped rollback function is structurally
//! identical (same CAS + store shape, narrowed to one `AtomicPtr` among
//! many), so the same counterfactual applies unchanged.

#![cfg(feature = "alloc-global")]

use sefer_alloc::registry::bootstrap;

/// Anti-livelock: after a chunk's OOM-bailout rollback runs, that chunk's
/// `AtomicPtr<RegistryChunk>` must be back at `null` (`UNINIT`), not stuck at
/// `SENTINEL_INITIALIZING` -- otherwise every current/future `slot()` call
/// whose index falls in that chunk's range spins forever (Task #131,
/// re-scoped to chunk granularity by R6-OPT-P0-2 round 1).
#[test]
fn oom_bailout_rollback_clears_chunk_sentinel_not_stuck() {
    // Target the LAST chunk index -- see the module doc's "Race safety"
    // section for why no other test in this suite can plausibly have
    // materialised it already.
    let last_chunk = bootstrap::dbg_num_chunks() - 1;

    match bootstrap::dbg_rollback_chunk_sentinel_reenterable(last_chunk) {
        Some(rolled_back_cleanly) => {
            assert!(
                rolled_back_cleanly,
                "rollback_chunk_sentinel() must clear the chunk's AtomicPtr back \
                 to null so a subsequent CAS(null, SENTINEL) succeeds -- if this \
                 is false, the sentinel is stuck and every slot() caller \
                 touching this chunk's index range (present and future) spins \
                 forever (Task #131 livelock, chunk-scoped)"
            );
        }
        None => {
            // The chunk was already materialised (or contended) by the time
            // this test ran under the serial guard -- the hook correctly
            // refused to disturb it. Not expected given the chunk-index
            // choice above, but not a failure of the property under test
            // either way; nothing to assert here.
        }
    }

    // Whichever branch above ran, the registry must still work normally
    // afterward -- the hook is documented to always restore the chunk
    // pointer to what it observed on entry, and a subsequent claim must
    // still be able to mint/materialise slots normally.
    let _ = bootstrap::count_for_test();
    let heap = sefer_alloc::registry::HeapRegistry::claim();
    assert!(
        !heap.is_null(),
        "claim() must still work normally after the chunk rollback hook ran"
    );
    // SAFETY: `heap` was just returned by `claim` and not yet recycled.
    unsafe { sefer_alloc::registry::HeapRegistry::recycle(heap) };
}
