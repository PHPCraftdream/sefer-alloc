#![allow(unsafe_code)]
//! The process-global [`ensure()`] accessor, the per-chunk materialisation
//! slow path ([`ensure_chunk_slow`]), and the test-only dbg hooks (OOM
//! injection, sentinel-rollback probe, count/slot introspection). Split out
//! of the flat `bootstrap.rs` (structural reorg step 5); the module-level
//! history and soundness narrative lives in [`super`]'s doc.

use core::sync::atomic::{AtomicBool, Ordering};

// The `OncePtrCell` real/shim swap below mirrors the import-site comment in
// `registry.rs` (CRATE-P3): under `--cfg loom` the const-capable
// `core`-atomic shim keeps the chunk-cell type nameable with an identical
// API surface, since loom's atomics have no const constructor.
use super::chunk::{RegistryChunk, CHUNK_SIZE, NUM_CHUNKS};
#[cfg(loom)]
use super::loom_shim::OncePtrCell;
use super::registry::{Registry, REGISTRY};
#[cfg(not(loom))]
use once_ptr_cell::OncePtrCell;
use once_ptr_cell::RollbackProbe;

// R34-15/task #534: test-only OOM injection for `ensure_chunk_slow`'s winner
// closure. When set, the closure returns `None` (simulating the OS refusing
// the `leak_zeroed_pages` reservation) WITHOUT actually exhausting VM, so a
// test can deterministically exercise the chunk-materialisation-OOM code path
// and prove the free path returns gracefully instead of aborting. This is a
// plain `AtomicBool` read — it does NOT touch allocator metadata through a
// raw pointer, so it is a safe test-injection point (not an `unsafe fn`), and
// it is `internals`-gated so it does not widen the surface of a `production`
// build.
#[cfg(feature = "internals")]
static DBG_INJECT_CHUNK_OOM: AtomicBool = AtomicBool::new(false);

/// Return a `&'static` reference to the process-global registry.
///
/// With the slot array chunked (R6-OPT-P0-2 round 1), `Registry` itself needs
/// no lazy initialisation at all — it is a plain `static` of atomics, valid
/// from process start. All the laziness that used to live at THIS level (the
/// `UNINIT → INITIALIZING → READY` CAS dance) now lives one level down, per
/// chunk, inside [`Registry::slot`] — see that method and [`ensure_chunk_slow`].
#[inline]
pub fn ensure() -> &'static Registry {
    &REGISTRY
}

