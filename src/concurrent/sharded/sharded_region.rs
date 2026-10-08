#![allow(deprecated)]
//! [`ShardedRegion<T>`] — N-way parallel writes via thread-local shard binding,
//! with cross-thread removal (lock-free eviction CAS, blocking remote-free
//! enqueue) and a **shard lifecycle** (Phase 7b,
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
//! [`hand`](crate::concurrent::epoch::hand) organ.
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
//! ### Per-region binding (R11 P4-3)
//!
//! The shard cache (`MY_SHARDS`) is a bounded per-thread table keyed by a
//! never-reused, process-global region id: `(region_id, shard)`. A binding
//! made against region A is never trusted for region B, so a thread driving
//! several regions claims (or shares) a shard in each one independently, and
//! owner/remote removal routing compares against THIS region's binding only.
//!
//! The table holds at most `MAX_REMEMBERED_REGIONS` (8) entries per thread;
//! when full, the OLDEST entry is evicted (FIFO) to admit a new region. A
//! thread that drives more live regions than that loses the evicted region's
//! cached binding: its next insert there re-runs the claim scan (a fresh
//! exclusive claim if one is free, otherwise modulo sharing), and until then
//! its removals in that region take the remote path. The evicted claim's
//! `occupied` token is NOT released early — it stays held until thread exit
//! (the [`ErasedGuard`]) — so such a thread may hold more than one shard of
//! one region; this is a throughput loss, never a correctness one. Entries of
//! dropped regions are never matched again (ids are not reused) and age out
//! by the same FIFO eviction.
//!
//! ### Dead-region token retention (R13-02)
//!
//! A claim is a `Weak` to the region's out-of-line token backing
//! ([`TokenBlock`]): dropping the region destroys the token storage even
//! while every claiming thread still holds a claim record. A dead claim can
//! no longer upgrade — its `occupied == true` state died with the storage —
//! releases nothing at thread exit, and is eligible for pruning at cold
//! claim/bind points after an observed change in the global wrapping death
//! hint. Claims on LIVE regions always upgrade and are never pruned or
//! cap-evicted; each retained dead claim costs a fixed-size `Weak` header,
//! never the token storage. Unrelated region deaths can also trigger sweeps.
//! Saving the pre-sweep hint preserves a racing death for a later cold
//! point that observes it; a full 2^64-death wrap can alias the saved hint
//! and delay pruning until another observed change (R14-02).
//!
//! ### Late TLS destructors (R14-04)
//!
//! Late inserts keep an available cached binding or modulo-share on a cold
//! miss, without acquiring an unrecordable token. Explicit
//! [`bind_current_thread_to_shard`](ShardedRegion::bind_current_thread_to_shard)
//! returns `false` if either router TLS cell is destroyed (R14-04).
//!
//! ## Cross-thread removal (7b)
//!
//! [`remove`](ShardedRegion::remove) routes by `handle.shard`:
//!
//! - if it equals the CALLING thread's claimed shard → owner path
//!   ([`EpochRegion::remove`], which takes the shard's writer mutex for
//!   free-list bookkeeping only; the evict itself is a CAS).
//! - otherwise → [`EpochRegion::remote_evict`], which performs the
//!   generation-CAS eviction WITHOUT taking the owner shard's writer mutex
//!   (only the eviction CAS is lock-free), then enqueues the freed index into
//!   a per-shard remote-free queue (`Mutex<Vec<u32>>`) — a brief blocking
//!   lock on the REMOTE thread, never on the owner shard's writer mutex —
//!   for the owner to drain later.
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
//! A claim whose region has since died is eligible for cold claim/bind
//! pruning after an observed death-hint change (R13-02/R14-02) — its token
//! storage no longer exists to release.
//!
//! ## Why the guard is type-erased
//!
//! A `thread_local!` is monomorphic — there can be only one guard cell per
//! program, but a process may host `ShardedRegion<A>` and `ShardedRegion<B>`
//! concurrently. So the guard owns **type-erased** claims (`Weak<TokenBlock>`,
//! carrying no `T`) rather than anything mentioning `T`. This keeps a single
//! TLS registry sound across multiple `T`. The claim is `Weak`, not strong,
//! so the region — the backing's only strong owner — frees the token storage
//! when it drops (R13-02), and the guard's `Drop` upgrades each claim: live
//! regions get their token flipped at thread exit exactly as before; dead
//! regions have nothing left to flip.
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
//! - **I4 — accounting:** [`len`](ShardedRegion::len)/[`is_empty`](ShardedRegion::is_empty)
//!   sum/scan the per-shard `AtomicUsize` counts. Exact only when no
//!   concurrent mutation is in flight; under concurrent insert/remove the
//!   result is an approximate, non-linearizable observation (see the method
//!   docs) — not a drain-complete/shutdown signal.
//! - **Multi-shard locality:** a handle minted in shard A carries
//!   `shard == A` and is routed *only* to shard A.
//!
//! [`EpochRegion<T>`]: crate::concurrent::EpochRegion

