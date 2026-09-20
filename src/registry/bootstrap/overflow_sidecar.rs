#![allow(unsafe_code)]
//! R6-OPT-P0-2 (round 2): lazy `HeapOverflow` sidecar materialisation — the
//! third instance of the CAS-then-spin-then-publish protocol, plus the
//! [`deref_overflow_sidecar`] safe membrane. Formerly the inner
//! `#[cfg(feature = "alloc-xthread")] mod overflow_sidecar` of the flat
//! `bootstrap.rs` (structural reorg step 5 — the file IS the module now, and
//! the `bootstrap::overflow_sidecar` path is preserved); the module-level
//! history and soundness narrative lives in [`super`]'s doc and
//! `heap_overflow.rs`'s module doc for the two-tier design and the
//! wedge-hazard correctness argument this module is the linchpin of.

use crate::registry::heap_overflow::{
    HeapOverflowSidecar, SIDECAR_CAP, SIDECAR_SENTINEL_INITIALIZING,
};
use core::hint::spin_loop;
use core::sync::atomic::{AtomicPtr, Ordering};

/// Byte size of one [`HeapOverflowSidecar`], rounded up to a multiple of
/// `aligned_vmem::PAGE` — mirrors `registry_chunk::CHUNK_SIZE`'s identical
/// rounding for the exact same `reserve_aligned` size-contract reason
/// (`size` must be a non-zero multiple of `PAGE`). Zero-sized only when
/// `SIDECAR_CAP == 0` (miri: `INLINE_CAP == HEAP_OVERFLOW_CAP`), in which
/// case [`ensure_overflow_sidecar`] never calls `reserve_aligned` at all
/// (see that function's `SIDECAR_CAP == 0` fast-return) so this constant
/// is unused on that path — kept `pub(super)` rather than `#[cfg]`-gated
/// away entirely so the arithmetic stays visible/auditable in one place
/// regardless of feature/miri configuration.
pub(super) const SIDECAR_SIZE: usize = {
    let raw = core::mem::size_of::<HeapOverflowSidecar>();
    if raw == 0 {
        aligned_vmem::PAGE
    } else {
        let page = aligned_vmem::PAGE;
        (raw + page - 1) & !(page - 1)
    }
};

/// Ensure `sidecar_ptr`'s [`HeapOverflowSidecar`] is materialised,
/// returning `true` once it is safe to index (real, non-null,
/// non-sentinel pointer observed), or `false` if materialisation failed
/// (OS OOM on the `reserve_aligned` call). Fast path: one `Acquire` load.
/// Slow path: CAS(null→SENTINEL) race, exactly like
/// [`ensure_chunk_slow`](super::ensure::ensure_chunk_slow), narrowed to ONE
/// sidecar pointer instead of a chunk-array slot.
///
/// **The wedge-hazard contract this function exists to uphold:** the
/// caller (`HeapOverflow::push_impl`) MUST call this BEFORE attempting
/// its `tail` CAS reservation for any index `>= INLINE_CAP`, and MUST
/// treat a `false` return as "do not reserve — return false from push",
/// never advancing `tail`. This function itself does not touch `tail` at
/// all (it only knows about `sidecar_ptr`); the ordering discipline lives
/// entirely in the caller, documented there (`HeapOverflow::push`'s doc
/// comment) and enforced by inspection (this function has no way to
/// enforce it from its own signature — a `bool` return, matched by every
/// call site).
///
/// On OOM, unlike [`ensure_chunk_slow`](super::ensure::ensure_chunk_slow)'s
/// registry-chunk OOM
/// (which had NO existing "try again later" contract at its call site and
/// had to fall back to `abort()`), `HeapOverflow::push` ALREADY has a
/// clean, pre-existing failure contract: it returns `bool`, and every
/// caller already treats `false` as "the ring is momentarily full,
/// concede to the documented-sound bounded leak" (see
/// `push_with_overflow_retry`'s existing handling in
/// `heap_core_xthread`). So this function's OOM branch simply rolls
/// the sentinel back (the SAME anti-livelock argument the chunk path's
/// own rollback rests on, narrowed to one sidecar pointer)
/// and returns `false` — strictly SIMPLER than the chunk path's OOM
/// handling: no `abort()` needed at all, because the surrounding protocol
/// already has the right shape.
pub(crate) fn ensure_overflow_sidecar(sidecar_ptr: &AtomicPtr<HeapOverflowSidecar>) -> bool {
    // `SIDECAR_CAP == 0` only under miri (`INLINE_CAP == HEAP_OVERFLOW_CAP`
    // there — see `heap_overflow.rs`'s `INLINE_CAP` doc comment). No
    // caller can ever observe `t >= INLINE_CAP` in that configuration (the
    // ring's own full-check already rejects any `t >=
    // HEAP_OVERFLOW_CAP == INLINE_CAP` before this function would be
    // called), so this is unreachable in practice — kept as an explicit,
    // cheap guard rather than relying on that reasoning silently, so a
    // future constant change fails safely (returns "materialisation
    // failed") instead of calling `reserve_aligned(0, ..)`, which
    // `aligned_vmem` already rejects (`size == 0` → `None`) but there is
    // no reason to route through a real syscall attempt for a
    // structurally-empty sidecar.
    if SIDECAR_CAP == 0 {
        return false;
    }

    let p = sidecar_ptr.load(Ordering::Acquire);
    let p_usize = p.addr();
    if p_usize != 0 && p_usize != SIDECAR_SENTINEL_INITIALIZING {
        return true;
    }
    ensure_overflow_sidecar_slow(sidecar_ptr)
}

