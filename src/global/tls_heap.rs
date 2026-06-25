//! Raw-pointer TLS binding for the malloc face (Phase 12.3, §2.2 of
//! `MALLOC_PLAN_PHASE12-13.md`).
//!
//! This is the reentrancy-safe TLS routing that replaces the Phase 11
//! `RefCell<Option<Heap>>` binding for the global face. The keystone move:
//! the heap is NOT owned by the TLS slot (RAII-dropped on thread exit); it
//! is a slot in the global [`HeapRegistry`], and the TLS slot caches only a
//! raw `*mut HeapCore` to it. Thread exit does NOT drop the heap; the
//! [`AbandonGuard`] abandons its segments back to the registry and recycles
//! the slot.
//!
//! ## Why raw `Cell<*mut HeapCore>` (no `RefCell`)
//!
//! `RefCell<Option<Heap>>` turns reentrancy into a refusal: under libtest's
//! parallel harness the global allocator is called while a borrow is already
//! held (e.g. panic infrastructure, capture buffers) → `try_borrow_mut`
//! returns `Err` → the malloc face returns null → the process aborts. The
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
//! these bins; cross-thread frees go through the [`ThreadFreeStack`], not
//! the bins directly. The registry's atomic protocol (M5-clean bootstrap,
//! claim/recycle CAS) establishes the single writer; this file relies on
//! that, it does not re-establish it.
//!
//! ## TLS destructor ordering
//!
//! `LOCAL` and `GUARD` are both `thread_local!`s. Rust destroys thread-locals
//! in reverse-declaration order per-thread, but the standard does not
//! guarantee cross-key ordering against arbitrary other TLS keys the runtime
//! may have registered. The guard therefore holds its OWN copy of the heap
//! pointer (set in [`bind_slow`]) and NEVER reads `LOCAL` in its `Drop`. If
//! `LOCAL` is already torn down when the guard drops, the guard's copy still
//! has the pointer it needs to abandon+recycle. (If the guard's OWN slot is
//! torn down first, `LOCAL`'s drop is a no-op — `Cell` has no drop glue.)
//!
//! ## Never-null (M10)
//!
//! [`current()`] returns a non-null `*mut HeapCore` in every case:
//! - the cached pointer is set → return it;
//! - the cached pointer is null (first call) → [`bind_slow`] claims a slot
//!   and publishes it, or on registry exhaustion falls back to the
//!   primordial heap;
//! - the TLS slot is destroyed (thread teardown) → [`fallback_ptr`] returns
//!   the always-live process-global fallback heap.
//!
//! So the malloc face never returns null for a serviceable request (M10).
//!
//! [`HeapRegistry`]: crate::registry::HeapRegistry
//! [`ThreadFreeStack`]: crate::heap::thread_free::ThreadFreeStack

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