use core::cell::RefCell;

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

use crate::concurrent::{EpochHandle, EpochRegion, ShardedHandle};

/// The `Arc`-shared interior of a [`ShardedRegion`]: the shards themselves plus
/// the round-robin fallback cursor. The per-shard `occupied` tokens live in a
/// SEPARATE out-of-line [`TokenBlock`] ([`ShardedRegion::tokens`]) so a
/// type-erased [`ErasedGuard`] (which carries no `T`) can hold `Weak` claims
/// against it, letting a single `thread_local!` registry release any thread's
/// claim on exit while the token storage itself still dies with the region
/// (R13-02).
struct ShardedInner<T> {
    shards: Box<[EpochRegion<T>]>,
    /// Atomic round-robin cursor for the graceful-degradation fallback (when
    /// no free shard is available). `fetch_add` then modulo shard count.
    next_shard: AtomicUsize,
}

/// The out-of-line per-shard `occupied`-token backing (R13-02).
///
/// A [`ShardedRegion`] owns the backing; TLS claims are `Weak`. An exit-release
/// upgrade may retain it temporarily until that release completes.
/// The final strong owner's drop destroys the boxed slot array even while
/// dead claims retain weak references to the fixed-size header.
/// A lingering `Weak` pins only this
/// fixed-size header allocation, never the `slots` storage; a plain
/// `Arc<[AtomicBool]>` claim could not do that, because `Weak<[T]>` keeps
/// the INLINE `[T]` allocation itself alive until the last weak reference
/// drops.
struct TokenBlock {
    slots: Box<[AtomicBool]>,
}

/// Undestroyed token backing count, including transient exit-release upgrades.
/// Instrumentation for `ShardedRegion::_live_token_blocks_for_tests`
/// (R13-02's storage-lifetime oracle); absent from builds without
/// `internals`.
#[cfg(feature = "internals")]
static LIVE_TOKEN_BLOCKS: AtomicUsize = AtomicUsize::new(0);

/// Wrapping token-death hint: unchanged means no claim checks at a cold point.
/// Any region's death can trigger a sweep, including unrelated regions.
/// Release/Acquire makes an observed death's zero strong count visible.
static TOKEN_BACKING_DEATHS: AtomicU64 = AtomicU64::new(0);

/// `internals` work-bound oracle (R14-02): how many per-claim
/// `strong_count` checks `prune_dead_claims` has performed process-wide.
#[cfg(feature = "internals")]
static PRUNE_CLAIM_CHECKS: AtomicUsize = AtomicUsize::new(0);

impl TokenBlock {
    /// `n` slots, every shard FREE (`occupied == false`); a thread claims by
    /// CASing false → true.
    fn new(n: usize) -> Self {
        let slots: Vec<AtomicBool> = (0..n).map(|_| AtomicBool::new(false)).collect();
        #[cfg(feature = "internals")]
        LIVE_TOKEN_BLOCKS.fetch_add(1, Ordering::Release);
        Self {
            slots: slots.into_boxed_slice(),
        }
    }
}