#[cold]
fn ensure_overflow_sidecar_slow(sidecar_ptr: &AtomicPtr<HeapOverflowSidecar>) -> bool {
    let sentinel =
        core::ptr::without_provenance_mut::<HeapOverflowSidecar>(SIDECAR_SENTINEL_INITIALIZING);
    match sidecar_ptr.compare_exchange(
        core::ptr::null_mut(),
        sentinel,
        // Acquire on success: pairs with our later Release store of the
        // real pointer, establishing happens-before for future Acquire
        // readers — identical to `ensure_chunk_slow`'s CAS.
        Ordering::Acquire,
        Ordering::Relaxed,
    ) {
        Ok(_) => {
            // ── Winner branch ──────────────────────────────────────────
            // M5 (reentrancy-free) proof: identical to `ensure_chunk_slow`
            // — `aligned_vmem::reserve_aligned` is a direct OS syscall, no
            // `std::alloc`/`Box`/`Vec`, no transitive dependency on
            // `sefer_alloc::registry::*`. Under miri it falls back to
            // `std::alloc`, but under miri we are NOT the global
            // allocator, so no reentrancy. (In practice `SIDECAR_CAP == 0`
            // under miri means this branch is unreachable there — see the
            // guard in `ensure_overflow_sidecar` above — but the proof
            // holds regardless.)
            // CRATE-P2 (item g): reserve + zero-under-miri + leak folded
            // into `aligned_vmem::leak_zeroed_pages` (the miri `write_bytes`
            // that used to sit below is now inside that helper). `base`
            // points at a fully-zeroed `SIDECAR_SIZE` span leaked for the
            // process lifetime; `SIDECAR_ALIGN` (PAGE) is subsumed because
            // `leak_zeroed_pages` always reserves PAGE-aligned (>=
            // `align_of::<HeapOverflowSidecar>()`).
            let base = match aligned_vmem::leak_zeroed_pages(SIDECAR_SIZE) {
                Some(p) => p.as_ptr() as *mut HeapOverflowSidecar,
                None => {
                    // OOM: roll the sentinel back (anti-livelock — see
                    // `rollback_chunk_sentinel`'s identical argument,
                    // narrowed to this one sidecar pointer) and report
                    // failure. Unlike the chunk path, NO abort: `push`'s
                    // pre-existing `bool` contract already has a sound
                    // "ring momentarily unavailable" outcome for this —
                    // see this function's own doc comment.
                    rollback_overflow_sidecar_sentinel(sidecar_ptr);
                    return false;
                }
            };

            // In-place initialisation: no field writes needed at all —
            // OS-zeroed pages already form a fully valid
            // `HeapOverflowSidecar` (every `AtomicPtr<u8>`/`AtomicU32` at
            // its null/`ENTRY_EMPTY_BASE` initial value), identical
            // reasoning to `ensure_chunk_slow`'s "nothing to write" case.
            //
            // SAFETY: `base` is non-null, aligned to `SIDECAR_ALIGN`
            // (PAGE, well above `align_of::<HeapOverflowSidecar>()`), and
            // valid for `SIDECAR_SIZE` bytes (>=
            // `size_of::<HeapOverflowSidecar>()`). We are the sole writer
            // (only one CAS winner can reach this branch). The memory is
            // OS-provided zero-initialised pages, already a fully valid
            // `HeapOverflowSidecar` bit-pattern.

            // Publish with Release so every subsequent Acquire load
            // (`HeapOverflow::slot`, `dbg_sidecar_is_materialised`, the
            // fast path above) sees the fully written sidecar.
            sidecar_ptr.store(base, Ordering::Release);

            // The reservation was already leaked for the process lifetime by
            // `leak_zeroed_pages` — same discipline as every other
            // lazy-materialisation site in this crate.

            true
        }
        Err(_) => {
            // ── Loser branch ───────────────────────────────────────────
            // Spin until a real (non-null, non-sentinel) pointer is
            // observed — identical shape to `ensure_chunk_slow`'s loser
            // branch, narrowed to one sidecar pointer. The window is
            // small: one OS reservation of `SIDECAR_SIZE` bytes plus one
            // publish store.
            loop {
                let p = sidecar_ptr.load(Ordering::Acquire);
                let p_usize = p.addr();
                if p_usize != 0 && p_usize != SIDECAR_SENTINEL_INITIALIZING {
                    return true;
                }
                if p_usize == 0 {
                    // R6-OPT-P0-2 (round 2): the winner hit OOM and rolled
                    // the sentinel back to null. There is no live winner
                    // left to wait on — spinning further would wait
                    // forever (no one will ever publish a real pointer
                    // until a NEW `ensure_overflow_sidecar` call wins a
                    // fresh CAS). Report failure to THIS caller; it is
                    // free to retry (a later `push` call re-enters
                    // `ensure_overflow_sidecar`'s fast path, observes
                    // null again, and may itself win the CAS).
                    return false;
                }
                spin_loop();
            }
        }
    }
}

