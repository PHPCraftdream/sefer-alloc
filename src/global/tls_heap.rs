//! Raw-pointer TLS binding for the alloc face (Phase 12.3, §2.2 of
//! `ALLOC_PLAN_PHASE12-13.md`).
//!
//! This is the reentrancy-safe TLS routing that replaces the Phase 11
//! `RefCell<Option<Heap>>` binding for the global face. The keystone move:
//! the heap is NOT owned by the TLS slot (RAII-dropped on thread exit); it
//! is a slot in the global [`HeapRegistry`], and the TLS slot caches only a
//! raw `*mut HeapCore` to it. Thread exit does NOT drop the heap; the
//! `AbandonGuard` abandons its segments back to the registry and recycles
//! the slot.
//!
//! ## Why raw `Cell<*mut HeapCore>` (no `RefCell`)
//!
//! `RefCell<Option<Heap>>` turns reentrancy into a refusal: under libtest's
//! parallel harness the global allocator is called while a borrow is already
//! held (e.g. panic infrastructure, capture buffers) → `try_borrow_mut`
//! returns `Err` → the alloc face returns null → the process aborts. The
//! raw-pointer `Cell` has no borrow state: reading it is always a single
//! load, never fails. Reentrancy is structurally excluded by M5 (no
//! `Vec`/`Box`/`std::alloc` on the alloc path), so there is no reentrant
//! mutation to guard against.
//!
//! ## Soundness of the raw pointer
//!
//! `*mut HeapCore` is sound to cache and dereference under the
//! **single-writer invariant**: the ONLY mutator of a heap's bins is its
//! owning thread (the one that won the `FREE → LIVE` CAS in `claim`).
//! `current()` is called only on the owning thread (it reads its own TLS),
//! so the `&mut HeapCore` it yields is exclusive. No other thread writes
//! these bins; cross-thread frees go through the `ThreadFreeStack`, not
//! the bins directly. The registry's atomic protocol (M5-clean bootstrap,
//! claim/recycle CAS) establishes the single writer; this file relies on
//! that, it does not re-establish it.
//!
//! ## TLS destructor ordering
//!
//! `LOCAL` and `GUARD` are both `thread_local!`s, declared in that order.
//! Rust destroys thread-locals in REVERSE declaration order per-thread, so
//! on this thread's teardown `GUARD` is dropped FIRST and `LOCAL` SECOND
//! (`LOCAL` is a bare `Cell`, so its own "drop" is a no-op — the hazard is
//! not `LOCAL`'s destructor, it is `LOCAL` outliving `GUARD`'s destructor
//! and being read again afterwards).
//!
//! The guard holds its OWN copy of the heap pointer (set in `bind_slow`)
//! and never reads `LOCAL` to decide what to recycle — that part of the
//! reasoning is unchanged. But `GUARD::drop` runs `HeapRegistry::recycle`,
//! which releases the slot back to the free pool; another thread may then
//! `claim` that exact slot before this thread finishes exiting. If `LOCAL`
//! on the exiting thread still held its stale (pre-recycle) pointer at that
//! point, ANY further use of `LOCAL` on this thread — for instance a `Drop`
//! impl belonging to some OTHER thread-local that was declared before
//! `LOCAL` (and therefore is destroyed after it, per the same reverse-order
//! rule) and that happens to allocate/deallocate memory — would resolve
//! back to the now-reclaimed-by-someone-else slot and hand out a second
//! `&mut HeapCore` aliasing the new owner's. This is exactly the guard's
//! job to prevent: it stamps `LOCAL` with the `TORN` sentinel BEFORE
//! calling `recycle`, i.e. while `LOCAL` is still guaranteed live (`GUARD`
//! drops before `LOCAL` in the reverse-declaration order above). Every
//! resolver ([`current`], [`current_for_alloc`],
//! [`current_for_alloc_with_config`]) checks for `TORN` before the
//! non-null check and, on a match, routes to the always-live fallback heap
//! instead of re-arming a new slot (which would leak the just-recycled one
//! and could resurrect a slot that another thread already re-claimed).
//!
//! ## Never-null (M10)
//!
//! [`current()`] returns a non-null `*mut HeapCore` in every case:
//! - the cached pointer is set → return it;
//! - the cached pointer is null (first call) → `bind_slow` claims a slot
//!   and publishes it, or on registry exhaustion falls back to the
//!   primordial heap;
//! - the TLS slot is destroyed (thread teardown) → `fallback_ptr` returns
//!   the always-live process-global fallback heap.
//!
//! So the alloc face never returns null for a serviceable request (M10).