/// Slow path for [`Registry::ensure_chunk`] / [`Registry::try_ensure_chunk`]:
/// drive the chunk's [`once_ptr_cell::OncePtrCell`] through its
/// `get_or_try_init` — the CAS-reserve / OS-reserve / Release-publish /
/// spin-while-INITIALIZING-loser / OOM-rollback protocol now lives INSIDE the
/// cell (the extracted `UNINIT -> INITIALIZING -> READY` state machine). This
/// function supplies only the winner's fallible OS reservation closure; the
/// OOM policy (abort for the alloc path, return `None` for the free path) lives
/// in the CALLER ([`Registry::ensure_chunk`] vs [`Registry::try_ensure_chunk`]).
///
/// Returns `None` on chunk-materialisation OOM. The cell has ALREADY rolled its
/// sentinel back to null by then (anti-livelock — losers re-race; a future
/// call can retry this chunk index). Before R34-15 (task #534) this function
/// aborted on OOM directly; the abort policy now lives in `ensure_chunk` (alloc
/// path only), while `try_ensure_chunk` (free path) passes the `None` through.
///
/// The cell guarantees exactly-once init, a single published pointer for all
/// racers, Release/Acquire happens-before, and — critically for M5 — that a
/// loser observing the OOM rollback (sentinel back to null) re-races the CAS
/// rather than spinning forever on a READY that will never come.
///
/// ## CRATE-P3 — why the overflow-sidecar path below did NOT also migrate
///
/// The chunk site maps cleanly onto `OncePtrCell<RegistryChunk>`: it wants a
/// `&'static RegistryChunk`, and `RegistryChunk` is never a ZST.
/// The `alloc-xthread` overflow-sidecar path (`ensure_overflow_sidecar` in
/// `super::overflow_sidecar`)
/// deliberately stays spelled out inline because it does NOT fit the generic
/// cell's shape without weakening it: (a) it returns a `bool`
/// materialised-or-not and the DEREF happens separately in
/// `deref_overflow_sidecar` (a different membrane split than the chunk's
/// `&'static`-returning resolver); (b) its OOM contract is "return `false`, let
/// the caller's existing bounded-leak path retry LATER" — a loser that observes
/// the rollback returns `false` immediately rather than re-racing within the
/// same call, the opposite of the cell's re-race-now liveness; and (c) under
/// miri `SIDECAR_CAP == 0` makes `HeapOverflowSidecar` a ZST (align 1), which
/// would trip `OncePtrCell`'s `align_of >= 2` sentinel-collision guard at
/// `const` construction. Forcing it would risk the M5-critical wedge-hazard
/// ordering for no real dedup gain, so it is left as an honest inline second
/// instance — the shared protocol it relies on is still proved by the crate's
/// real-type loom suite.
#[cold]
pub(super) fn ensure_chunk_slow(
    chunk_cell: &OncePtrCell<RegistryChunk>,
) -> Option<&'static RegistryChunk> {
    let published = chunk_cell.get_or_try_init(|| {
        // ── Winner init closure ───────────────────────────────────────────
        // We hold the cell's INITIALIZING sentinel; we are the SOLE
        // initialiser of THIS chunk. Allocate it from OS VM.
        //
        // M5 (reentrancy-free) proof: unchanged from the pre-extraction inline
        // winner branch — `aligned_vmem::leak_zeroed_pages` is a direct OS
        // syscall (reserve + zero-under-miri + `mem::forget`-leak), no
        // `std::alloc`/`Box`/`Vec`, no transitive dependency on
        // `sefer_alloc::registry::*`. Under miri it falls back to `std::alloc`,
        // but under miri we are NOT the global allocator, so no reentrancy.
        // The whole `CHUNK_SIZE` span is guaranteed zeroed on every backend, so
        // `base` points at a fully valid all-zero `RegistryChunk`:
        //   next_free   = 0 (NOT NEXT_FREE_TAIL — lazy init, RAD-1)
        //   state       = 0 = STATE_FREE
        //   generation  = 0
        //   heap        = MaybeUninit::uninit() (zero is fine)
        //   initialised = 0 = false
        //   remote.*    = 0 / null
        //   overflow    = all-zero `HeapOverflow`
        // — genuinely nothing to write; OS-zeroed pages ARE a valid state. The
        // reservation is PAGE-aligned (>= `align_of::<RegistryChunk>()` <= 64)
        // and leaked for the process lifetime, so the `&'static` references
        // `heap_registry::bind_slot_counters` plants into slot fields stay
        // valid forever.
        //
        // Returning `None` on OS refusal makes the cell roll its sentinel back
        // to null (anti-livelock — losers re-race) BEFORE we get control back
        // to run the OOM policy in the caller.

        // R34-15/task #534: test-only OOM injection. When the flag is set the
        // closure returns `None` WITHOUT calling `leak_zeroed_pages`, so a test
        // can deterministically exercise the chunk-materialisation-OOM path and
        // prove the free path returns gracefully instead of aborting.
        #[cfg(feature = "internals")]
        if DBG_INJECT_CHUNK_OOM.load(Ordering::Relaxed) {
            return None;
        }

        let p = aligned_vmem::leak_zeroed_pages(CHUNK_SIZE)?;
        Some(p.cast::<RegistryChunk>())
    });

    published.map(|p| {
        // SAFETY: `OncePtrCell::get_or_try_init` returned `Some` only after
        // the winner published a real (non-null, non-sentinel) pointer with
        // `Release` and every other racer observed it under `Acquire`. The
        // pointee is the fully-zeroed `RegistryChunk` reserved above (a
        // valid state), leaked for the process lifetime, so `&'static` is
        // sound.
        unsafe { p.as_ref() }
    })
}

