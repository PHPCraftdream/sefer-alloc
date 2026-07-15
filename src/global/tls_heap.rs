//! Raw-pointer TLS binding for the alloc face (Phase 12.3, §2.2 of
//! `ALLOC_PLAN_PHASE12-13.md`).
//!
//! This is the reentrancy-safe TLS routing that replaces the Phase 11
//! `RefCell<Option<Heap>>` binding for the global face. The keystone move:
//! the heap is NOT owned by the TLS slot (RAII-dropped on thread exit); it
//! is a slot in the global [`HeapRegistry`], and the TLS slot caches only a
//! raw `*mut HeapCore` to it. On thread exit, `AbandonGuard::drop` does NOT
//! abandon segments — it recycles the slot (`LIVE → FREE`) with the `HeapCore`
//! and all its segments staying whole, for reuse by whichever thread claims
//! the slot next (whole-slot reuse, Phase 12.5).
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
//! these bins; cross-thread frees go through the segment's `RemoteFreeRing`,
//! not the bins directly. The registry's atomic protocol (M5-clean bootstrap,
//! claim/recycle CAS) establishes the single writer; this file relies on
//! that, it does not re-establish it.
//!
//! ## TLS teardown and the TORN sentinel
//!
//! `LOCAL` (the cached `*mut HeapCore`) and `GUARD` (the recycle-on-death
//! guard) are both `thread_local!`s. The hazard at thread teardown is: once
//! `GUARD::drop` runs `HeapRegistry::recycle`, the slot returns to the free
//! pool and another thread may `claim` it. If a resolver on the *exiting*
//! thread then read `LOCAL` and found its stale (pre-recycle) pointer, it
//! would hand out a second `&mut HeapCore` aliasing the new owner's. The
//! guard prevents this by stamping `LOCAL` with the `TORN` sentinel BEFORE
//! it calls `recycle`.
//!
//! Note we do NOT rely on any thread-local destructor *ordering*: std makes
//! no such guarantee (destructor order is unspecified and platform-dependent
//! — ELF, for one, tears down in reverse *registration* order via
//! `__cxa_thread_atexit`, not declaration order). The mechanism is sound
//! regardless, for three independent reasons:
//!
//! (a) `LOCAL` is a `const`-initialised `Cell<*mut HeapCore>` with **no
//!     `Drop` impl** — it has no destructor at all. On native-TLS platforms
//!     it is therefore never "destroyed"; it simply stays readable (holding
//!     whatever the guard last stamped) for the entire thread teardown, so
//!     "`GUARD` runs before `LOCAL` becomes unreadable" holds trivially, no
//!     ordering assumption required.
//! (b) TLS accessibility is monotone within a single thread's program order:
//!     if a post-recycle resolver's `LOCAL.try_with` returned `Ok`, then the
//!     earlier-in-program-order `mark_local_torn` (run by `GUARD::drop`
//!     before `recycle`) must ALSO have returned `Ok` and already written
//!     `TORN`. So any resolver that can still read `LOCAL` reads `TORN`, never
//!     the stale pre-recycle pointer — whatever the destructor order.
//! (c) On os-keyed platforms where `LOCAL`'s storage may already be gone, the
//!     resolvers' `try_with` returns `Err` → they route to the always-live
//!     Fallback heap, which is likewise safe.
//!
//! Every resolver ([`current`], [`current_for_alloc`],
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
//   * calling `HeapRegistry::recycle` (which is an `unsafe fn` whose
//     contract is "pointer previously returned by `claim`").
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
        // Stamp `LOCAL` as TORN *before* recycling the slot below. This does
        // NOT rely on thread-local destructor ordering (std does not specify
        // it — see the module doc's "TLS teardown and the TORN sentinel"
        // section). It is sound because: `LOCAL` is a `Drop`-less `const` Cell
        // that (on native TLS) is never torn down, so it stays readable; and
        // TLS access is monotone in program order, so any later resolver that
        // still gets `Ok` from `LOCAL.try_with` runs AFTER this write and
        // therefore observes `TORN`, never the stale pre-recycle pointer. If
        // `try_with` here returns `Err` (`LOCAL` already gone on an os-keyed
        // platform), no post-teardown reader of `LOCAL` can run either — those
        // resolvers get `Err` too and route to Fallback — so the no-op is safe.
        mark_local_torn();
        // UBFIX-10 (M-9): opportunistic Large-deferred-free drain on thread
        // exit. Before this task, `HeapCore::drain_large_deferred_free` ran
        // ONLY from the two Large-classified call sites inside `alloc`/
        // `realloc` — so a heap whose owning thread stopped issuing Large
        // requests before it exited (e.g. it only ever allocated Small
        // blocks, or its last Large request happened long before any
        // cross-thread free of one of its Large segments arrived) could carry
        // a non-empty deferred-free stack all the way to thread exit. Under
        // the Phase 12.5 shard model the slot's `HeapCore` (including this
        // stack's head) survives recycle intact and is reused whole by
        // whichever thread next claims this slot — so the entries are not
        // permanently unreachable, but if no future claimant ever allocates a
        // Large block on this slot either, they stay queued (mapped, unused
        // segments) indefinitely. Draining here, once, right before the slot
        // goes back to the free pool, reclaims them opportunistically instead
        // of leaving that outcome to chance.
        //
        // Placement: BEFORE the `recycle` CAS below, i.e. while this thread is
        // still the slot's sole owner/writer (`STATE_LIVE`) — exactly the
        // single-writer window every other mutation of this heap already
        // relies on. Draining after `recycle` would race a new claimant.
        //
        // Cost: thread exit is definitionally cold (runs once per thread,
        // never on the alloc/dealloc hot path), and
        // `drain_large_deferred_free`'s pop loop starts with a single Acquire
        // load of the stack head, returning immediately when empty — so the
        // common case (nothing queued) costs one atomic load on a path that
        // is already off every benched hot path.
        //
        // SAFETY: `heap` was returned by `HeapRegistry::claim` and is still
        // LIVE (same justification as the `recycle` call below); `HeapCore`
        // is `#![deny(unsafe_code)]`, so the dereference happens through the
        // crate's own safe `&mut *heap` — sound because this thread is the
        // heap's sole owner until the CAS below flips it to FREE.
        #[cfg(feature = "alloc-xthread")]
        unsafe {
            (*heap).drain_large_deferred_free();
        }
        // task #95 / N1 — teardown trim. Flush every tcache class, drain the
        // small-segment pool, and evict the entire large cache, returning
        // retained memory to the OS. Same placement window as the
        // `drain_large_deferred_free` call above: BEFORE the `recycle` CAS,
        // while this thread is still the slot's sole owner/writer
        // (`STATE_LIVE`). Without this trim, a wave of short-lived threads
        // leaves tcache-buffered blocks, pooled small segments (up to 16 MiB
        // each), and cached large spans pinned on each recycled slot —
        // RSS/commit stays proportional to peak thread count, not current
        // load (performance_review.md finding N1).
        //
        // Cost: thread exit is definitionally cold (runs once per thread,
        // never on the alloc/dealloc hot path). Each sub-step starts with a
        // cheap check (tcache class count == 0 → skip; pool empty → skip;
        // large cache empty → skip) so a heap that already has nothing
        // retained costs only a handful of loads on a path that is already
        // off every benched hot path.
        //
        // SAFETY: same as `drain_large_deferred_free` above — `heap` was
        // returned by `HeapRegistry::claim` and is still LIVE; this thread
        // is the heap's sole owner until the CAS below flips it to FREE.
        unsafe {
            (*heap).trim_for_recycle();
        }
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
        // full stop. The abandon/adopt substrate (the `abandoned_segs` Treiber
        // stack + `owner_state` ABANDONED→LIVE adoption CAS) has been REMOVED
        // (task #97 / R4-5): it was unreachable on this whole-slot-reuse path
        // and internally inconsistent; git history preserves it if a future
        // decommit-when-empty policy ever needs to reintroduce segment
        // transfer.
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
/// [`current_for_alloc`] instead).
///
/// # Locking obligation on the fallback pointer
///
/// By discarding the tag this accessor erases a synchronisation obligation
/// that [`current_for_alloc`] surfaces via [`CurrentHeap::Fallback`]: on the
/// TORN and `Err` (TLS-teardown) branches the returned pointer is the
/// process-**global fallback heap**, shared by every thread whose TLS is
/// unavailable. Mutable (`&mut HeapCore`) access to the OWN-thread pointer is
/// sound lock-free (single-writer registry-slot invariant), but the fallback
/// pointer has NO such owner — mutating it is sound ONLY under the
/// `fallback::with_heap` spinlock. A caller cannot tell from the bare pointer
/// which case it got, so a direct-API consumer of `current()` must either treat
/// the result as read-only or route every mutation through `fallback::with_heap`
/// (the tagged [`current_for_alloc`] is the safe default — prefer it). This
/// obligation is why the accessor is currently unused by the alloc face.
///
/// L-9g: kept `pub(crate)` (not `pub`) precisely because of the hazard above —
/// the untagged pointer is easy to misuse across a crate boundary where the
/// caller cannot see this doc comment's obligation at the call site. Crate-
/// internal callers already have the full context; an external consumer that
/// needs this distinction should use the tagged [`current_for_alloc`] /
/// [`CurrentHeap`] instead.
///
/// Inlined so the fast path collapses to a TLS-get + branch in the callers.
#[must_use]
#[inline]
#[allow(dead_code)] // The alloc face uses `current_for_alloc` (tagged). Kept for direct API.
pub(crate) fn current() -> *mut HeapCore {
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

/// R6-OPT-P0-1: which heap [`current_for_dealloc`] resolved to, for the
/// **dealloc-only** entry point. Distinct from [`CurrentHeap`] because the
/// bind-less case here is NOT the fallback heap — it is "definitely a
/// foreign pointer, route it WITHOUT ever materialising (binding OR
/// fallback-locking) a `HeapCore` at all". See [`current_for_dealloc`]'s doc
/// comment for the full rationale.
#[cfg(feature = "alloc-xthread")]
#[must_use]
pub enum CurrentHeapForDealloc {
    /// A registry slot owned by this thread (identical fast path to
    /// [`CurrentHeap::Own`] — the thread has a real, bound heap).
    Own(*mut HeapCore),
    /// This thread never bound a heap (TLS still null), or its heap's slot
    /// was already recycled (`TORN`), or its TLS is torn down (`Err`). Any
    /// pointer reaching `dealloc` on such a thread is foreign BY
    /// CONSTRUCTION (see module-level rationale in `current_for_dealloc`) —
    /// route it directly through the heap-instance-independent
    /// [`HeapCore::dealloc_foreign_routing`] with `our_head = None`, WITHOUT
    /// claiming a registry slot and WITHOUT taking the fallback spinlock.
    ForeignNoBind,
}

/// R6-OPT-P0-1: a **dealloc-only** resolver — reads `LOCAL` exactly like
/// [`current_for_alloc`], but the bind-less case (`null` / `TORN` / `Err`)
/// does **not** bind a heap or resolve the fallback pointer at all. This is
/// the fix for the diagnosed defect: `SeferAlloc::dealloc` used to call
/// `current_heap()` (== `current_for_alloc`) unconditionally, which for a
/// thread whose TLS is `null` (never allocated anything itself — e.g. a
/// worker thread that only ever receives a pointer via a channel from a
/// producer thread, frees it, and exits) called `bind_slow_tagged()` →
/// `HeapRegistry::claim()` → materialised a FULL `HeapCore` → reserved/
/// committed a 4 MiB primordial segment, JUST to free one foreign pointer.
/// For a `TORN` thread it instead routed through `fallback::with_heap`,
/// taking the fallback's spinlock, to service what is — in the
/// overwhelming majority of cases — a foreign pointer that does not even
/// belong to the fallback heap.
///
/// **Passive, read-only.** This resolver reads `LOCAL` and nothing else — it
/// never writes `LOCAL`, never calls `HeapRegistry::claim`, and never calls
/// `fallback::with_heap`. Only present under `alloc-xthread`: without
/// cross-thread routing there is no heap-independent way to route a foreign
/// pointer at all (see `SeferAlloc::dealloc`'s `not(alloc-xthread)` arm,
/// which keeps the OLD `current_for_alloc` + bind/fallback behavior for that
/// configuration).
///
/// - real pointer (own heap bound) → [`CurrentHeapForDealloc::Own`] —
///   identical fast path to [`current_for_alloc`]'s `Own` arm, unchanged.
/// - `null` (never bound) → [`CurrentHeapForDealloc::ForeignNoBind`]. Does
///   **not** call `bind_slow`/`bind_slow_tagged` — that is the entire point
///   of this task: a thread whose TLS is null has never allocated anything
///   of its own under this allocator instance (`SeferAlloc::alloc` always
///   binds on first use), so any pointer reaching `dealloc` here must have
///   arrived from elsewhere (e.g. a channel) — it is foreign by
///   construction, and the caller routes it via
///   `HeapCore::dealloc_foreign_routing(ptr, base, layout, None)` without
///   ever touching the registry.
/// - `TORN` (this thread's `AbandonGuard` already recycled its slot) →
///   ALSO [`CurrentHeapForDealloc::ForeignNoBind`] — see the module doc's
///   "TLS teardown and the TORN sentinel" section for why the cached
///   pointer must not be dereferenced. Unlike [`current_for_alloc`], this
///   does NOT route through `fallback::with_heap` (no fallback spinlock is
///   taken) — see this function's own module-level trade-off note in
///   `sefer_alloc.rs`'s `dealloc` for the deliberate, documented narrowing
///   this causes for the rare "TORN AND the pointer happens to be
///   fallback-owned" case.
/// - `Err` (TLS destroyed) → same `ForeignNoBind` treatment as `TORN`.
#[cfg(feature = "alloc-xthread")]
#[inline(always)]
pub fn current_for_dealloc() -> CurrentHeapForDealloc {
    match LOCAL.try_with(|c| c.get()) {
        // Same Э2 (task #145) one-branch collapse as `current_for_alloc`:
        // real p → `< MAX-1` → Own (fast); null (0) and TORN (MAX) both fall
        // to the cold arm below, where THIS resolver (unlike
        // `current_for_alloc`) maps BOTH to `ForeignNoBind` — neither binds
        // a slot nor resolves the fallback pointer.
        Ok(p) if p.addr().wrapping_sub(1) < usize::MAX - 1 => CurrentHeapForDealloc::Own(p),
        // null (first call ever, on this thread) OR TORN (slot already
        // recycled): both are "no live heap of our own to consult" — route
        // as foreign, no bind, no fallback lock.
        Ok(_) => CurrentHeapForDealloc::ForeignNoBind,
        // TLS destroyed: same treatment.
        Err(_) => CurrentHeapForDealloc::ForeignNoBind,
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

/// Shared post-claim logic: arm the `AbandonGuard`, publish the pointer into
/// `LOCAL`, and return the tagged result. Called from both
/// [`bind_slow_tagged`] and [`bind_slow_tagged_with_config`].
///
/// task #38: this used to also call `HeapCore::install_thread_free` here
/// ("install the cross-thread TFS handle on the bind-slow path"). That call
/// was dead by construction and has been removed: since task H1 (#13), the
/// cross-thread free-stack head is planted by
/// [`HeapCore::bind_thread_free`](crate::registry::heap_core::HeapCore::bind_thread_free),
/// called from `HeapRegistry::claim`/`claim_with_config` (via
/// `bind_slot_counters`) BEFORE either function returns `heap` to this
/// caller — so `thread_free` is always `Some` by the time `finish_bind` runs,
/// and `install_thread_free` (a pure accessor, `self.thread_free.map_or(null,
/// |h| h as *const _)`) had no side effect to perform and its return value
/// was discarded. Verified by tracing every `heap`-producing path
/// (`claim`/`claim_with_config`'s first-claim AND re-claim legs) to the
/// planting call before any return.
///
/// ## UBFIX-10 (L-6): guard-arm-before-claim-is-observable, with rollback
///
/// Before this fix, both `LOCAL.try_with` and `GUARD.try_with` below silently
/// discarded their `Err`. The dangerous case is `GUARD.try_with` failing (TLS
/// initialisation of the `AbandonGuard` slot can fail if this thread is
/// already tearing down — e.g. `finish_bind` is reached from a resolver
/// called out of some OTHER thread-local's `Drop`, after `std` has started
/// rejecting new TLS-slot initialisation on this thread): the slot returned
/// by `HeapRegistry::claim`/`claim_with_config` above is ALREADY `STATE_LIVE`
/// (the CAS that claims it already ran, inside `claim`, before `finish_bind`
/// was ever called) — but with no armed guard, NOTHING will ever call
/// `HeapRegistry::recycle` on it. The slot is claimed but unguarded: LIVE
/// forever, unreachable by any future `claim` (the free-pool never sees it
/// again) — a permanent availability/resource leak (never UB — this thread
/// never actually gets a usable heap in this branch), one slot per occurrence
/// (out of `MAX_HEAPS`), silent (no error surfaces to the allocation caller,
/// which routes to Fallback exactly as if this were a normal registry
/// exhaustion).
///
/// The fix: arm `GUARD` FIRST (before publishing into `LOCAL`, before
/// returning `Own` to the caller). If arming fails, this claimed slot has no
/// living owner and must not be handed out — recycle it immediately (the
/// exact same `HeapRegistry::recycle` the guard itself would otherwise have
/// called on thread exit) and return `Fallback`, exactly as the
/// registry-exhaustion / primordial-OOM branch above does. `LOCAL` is
/// published only AFTER the guard is confirmed armed, so a partially-bound
/// state (guard armed, `LOCAL` not yet set) can only ever be the LESS severe
/// case: `current()`/`current_for_alloc()` would just re-enter `bind_slow`
/// next call (a re-claim, cheap — `claim` reuses the same slot when
/// `new_gen != 1`) rather than reading a claimed-but-unguarded slot.
///
/// SAFETY: `heap` was just returned by `claim`/`claim_with_config` and has
/// not been recycled yet (this is the only code path that could recycle it
/// between claim and here) — the single-caller contract `HeapRegistry::recycle`
/// documents ("pointer previously returned by `claim`, not yet recycled") is
/// satisfied.
#[cold]
fn finish_bind(heap: *mut HeapCore) -> CurrentHeap {
    let heap = if heap.is_null() {
        // Registry exhausted or primordial OOM: fall back, never null.
        return CurrentHeap::Fallback;
    } else {
        heap
    };

    // UBFIX-10 (L-6): arm the guard FIRST. If this fails, the claimed slot
    // has no living owner to ever recycle it — roll back by recycling it
    // here instead of handing out a claimed-but-unguarded slot.
    if GUARD.try_with(|g| g.heap.set(heap)).is_err() {
        // SAFETY: `heap` was returned by `claim`/`claim_with_config` above
        // and has not yet been recycled (this is the first and only chance —
        // no guard was armed to do it later).
        unsafe { HeapRegistry::recycle(heap) };
        return CurrentHeap::Fallback;
    }

    // Guard is armed. Publish into LOCAL (so subsequent `current()` calls hit
    // the fast path). If THIS fails (rarer still, and less severe — the
    // guard is already armed and will recycle correctly on thread exit),
    // every call on this thread simply re-enters `bind_slow` and re-claims
    // (cheap re-claim of the same slot), never reading a stale/unset LOCAL.
    let _ = LOCAL.try_with(|c| c.set(heap));
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

/// Test-only hook (R6-OPT-P0-1): the [`current_for_dealloc`] analogue of
/// [`dbg_teardown_then_resolve_is_fallback`] above — same technique (poison
/// `LOCAL` via the exact production [`mark_local_torn`] function, resolve,
/// restore), but checks that a TORN thread's DEALLOC resolver reports
/// [`CurrentHeapForDealloc::ForeignNoBind`] rather than re-arming a bind or
/// (unlike the alloc-side hook) routing through the fallback at all — this
/// resolver's whole point is that TORN does NOT touch the fallback lock.
///
/// `#[doc(hidden)]` — not part of the public API; exists solely so the
/// integration test in `tests/` can reach this otherwise-private teardown
/// behaviour, mirroring the established `dbg_teardown_then_resolve_is_fallback`
/// pattern.
#[cfg(feature = "alloc-xthread")]
#[doc(hidden)]
#[must_use]
pub fn dbg_teardown_then_resolve_is_foreign_no_bind() -> bool {
    let saved = LOCAL.with(|c| c.get());
    mark_local_torn();
    let result = current_for_dealloc();
    LOCAL.with(|c| c.set(saved));
    matches!(result, CurrentHeapForDealloc::ForeignNoBind)
}

/// Test-only hook (R6-OPT-P0-1): poison THIS thread's `LOCAL` to [`TORN`]
/// (via the exact production [`mark_local_torn`] function — not a
/// reimplementation) and return the PRE-poison value, so a test can drive a
/// real end-to-end call (e.g. a real `SeferAlloc::dealloc`) under the TORN
/// state and later restore the saved value via
/// [`dbg_restore_local_for_test`]. Unlike
/// [`dbg_teardown_then_resolve_is_foreign_no_bind`] (which pokes, resolves,
/// and restores all in one call), this pair lets the poisoned state persist
/// across an arbitrary caller-supplied operation in between — needed to
/// exercise the REAL `dealloc` entry point (not just the resolver) under
/// TORN from a test.
///
/// `#[doc(hidden)]` — not part of the public API; exists solely so
/// `tests/dealloc_only_no_bind_torn.rs` can reach this otherwise-private
/// teardown behaviour, mirroring the established test-only-export pattern.
#[doc(hidden)]
#[must_use]
pub fn dbg_mark_local_torn_for_test() -> *mut HeapCore {
    let saved = LOCAL.with(|c| c.get());
    mark_local_torn();
    saved
}

/// Test-only hook (R6-OPT-P0-1): restore `LOCAL` to a value previously
/// returned by [`dbg_mark_local_torn_for_test`]. Pairs with that function;
/// see its doc comment.
///
/// `#[doc(hidden)]` — not part of the public API.
#[doc(hidden)]
pub fn dbg_restore_local_for_test(saved: *mut HeapCore) {
    let _ = LOCAL.try_with(|c| c.set(saved));
}
