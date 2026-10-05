//! Raw-pointer TLS binding for the alloc face (Phase 12.3, §2.2 of
//! `ALLOC_PLAN_PHASE12-13.md`).
//!
//! This is the reentrancy-safe TLS routing that replaces the Phase 11
//! `RefCell<Option<Heap>>` binding for the global face. The keystone move:
//! the heap is NOT owned by the TLS slot (RAII-dropped on thread exit); it
//! is a slot in the global [`HeapRegistry`], and the TLS slot caches only a
//! raw `*mut HeapCore` to it. On thread exit, `AbandonGuard::drop` does NOT
//! abandon segments — it releases the slot (`LIVE → FREE`) by dropping its
//! [`HeapLease`], with the `HeapCore` and all its segments staying whole, for
//! reuse by whichever thread claims the slot next (whole-slot reuse, Phase
//! 12.5).
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
//! Every resolver in this module is called only on the owning thread (it
//! reads its own TLS), so the `&mut HeapCore` it yields is exclusive. No
//! other thread writes
//! these bins; cross-thread frees go through the segment's `RemoteFreeRing`,
//! not the bins directly. The registry's atomic protocol (M5-clean bootstrap,
//! claim/recycle CAS) establishes the single writer; this file relies on
//! that, it does not re-establish it.
//!
//! ## TLS teardown and the TORN sentinel
//!
//! `LOCAL` (the cached `*mut HeapCore`) and `GUARD` (the release-on-death
//! guard) are both `thread_local!`s. The hazard at thread teardown is: once
//! `GUARD::drop` drops the `HeapLease` (its `Drop` does the `LIVE → FREE`
//! Release CAS), the slot returns to the free pool and another thread may
//! `claim` it. If a resolver on the *exiting*
//! thread then read `LOCAL` and found its stale (pre-release) pointer, it
//! would hand out a second `&mut HeapCore` aliasing the new owner's. The
//! guard prevents this by stamping `LOCAL` with the `TORN` sentinel BEFORE
//! it drops the lease.
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
//!     if a post-release resolver's `LOCAL.try_with` returned `Ok`, then the
//!     earlier-in-program-order `mark_local_torn` (run by `GUARD::drop`
//!     before the lease's `LIVE → FREE` Drop) must ALSO have returned `Ok` and already written
//!     `TORN`. So any resolver that can still read `LOCAL` reads `TORN`, never
//!     the stale pre-release pointer — whatever the destructor order.
//! (c) On os-keyed platforms where `LOCAL`'s storage may already be gone, the
//!     resolvers' `try_with` returns `Err` → they route to the always-live
//!     Fallback heap, which is likewise safe.
//!
//! Every binding resolver ([`current_for_alloc`],
//! [`current_for_alloc_with_config`]) checks for `TORN` before the
//! non-null check and, on a match, routes to the always-live fallback heap
//! instead of re-arming a new slot (which would leak the just-recycled one
//! and could resurrect a slot that another thread already re-claimed). The
//! two PASSIVE resolvers ([`current_for_dealloc`], [`current_for_trim`]) map
//! `TORN` (and `null`) to "nothing to do" instead — see their own doc
//! comments for why binding/falling-back is wrong for their callers.
//!
//! ## Never-null (M10)
//!
//! [`current_for_alloc()`] resolves to a non-null `*mut HeapCore` in every
//! case:
//! - the cached pointer is set → return it (tagged [`CurrentHeap::Own`]);
//! - the cached pointer is null (first call) → `bind_slow_tagged` claims a
//!   slot and publishes it, or on registry exhaustion falls back to the
//!   primordial heap;
//! - the TLS slot is destroyed (thread teardown) → the fallback branch
//!   resolves to the always-live process-global fallback heap.
//!
//! So the alloc face never returns null for a serviceable request (M10).

// The crate is `#![deny(unsafe_code)]` with `alloc-global` on (see
// `src/lib.rs`); this is the documented raw-pointer TLS seam (Phase 12.3).
// `allow` lifts the crate-level `deny` for this file only — `unsafe`
// anywhere else in the crate is a hard error. Since Ph4b (#2092) the
// `unsafe` surface here is empty: the only core access goes through
// `HeapLease::core` (the sole, `pub(crate)` SAFETY seam of the typed lease,
// a SAFE function whose S1–S4 contract is proven inside `claim.rs`), and
// `HeapLease::drop` owns the `LIVE → FREE` Release CAS.
#![allow(unsafe_code)]

use core::cell::Cell;

use crate::registry::heap_registry::HeapLease;
use crate::registry::{HeapCore, HeapRegistry};