/// Test-only hook (R6-OPT-P0-2 round 1, generalising the pre-chunking
/// `dbg_rollback_sentinel_reenterable`): proves the anti-livelock rollback in
/// `OncePtrCell`'s OOM path actually clears the sentinel for a SPECIFIC
/// chunk of the LIVE process-global registry, without invoking
/// `std::process::abort` (which would kill the test harness).
///
/// Takes a `chunk_idx` (not a `&AtomicPtr<RegistryChunk>`, which would leak
/// the crate-private [`RegistryChunk`] type into this function's `pub`
/// signature — `RegistryChunk` is deliberately `pub(crate)`, mirroring the
/// established pattern elsewhere in this module of keeping the real type
/// private and exposing only thin `pub` forwarders that operate on indices/
/// primitives). Callers MUST pick a
/// `chunk_idx` they can guarantee is not concurrently materialised by another
/// test running in the same process (e.g. a high chunk index no other test
/// in the suite claims enough slots to reach) — step 1 below is the runtime
/// guard that makes this safe even if that assumption is violated: it only
/// proceeds when the chunk is observed genuinely `UNINIT`.
///
/// ## Why this operates on the LIVE registry's chunk pointer
///
/// The bug class is specifically about the interaction between a chunk
/// pointer's three-state protocol (`null` / `SENTINEL_INITIALIZING` / real
/// pointer) and the rollback. A hook on a separate test-only atomic would
/// only prove that a *copy* of the protocol works, not that
/// `rollback_chunk_sentinel` (the actual function the fix calls) restores the
/// actual invariant `ensure_chunk_slow` depends on. So this hook drives the
/// REAL `Registry::chunks[chunk_idx]` through the fix's exact code path:
///
/// 1. It CAS-acquires the chunk pointer itself from `null` to
///    `SENTINEL_INITIALIZING` (the same transition the real
///    `ensure_chunk_slow` winner performs). If the chunk has ALREADY been
///    materialised (a real, non-null non-sentinel pointer) or is
///    (impossibly, under this test's own discipline) mid-init by another
///    caller, the CAS simply fails and this function reports
///    [`RollbackProbe::NotApplicable`] — it never disturbs a live or
///    contended chunk.
/// 2. With the sentinel now in place (as if we were the real
///    materialisation winner that hit OOM), it performs the IDENTICAL
///    sentinel-to-null `Release` store the production OOM-bailout performs
///    before `std::process::abort()`.
/// 3. It then verifies the anti-livelock postcondition directly: a
///    subsequent `compare_exchange(null, SENTINEL, ..)` must SUCCEED,
///    proving the rollback actually cleared the sentinel back to `null` (if
///    the rollback were a no-op, this CAS would fail with `Err(SENTINEL)`
///    and a real winner — or any future `slot()` caller touching this
///    chunk — would spin forever).
/// 4. It immediately restores the chunk pointer to `null` (the value
///    observed on entry), leaving it exactly as it found it — so, unlike a
///    real OOM (which permanently loses that chunk), this hook's target
///    chunk index remains available for a LATER real `claim()` to
///    materialise normally.
///
/// Returns exactly what the forwarded-to cell method returns, and carries
/// its contract verbatim: [`RollbackProbe::Proven`] if the rollback was
/// proven to clear the sentinel, [`RollbackProbe::NotApplicable`] if the
/// check could not run — either the chunk was not observed `UNINIT` on
/// entry (already materialised, or contended), or a concurrent claimer
/// re-won it during the probe's own rollback-then-reCAS window.
///
/// **There is deliberately no "rollback is broken" answer**, and an earlier
/// version of this doc was wrong to promise one: the probe cannot
/// distinguish a broken rollback from a legitimate concurrent owner, since
/// both make its postcondition CAS fail identically. A caller must treat
/// `NotApplicable` as "could not test", never as failure.
#[doc(hidden)]
pub fn dbg_rollback_chunk_sentinel_reenterable(chunk_idx: usize) -> RollbackProbe {
    // Forward to `OncePtrCell::dbg_rollback_reenterable`, which drives the
    // chunk's REAL cell through the EXACT `null -> sentinel -> rollback ->
    // re-CAS` sequence the internal OOM-bailout runs and proves the
    // anti-livelock postcondition (after rollback, a fresh CAS(null -> sentinel)
    // succeeds — no future materialisation winner or spinning loser is wedged).
    // The rollback logic now lives inside `OncePtrCell` (the extracted state
    // machine), so this hook exercises the shipped code path, not a copy — and
    // on the LIVE process-global registry chunk, exactly as before the
    // extraction. The cell's own entry CAS is the "only touch it if UNINIT"
    // guard (reports NotApplicable on a materialised/contended chunk), so a
    // caller-chosen high chunk index no other test claims stays safe.
    REGISTRY.chunks[chunk_idx].dbg_rollback_reenterable()
}

