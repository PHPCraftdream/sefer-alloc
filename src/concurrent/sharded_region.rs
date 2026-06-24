//! [`ShardedRegion<T>`] — N-way parallel writes via thread-local shard binding
//! (Phase 7a, `experimental`).
//!
//! This is pure **safe composition** on top of [`EpochRegion<T>`]: the
//! single-writer-per-shard principle gives each writer *thread* its own
//! [`EpochRegion`], so two writers in different shards never meet on a lock.
//! Reads stay the untouched lock-free `EpochRegion` seqlock. **Zero new
//! `unsafe`** appears here — all pointer work lives in the existing confined
//! [`hand`](super::hand) organ.
//!
//! ## The router
//!
//! On a thread's *first* [`insert`](ShardedRegion::insert), the TLS router
//! lazily claims a shard for that thread via an atomic round-robin counter
//! (`fetch_add` modulo the shard count) and caches the result in a
//! `thread_local` cell. Every subsequent op on that thread routes to its
//! claimed shard. Threads beyond `n` share a shard by modulo — graceful
//! degradation (still correct, just less parallel).
//!
//! ## Honest edge (7a)
//!
//! A claimed shard is **never released** in 7a: once a thread binds to a shard
//! id, that binding persists for the thread's lifetime even after the thread
//! exits. This fits a **bounded pool of long-lived threads** (the stated 7a
//! target — e.g. a fixed worker pool). A thread-per-connection churn model
//! would exhaust shard ids over time (the shard lifecycle — release, adoption,
//! drain — lands in 7b). Until then: size `n` to your stable thread count.
//!
//! ## Invariants upheld
//!
//! All of [`EpochRegion`]'s invariants hold *per shard*, and the shard routing
//! preserves them across shards:
//!
//! - **I1 — resolution:** a fresh [`ShardedHandle<T>`] resolves to its value
//!   until `remove`d (routed to its own shard).
//! - **I2 — tombstone:** after `remove(h)`, `get_with(h, …)` is `None` forever;
//!   a second `remove(h)` is a no-op `false`.
//! - **I3 — no ABA:** `remove` delegates to `EpochRegion::remove`, which bumps
//!   the slot's generation.
//! - **I4 — accounting:** [`len`](Self::len) sums the live counts across all
//!   shards.
//! - **Multi-shard locality:** a handle minted in shard A carries
//!   `shard == A` and is routed *only* to shard A, so it can never resolve
//!   against shard B's slot table (asserted in `tests/sharded.rs`).
//!
//! [`EpochRegion<T>`]: crate::concurrent::EpochRegion

use core::cell::Cell;

use crate::concurrent::{EpochHandle, EpochRegion, ShardedHandle};

/// A `u16`-indexed array of [`EpochRegion<T>`] shards with a thread-local
/// router that lazily binds each writer thread to one shard.
///
/// See the [module docs](self) for the design, the router, and the honest 7a
/// edge (shards are not released).
pub struct ShardedRegion<T> {
    shards: Box<[EpochRegion<T>]>,
    /// Atomic round-robin cursor for lazy shard claiming. `fetch_add` then
    /// modulo shard count → the shard id a new thread binds to. Monotonic;
    /// never resets (7a has no shard release).
    next_shard: std::sync::atomic::AtomicUsize,
}

// The TLS router: caches the shard id a thread has claimed, or `None` if it has
// not yet bound. Lazily populated on the thread's first `insert`.
thread_local! {
    static MY_SHARD: Cell<Option<u16>> = const { Cell::new(None) };
}

/// The default per-shard capacity when none is specified. Generous enough that
/// a moderate workload does not immediately hit the fixed-capacity `Err` path,
/// while staying modest in memory (each shard pre-allocates its slot table).
const DEFAULT_CAP_PER_SHARD: usize = 1024;

/// The hard cap on shard count, matching the `u16` shard id space.
const MAX_SHARDS: usize = u16::MAX as usize;

impl<T> ShardedRegion<T> {
    /// Creates a sharded region with `n` shards, each pre-allocated with
    /// `cap_per_shard` vacant slots.
    ///
    /// Each shard is an independent [`EpochRegion`] with its own writer mutex
    /// and free list; writers in different shards never contend. `n` is capped
    /// at `u16::MAX` (the shard-id space) — a larger `n` is clamped with a
    /// panic, since it almost certainly indicates a caller bug.
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
        Self {
            shards: shards.into_boxed_slice(),
            next_shard: std::sync::atomic::AtomicUsize::new(0),
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
        self.shards.len()
    }