// The drop probe runs exactly when the region's token storage is destroyed:
// it records the death in the global death-generation counter (see
// `prune_dead_claims`) and — under `internals` — maintains the R13-02
// live-backing oracle counter. The boxed slots drop identically in both
// builds.
impl Drop for TokenBlock {
    fn drop(&mut self) {
        // A full 2^64-death cycle can alias the saved hint and delay weak-header
        // pruning until another observed death. Token storage still drops,
        // and live claims remain retained for exit release.
        TOKEN_BACKING_DEATHS.fetch_add(1, Ordering::Release);
        #[cfg(feature = "internals")]
        LIVE_TOKEN_BLOCKS.fetch_sub(1, Ordering::Release);
    }
}

/// A type-erased thread-local guard that RELEASES every exclusively-claimed
/// shard on `Drop` (thread exit). Each claim holds a `Weak` to the region's
/// out-of-line token backing ([`TokenBlock`], carrying no `T`) so the guard
/// can flip the token of every region still alive at thread exit, while a
/// dropped region's token storage dies with the region instead of being
/// pinned by this guard (R13-02).
///
/// A thread may hold MORE than one exclusive claim: e.g. it bound region A's
/// shard `i`, then (having no `MY_SHARDS` entry for region B) scanned region
/// B's `tokens` and won a CAS there — see the module note "Per-region binding".
/// So the guard tracks ALL of the thread's won claims and releases each on
/// `Drop`; an empty `claims` vector means the thread only ever degraded to
/// modulo sharing (nothing to release).
struct ErasedGuard {
    /// Every exclusive claim THIS thread has won. Append-only across a
    /// thread's lifetime; each live one is released on `Drop`, and dead ones
    /// (their region dropped) are pruned at each cold claim/bind point —
    /// but only after an observed token-backing death (R14-02). The scan in
    /// `claim_or_get_shard` can never re-win an already-held shard (its
    /// `occupied` token is `true`), so the list never holds a duplicate
    /// `(tokens, shard)`.
    claims: Vec<ErasedClaim>,
    /// Pre-sweep death hint; a racing death retriggers the next cold sweep.
    last_death_epoch_seen: u64,
}

/// One exclusive shard claim recorded for thread-exit release: a `Weak` to
/// the token backing the CAS was won against (a region's `Arc<TokenBlock>`,
/// type-erased so a single `thread_local!` registry serves every `T`) and the
/// shard id within it. `Weak`, not strong, so the claim never pins a dropped
/// region's token storage (R13-02). A thread may accumulate several when it
/// wins claims against more than one shard or more than one region's backing.
struct ErasedClaim {
    tokens: Weak<TokenBlock>,
    shard: u16,
}

impl Drop for ErasedGuard {
    fn drop(&mut self) {
        // Release every exclusively-claimed shard whose region is still
        // alive; only this guard owns each claim, and the Release store
        // pairs with an adopting thread's Acquire-on-success CAS. A claim
        // whose region died first cannot upgrade — its storage, and the
        // `occupied = true` state with it, is already gone (R13-02) — so
        // there is nothing to release and no stale state to observe.
        for claim in &self.claims {
            if let Some(tokens) = claim.tokens.upgrade() {
                if let Some(occupied) = tokens.slots.get(usize::from(claim.shard)) {
                    occupied.store(false, Ordering::Release);
                }
            }
        }
        // If `claims` is empty we only ever degraded to modulo sharing —
        // nothing to release.
    }
}

/// Sweep dead weak claims only after an observed backing death.
/// Acquire pairs with the death's Release; save the pre-sweep hint so a
/// racing death retriggers cleanup. Live exit-release obligations stay intact.
fn prune_dead_claims(guard: &mut ErasedGuard) {
    let death_epoch = TOKEN_BACKING_DEATHS.load(Ordering::Acquire);
    if death_epoch == guard.last_death_epoch_seen {
        return;
    }
    guard.claims.retain(|claim| {
        #[cfg(feature = "internals")]
        PRUNE_CLAIM_CHECKS.fetch_add(1, Ordering::Relaxed);
        claim.tokens.strong_count() > 0
    });
    guard.last_death_epoch_seen = death_epoch;
}