// The crate is `#![deny(unsafe_code)]` with `alloc-global` on (see
// `src/lib.rs`); this is the documented raw-pointer TLS seam (Phase 12.3).
// `allow` lifts the crate-level `deny` for this file only — `unsafe`
// anywhere else in the crate is a hard error. The `unsafe` surface here is:
//   * dereferencing the cached `*mut HeapCore` into `&mut HeapCore` (sound
//     under the single-writer invariant documented above), and
//   * calling `HeapRegistry::recycle` / `abandon_segments` (which are
//     `unsafe fn`s whose contract is "pointer previously returned by
//     `claim`").
// Every `unsafe` block carries a `// SAFETY:` proof.
#![allow(unsafe_code)]

use core::cell::Cell;

use crate::global::fallback;
use crate::registry::{HeapCore, HeapRegistry};

/// Sentinel value stamped into [`LOCAL`] by [`mark_local_torn`] the instant
/// this thread's [`AbandonGuard`] starts tearing down (before it recycles
/// the slot). It is a "poison" marker, not a heap pointer: it is NEVER
/// dereferenced, only compared against in the three resolvers below. Chosen
/// as `usize::MAX` so it is:
/// - distinct from `null` (the "never bound yet" state), and
/// - distinct from any real `*mut HeapCore` (a live allocation can never sit
///   at the top of the address space — the registry's slot array and every
///   OS-backed segment are far below `usize::MAX`).
///
/// See the module doc's "TLS destructor ordering" section for why this is
/// necessary: without it, a stale non-null `LOCAL` value would survive
/// `GUARD`'s recycle of the slot, and a resolver reading `LOCAL` afterwards
/// (e.g. from another thread-local's `Drop` that allocates, running after
/// `GUARD` in the reverse-declaration teardown order) would hand out a
/// `&mut HeapCore` into a slot some other thread may have already
/// re-claimed — a second writer, i.e. a data race / UAF.
const TORN: *mut HeapCore = usize::MAX as *mut HeapCore;

thread_local! {
    /// The cached raw pointer to this thread's heap (a slot in the global
    /// [`HeapRegistry`]). `null` until the first call to [`current()`];
    /// non-null thereafter, until the thread exits — at which point the
    /// [`AbandonGuard`] recycles the slot AND stamps this cell to [`TORN`]
    /// (via [`mark_local_torn`]) BEFORE releasing it, so a post-teardown
    /// read never observes the stale pre-recycle pointer. Some other
    /// thread-local's `Drop` (declared before `LOCAL`, hence destroyed
    /// after it — reverse declaration order) can legitimately still
    /// allocate/deallocate after `GUARD` has dropped; every resolver checks
    /// for `TORN` and routes such a call to the fallback heap instead of
    /// dereferencing this stale slot.
    ///
    /// Stored as `Cell<*mut HeapCore>` (not `RefCell`) so there is no
    /// borrow state to fail under reentrancy: reading is a single load.
    static LOCAL: Cell<*mut HeapCore> = const { Cell::new(core::ptr::null_mut()) };

    /// The thread-exit abandon guard. Holds a COPY of the heap pointer
    /// (set in [`bind_slow`]) so its `Drop` does not need to read `LOCAL`
    /// (which may already be torn down). On drop: if the copy is non-null,
    /// abandon the heap's segments to the registry (a no-op stub in 12.3;
    /// the real walk arrives in 12.4) and recycle the slot. Null copy →
    /// nothing to abandon (the thread never bound a heap).
    static GUARD: AbandonGuard = const { AbandonGuard::new() };
}

/// The per-thread abandon guard. See the module docs for the TLS destructor
/// ordering reasoning.
struct AbandonGuard {
    /// A copy of the heap pointer this thread bound via [`bind_slow`]. Read
    /// ONLY in `Drop` (never in `LOCAL`-reading code paths). Storing the
    /// copy here is what makes the guard robust to `LOCAL` being torn down
    /// first.
    heap: Cell<*mut HeapCore>,
}