/// Test-only re-export (R6-OPT-P0-2 round 1) of
/// [`chunk::NUM_CHUNKS`](super::chunk::NUM_CHUNKS): the
/// total number of chunks the slot space is split into. Lets a test pick a
/// chunk index guaranteed to be the LAST one (`dbg_num_chunks() - 1`) —
/// unreachable by ordinary `claim()` traffic in a suite that never claims
/// anywhere near `MAX_HEAPS` slots — so it can safely exercise
/// [`dbg_rollback_chunk_sentinel_reenterable`] without any chance of
/// colliding with a chunk another test's `claim()` calls have materialised.
#[doc(hidden)]
#[must_use]
pub const fn dbg_num_chunks() -> usize {
    NUM_CHUNKS
}

/// The current high-water `count` (test introspection). Each test claims
/// fresh slots; because `count` is monotonic across the suite (we never
/// reset the slot array — that would leak the lazily-materialised
/// `HeapCore`s), a test derives its expected slot indices relative to the
/// count it observed at entry.
pub fn count_for_test() -> u32 {
    ensure().count.load(Ordering::Acquire)
}

// -------------------------------------------------------------------------
// R34-15/task #534: test-only OOM-injection hooks for the chunk-materialisation
// path. These let a test deterministically force `ensure_chunk_slow`'s OOM
// branch (see `DBG_INJECT_CHUNK_OOM` above) and prove the free path
// (`slot_or_none`) returns `None` instead of aborting. Gated on `internals`
// (same surface the rest of the `#[doc(hidden)]` test hooks in this file use);
// NOT `bench-internals`-gated because these hooks do NOT touch allocator
// metadata through a raw pointer — `DBG_INJECT_CHUNK_OOM` is a plain
// `AtomicBool` read, and `dbg_slot_or_none` is a thin wrapper around the
// safe `pub(crate) slot_or_none` method (no raw pointers, no `unsafe`).
// -------------------------------------------------------------------------

/// `#[doc(hidden)]` test hook (R34-15/task #534) — not part of the public
/// API. Sets/clears the chunk-materialisation OOM injection flag. When set,
/// `ensure_chunk_slow`'s winner closure returns `None` WITHOUT calling
/// `leak_zeroed_pages`, simulating the OS refusing the VM reservation. The
/// flag is process-global; a test that sets it MUST clear it before returning
/// (use a Drop guard) so subsequent tests are unaffected.
#[cfg(feature = "internals")]
#[doc(hidden)]
pub fn dbg_set_inject_chunk_oom(on: bool) {
    DBG_INJECT_CHUNK_OOM.store(on, Ordering::SeqCst);
}

/// `#[doc(hidden)]` test hook (R34-15/task #534) — not part of the public
/// API. Exercises the free-path [`Registry::slot_or_none`]: returns `true` if
/// the slot was resolved (chunk already materialised, or materialisation
/// succeeded), `false` if the chunk could not be materialised (OOM). A test
/// sets the OOM injection flag via [`dbg_set_inject_chunk_oom`], calls this
/// with a high slot index whose chunk has NOT been materialised yet, and
/// asserts it returns `false` — proving the free path returns gracefully
/// instead of aborting (the pre-R34-15 behaviour).
#[cfg(feature = "internals")]
#[doc(hidden)]
#[must_use]
pub fn dbg_slot_or_none(idx: usize) -> bool {
    ensure().slot_or_none(idx).is_some()
}
