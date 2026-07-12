#![allow(deprecated)]
//! [`ShardedRegion<T>`] — N-way parallel writes via thread-local shard binding,
//! with **lock-free cross-thread removal** and a **shard lifecycle** (Phase 7b,
//! `experimental`; supersedes 7a's claim-and-never-release model).
//!
//! **Status — legacy/research-tier:** superseded by the production `alloc-xthread`
//! cross-thread free path; kept under the `experimental` feature for backward
//! compatibility and as a research baseline, and `#[deprecated]` on the struct
//! below. No new development is planned (see the `concurrent` module docs).
//!
//! This is pure **safe composition** on top of [`EpochRegion<T>`]: the
//! single-writer-per-shard principle gives each writer *thread* its own
//! [`EpochRegion`], so two writers in different shards never meet on a lock.
//! Reads stay the untouched lock-free `EpochRegion` seqlock. **Zero new
//! `unsafe`** appears here — all pointer work lives in the existing confined
//! [`hand`](super::hand) organ.
//!
//! ## The router (7b)
//!
//! On a thread's *first* [`insert`](ShardedRegion::insert), the TLS router
//! claims a shard for that thread: it scans the per-shard `occupied` tokens for
//! a FREE one (atomic `compare_exchange false → true`), or — if every shard is
//! occupied — falls back to modulo round-robin (graceful degradation: two
//! threads share a shard, still correct, just less parallel). The claim is
//! cached in a `thread_local` cell, and a separate TLS type-erased
//! [`ErasedGuard`] whose `Drop` **releases** the shard on thread exit is
//! installed so a dead thread's shard id can be reused by a new thread.
//!
//! ### One region per thread pool (design assumption)
//!
//! The router's TLS cells (`MY_SHARD`, `ERASED_GUARD`) are **process-global** —
//! one binding per thread, shared across every `ShardedRegion` instance (they
//! cannot be keyed by region without a per-region id). The intended use is a
//! SINGLE long-lived `ShardedRegion` shared across a thread pool. If one thread
//! drives two different regions, they share the one TLS binding: this stays
//! correct (`claim_or_get_shard` re-validates the cached id against the current
//! region's shard count and re-claims if it is out of range, so a smaller
//! region never indexes out of bounds), but the two regions may not each get an
//! exclusive per-thread shard. For the targeted one-region-per-pool topology
//! this is a non-issue.
//!
//! ## Cross-thread removal (7b)
//!
//! [`remove`](Self::remove) routes by `handle.shard`:
//!
//! - if it equals the CALLING thread's claimed shard → owner path
//!   ([`EpochRegion::remove`], which takes the shard's writer mutex for
//!   free-list bookkeeping only; the evict itself is a CAS).
//! - otherwise → the **lock-free** [`EpochRegion::remote_evict`], which performs
//!   the generation-CAS eviction WITHOUT taking the owner shard's writer mutex
//!   and enqueues the freed index into a per-shard remote-free queue the owner
//!   drains later.
//!
//! This is the 7b win: a non-owner-thread remove does not contend on the owner
//! shard's lock.
//!
//! ## Shard lifecycle (7b)
//!
//! A claimed shard is **releasable**: the TLS [`ErasedGuard`] flips the shard's
//! `occupied` token to `false` on `Drop` (thread exit). A new thread may then
//! claim that freed shard. A dead thread's LIVE slots stay resolvable — reads
//! route by `handle.shard` and do NOT depend on ownership (a read never checks
//! `occupied`; it just resolves the slot via the seqlock). An adopting thread
//! that reuses a freed shard drains its abandoned remote-free queue on its
//! first op (the `EpochRegion::insert`/`remove` drain does this automatically).
//!
//! ## Why the guard is type-erased
//!
//! A `thread_local!` is monomorphic — there can be only one guard cell per
//! program, but a process may host `ShardedRegion<A>` and `ShardedRegion<B>`
//! concurrently. So the guard owns the **type-erased** `occupied` tokens
//! (`Arc<[AtomicBool]>`, carrying no `T`) rather than an `Arc<ShardedInner<T>>`.
//! This keeps a single TLS registry sound across multiple `T`. The `Arc` keeps
//! the tokens alive after the region-handling `&self` borrow is gone, so the
//! guard's `Drop` can flip the token at thread-exit even if the
//! `ShardedRegion<T>` is dropped first (the `Arc` refcount holds the tokens).
//!
//! ## Invariants upheld
//!
//! All of [`EpochRegion`]'s invariants hold *per shard*, and the shard routing
//! preserves them across shards:
//!
//! - **I1 — resolution:** a fresh [`ShardedHandle<T>`] resolves to its value
//!   until `remove`d (routed to its own shard — owner or remote path).
//! - **I2 — tombstone:** after `remove(h)`, `get_with(h, …)` is `None` forever;
//!   a second `remove(h)` is a no-op `false` (the CAS returns `Stale`).
//! - **I3 — no ABA:** `remove`/`remote_evict` bumps the slot's generation via
//!   `AtomicSlot::try_evict_at`.
//! - **I4 — accounting:** [`len`](Self::len) sums the live counts (now
//!   `AtomicUsize` per shard, correct under concurrent remote removal).
//! - **Multi-shard locality:** a handle minted in shard A carries
//!   `shard == A` and is routed *only* to shard A.
//!
//! [`EpochRegion<T>`]: crate::concurrent::EpochRegion