impl AbandonGuard {
    /// Construct an empty guard (no heap bound yet). `const` so it can
    /// initialise a `thread_local!` slot.
    const fn new() -> Self {
        Self {
            heap: Cell::new(core::ptr::null_mut()),
        }
    }
}

/// Stamp [`LOCAL`] with the [`TORN`] sentinel. The single choke point for
/// poisoning `LOCAL` — [`AbandonGuard::drop`] and the `#[doc(hidden)]` test
/// hook [`dbg_teardown_then_resolve_is_fallback`] both call this SAME
/// function (rather than duplicating the `LOCAL.try_with(|c| c.set(TORN))`
/// call), so the test hook exercises the exact poisoning logic the real
/// teardown path uses — not a reimplementation of it that could drift.
///
/// A `try_with` `Err` (thread-local already torn down) is silently ignored:
/// if `LOCAL` itself is gone, no resolver can read it again on this thread,
/// so there is nothing left to protect.
#[inline]
fn mark_local_torn() {
    let _ = LOCAL.try_with(|c| c.set(TORN));
}

impl Drop for AbandonGuard {
    fn drop(&mut self) {
        let heap = self.heap.get();
        if heap.is_null() {
            return; // This thread never bound a registry heap.
        }
        // Stamp `LOCAL` as TORN *before* recycling the slot below. Ordering
        // is load-bearing: `GUARD` (this `Drop`) runs BEFORE `LOCAL` is torn
        // down, because thread-locals are destroyed in reverse declaration
        // order and `LOCAL` is declared first, `GUARD` second (see the
        // module doc's "TLS destructor ordering" section) — so `LOCAL` is
        // still a live thread-local at this point and `try_with` succeeds.
        // If it were somehow already gone (`Err`), there is nothing to poison
        // and no post-teardown reader of `LOCAL` could run either, so the
        // no-op is safe.
        mark_local_torn();
        // Phase 12.5 (architectural turn): thread death = RELEASE THE SLOT
        // ONLY. We do NOT abandon/walk/clear the heap. The HeapCore (with ALL
        // its segments + the inline TFS head) STAYS WHOLE in the slot — it is
        // not dropped, not fragmented, not transferred. A later thread that
        // claims this recycled slot reuses the SAME HeapCore in full (claim
        // does not re-materialise when `new_gen != 1`): its segments, its free
        // lists, and crucially its segments' per-segment `RemoteFreeRing`s,
        // which still hold any cross-thread frees pushed after this thread
        // exited. The reclaiming thread reclaims those entries LAZILY on a
        // free-list miss (`AllocCore::find_segment_with_free` drains each owned
        // segment's ring via `reclaim_offset`) — this is the shard-reuse
        // discipline (a freed shard's remote-free queue is drained by the new
        // owner, exactly as `ShardedRegion` 7b models).
        //
        // Why NO abandon walk: the abandon/adopt protocol TRANSFERRED SEGMENTS
        // BETWEEN HEAPS, which meant two heaps could write the same segment's
        // BinTable/header concurrently (a data race that tore the header and
        // corrupted free lists). The shard model restores the single-writer
        // invariant — a segment is written ONLY by its slot's current owner,
        // full stop. The abandon/adopt primitives (abandoned_segs Treiber,
        // owner_state CAS) remain as a loom-proven substrate for a future
        // decommit-when-empty policy, but they are OFF the hot path.
        //
        // `owner_thread_free` points at the slot's inline TFS, whose address is
        // stable for the process lifetime. Across release→claim it does NOT
        // change, so it is stamped ONCE (on the segment's first alloc) and
        // never cleared/re-stamped — removing the racy cross-thread header
        // writes that caused the corruption.
        //
        // SAFETY: `heap` was returned by `HeapRegistry::claim` (set in
        // `bind_slow`) and has not yet been recycled (the guard drops once,
        // on thread exit). The slot is still LIVE; `recycle` is the matching
        // half of `claim` (CAS LIVE→FREE + push_free_slot).
        unsafe { HeapRegistry::recycle(heap) };
    }
}