    /// Total live entries across all shards (I4).
    ///
    /// Sums each shard's [`EpochRegion::len`]. Under concurrency this is a
    /// momentary observation — a writer in any shard may change it immediately
    /// afterwards. Acquires each shard's writer mutex in turn, so it is NOT
    /// lock-free (use it for accounting/diagnostics, not on a hot path).
    ///
    /// # Panics
    ///
    /// Panics if any shard's writer mutex is poisoned.
    #[must_use]
    pub fn len(&self) -> usize {
        self.shards.iter().map(EpochRegion::len).sum()
    }

    /// Whether the region holds no live values across any shard (I4).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.shards.iter().all(EpochRegion::is_empty)
    }

    /// Lazily claims a shard for the calling thread (on first use) and returns
    /// its id. Subsequent calls return the cached binding from TLS.
    ///
    /// The claim is an atomic `fetch_add` on the round-robin cursor, modulo the
    /// shard count. Threads beyond `n` share a shard by modulo — correct, just
    /// less parallel. The binding is cached in a `thread_local` cell so the
    /// fast path is a plain TLS read with no atomic.
    fn claim_or_get_shard(&self) -> u16 {
        MY_SHARD.with(|cell| {
            if let Some(id) = cell.get() {
                return id;
            }
            // First insert on this thread: claim the next shard round-robin.
            // `fetch_add` gives every thread a distinct monotonic ticket; the
            // modulo spreads tickets across shards. Relaxed is fine — we only
            // need mutual exclusion of the *tickets*, and the TLS cell then
            // caches the result so each thread claims exactly once.
            let ticket = self
                .next_shard
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let n = self.shards.len();
            let id = u16::try_from(ticket % n).expect(
                "shard id fits u16: ticket%n where n<=u16::MAX cannot exceed u16::MAX",
            );
            cell.set(Some(id));
            id
        })
    }

    /// Inserts `value` into the calling thread's claimed shard, returning a
    /// fresh [`ShardedHandle<T>`] that resolves to it (I1), or `Err(value)` if
    /// that shard is full (mirroring [`EpochRegion::insert`]).
    ///
    /// On the thread's first insert, lazily claims a shard via the TLS router
    /// (see [the router docs](self#the-router)). The returned handle carries
    /// the shard id, so later reads/removes route back to this shard.
    ///
    /// # Errors
    ///
    /// Returns `Err(value)` (handing the value back unchanged) when the calling
    /// thread's shard is full — every slot occupied or retired. The shard does
    /// not grow; remove a value to free a slot, or size the shard larger at
    /// construction ([`with_shards`](Self::with_shards)).
    ///
    /// # Panics
    ///
    /// Panics if the shard's writer mutex is poisoned.
    pub fn insert(&self, value: T) -> Result<ShardedHandle<T>, T> {
        let shard = self.claim_or_get_shard();
        match self.shards[usize::from(shard)].insert(value) {
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
    /// A handle minted in shard A is routed *only* to shard A; it can never
    /// resolve against shard B's slot table (the multi-shard locality
    /// property). If `handle.shard` is out of range (a handle from a different
    /// region, or corruption), this returns `None` rather than panicking.
    pub fn get_with<R>(&self, handle: ShardedHandle<T>, f: impl FnOnce(&T) -> R) -> Option<R> {
        let shard = self.shards.get(usize::from(handle.shard))?;
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
    /// Routes by `handle.shard` to the owning shard, then delegates to that
    /// shard's [`EpochRegion::remove`] (which bumps the generation — I3). Note:
    /// in 7a the *caller's* thread does the remove against the owning shard's
    /// writer mutex; a non-owner-thread remove is correct (the mutex
    /// serialises it) but contends on that shard's lock. The lock-free
    /// cross-thread remove lands in 7b.
    ///
    /// If `handle.shard` is out of range, this returns `false` rather than
    /// panicking.
    ///
    /// # Panics
    ///
    /// Panics if the owning shard's writer mutex is poisoned.
    pub fn remove(&self, handle: ShardedHandle<T>) -> bool {
        let Some(shard) = self.shards.get(usize::from(handle.shard)) else {
            return false;
        };
        shard.remove(handle.inner)
    }

    /// Resets the calling thread's TLS shard binding to `None`, so its *next*
    /// [`insert`](Self::insert) claims a fresh shard.
    ///
    /// **Diagnostics/testing only.** This does NOT release the previously
    /// claimed shard (7a has no shard release — see the [honest
    /// edge](self#honest-edge-7a)); it only clears the TLS cache so the round
    /// robin advances on the next insert. Production code should not call this.
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