use core::cell::{Cell, RefCell};

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use crate::concurrent::{EpochHandle, EpochRegion, ShardedHandle};

/// The `Arc`-shared interior of a [`ShardedRegion`]: the shards themselves plus
/// the round-robin fallback cursor. The per-shard `occupied` tokens live in a
/// SEPARATE `Arc<[AtomicBool]>` ([`ShardedRegion::tokens`]) so they can be
/// owned by a type-erased [`ErasedGuard`] (which carries no `T`), letting a
/// single `thread_local!` registry release any thread's claim on exit.
struct ShardedInner<T> {
    shards: Box<[EpochRegion<T>]>,
    /// Atomic round-robin cursor for the graceful-degradation fallback (when
    /// no free shard is available). `fetch_add` then modulo shard count.
    next_shard: AtomicUsize,
}

/// A type-erased thread-local guard that RELEASES its shard on `Drop` (thread
/// exit). Owns an `Arc<[AtomicBool]>` of the occupied tokens (carrying no `T`)
/// so it can flip the token even after the region-handling `&self` borrow is
/// gone — the `Arc` keeps the tokens alive. `shard` is `None` for a thread
/// that degraded to modulo sharing without an exclusive claim (in that case
/// there is nothing to release).
struct ErasedGuard {
    tokens: Arc<[AtomicBool]>,
    /// `Some(id)` iff THIS thread exclusively claimed shard `id` (won the
    /// `compare_exchange`). `None` for the modulo-degradation fallback (shared
    /// shard — no exclusive release).
    shard: Option<u16>,
}

impl Drop for ErasedGuard {
    fn drop(&mut self) {
        if let Some(id) = self.shard {
            // We are the unique owner of this shard (the CAS that claimed it
            // was atomic, and only THIS guard carries the claim for `id`).
            // Release it so a new thread may adopt it. Release ordering pairs
            // with the adopting thread's Acquire-on-success CAS, so the adopter
            // observes the released state.
            if let Some(occupied) = self.tokens.get(usize::from(id)) {
                occupied.store(false, Ordering::Release);
            }
        }
        // If `shard` is None we degraded to modulo sharing — nothing to
        // release (no exclusive claim was recorded).
    }
}

// The TLS router: `MY_SHARD` caches the claimed shard id for the fast path (a
// plain integer TLS read); `ERASED_GUARD` holds the type-erased guard whose
// `Drop` releases an exclusively-claimed shard on thread exit. `RefCell`
// because `Option<ErasedGuard>` is not `Copy` (the guard owns an `Arc`).
thread_local! {
    static MY_SHARD: Cell<Option<u16>> = const { Cell::new(None) };
}

thread_local! {
    static ERASED_GUARD: RefCell<Option<ErasedGuard>> = const { RefCell::new(None) };
}