/// The hot accessor: return the current thread's heap pointer, never null.
///
/// Fast path: a single TLS load + null check. On first call (null) it calls
/// `bind_slow` (cold); if the TLS is torn down (thread teardown) it calls
/// `fallback_ptr` (cold) — the process-global fallback heap, also never
/// null.
///
/// This is the un-tagged variant, for callers that do not need to
/// distinguish own-thread vs fallback (the alloc face uses
/// [`current_for_alloc`] instead). Kept `pub` as the canonical accessor for
/// future direct-API consumers and tests.
///
/// Inlined so the fast path collapses to a TLS-get + branch in the callers.
#[must_use]
#[inline]
#[allow(dead_code)] // The alloc face uses `current_for_alloc` (tagged). Kept for direct API.
pub fn current() -> *mut HeapCore {
    match LOCAL.try_with(|c| c.get()) {
        // Э2 (task #145) — TWO SENTINELS, ONE BRANCH. `null = 0` and
        // `TORN = usize::MAX` are the two ends of the address range; every
        // REAL `*mut HeapCore` sits strictly between them. So a single
        // unsigned compare separates "real pointer" (the hot path) from
        // "either sentinel" (both cold):
        //   real p (1..=MAX-1): p.addr()-1 ∈ 0..=MAX-2, all `< MAX-1` → fast
        //   null (0):           0.wrapping_sub(1) = MAX,   NOT `< MAX-1` → cold
        //   TORN (MAX):         MAX-1,                     NOT `< MAX-1` → cold
        // The cold arm then splits on the exact value (0 → bind, MAX → torn),
        // preserving the #129 mapping BYTE-for-BYTE (null → `bind_slow`,
        // TORN → `fallback_ptr`).
        Ok(p) if p.addr().wrapping_sub(1) < usize::MAX - 1 => p,
        // Cold split: null (first call / reset) → bind a slot; TORN (this
        // thread's GUARD already recycled its slot — the cached pointer is
        // stale and MUST NOT be dereferenced) → route to the fallback rather
        // than `bind_slow` (which would re-arm the dropped GUARD and leak the
        // freshly recycled slot). See "TLS destructor ordering".
        Ok(p) if p.is_null() => bind_slow(),
        Ok(_) => fallback_ptr(), // p == TORN
        // TLS destroyed (thread teardown): fall back, never null.
        Err(_) => fallback_ptr(),
    }
}

/// Which heap [`current_for_alloc`] resolved to. The alloc face uses this to
/// decide whether to take the lock-free own-thread fast path
/// ([`Own`](Self::Own)) or the spinlock-guarded fallback path
/// ([`Fallback`](Self::Fallback)). Carrying the tag in
/// the return value avoids a second `fallback::heap_ptr()` call (which would
/// needlessly initialise the fallback even when the fast path won).
#[must_use]
pub enum CurrentHeap {
    /// A registry slot owned by this thread. Lock-free `&mut HeapCore` access
    /// is sound under the single-writer invariant.
    Own(*mut HeapCore),
    /// The process-global fallback heap. Access MUST go through
    /// `fallback::with_heap` (spinlock-guarded) for mutual exclusion. The
    /// fallback pointer itself is re-fetched inside `with_heap` (it is a
    /// stable `'static` once initialised), so this variant carries no data.
    Fallback,
}