// The TLS router: `MY_SHARDS` caches per-region claimed shard ids for the fast
// path (a short table scan); `ERASED_GUARD` holds the type-erased guard whose
// `Drop` releases an exclusively-claimed shard on thread exit. `RefCell`
// because `Option<ErasedGuard>` is not `Copy` (the guard owns claims).
thread_local! {
    static MY_SHARDS: RefCell<Vec<(u64, u16)>> = const { RefCell::new(Vec::new()) };
}

/// Max regions whose shard binding one thread remembers; oldest-first (FIFO)
/// eviction beyond it. See the module note "Per-region binding".
const MAX_REMEMBERED_REGIONS: usize = 8;

/// Source of per-instance region ids: process-global, monotonic, never reused.
static NEXT_SHARDED_REGION_ID: AtomicU64 = AtomicU64::new(0);

/// The calling thread's cached shard for `region_id`, if any. `None` also
/// during TLS teardown.
fn cached_shard(region_id: u64) -> Option<u16> {
    MY_SHARDS
        .try_with(|t| {
            t.borrow()
                .iter()
                .find(|(id, _)| *id == region_id)
                .map(|&(_, shard)| shard)
        })
        .ok()
        .flatten()
}

/// Records `shard` as the calling thread's binding for `region_id`, replacing
/// any prior entry for that region and FIFO-evicting when the table is full.
fn remember_shard(region_id: u64, shard: u16) {
    let _ = MY_SHARDS.try_with(|t| {
        let mut t = t.borrow_mut();
        if let Some(e) = t.iter_mut().find(|(id, _)| *id == region_id) {
            e.1 = shard;
            return;
        }
        if t.len() >= MAX_REMEMBERED_REGIONS {
            t.remove(0);
        }
        t.push((region_id, shard));
    });
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
/// See the module-level design above for the router, the cross-thread
/// removal (lock-free eviction CAS; blocking remote-free enqueue), and the
/// shard lifecycle.
#[deprecated(
    since = "0.1.0",
    note = "concurrent regions are legacy/research-tier; use the production allocator stack (`alloc-xthread`) for cross-thread allocation needs"
)]
pub struct ShardedRegion<T> {
    /// Unique instance id keying this region's per-thread shard binding.
    id: u64,
    inner: Arc<ShardedInner<T>>,
    /// Per-shard `occupied` tokens in the out-of-line [`TokenBlock`] this
    /// region STRONGLY owns; [`ErasedGuard`] claims hold `Weak` references,
    /// so the token storage dies with the region (R13-02) while a thread's
    /// `Drop` can still flip the token of every region alive at its exit.
    /// Type-erased (no `T`) for the single-registry reason (see module docs).
    tokens: Arc<TokenBlock>,
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
        // Every shard starts FREE (occupied == false) in an out-of-line
        // backing this region strongly owns; claimant threads only ever hold
        // `Weak` claims against it (R13-02).
        Self {
            id: NEXT_SHARDED_REGION_ID.fetch_add(1, Ordering::Relaxed),
            inner: Arc::new(ShardedInner {
                shards: shards.into_boxed_slice(),
                next_shard: AtomicUsize::new(0),
            }),
            tokens: Arc::new(TokenBlock::new(n)),
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
    /// Exact only when no concurrent mutation is in flight. Under concurrent
    /// insert/remove this is an approximate, non-linearizable observation:
    /// the per-shard sum is not a snapshot, so it can report a count that
    /// never held at any single instant. Do not use as a drain-complete or
    /// shutdown signal.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.shards.iter().map(EpochRegion::len).sum()
    }