/// Roll `sidecar_ptr` back from `SENTINEL_INITIALIZING` to `null` —
/// mirrors the chunk path's own sentinel rollback exactly (same
/// anti-livelock argument, narrowed to one sidecar pointer instead of a
/// chunk slot).
/// Kept as its own function so the test-only hook below exercises EXACTLY
/// the same code the production OOM-bailout runs.
#[cold]
fn rollback_overflow_sidecar_sentinel(sidecar_ptr: &AtomicPtr<HeapOverflowSidecar>) {
    sidecar_ptr.store(core::ptr::null_mut(), Ordering::Release);
}

/// Test-only hook, generalising
/// [`super::dbg_rollback_chunk_sentinel_reenterable`]
/// to the sidecar pointer: proves [`rollback_overflow_sidecar_sentinel`]
/// actually clears the sentinel on a REAL `AtomicPtr<HeapOverflowSidecar>`,
/// without invoking any process-terminating path (there is none on this
/// side — see `ensure_overflow_sidecar_slow`'s OOM branch, which already
/// returns `false` rather than aborting). Operates on a caller-supplied
/// standalone pointer (not a live registry slot's sidecar) since
/// `HeapOverflow` instances used in tests are typically standalone
/// (`new_boxed_for_test`), not registry-resident — unlike the chunk
/// hook, there is no shared process-global sidecar to accidentally
/// disturb, so this hook does not need the "only touch it if UNINIT"
/// guard the chunk hook needs (a test-owned standalone `HeapOverflow` is
/// never contended by another test).
///
/// `pub(crate)` (not `pub`): `HeapOverflowSidecar` is `pub(crate)`, so a
/// `pub fn` taking `&AtomicPtr<HeapOverflowSidecar>` would leak a
/// private type into a public signature. The actual test-facing surface
/// is [`HeapOverflow::dbg_rollback_sidecar_sentinel_for_test`], a thin
/// `#[doc(hidden)] pub` forwarder on `HeapOverflow` itself (mirroring
/// `dbg_reserve_unpublished_for_test`'s existing "test hook lives on the
/// type, not on a raw field" discipline) that calls this function with
/// its own private `sidecar` field.
pub(crate) fn dbg_rollback_overflow_sidecar_sentinel_reenterable(
    sidecar_ptr: &AtomicPtr<HeapOverflowSidecar>,
) -> bool {
    let sentinel =
        core::ptr::without_provenance_mut::<HeapOverflowSidecar>(SIDECAR_SENTINEL_INITIALIZING);

    // Step 1: CAS-acquire the sentinel (as if we were the real
    // materialisation winner that hit OOM). Caller guarantees the pointer
    // starts null (standalone test-owned ring, never contended).
    sidecar_ptr
        .compare_exchange(
            core::ptr::null_mut(),
            sentinel,
            Ordering::Acquire,
            Ordering::Relaxed,
        )
        .expect("dbg_rollback_overflow_sidecar_sentinel_reenterable: pointer must start null");

    // Step 2: run the EXACT rollback the production OOM-bailout runs.
    rollback_overflow_sidecar_sentinel(sidecar_ptr);

    // Step 3: prove the anti-livelock postcondition — a fresh CAS(null,
    // SENTINEL) must now succeed.
    let postcondition_holds = sidecar_ptr
        .compare_exchange(
            core::ptr::null_mut(),
            sentinel,
            Ordering::Acquire,
            Ordering::Relaxed,
        )
        .is_ok();

    // Step 4: restore to null, exactly as observed on entry.
    sidecar_ptr.store(core::ptr::null_mut(), Ordering::Release);

    postcondition_holds
}