/// The alloc-face entry: resolve the current heap AND whether it is the
/// fallback, in one pass. Used by [`SeferAlloc`](super::SeferAlloc) to
/// avoid a redundant `fallback::heap_ptr()` comparison (which would
/// needlessly initialise the fallback on every alloc).
///
/// Under `alloc-decommit`, [`current_for_alloc_with_config`] is used
/// instead (it threads the config into the TLS bind). This function is
/// kept for `not(alloc-decommit)` builds and direct-API consumers.
#[cfg_attr(feature = "alloc-decommit", allow(dead_code))]
#[inline(always)]
pub fn current_for_alloc() -> CurrentHeap {
    match LOCAL.try_with(|c| c.get()) {
        // Э2 (task #145) — TWO SENTINELS, ONE BRANCH on the process's hottest
        // path. `null = 0` and `TORN = usize::MAX` are the range ends; every
        // real `*mut HeapCore` lies strictly between, so one unsigned compare
        // catches the hot "real pointer" case:
        //   real p (1..=MAX-1): p.addr()-1 ∈ 0..=MAX-2 → `< MAX-1` → Own (fast)
        //   null (0):           wraps to MAX → NOT `< MAX-1` → cold
        //   TORN (MAX):         MAX-1        → NOT `< MAX-1` → cold
        // Semantics are byte-identical to the previous TORN-then-null match
        // (same #129 mapping): the cold arm below splits null → bind, TORN →
        // Fallback. The `dbg_teardown_then_resolve_is_fallback` #129 hook
        // relies on TORN → Fallback and is preserved.
        Ok(p) if p.addr().wrapping_sub(1) < usize::MAX - 1 => CurrentHeap::Own(p),
        // First call on this thread: bind a slot. bind_slow returns either
        // an Own pointer or, on registry exhaustion, the fallback marker.
        Ok(p) if p.is_null() => bind_slow_tagged(),
        // Stale post-recycle pointer (this thread's GUARD already dropped) —
        // see "TLS destructor ordering" in the module doc. MUST route to
        // Fallback, not `bind_slow_tagged` (which would re-arm the dropped
        // GUARD and leak the recycled slot).
        Ok(_) => CurrentHeap::Fallback, // p == TORN
        // TLS destroyed: fall back, never null.
        Err(_) => CurrentHeap::Fallback,
    }
}

/// Like [`current_for_alloc`] but plumbs `config` into the newly claimed
/// `HeapCore` on first call (the TLS bind slow path). On subsequent calls
/// (TLS pointer already set) the fast path returns the cached pointer
/// without touching the config.
///
/// **Config is taken by reference** so the hot fast path (TLS pointer
/// cached) never materialises the ~40-byte `LargeCacheConfig` value on the
/// stack. The 40-byte copy happens only on the cold `bind_slow` branch,
/// where it is amortised across the thread's lifetime.
///
/// Only present under `alloc-decommit` — without that feature the config
/// concept does not exist and [`current_for_alloc`] is used directly.
#[cfg(feature = "alloc-decommit")]
#[inline(always)]
pub fn current_for_alloc_with_config(config: &crate::alloc_core::LargeCacheConfig) -> CurrentHeap {
    match LOCAL.try_with(|c| c.get()) {
        // See `current_for_alloc` — same Э2 (task #145) one-branch collapse:
        // real p → `< MAX-1` → Own (fast); null (0) → cold bind; TORN (MAX) →
        // cold Fallback (must NOT re-arm the recycled guard). Byte-identical
        // #129 mapping.
        Ok(p) if p.addr().wrapping_sub(1) < usize::MAX - 1 => CurrentHeap::Own(p),
        Ok(p) if p.is_null() => bind_slow_tagged_with_config(*config),
        Ok(_) => CurrentHeap::Fallback, // p == TORN
        Err(_) => CurrentHeap::Fallback,
    }
}

/// Bind a registry slot to this thread: claim, publish the pointer into
/// `LOCAL`, arm the [`AbandonGuard`] with a copy, install the cross-thread
/// TFS (under `alloc-xthread`), and return the pointer (or, on registry
/// exhaustion, the fallback marker). `#[cold]` — runs once per thread.
///
/// On registry exhaustion (every slot is LIVE and the free pool is empty —
/// pathological: > `MAX_HEAPS` simultaneous threads), returns
/// [`CurrentHeap::Fallback`] (the alloc face then routes through the
/// always-live primordial heap — never null, M10).
#[cold]
fn bind_slow() -> *mut HeapCore {
    match bind_slow_tagged() {
        CurrentHeap::Own(p) => p,
        // Registry exhausted / OOM: the fallback pointer (re-fetched here
        // for the un-tagged API; never null unless the fallback itself OOM'd
        // at init, in which case the caller surfaces null).
        CurrentHeap::Fallback => fallback_ptr(),
    }
}

/// The tagged variant of [`bind_slow`], used by [`current_for_alloc`] so the
/// alloc face knows whether it got an own-thread slot or the fallback (and
/// therefore whether to take the lock-free path or the spinlock path).
#[cold]
fn bind_slow_tagged() -> CurrentHeap {
    let heap = HeapRegistry::claim();
    finish_bind(heap)
}