/// Sentinel value stamped into [`LOCAL`] by [`mark_local_torn`] the instant
/// this thread's [`AbandonGuard`] starts tearing down (before the slot is
/// released). It is a "poison" marker, not a heap pointer: it is NEVER
/// dereferenced, only compared against in the three resolvers below. Chosen
/// as `usize::MAX` so it is:
/// - distinct from `null` (the "never bound yet" state), and
/// - distinct from any real `*mut HeapCore` (a live allocation can never sit
///   at the top of the address space — the registry's slot array and every
///   OS-backed segment are far below `usize::MAX`).
///
/// See the module doc's "TLS destructor ordering" section for why this is
/// necessary: without it, a stale non-null `LOCAL` value would survive
/// `GUARD`'s release of the slot, and a resolver reading `LOCAL` afterwards
/// (e.g. from another thread-local's `Drop` that allocates, running after
/// `GUARD` in the reverse-declaration teardown order) would hand out a
/// `&mut HeapCore` into a slot some other thread may have already
/// re-claimed — a second writer, i.e. a data race / UAF.
const TORN: *mut HeapCore = usize::MAX as *mut HeapCore;

thread_local! {
    /// The cached raw pointer to this thread's heap (a slot in the global
    /// [`HeapRegistry`]). `null` until the first call to [`current_for_alloc`];
    /// non-null thereafter, until the thread exits — at which point the
    /// [`AbandonGuard`] releases the slot (via the [`HeapLease`]'s `LIVE → FREE`
    /// Drop) AND stamps this cell to [`TORN`]
    /// (via [`mark_local_torn`]) BEFORE releasing it, so a post-teardown
    /// read never observes the stale pre-release pointer. Some other
    /// thread-local's `Drop` (declared before `LOCAL`, hence destroyed
    /// after it — reverse declaration order) can legitimately still
    /// allocate/deallocate after `GUARD` has dropped; every resolver checks
    /// for `TORN` and routes such a call to the fallback heap instead of
    /// dereferencing this stale slot.
    ///
    /// Stored as `Cell<*mut HeapCore>` (not `RefCell`) so there is no
    /// borrow state to fail under reentrancy: reading is a single load.
    static LOCAL: Cell<*mut HeapCore> = const { Cell::new(core::ptr::null_mut()) };

    /// The thread-exit abandon guard. Holds the thread's [`HeapLease`]
    /// (set in [`finish_bind`]) so its `Drop` does not need to read `LOCAL`
    /// (which may already be torn down). On drop: take the lease — `None` →
    /// nothing to do (the thread never bound a heap); otherwise stamp
    /// [`LOCAL`] [`TORN`], trim the core, and let the lease's own `Drop`
    /// do the `LIVE → FREE` Release CAS.
    ///
    /// Stored as `Cell<Option<HeapLease>>` — not `RefCell` and not a bare
    /// lease — so the guard itself stays `Drop`-less: `Cell` has no borrow
    /// state to fail under reentrancy, and `take()` MOVES the lease out
    /// without dropping it, so the `LIVE → FREE` CAS runs exactly once, at
    /// the explicit `drop(lease)` in the teardown path. Reentrancy semantics
    /// are unchanged: `GUARD` was already a `thread_local!` with a
    /// destructor.
    static GUARD: AbandonGuard = const { AbandonGuard::new() };
}

/// The per-thread abandon guard. See the module docs for the TLS destructor
/// ordering reasoning.
struct AbandonGuard {
    /// The typed lease for the heap this thread bound via
    /// [`bind_slow_tagged`]. Read ONLY in `Drop` (never in `LOCAL`-reading
    /// code paths). Storing it here is what makes the guard robust to
    /// `LOCAL` being torn down first.
    lease: Cell<Option<HeapLease>>,
}