thread_local! {
    /// The cached raw pointer to this thread's heap (a slot in the global
    /// [`HeapRegistry`]). `null` until the first call to [`current()`];
    /// non-null thereafter (until the thread exits, at which point the
    /// [`AbandonGuard`] recycles the slot and the pointer becomes stale —
    /// but no one reads it after the guard drops, because the guard's drop
    /// is the last thing the thread does).
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

impl Drop for AbandonGuard {
    fn drop(&mut self) {
        let heap = self.heap.get();
        if heap.is_null() {
            return; // This thread never bound a registry heap.
        }
        // Phase 12.5 (architectural turn): thread death = RELEASE THE SLOT
        // ONLY. We do NOT abandon/walk/clear the heap. The HeapCore (with ALL
        // its segments + the inline TFS head) STAYS WHOLE in the slot — it is
        // not dropped, not fragmented, not transferred. A later thread that
        // claims this recycled slot reuses the SAME HeapCore in full (claim
        // does not re-materialise when `new_gen != 1`): its segments, its free
        // lists, and crucially its inline TFS, which still holds any
        // cross-thread frees pushed after this thread exited. The reclaiming
        // thread drains that TFS on its first `alloc` (`HeapCore::alloc`
        // already calls `drain_thread_free` under xthread) — this is the
        // shard-reuse discipline (a freed shard's remote-free queue is drained
        // by the new owner on first op, exactly as `ShardedRegion` 7b models).
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
/// [`bind_slow`] (cold); if the TLS is torn down (thread teardown) it calls
/// [`fallback_ptr`] (cold) — the process-global fallback heap, also never
/// null.
///
/// This is the un-tagged variant, for callers that do not need to
/// distinguish own-thread vs fallback (the malloc face uses
/// [`current_for_alloc`] instead). Kept `pub` as the canonical accessor for
/// future direct-API consumers and tests.
///
/// Inlined so the fast path collapses to a TLS-get + branch in the callers.
#[must_use]
#[inline]
#[allow(dead_code)] // The malloc face uses `current_for_alloc` (tagged). Kept for direct API.
pub fn current() -> *mut HeapCore {
    match LOCAL.try_with(|c| c.get()) {
        Ok(p) if !p.is_null() => p,
        // First call on this thread (or LOCAL was reset): bind a slot.
        Ok(_) => bind_slow(),
        // TLS destroyed (thread teardown): fall back, never null.
        Err(_) => fallback_ptr(),
    }
}

/// Which heap [`current_for_alloc`] resolved to. The malloc face uses this to
/// decide whether to take the lock-free own-thread fast path ([`Own`]) or
/// the spinlock-guarded fallback path ([`Fallback`]). Carrying the tag in
/// the return value avoids a second `fallback::heap_ptr()` call (which would
/// needlessly initialise the fallback even when the fast path won).
#[must_use]
pub enum CurrentHeap {
    /// A registry slot owned by this thread. Lock-free `&mut HeapCore` access
    /// is sound under the single-writer invariant.
    Own(*mut HeapCore),
    /// The process-global fallback heap. Access MUST go through
    /// [`fallback::with_heap`] (spinlock-guarded) for mutual exclusion. The
    /// fallback pointer itself is re-fetched inside `with_heap` (it is a
    /// stable `'static` once initialised), so this variant carries no data.
    Fallback,
}

/// The malloc-face entry: resolve the current heap AND whether it is the
/// fallback, in one pass. Used by [`SeferMalloc`](super::SeferMalloc) to
/// avoid a redundant `fallback::heap_ptr()` comparison (which would
/// needlessly initialise the fallback on every alloc).
#[inline]
pub fn current_for_alloc() -> CurrentHeap {
    match LOCAL.try_with(|c| c.get()) {
        Ok(p) if !p.is_null() => CurrentHeap::Own(p),
        // First call on this thread: bind a slot. bind_slow returns either
        // an Own pointer or, on registry exhaustion, the fallback marker.
        Ok(_) => bind_slow_tagged(),
        // TLS destroyed: fall back, never null.
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
/// [`CurrentHeap::Fallback`] (the malloc face then routes through the
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
/// malloc face knows whether it got an own-thread slot or the fallback (and
/// therefore whether to take the lock-free path or the spinlock path).
#[cold]
fn bind_slow_tagged() -> CurrentHeap {
    let heap = HeapRegistry::claim();
    let heap = if heap.is_null() {
        // Registry exhausted or primordial OOM: fall back, never null.
        return CurrentHeap::Fallback;
    } else {
        heap
    };

    // Under `alloc-xthread`: install the cross-thread TFS handle. This is
    // the ONE `std::alloc` touch on the TLS path (a single `Box::new`); it
    // is explicitly OUTSIDE the registry bootstrap (which is M5-clean). If
    // the box alloc fails (the global OOM), we proceed without a TFS — the
    // heap serves own-thread allocations only, and cross-thread frees to its
    // segments are a safe no-op (unstamped). M10 (never null) is preserved.
    #[cfg(feature = "alloc-xthread")]
    {
        // SAFETY: `heap` was returned by `claim` and is the slot's sole
        // writer (the CAS won). We have exclusive `&mut` access by the
        // single-writer invariant. `install_thread_free` is idempotent.
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