/// Like [`bind_slow_tagged`] but uses [`HeapRegistry::claim_with_config`] so
/// the newly materialised `HeapCore` is configured with `config`. On a
/// re-claim the existing `HeapCore` is reused as-is.
///
/// Only present under `alloc-decommit`.
#[cfg(feature = "alloc-decommit")]
#[cold]
fn bind_slow_tagged_with_config(config: crate::alloc_core::LargeCacheConfig) -> CurrentHeap {
    let heap = HeapRegistry::claim_with_config(config);
    finish_bind(heap)
}

/// Shared post-claim logic: install the cross-thread TFS (under
/// `alloc-xthread`), publish the pointer into `LOCAL`, arm the
/// `AbandonGuard`, and return the tagged result. Called from both
/// [`bind_slow_tagged`] and [`bind_slow_tagged_with_config`].
#[cold]
fn finish_bind(heap: *mut HeapCore) -> CurrentHeap {
    let heap = if heap.is_null() {
        // Registry exhausted or primordial OOM: fall back, never null.
        return CurrentHeap::Fallback;
    } else {
        heap
    };

    // Under `alloc-xthread`: install the cross-thread TFS handle. Since Phase
    // 12.5 this performs NO `std::alloc`: the TFS is an INLINE `AtomicPtr<u8>`
    // field on `HeapCore` and `install_thread_free` is a no-op returning that
    // field's stable address (a `Box::new` here would recurse into
    // `SeferAlloc::alloc` → `bind_slow` → `install_thread_free` forever — see
    // `registry::heap_core`). It cannot fail, so the TLS bind path stays
    // M5-clean and M10 (never null) is preserved.
    #[cfg(feature = "alloc-xthread")]
    {
        // SAFETY: `heap` was returned by `claim`/`claim_with_config` and is
        // the slot's sole writer (the CAS won). We have exclusive `&mut`
        // access by the single-writer invariant. `install_thread_free` is
        // idempotent.
        let heap_ref: &mut HeapCore = unsafe { &mut *heap };
        heap_ref.install_thread_free();
    }

    // Publish into LOCAL (so subsequent `current()` calls hit the fast path)
    // and arm the guard with a COPY (so its Drop does not read LOCAL).
    let _ = LOCAL.try_with(|c| c.set(heap));
    let _ = GUARD.try_with(|g| g.heap.set(heap));
    CurrentHeap::Own(heap)
}

/// The fallback heap pointer — the process-global always-live heap. Used
/// when the TLS is destroyed (thread teardown) or the registry is exhausted.
/// Never null (M10). `#[cold]` — these windows are rare.
#[cold]
fn fallback_ptr() -> *mut HeapCore {
    fallback::heap_ptr()
}

/// Test-only hook (task #129): deterministically exercises the TORN→Fallback
/// mapping WITHOUT going through real thread teardown (which is
/// non-deterministic to trigger on demand). It calls the exact same
/// [`mark_local_torn`] function [`AbandonGuard::drop`] calls — not a
/// reimplementation — so a pass here is evidence about the real teardown
/// path, not a parallel code path that could drift from it.
///
/// Saves `LOCAL`'s current value, poisons it via `mark_local_torn`, resolves
/// [`current_for_alloc`], restores the saved value, and reports whether the
/// resolution was [`CurrentHeap::Fallback`]. `current_for_alloc` only reads
/// `LOCAL` on the `TORN` arm (it never writes it), so restoring the saved
/// value after the call fully undoes the poisoning for any subsequent
/// allocation on this thread.
///
/// `#[doc(hidden)]` — not part of the public API; exists solely so the
/// integration test in `tests/` can reach this otherwise-private teardown
/// behaviour.
#[doc(hidden)]
#[must_use]
pub fn dbg_teardown_then_resolve_is_fallback() -> bool {
    let saved = LOCAL.with(|c| c.get());
    mark_local_torn();
    let result = current_for_alloc();
    LOCAL.with(|c| c.set(saved));
    matches!(result, CurrentHeap::Fallback)
}