/// The default per-shard capacity when none is specified. Generous enough that
/// a moderate workload does not immediately hit the fixed-capacity `Err` path,
/// while staying modest in memory (each shard pre-allocates its slot table).
const DEFAULT_CAP_PER_SHARD: usize = 1024;

/// The hard cap on shard count, matching the `u16` shard id space.
const MAX_SHARDS: usize = u16::MAX as usize;

/// A `u16`-indexed array of [`EpochRegion<T>`] shards with a thread-local
/// router that lazily binds each writer thread to one shard, **releasable** on
/// thread exit (Phase 7b).
///
/// See the [module docs](self) for the design, the router, the lock-free
/// cross-thread removal, and the shard lifecycle.
#[deprecated(
    since = "0.1.0",
    note = "concurrent regions are legacy/research-tier; use the production allocator stack (`alloc-xthread`) for cross-thread allocation needs"
)]
pub struct ShardedRegion<T> {
    inner: Arc<ShardedInner<T>>,
    /// Per-shard `occupied` tokens, `Arc`-shared with every live
    /// [`ErasedGuard`] so a thread's `Drop` can flip its token at exit. Type-
    /// erased (no `T`) for the single-registry reason (see module docs).
    tokens: Arc<[AtomicBool]>,
}

impl<T> ShardedRegion<T> {
    /// Creates a sharded region with `n` shards, each pre-allocated with
    /// `cap_per_shard` vacant slots.
    ///
    /// Each shard is an independent [`EpochRegion`] with its own writer mutex,
    /// free list, and remote-free queue; writers in different shards never
    /// contend. `n` is capped at `u16::MAX` (the shard-id space) — a larger
    /// `n` is clamped with a panic, since it almost certainly indicates a
    /// caller bug.
    ///
    /// # Panics
    ///
    /// Panics if `cap_per_shard` overflows `u32` (delegated to
    /// [`EpochRegion::with_capacity`]) or if `n == 0` (a region with no shards
    /// cannot accept any insert).
    #[must_use]
    pub fn with_shards(n: usize, cap_per_shard: usize) -> Self {
        assert!(n > 0, "ShardedRegion::with_shards: n must be > 0");
        assert!(
            n <= MAX_SHARDS,
            "ShardedRegion::with_shards: n={n} exceeds the u16 shard-id space ({MAX_SHARDS})"
        );
        let shards: Vec<EpochRegion<T>> = (0..n)
            .map(|_| EpochRegion::with_capacity(cap_per_shard))
            .collect();
        // Every shard starts FREE (occupied == false). A thread claims by
        // CASing false → true.
        let tokens: Vec<AtomicBool> = (0..n).map(|_| AtomicBool::new(false)).collect();
        Self {
            inner: Arc::new(ShardedInner {
                shards: shards.into_boxed_slice(),
                next_shard: AtomicUsize::new(0),
            }),
            tokens: Arc::from(tokens.into_boxed_slice()),
        }
    }

    /// Creates a sharded region whose shard count matches the host's available
    /// parallelism (`std::thread::available_parallelism`, falling back to 1 on
    /// error), each shard with a sensible default capacity.
    ///
    /// This is the natural default for a bounded pool of long-lived worker
    /// threads: one shard per hardware thread means writers rarely collide.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of shards (fixed for the region's lifetime).
    #[must_use]
    pub fn shard_count(&self) -> usize {
        self.inner.shards.len()
    }