impl AbandonGuard {
    /// Construct an empty guard (no heap bound yet). `const` so it can
    /// initialise a `thread_local!` slot.
    const fn new() -> Self {
        Self {
            lease: Cell::new(None),
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
        let Some(mut lease) = self.lease.take() else {
            return; // This thread never bound a registry heap.
        };
        // Stamp `LOCAL` as TORN *before* the lease's `LIVE → FREE` Drop below.
        // This does NOT rely on thread-local destructor ordering (std does not
        // specify it — see the module doc's "TLS teardown and the TORN
        // sentinel" section). It is sound because: `LOCAL` is a `Drop`-less
        // `const` Cell that (on native TLS) is never torn down, so it stays
        // readable; and TLS access is monotone in program order, so any later
        // resolver that still gets `Ok` from `LOCAL.try_with` runs AFTER this
        // write and therefore observes `TORN`, never the stale pre-release
        // pointer. If `try_with` here returns `Err` (`LOCAL` already gone on
        // an os-keyed platform), no post-teardown reader of `LOCAL` can run
        // either — those resolvers get `Err` too and route to Fallback — so
        // the no-op is safe.
        mark_local_torn();
        // Complete the terminal sidecar cut and owner cache trim before
        // releasing this thread's ownership. Never mutate the core after the
        // lease Drop.
        // S1–S4 of addendum §2.1 via `HeapLease::core` — this thread is the
        // sole owner until the `drop(lease)` below; trim mutates only
        // owner-owned state.
        lease.core().trim_for_recycle();
        // HeapLease::drop: LIVE → FREE (Release), abort on a lost CAS —
        // owner decision A3. It allocates nothing and never touches TLS.
        drop(lease);
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
        // First call on this thread: bind a slot. bind_slow_tagged returns
        // either an Own pointer or, on registry exhaustion, the fallback marker.
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

/// Which route the dealloc-only resolver selected without binding a heap.
#[cfg(feature = "alloc-xthread")]
#[must_use]
pub enum CurrentHeapForDealloc {
    /// A registry slot owned by this thread (identical fast path to
    /// [`CurrentHeap::Own`] — the thread has a real, bound heap).
    Own(*mut HeapCore),
    /// This thread never bound a heap, its slot was recycled, or its TLS is
    /// torn down. For pointers not identified as fallback-owned, route through
    /// foreign deallocation routing without claiming a slot or taking
    /// the fallback lock.
    ForeignNoBind,
}

/// R6-OPT-P0-1: a **dealloc-only** resolver — it does not bind a heap. A
/// bind-less pointer follows the allocation-independent remote route.
/// `SeferAlloc::dealloc` used to call
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
/// **Passive, read-only.** This resolver reads `LOCAL`; it never writes it,
/// never calls `HeapRegistry::claim`, and never acquires the fallback lock.
/// Gated on `alloc-xthread`, which `alloc-global` implies.
///
/// - real pointer (own heap bound) → [`CurrentHeapForDealloc::Own`] —
///   identical fast path to [`current_for_alloc`]'s `Own` arm, unchanged.
/// - `null` / `TORN` / `Err` → [`CurrentHeapForDealloc::ForeignNoBind`]; the
///   stale pointer is never dereferenced.
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
/// cached) never materialises a `LargeCacheConfig` value copy on the
/// stack. That copy happens only on the cold `bind_slow_tagged_with_config`
/// branch, where it is amortised across the thread's lifetime.
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

/// A **passive, read-only** resolver for callers that must NOT bind a slot
/// as a side effect of asking "do I already have a heap?" — e.g.
/// [`SeferAlloc::trim_current_thread`](super::SeferAlloc::trim_current_thread),
/// which has nothing useful to do on a thread that has never allocated (there
/// is no tcache/pool/cache state to trim), so speculatively claiming a fresh
/// registry slot just to immediately trim it empty would only waste a slot.
///
/// Same `LOCAL`-read shape as [`current_for_alloc`]/[`current_for_dealloc`],
/// but ALL three "no live own-thread heap" cases — `null` (never bound),
/// `TORN` (this thread's `AbandonGuard` already recycled its slot), and
/// `Err` (TLS destroyed) — map to `None`. **Never calls
/// `bind_slow_tagged` and never resolves the fallback pointer**: unlike
/// [`current_for_alloc`] (whose `null` arm binds) and unlike
/// [`current_for_dealloc`] (whose bind-less arm still routes a live
/// pointer through cross-thread free-routing), this resolver's whole
/// contract is "tell me only what already exists, touch nothing new."
///
/// - real pointer (own heap already bound) → `Some(heap)` — identical fast
///   path to [`current_for_alloc`]'s `Own` arm.
/// - `null` / `TORN` / `Err` → `None` — nothing to report, nothing claimed.
#[inline(always)]
pub fn current_for_trim() -> Option<*mut HeapCore> {
    match LOCAL.try_with(|c| c.get()) {
        // Same Э2 (task #145) one-branch collapse as `current_for_alloc`/
        // `current_for_dealloc`: real p → `< MAX-1` → Some (fast); null (0)
        // and TORN (MAX) both fall to the cold arm below, where THIS
        // resolver (like `current_for_dealloc`, unlike `current_for_alloc`)
        // never binds and never touches the fallback — it maps every
        // no-live-heap case to `None`.
        Ok(p) if p.addr().wrapping_sub(1) < usize::MAX - 1 => Some(p),
        // null (never bound) or TORN (slot already recycled): nothing to
        // trim, and nothing should be claimed just to check.
        Ok(_) => None,
        // TLS destroyed: same treatment — nothing to report.
        Err(_) => None,
    }
}

/// Claim a registry slot, used by [`current_for_alloc`] so the alloc face
/// knows whether it got an own-thread slot or the fallback (and therefore
/// whether to take the lock-free path or the spinlock path). `#[cold]` —
/// runs once per thread.
///
/// On registry exhaustion (every slot is LIVE and the free pool is empty —
/// pathological: > `MAX_HEAPS` simultaneous threads), returns
/// [`CurrentHeap::Fallback`] (the alloc face then routes through the
/// always-live primordial heap — never null, M10).
#[cold]
fn bind_slow_tagged() -> CurrentHeap {
    finish_bind(HeapRegistry::claim_lease())
}

/// Like [`bind_slow_tagged`] but uses [`HeapRegistry::claim_lease_with_config`]
/// so the newly materialised `HeapCore` is configured with `config`. On a
/// re-claim the existing `HeapCore` is reused as-is.
///
/// Only present under `alloc-decommit`.
#[cfg(feature = "alloc-decommit")]
#[cold]
fn bind_slow_tagged_with_config(config: crate::alloc_core::LargeCacheConfig) -> CurrentHeap {
    finish_bind(HeapRegistry::claim_lease_with_config(config))
}

/// Shared post-claim logic: store the [`HeapLease`] into the `AbandonGuard`,
/// publish the pointer into `LOCAL`, and return the tagged result. Called
/// from both [`bind_slow_tagged`] and [`bind_slow_tagged_with_config`].
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
/// ## Order: publish `LOCAL`, then arm `GUARD` (fxx R2-01; was UBFIX-10's guard-first)
///
/// On std <= 1.92 arming `GUARD` (a `Drop` `thread_local!`) registers its
/// destructor by pushing onto std's `DTORS` `Vec` on the GLOBAL allocator, so
/// the push can re-enter this allocator on the same thread. With `LOCAL`
/// still null that re-entry bound a second slot and armed `GUARD` again
/// under std's live `RefMut` -> `rtabort!("the global allocator may not use
/// TLS with destructors")`. Publishing `LOCAL` first makes the re-entry
/// resolve to `Own(heap)`. On std >= 1.93 (`DTORS` on `System`, the crate's
/// MSRV) there is no re-entry and the order is inert; it stays as defence.
/// No `&mut HeapCore` is live here (the raw pointer is a same-thread TLS
/// derivative of the lease per addendum §2.1 S3), so a nested alloc through
/// `heap` does not alias the caller's later borrow.
///
/// ## Rollback (UBFIX-10 / L-6: never hand out a slot without a live guard)
///
/// - `LOCAL.try_with` fails (os-keyed TLS only; native `LOCAL` is a
///   `Drop`-less `const` `Cell`): nothing published or stored -> drop the
///   lease (its `Drop` does the `LIVE → FREE` Release), `Fallback`.
/// - `GUARD.try_with` fails: reset `LOCAL` to null BEFORE dropping the lease
///   (owner decision), then `Fallback`. The next call on this thread
///   re-attempts a bind, as before this order change. Blocks carved through
///   the heap meanwhile stay valid: the release returns the slot, not its
///   segments, and a later free from this thread sees null `LOCAL` and routes
///   as foreign.
///
/// `drop(lease)` deliberately runs OUTSIDE any `try_with` closure: dropping
/// inside a closure whose borrow of the TLS cell is live risks a nested-TLS
/// access. `HeapLease::drop` allocates nothing and never touches TLS (it is
/// one CAS plus two relaxed stores on the registry), so it is safe on the
/// teardown/dtor path.
#[cold]
fn finish_bind(lease: Option<HeapLease>) -> CurrentHeap {
    let Some(mut lease) = lease else {
        // Registry exhausted or primordial OOM: fall back, never null.
        return CurrentHeap::Fallback;
    };
    // Nested bind (re-entrancy): an allocation made INSIDE the outer claim —
    // e.g. NUMA topology initialisation — re-enters the global allocator on
    // this thread while `LOCAL` is still null, and that inner call completes
    // a full bind (publishes `LOCAL`, arms `GUARD`) before the outer claim
    // returns. Keep the inner bind and release the outer lease: publishing
    // the outer heap over it would orphan the inner slot (still LIVE, never
    // recycled) and reroute this thread's inner blocks as foreign.
    if let Ok(current) = LOCAL.try_with(|c| c.get()) {
        if !current.is_null() && current != TORN {
            drop(lease); // LIVE → FREE; allocates nothing, no TLS.
            return CurrentHeap::Own(current);
        }
    }
    // S1–S4 of addendum §2.1 via `HeapLease::core`; the raw pointer is a
    // same-thread TLS derivative of the lease's exclusive borrow.
    let heap = lease.core() as *mut HeapCore;

    // fxx R2-01: publish LOCAL first so a std <= 1.92 re-entry from GUARD's
    // destructor registration resolves to this slot, not a second bind.
    if LOCAL.try_with(|c| c.set(heap)).is_err() {
        // HeapLease Drop: LIVE → FREE (Release). Allocates nothing, no TLS.
        drop(lease);
        return CurrentHeap::Fallback;
    }

    // UBFIX-10 (L-6): no guard -> un-publish LOCAL, then release the slot.
    // `Cell::set` on `Some(lease)` would drop a previous lease if one were
    // still stored (None in normal operation); use take/replace to move it
    // in without relying on that. The lease travels via `Option::take` so it
    // is only moved when the closure actually runs (on `try_with` `Err` the
    // closure never executes and ownership stays with the caller).
    let mut lease = Some(lease);
    let armed = GUARD
        .try_with(|g| {
            let old = g.lease.take();
            debug_assert!(old.is_none());
            drop(old); // normal operation: None, no-op
            g.lease.set(lease.take());
        })
        .is_ok();
    let Some(lease) = lease else {
        return CurrentHeap::Own(heap);
    };
    if !armed {
        let _ = LOCAL.try_with(|c| c.set(core::ptr::null_mut()));
        // Rollback: LOCAL := null BEFORE Drop lease (owner decision).
        // HeapLease Drop: LIVE → FREE (Release). Allocates nothing, no TLS.
        drop(lease);
        return CurrentHeap::Fallback;
    }

    CurrentHeap::Own(heap)
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
// R29-7 (task #438): gated behind `bench-internals` (CLAUDE.md's
// benchmark-hook rule 2 — no production caller). Kept a SAFE `fn`: its body
// only reads/writes the bit-pattern of `LOCAL` (a `Cell<*mut HeapCore>`) and
// stamps [`TORN`] via [`mark_local_torn`] — it never dereferences anything,
// so there is no safety contract it could carry (per the tier-2 rule, each
// `unsafe` site must have its OWN documented reason; API symmetry with its
// `unsafe` twin below is not one). The pairing IS real — a caller should use
// the two together — but that is a usage convention, not a soundness duty.
#[doc(hidden)]
#[cfg(feature = "bench-internals")]
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
/// # Safety
///
/// `saved` must be EITHER null OR a pointer previously returned by
/// [`dbg_mark_local_torn_for_test`] for a heap THIS EXACT THREAD legitimately
/// owns, and it must NEVER be sent across threads. Installing a pointer owned
/// by — or shared with — another thread aliases two threads onto one
/// [`HeapCore`], violating the single-writer invariant this module's
/// "Soundness of the raw pointer" section rests on, which is UB. An arbitrary
/// or dangling non-null pointer is likewise UB: the next `current_for_alloc`
/// resolver classifies a non-nullish value as `CurrentHeap::Own` and
/// dereferences it.
///
/// `#[doc(hidden)]` — not part of the public API.
// R29-7 (task #438): `pub unsafe fn` + `bench-internals`-gated. This hook
// installs a caller-supplied raw pointer as THIS thread's live `LOCAL`
// binding with zero validation — a direct instance of the R25-1 (task #395)
// safe-`pub fn`-that-touches-allocator-state hole this crate's benchmark-hook
// rule targets. It is covered by this file's existing tier-1
// `#![allow(unsafe_code)]` (NO item-level `#[allow(unsafe_code)]` is added, so
// no new tier-2 site is created): `tls_heap.rs` already holds `unsafe` for the
// lease-core handoff + `trim_for_recycle`. `dbg_dealloc_own_thread_with_base` /
// `dbg_flush_class_only` in `heap_core_diag.rs` are the item-scoped (tier-2)
// positive pattern this mirrors where the enclosing file is otherwise safe.
#[doc(hidden)]
#[cfg(feature = "bench-internals")]
pub unsafe fn dbg_restore_local_for_test(saved: *mut HeapCore) {
    let _ = LOCAL.try_with(|c| c.set(saved));
}