    /// Whether the region holds no live values across any shard (I4).
    ///
    /// Exact only when no concurrent mutation is in flight. Under concurrent
    /// insert/remove this is an approximate, non-linearizable observation:
    /// the per-shard scan is not a snapshot, so it can report `true` even
    /// though at least one entry was live at every instant — e.g. a
    /// cross-shard move (insert into shard A, then remove from shard B) can
    /// make each shard appear empty in sequence. Do not use as a
    /// drain-complete or shutdown signal.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.shards.iter().all(EpochRegion::is_empty)
    }

    /// Returns the calling thread's shard id bound to THIS region, or `None`
    /// if it has not bound here. Fast path: a short TLS table scan (no atomic).
    fn my_shard(&self) -> Option<u16> {
        cached_shard(self.id)
    }

    /// Lazily claims a shard for the calling thread (on first use) and returns
    /// its id. Subsequent calls return the cached binding from TLS.
    ///
    /// **7b claim protocol:**
    /// 1. Scan the `occupied` tokens for a FREE shard; atomically claim it via
    ///    `compare_exchange(false → true)`. The first free shard wins.
    /// 2. If NO shard is free, fall back to modulo round-robin (graceful
    ///    degradation: share a shard, still correct); no exclusive claim is
    ///    recorded — nothing to release on thread exit.
    ///
    /// The binding (id + a type-erased [`ErasedGuard`] holding `Weak` claims
    /// to the token backing) is cached in TLS so the fast path is a plain
    /// integer read, and the guard's `Drop` releases an exclusively-claimed
    /// shard of every region still alive on thread exit.
    ///
    /// A late TLS destructor keeps a cached route or modulo-shares, without
    /// acquiring an unrecordable exclusive claim (R14-04).
    fn claim_or_get_shard(&self) -> u16 {
        let n = self.inner.shards.len();
        if let Some(id) = self.my_shard() {
            // The cache is keyed by region id, so `id` was bound against THIS
            // region; the range check is a cheap defensive belt only.
            if usize::from(id) < n {
                return id;
            }
        }
        // Probe before CAS: destroyed TLS cannot record/release a new token.
        let router_intact =
            MY_SHARDS.try_with(|_| ()).is_ok() && ERASED_GUARD.try_with(|_| ()).is_ok();
        let mut claimed_exclusively: Option<u16> = None;
        if router_intact {
            ERASED_GUARD.with(|slot| {
                let mut slot = slot.borrow_mut();
                let guard = slot.get_or_insert_with(|| ErasedGuard {
                    claims: Vec::new(),
                    last_death_epoch_seen: TOKEN_BACKING_DEATHS.load(Ordering::Acquire),
                });
                prune_dead_claims(guard);
                // Claim and append before leaving the guard borrow.
                // Held tokens cannot be won twice; Acquire pairs with exit Release.
                for (i, occupied) in self.tokens.slots.iter().enumerate() {
                    if occupied
                        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                        .is_ok()
                    {
                        let won = u16::try_from(i).expect("shard index fits u16: n <= u16::MAX");
                        guard.claims.push(ErasedClaim {
                            tokens: Arc::downgrade(&self.tokens),
                            shard: won,
                        });
                        claimed_exclusively = Some(won);
                        break;
                    }
                }
            });
        }
        // 2. Graceful degradation: no free shard (or no intact router TLS) →
        //    modulo round-robin. The ticket is monotonic; modulo spreads
        //    across shards.
        let id = claimed_exclusively.unwrap_or_else(|| {
            let ticket = self.inner.next_shard.fetch_add(1, Ordering::Relaxed);
            u16::try_from(ticket % n)
                .expect("shard id fits u16: ticket%n where n<=u16::MAX cannot exceed u16::MAX")
        });
        if router_intact {
            // Cache the id (fast path).
            remember_shard(self.id, id);
        }
        id
    }

    /// Inserts `value` into the calling thread's claimed shard, returning a
    /// fresh [`ShardedHandle<T>`] that resolves to it (I1), or `Err(value)` if
    /// that shard is full (mirroring [`EpochRegion::insert`]).
    ///
    /// On the thread's first insert, lazily claims a shard via the TLS router
    /// (see the router design in this module). The returned handle carries
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
    ///
    /// R2-02 (independent src review round 2, task #2004): `T: Send +
    /// 'static` — delegates to `EpochRegion::insert`, whose doc comment has
    /// the full `defer_destroy` rationale.
    pub fn insert(&self, value: T) -> Result<ShardedHandle<T>, T>
    where
        T: Send + 'static,
    {
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
    ///
    /// R2-02 (independent src review round 2, task #2004): `T: Sync` —
    /// delegates to `EpochRegion::get_with`, whose doc comment has the full
    /// concurrent-shared-read rationale.
    pub fn get_with<R>(&self, handle: ShardedHandle<T>, f: impl FnOnce(&T) -> R) -> Option<R>
    where
        T: Sync,
    {
        let shard = self.inner.shards.get(usize::from(handle.shard))?;
        shard.get_with(handle.inner, f)
    }

    /// Convenience: resolves `handle` and returns a clone of the value, or
    /// `None` if stale/removed/out-of-range. Routes by `handle.shard`; lock-free
    /// like [`get_with`](Self::get_with).
    pub fn get_cloned(&self, handle: ShardedHandle<T>) -> Option<T>
    where
        T: Clone + Sync,
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
    /// itself is a CAS). Otherwise it takes the remote path
    /// (`EpochRegion::remote_evict`), which performs the lock-free
    /// generation-CAS eviction WITHOUT the owner shard's writer mutex, then
    /// enqueues the freed index (briefly taking the remote-free queue's
    /// `Mutex<Vec<u32>>` on the CALLING thread) for the owner to drain later.
    /// A thread that has not yet claimed a
    /// shard is treated as remote for every handle.
    ///
    /// "The calling thread's claimed shard" is the binding for THIS region
    /// (per-thread table keyed by region id); a binding in another region
    /// never selects the owner path here.
    ///
    /// If `handle.shard` is out of range, this returns `false` rather than
    /// panicking.
    ///
    /// # Panics
    ///
    /// Panics if the owning shard's writer mutex is poisoned (owner path only;
    /// the remote path takes no writer mutex).
    ///
    /// R2-02 (independent src review round 2, task #2004): `T: Send +
    /// 'static` — delegates to `EpochRegion::remove`/`remote_evict`, whose
    /// doc comments have the full `defer_destroy` rationale.
    pub fn remove(&self, handle: ShardedHandle<T>) -> bool
    where
        T: Send + 'static,
    {
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
    /// After this returns `true`, subsequent [`insert`](Self::insert) calls
    /// use the cached shard binding. [`get_with`](Self::get_with) and
    /// [`remove`](Self::remove) still address `handle.shard`; the binding only
    /// selects owner versus remote removal. The token claim is best-effort,
    /// not exclusive ownership or an OS affinity guarantee. Insert and owner
    /// removal take the writer mutex; remote removal can take the queue mutex.
    /// Binding therefore promises neither nonblocking nor async-safe execution.
    ///
    /// # Returns
    ///
    /// - `true` if `shard < shard_count()` and router TLS is intact.
    ///   This records routing only; any OS affinity pin remains best-effort.
    /// - `false` during router TLS teardown; no token or binding is acquired.
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
    /// thread exit via the `ErasedGuard`'s `Drop`). In the intended
    /// thread-per-core topology each thread binds exactly once at startup, so
    /// this does not arise.
    #[must_use]
    pub fn bind_current_thread_to_shard(&self, shard: u16) -> bool {
        if usize::from(shard) >= self.inner.shards.len() {
            return false;
        }
        // Refuse before CAS if the binding or exit guard cannot be retained.
        if MY_SHARDS.try_with(|_| ()).is_err() || ERASED_GUARD.try_with(|_| ()).is_err() {
            return false;
        }
        // Record the routing binding (fast-path TLS cache).
        remember_shard(self.id, shard);
        // Append won claims; only exit releases live tokens. Never cap-evict.
        // A failed best-effort CAS still records the requested routing binding.
        ERASED_GUARD.with(|slot| {
            let mut slot = slot.borrow_mut();
            let guard = slot.get_or_insert_with(|| ErasedGuard {
                claims: Vec::new(),
                last_death_epoch_seen: TOKEN_BACKING_DEATHS.load(Ordering::Acquire),
            });
            prune_dead_claims(guard);
            let claimed_exclusively = self.tokens.slots[usize::from(shard)]
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok();
            if claimed_exclusively {
                guard.claims.push(ErasedClaim {
                    tokens: Arc::downgrade(&self.tokens),
                    shard,
                });
            }
        });
        true
    }

    /// Clears the calling thread's TLS shard bindings (for every region), so
    /// its *next* [`insert`](Self::insert) in each region claims a fresh shard.
    ///
    /// **Diagnostics/testing only.** This does NOT release the previously
    /// claimed shard's `occupied` token (that happens on thread exit via the
    /// [`ErasedGuard`]'s `Drop`); it only clears the TLS id cache so the router
    /// re-runs the claim scan on the next insert. Production code should not
    /// call this.
    #[doc(hidden)]
    pub fn _reset_my_shard_binding_for_tests() {
        let _ = MY_SHARDS.try_with(|t| t.borrow_mut().clear());
    }

    /// **Diagnostics/testing only** (R2-21): forwards the shard's
    /// `EpochRegion::_remote_free_queue_buffer_identity_for_tests` —
    /// `None` if `shard` is out of range. Same identity-only contract as the
    /// forwarded hook (the pointer must never be dereferenced).
    #[doc(hidden)]
    pub fn _remote_free_queue_buffer_identity_for_tests(
        &self,
        shard: u16,
    ) -> Option<(usize, usize, usize)> {
        let shard = self.inner.shards.get(usize::from(shard))?;
        Some(shard._remote_free_queue_buffer_identity_for_tests())
    }

    /// Number of undestroyed token backing objects; not an RSS measurement.
    /// Temporary thread-exit upgrades are included until their final drop.
    ///
    /// Test-only lifetime oracle: construction increments and `TokenBlock::drop`
    /// decrements this counter. Together with the out-of-line Box representation,
    /// it detects TLS strong ownership retaining a destroyed region's tokens.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[must_use]
    pub fn _live_token_blocks_for_tests() -> usize {
        LIVE_TOKEN_BLOCKS.load(Ordering::Acquire)
    }

    /// Diagnostics only (R13-02): how many exclusive token claims THIS
    /// thread's TLS guard currently records — live claims plus not-yet-
    /// pruned dead ones. Pure length read; mutates nothing; 0 before the
    /// first claim and during TLS teardown.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[must_use]
    pub fn _tls_claim_count_for_tests() -> usize {
        ERASED_GUARD
            .try_with(|slot| slot.borrow().as_ref().map_or(0, |guard| guard.claims.len()))
            .unwrap_or(0)
    }

    /// Diagnostics only (R14-02): how many per-claim `strong_count` checks
    /// the process-global dead-claim sweeps have performed so far. Pure
    /// counter; 0 until a token backing has died. Gated to `internals` like
    /// the other test observers; used by the R14-02 work-bound oracle.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[must_use]
    pub fn _prune_claim_checks_for_tests() -> usize {
        PRUNE_CLAIM_CHECKS.load(Ordering::Relaxed)
    }

    /// Diagnostics only: force a stale advisory hint for a valid shard.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn _set_remote_free_hint_for_tests(&self, shard: u16, pending: bool) -> bool {
        let Some(shard) = self.inner.shards.get(usize::from(shard)) else {
            return false;
        };
        shard._set_remote_free_hint_for_tests(pending);
        true
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