    /// Total live entries across all shards (I4).
    ///
    /// Sums each shard's [`EpochRegion::len`] (an `AtomicUsize` per shard —
    /// correct under concurrent remote removal). Under concurrency this is a
    /// momentary observation.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.shards.iter().map(EpochRegion::len).sum()
    }

    /// Whether the region holds no live values across any shard (I4).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.shards.iter().all(EpochRegion::is_empty)
    }

    /// Returns the calling thread's claimed shard id, or `None` if it has not
    /// yet bound. Fast path: a plain TLS read (no atomic).
    fn my_shard(&self) -> Option<u16> {
        MY_SHARD.with(|cell| cell.get())
    }

    /// Lazily claims a shard for the calling thread (on first use) and returns
    /// its id. Subsequent calls return the cached binding from TLS.
    ///
    /// **7b claim protocol:**
    /// 1. Scan the `occupied` tokens for a FREE shard; atomically claim it via
    ///    `compare_exchange(false → true)`. The first free shard wins.
    /// 2. If NO shard is free, fall back to modulo round-robin (graceful
    ///    degradation: share a shard, still correct). In this case NO exclusive
    ///    claim is recorded (the guard's `shard` is `None` — nothing to
    ///    release on thread exit; the shared shard stays owned by whoever did
    ///    claim it, or stays free if nobody has).
    ///
    /// The binding (id + a type-erased [`ErasedGuard`] holding an `Arc` to the
    /// tokens) is cached in TLS so the fast path is a plain integer read, and
    /// the guard's `Drop` releases an exclusively-claimed shard on thread exit.
    fn claim_or_get_shard(&self) -> u16 {
        let n = self.inner.shards.len();
        if let Some(id) = self.my_shard() {
            // Robustness: the TLS binding is process-global (one cell across all
            // `ShardedRegion` instances — see the module note on one-region-per
            // -thread-pool). If a thread bound to a shard in a DIFFERENT region
            // with MORE shards, the cached id can exceed THIS region's shard
            // count; returning it verbatim would index out of bounds in
            // `insert`. Only trust the cache when it is in range for this
            // region; otherwise fall through and (re)claim a valid shard here.
            if usize::from(id) < n {
                return id;
            }
        }
        // 1. Try to exclusively claim a FREE shard (scan in order).
        let mut claimed_exclusively: Option<u16> = None;
        for (i, occupied) in self.tokens.iter().enumerate() {
            // Acquire on success: pairs with the releaser's Release store in
            // ErasedGuard::drop, so we observe the released state. Relaxed on
            // failure: we just move on to the next candidate.
            if occupied
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                claimed_exclusively =
                    Some(u16::try_from(i).expect("shard index fits u16: n <= u16::MAX"));
                break;
            }
        }
        // 2. Graceful degradation: no free shard → modulo round-robin. The
        //    ticket is monotonic; modulo spreads across shards.
        let id = claimed_exclusively.unwrap_or_else(|| {
            let ticket = self.inner.next_shard.fetch_add(1, Ordering::Relaxed);
            u16::try_from(ticket % n)
                .expect("shard id fits u16: ticket%n where n<=u16::MAX cannot exceed u16::MAX")
        });
        // Cache the id (fast path).
        MY_SHARD.with(|cell| cell.set(Some(id)));
        // Install (once per thread) the type-erased [`ErasedGuard`] whose `Drop`
        // releases an exclusively-claimed shard on thread exit. The guard owns
        // an `Arc::clone(&self.tokens)` so it outlives any `&self` borrow and
        // can flip the token at thread-exit (the `Arc` keeps the tokens alive).
        ERASED_GUARD.with(|slot| {
            // Idempotent: if a guard is already registered for this thread,
            // do nothing. On the same thread subsequent claims return the
            // cached id via `my_shard` and never reach here except on the
            // very first claim.
            let mut slot = slot.borrow_mut();
            if slot.is_some() {
                return;
            }
            *slot = Some(ErasedGuard {
                tokens: Arc::clone(&self.tokens),
                shard: claimed_exclusively,
            });
        });
        id
    }

    /// Inserts `value` into the calling thread's claimed shard, returning a
    /// fresh [`ShardedHandle<T>`] that resolves to it (I1), or `Err(value)` if
    /// that shard is full (mirroring [`EpochRegion::insert`]).
    ///
    /// On the thread's first insert, lazily claims a shard via the TLS router
    /// (see [the router docs](self#the-router-7b)). The returned handle carries
    /// the shard id, so later reads/removes route back to this shard.
    ///
    /// # Errors
    ///
    /// Returns `Err(value)` (handing the value back unchanged) when the calling
    /// thread's shard is full — every slot occupied or retired.
    ///
    /// # Panics
    ///
    /// Panics if the shard's writer mutex is poisoned.
    pub fn insert(&self, value: T) -> Result<ShardedHandle<T>, T> {
        let shard = self.claim_or_get_shard();
        match self.inner.shards[usize::from(shard)].insert(value) {
            Ok(inner) => Ok(ShardedHandle::new(shard, inner)),
            Err(value) => Err(value),
        }
    }

    /// Resolves `handle` and applies `f` to a shared borrow of the value,
    /// returning `Some(f(...))`, or `None` if the handle is stale/removed/
    /// out-of-range (I1, I2, I3).
    ///
    /// Routes by `handle.shard` to the owning shard, then delegates to that
    /// shard's lock-free [`EpochRegion::get_with`]. The borrow is confined to
    /// the call — `f` may not store the reference.
    ///
    /// **7b:** a read does NOT depend on shard ownership — it resolves the slot
    /// via the seqlock regardless of whether the owning thread is alive. So a
    /// DEAD thread's live slots stay resolvable (asserted in
    /// `tests/sharded_remote.rs`).
    pub fn get_with<R>(&self, handle: ShardedHandle<T>, f: impl FnOnce(&T) -> R) -> Option<R> {
        let shard = self.inner.shards.get(usize::from(handle.shard))?;
        shard.get_with(handle.inner, f)
    }

    /// Convenience: resolves `handle` and returns a clone of the value, or
    /// `None` if stale/removed/out-of-range. Routes by `handle.shard`; lock-free
    /// like [`get_with`](Self::get_with).
    pub fn get_cloned(&self, handle: ShardedHandle<T>) -> Option<T>
    where
        T: Clone,
    {
        self.get_with(handle, T::clone)
    }

    /// Removes the value for `handle`, returning `true` if it was live (and is
    /// now tombstoned), or `false` if it was already stale/removed/out-of-range
    /// (I2 — a second remove is a no-op `false`).
    ///
    /// **7b routing:** if `handle.shard` equals the CALLING thread's claimed
    /// shard, this takes the OWNER path ([`EpochRegion::remove`], which takes
    /// the shard's writer mutex for free-list bookkeeping only — the evict
    /// itself is a CAS). Otherwise it takes the **lock-free** remote path
    /// ([`EpochRegion::remote_evict`]), which performs the generation-CAS
    /// eviction WITHOUT the owner shard's writer mutex and enqueues the freed
    /// index for the owner to drain later. A thread that has not yet claimed a
    /// shard is treated as remote for every handle.
    ///
    /// If `handle.shard` is out of range, this returns `false` rather than
    /// panicking.
    ///
    /// # Panics
    ///
    /// Panics if the owning shard's writer mutex is poisoned (owner path only;
    /// the remote path takes no writer mutex).
    pub fn remove(&self, handle: ShardedHandle<T>) -> bool {
        let Some(shard) = self.inner.shards.get(usize::from(handle.shard)) else {
            return false;
        };
        // Owner path iff THIS thread claimed this shard. Otherwise remote
        // (lock-free, no owner mutex). A thread that never claimed (TLS empty)
        // is remote for every handle.
        let mine = self.my_shard() == Some(handle.shard);
        if mine {
            shard.remove(handle.inner)
        } else {
            shard.remote_evict(handle.inner)
        }
    }

    /// Explicitly binds the CALLING thread to a SPECIFIC shard `id` (Phase 7c,
    /// `pinning`), overriding the lazy round-robin/scan-free claim.
    ///
    /// After this returns `true`, the calling thread's subsequent
    /// [`insert`](Self::insert)/[`get_with`](Self::get_with)/[`remove`](Self::remove)
    /// route to shard `id` directly (the TLS router trusts a cached, in-range
    /// binding on the fast path). This is what makes the `shard == core`
    /// topology deterministic: a thread-per-core runner pins thread *i* to core
    /// *i* and binds it to shard *i*, so each thread owns exactly the shard
    /// matching its core — maximal cache locality, no cross-shard contention,
    /// and (because the hot path holds no lock) naturally async-safe.
    ///
    /// # Returns
    ///
    /// - `true` if `shard < shard_count()` — the binding was recorded. (Whether
    ///   the OS also honored a concurrent `core_affinity` pin is separate and
    ///   best-effort; this method only concerns the *routing* binding.)
    /// - `false` if `shard >= shard_count()` — rejected, no binding recorded.
    ///   This is the chosen contract (over `Result` / clamping) because an
    ///   out-of-range shard id is a caller bug that should be surfaced, not
    ///   silently routed elsewhere, and `bool` keeps the call ergonomic in the
    ///   thread-per-core runner where the caller already knows the count.
    ///
    /// # Concurrency & correctness
    ///
    /// Optionally claims shard `id`'s `occupied` token if it is free (so the
    /// shard-lifecycle release on thread exit still works), but correctness does
    /// NOT depend on exclusivity: two threads binding the same shard is graceful
    /// degradation — both route there, both stay correct, they just share the
    /// shard's writer mutex. The `occupied` CAS is best-effort; if it loses, the
    /// binding is still recorded.
    ///
    /// If the calling thread already has an exclusive claim on a DIFFERENT
    /// shard, that claim is NOT released by this call (releasing happens only on
    /// thread exit via the [`ErasedGuard`]'s `Drop`). In the intended
    /// thread-per-core topology each thread binds exactly once at startup, so
    /// this does not arise.
    #[must_use]
    pub fn bind_current_thread_to_shard(&self, shard: u16) -> bool {
        if usize::from(shard) >= self.inner.shards.len() {
            return false;
        }
        // Optionally claim the `occupied` token for this shard if it is free
        // (best-effort exclusivity for the lifecycle release). Acquire on
        // success pairs with the releaser's Release store in ErasedGuard::drop.
        let claimed_exclusively = self.tokens[usize::from(shard)]
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok();
        // Record the routing binding (fast-path TLS cache).
        MY_SHARD.with(|cell| cell.set(Some(shard)));
        // Install (once per thread) the type-erased guard whose Drop releases an
        // exclusively-claimed shard on thread exit. Idempotent: if a guard is
        // already registered, we do NOT overwrite it (its existing claim, if
        // any, is released at thread exit; overwriting would leak that claim).
        // If this bind won its CAS but a guard already exists from a prior claim
        // on a different shard, the just-won token is simply held until THIS
        // thread exits and the prior guard's Drop runs — correct, just not
        // released early. The thread-per-core runner binds once at startup, so
        // the idempotent path is the norm.
        ERASED_GUARD.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.is_none() {
                *slot = Some(ErasedGuard {
                    tokens: Arc::clone(&self.tokens),
                    shard: claimed_exclusively.then_some(shard),
                });
            }
        });
        true
    }

    /// Resets the calling thread's TLS shard binding to `None`, so its *next*
    /// [`insert`](Self::insert) claims a fresh shard.
    ///
    /// **Diagnostics/testing only.** This does NOT release the previously
    /// claimed shard's `occupied` token (that happens on thread exit via the
    /// [`ErasedGuard`]'s `Drop`); it only clears the TLS id cache so the router
    /// re-runs the claim scan on the next insert. Production code should not
    /// call this.
    #[doc(hidden)]
    pub fn _reset_my_shard_binding_for_tests() {
        MY_SHARD.with(|cell| cell.set(None));
    }
}

impl<T> Default for ShardedRegion<T> {
    fn default() -> Self {
        // available_parallelism is the natural shard count for a bounded pool
        // of long-lived threads (one shard per hardware thread → writers rarely
        // collide). Fall back to 1 on any error (e.g. host reporting failure).
        let n = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(1)
            .min(MAX_SHARDS);
        Self::with_shards(n.max(1), DEFAULT_CAP_PER_SHARD)
    }
}

// Explicitly opting out of a `From<Box<[EpochRegion<T>]>>` or similar — the
// constructor story is `with_shards` / `new` / `Default`, full stop. A stray
// `From` would let a caller construct a region whose shard count disagreed with
// `u16`, bypassing the `with_shards` assertion.
#[allow(clippy::unused_self)]
impl<T> ShardedRegion<T> {
    /// The inner `EpochHandle` a `ShardedHandle` wraps, discarding the shard
    /// routing. Exposed for tests/diagnostics that want to probe a specific
    /// shard directly; not needed for normal use.
    #[must_use]
    pub fn split_handle(handle: ShardedHandle<T>) -> (u16, EpochHandle<T>) {
        (handle.shard, handle.inner)
    }
}