/// Dereference a materialised sidecar pointer as `&'static HeapOverflowSidecar`.
/// The ONE place in the crate allowed to do so (mirrors [`Registry::slot`]'s
/// equivalent role for chunk memory) — `heap_overflow.rs` has no unsafe seam
/// of its own (see its module doc), so its `HeapOverflow::slot` resolver
/// calls this safe membrane function instead of dereferencing `p` itself.
///
/// # Panics (debug only)
///
/// The caller (`HeapOverflow::slot`) already `debug_assert`s `p` is non-null
/// and non-sentinel before calling this; this function trusts that contract
/// (a `debug_assert` here would be redundant with the caller's).
#[cfg(feature = "alloc-xthread")]
pub(crate) fn deref_overflow_sidecar(p: *mut HeapOverflowSidecar) -> &'static HeapOverflowSidecar {
    // SAFETY: `p` is a non-null, non-sentinel pointer the caller obtained
    // from an `Acquire` load of a `sidecar` field after `HeapOverflow::
    // push_impl` already called `ensure_overflow_sidecar` (which returned
    // `true`) for this same index, OR (on the drain side) an index a producer
    // already proved reachable by successfully publishing into it — in both
    // cases some earlier `ensure_overflow_sidecar_slow` winner published this
    // pointer with `Release` after fully constructing the `HeapOverflowSidecar`
    // (OS-zeroed pages are already a fully valid state — see that function's
    // in-place-init comment), and this Acquire load observes it, establishing
    // happens-before. The allocation is leaked (`mem::forget`) and outlives
    // any reference derived from it (process lifetime), exactly like a
    // `RegistryChunk`.
    unsafe { &*p }
}
