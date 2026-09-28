//! R1-06 (src review round 1, finding R1-06): a thread-local flag recording
//! whether the CURRENT thread presently holds the fallback heap's
//! process-wide spinlock (`global::fallback::LOCK`, acquired/released only by
//! that module's `LockGuard`).
//!
//! [`push_with_overflow_retry`](super::heap_core_xthread) consults this
//! (via [`held_by_current_thread`]) before entering its bounded sleep-retry
//! tier against a LIVE-but-stalled owner: that tier can block the calling
//! thread for up to `RETRY_STALLED_ROUNDS_GIVE_UP` (128) or
//! `RETRY_ROUND_SAFETY_CAP` (4096) probe rounds, each round beyond the first
//! separated by a real `std::thread::sleep` — see
//! `heap_core_xthread/overflow.rs`'s `push_with_overflow_retry` doc comment
//! for the full retry design. The fallback spinlock is a single,
//! process-wide mutual-exclusion lock (unlike a normal per-thread `HeapCore`,
//! which needs no lock at all): holding it across that retry window would
//! make every OTHER thread that needs the fallback (TLS teardown, registry
//! exhaustion, pre-TLS init) spin CPU-burning for the same window, even
//! though the two are unrelated in every other respect. Skipping straight to
//! the immediate-overflow-then-spill tier while this flag is set trades a
//! (rare) foreign free's latency for bounding every OTHER thread's wait on
//! the fallback lock — see [`held_by_current_thread`]'s call site in
//! `overflow.rs` for exactly which branch this affects. Spill is always
//! correct (the intrusive tier never drops a legal free); this only changes
//! WHICH of the two already-sound tiers a fallback-driven free reaches, and
//! only for the duration this thread holds the fallback lock.
//!
//! Set/cleared only by `global::fallback::LockGuard::acquire`/`Drop` (the
//! sole owner of the fallback spinlock's acquire/release) via [`set_held`].
//! `registry` does not depend on `global` anywhere else in the crate (the
//! dependency runs the other way — `global::fallback` already depends on
//! `registry::HeapCore`); this module therefore lives on the `registry` side
//! so `global::fallback` can call into it without introducing a reverse
//! module dependency.
//!
//! `Cell<bool>` thread-local, const-initialised: no allocation, no `Drop`
//! registration — safe to touch from inside the allocator's own alloc/dealloc
//! path (mirrors the existing `LAST_STALL_CONCESSIONS` TLS-safety argument in
//! `heap_core_xthread/stall.rs`).

#[cfg(feature = "alloc-xthread")]
std::thread_local! {
    static FALLBACK_LOCK_HELD: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
}

/// Record whether the CURRENT thread now holds (`true`) or has just released
/// (`false`) the fallback spinlock. Called only from
/// `global::fallback::LockGuard::acquire` (on successful acquisition) and its
/// `Drop` (on release) — see that type's doc comment.
#[cfg(feature = "alloc-xthread")]
pub(crate) fn set_held(held: bool) {
    // try_with: must never panic inside GlobalAlloc, even during TLS teardown.
    let _ = FALLBACK_LOCK_HELD.try_with(|c| c.set(held));
}

/// True iff the CALLING thread is presently inside a `global::fallback::
/// with_heap` closure (i.e. holds the fallback spinlock). Consulted by
/// [`push_with_overflow_retry`](super::heap_core_xthread) to skip its
/// bounded sleep-retry tier — see this module's doc comment for the full
/// rationale.
#[cfg(feature = "alloc-xthread")]
pub(super) fn held_by_current_thread() -> bool {
    // A torn-down slot reads as "not held": the caller falls back to the slower
    // stall-retry path, which is still correct.
    FALLBACK_LOCK_HELD.try_with(|c| c.get()).unwrap_or(false)
}
