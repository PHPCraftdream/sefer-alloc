//! [`SyncRegion`] — the safe concurrent default: a `Region` behind an `RwLock`.

use std::sync::{PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError};

use crate::{Handle, Region};

/// A thread-safe wrapper around [`Region<T>`] — the trusted concurrent baseline.
///
/// This is a coarse-grained `std::sync::RwLock<Region<T>>` with an ergonomic
/// guard-based API: multiple readers (`read`) or one writer (`write`) at a time.
/// It is the *always-shippable* concurrent answer: correct under any interleaving
/// because every mutation serialises through the lock. Note: "correct under any
/// interleaving" refers to **safety/serialization** — memory safety and API
/// contract invariants — not to bounded writer latency. `std::sync::RwLock` does
/// not guarantee portable fairness or starvation-freedom; on some platforms a new
/// reader may be delayed behind an awaiting writer depending on OS scheduling.
/// Finer-grained or lock-free alternatives are out of scope for this crate.
///
/// The wrapper stays `#![forbid(unsafe_code)]`: all interior mutability comes
/// from `std`'s `RwLock`. Use [`read`](Self::read) / [`write`](Self::write) for
/// multi-operation transactions (the borrows tie to the guard), or the
/// one-shot convenience methods ([`insert`](Self::insert),
/// [`remove`](Self::remove), …) which take `&self` and lock internally.
/// `reserve` and `capacity` have no one-shot form — reach them through a
/// held guard, e.g. `sr.write().reserve(n)` / `sr.read().capacity()`.
///
/// ## Poisoning policy
///
/// A panic while the **write** guard is held poisons the `RwLock` — `std` never
/// poisons on a read-guard panic (e.g. a panicking `T::clone` inside
/// [`get_cloned`](Self::get_cloned) releases the read lock cleanly, with no
/// poison). The **container structure** (Region/slotmap invariants) stays
/// intact regardless of the panic — this crate guarantees no memory
/// corruption. However, **interior side effects are the responsibility of `T`**:
/// if `T::clone` modifies internal state (e.g. `Cell`, atomics, internal `Mutex`)
/// before panicking, that modification persists. The `RwLock` itself does not
/// poison on read-guard panic, so the `SyncRegion` remains usable.
///
/// **Poison recovery guarantees container integrity only, not operation completion.**
/// The recovered `Region` has no memory corruption, but an interrupted operation
/// may have left partial effects visible: a panicking `T::Drop` during `clear()`
/// leaves the region partially cleared -- container-valid and reusable, but the
/// exact set of surviving values is an unspecified implementation detail of the
/// underlying `slotmap` version's unwind cleanup, not a stable contract (see
/// `Region::clear`'s own documentation), and a panicked multi-op `write()`
/// transaction leaves whatever partial effects it already applied. Callers whose
/// `T` carries cross-value invariants, or whose multi-op transactions need
/// all-or-nothing semantics, must implement their own signaling — this crate
/// provides none beyond what's documented here.
///
/// **Poison is cleared on recovery.** [`read`](Self::read) and
/// [`write`](Self::write) (and therefore every one-shot convenience method,
/// which locks internally) clear the lock's poison flag immediately after
/// recovering from it. This is a deliberate policy consistent with "poison
/// recovery guarantees container integrity only" above: since this crate
/// already always trusts the container after ANY recovery, permanently
/// forcing every subsequent access down `std::sync::RwLock`'s slower
/// poisoned-recovery path would cost real performance for no additional
/// safety — the container was already proven sound on the FIRST recovery.
/// One consequence: `SyncRegion` never exposes an `is_poisoned()` check,
/// because poisoned state is never observable for longer than the single
/// access that first recovers from it (except through [`Debug`], which
/// reports it without clearing). Applications that need a durable,
/// observable "a writer panicked here" signal for their own cross-value
/// invariants must implement it themselves (e.g. an `AtomicBool` alongside
/// the `SyncRegion`) — this crate's own poison flag is not a substitute,
/// by design.
///
/// ## Reentrancy
///
/// [`get_cloned`](Self::get_cloned) runs `T::clone`, and [`clear`](Self::clear) runs each
/// `T::Drop`, while the internal lock is held. If `T`'s `Clone` or `Drop` implementation
/// re-enters the same `SyncRegion` (directly or transitively), the thread deadlocks or
/// panics per `std::sync::RwLock`'s documented same-thread reacquisition behavior.
/// Even non-reentrant but slow `Clone`/`Drop` delays every other user: `clear` holds
/// the write lock across its entire linear sweep, while `get_cloned` holds the read lock
/// across the clone (readers are unaffected by the latter, but writers block). Never
/// call a one-shot convenience method (or a nested `read`/`write`) while the calling
/// thread already holds a read/write guard from the same `SyncRegion` — the one-shots
/// lock internally and the nested acquisition deadlocks (`std`'s `RwLock` is not reentrant;
/// even read-after-read can block behind a queued writer, since the platform's priority
/// policy is unspecified).
///
/// ## Contended reads
///
/// Under multi-threaded read contention, the one-shot convenience methods
/// ([`get_cloned`](Self::get_cloned), [`contains`](Self::contains),
/// [`len`](Self::len), [`is_empty`](Self::is_empty)) anti-scale: each call pays a
/// shared-cache-line lock acquisition that dominates the nanosecond-scale lookup.
/// Historical measurement (harness: `examples/contended_reads.rs`, regime: 8
/// readers, one-shot vs. batched reads on a noisy dev host) showed a ~4×
/// aggregate throughput loss at 8 readers and ~30× speedup from batching. A
/// later, more rigorous gate (`docs/perf/R828_STRUCTURAL_LEVERS_GATE.md` §2)
/// measured the same question with a different harness (`r828_batch_guard_probe.rs`)
/// and found **9.15×** — explicitly recorded as an open discrepancy, not silently
/// reconciled. The numbers above are retained for historical context; do not
/// treat them as a stable measurement of batching's benefit without consulting
/// the R828 gate's analysis.
///
/// ## Async runtimes
///
/// `SyncRegion` uses blocking `std::sync::RwLock`, which is not async-aware.
/// In an async context (e.g. `tokio`), this has concrete hazards:
///
/// - **Holding a guard across `.await` blocks the executor worker** for the
///   entire await duration, not just the critical section. The worker cannot
///   schedule other tasks while blocked.
///
/// - **One-shot methods (`get_cloned`, `insert`, `remove`, …) synchronously
///   block the OS thread** — they are not "async-safe just because they're
///   fast." Contention can still stall an executor worker.
///
/// - **`tokio::time::timeout` does NOT cancel a blocking lock acquisition.**
///   The timeout fires after the operation completes (or never fires if the
///   lock is held forever), but the blocking wait itself cannot be
///   interrupted.
///
/// - **`spawn_blocking` does NOT make an already-started operation
///   cancellation-safe.** Once a blocking call is in flight on a worker thread,
///   dropping the `JoinHandle` does not abort it.
///
/// For async-friendly ownership, use an async lock type such as
/// `tokio::sync::RwLock` or `async_rwlock` instead of this wrapper.
/// Guard-batching (see the contended reads section above) is valid only within
/// a synchronous section — it does not make blocking safe in async code.
pub struct SyncRegion<T> {
    inner: RwLock<Region<T>>,
}

impl<T> From<Region<T>> for SyncRegion<T> {
    /// Wraps an existing `Region<T>` in a `SyncRegion` for safe concurrent access.
    ///
    /// This provides a zero-copy conversion path from single-threaded to
    /// concurrent usage without invalidating existing handles — all `Handle<T>`
    /// values from the original `Region` remain valid and resolve correctly in
    /// the wrapped `SyncRegion`.
    fn from(region: Region<T>) -> Self {
        Self {
            inner: RwLock::new(region),
        }
    }
}

impl<T> SyncRegion<T> {
    /// Extracts the inner `Region<T>`, consuming this `SyncRegion`.
    ///
    /// This is the inverse of `From<Region<T>> for SyncRegion<T>`. It provides
    /// a zero-cost conversion from concurrent back to single-threaded usage,
    /// preserving all handles (they remain valid in the extracted `Region<T>`).
    ///
    /// If the `RwLock` is poisoned (due to a panic in a writer thread), this
    /// method recovers the `Region` anyway — the container structure is
    /// guaranteed intact, and `T`'s invariants are the caller's responsibility
    /// (see the [poisoning policy](Self#poisoning-policy) for full details).
    #[must_use]
    pub fn into_inner(self) -> Region<T> {
        self.inner
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl<T> SyncRegion<T> {
    /// Creates an empty region that allocates nothing until first use.
    ///
    /// # Panics
    ///
    /// Delegates to [`Region::new`] — see its `# Panics` section for the exact
    /// conditions (process-wide `region_id` counter exhaustion).
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(Region::new()),
        }
    }

    /// Creates an empty region with space pre-reserved for `capacity` entries.
    ///
    /// # Panics
    ///
    /// Delegates to [`Region::with_capacity`] — see its `# Panics` section for
    /// the exact conditions (capacity limit, allocation size overflow, and
    /// process-wide `region_id` counter exhaustion).
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: RwLock::new(Region::with_capacity(capacity)),
        }
    }

    /// Locks for shared read, returning a guard that hands out `&Region<T>`.
    ///
    /// Multiple readers may hold the guard concurrently. Recovers from poison
    /// and clears it (see the [poisoning policy](Self#poisoning-policy)'s
    /// "Poison is cleared on recovery" note) so a single past writer panic
    /// does not permanently slow every future access. Returns `std`'s
    /// own guard type directly — a deliberate, stable API commitment; migrating
    /// the internal lock implementation in the future would be a breaking change.
    pub fn read(&self) -> RwLockReadGuard<'_, Region<T>> {
        match self.inner.read() {
            Ok(g) => g,
            Err(p) => {
                let g = p.into_inner();
                self.inner.clear_poison();
                g
            }
        }
    }

    /// Locks for exclusive write, returning a guard that hands out `&mut Region<T>`.
    ///
    /// Blocks all other readers and writers until dropped. Recovers from poison
    /// and clears it (see the [poisoning policy](Self#poisoning-policy)'s
    /// "Poison is cleared on recovery" note) so a single past writer panic
    /// does not permanently slow every future access. Returns `std`'s
    /// own guard type directly — a deliberate, stable API commitment; migrating
    /// the internal lock implementation in the future would be a breaking change.
    pub fn write(&self) -> RwLockWriteGuard<'_, Region<T>> {
        match self.inner.write() {
            Ok(g) => g,
            Err(p) => {
                let g = p.into_inner();
                self.inner.clear_poison();
                g
            }
        }
    }

    /// Inserts `value`, returning a fresh handle that resolves to it (I1).
    ///
    /// One-shot convenience that locks for write internally. For a transaction
    /// that does several ops under one lock, use [`write`](Self::write) instead.
    ///
    /// # Panics
    ///
    /// Panics if the backing `slotmap` is full (2^32 - 2 live entries).
    #[must_use]
    pub fn insert(&self, value: T) -> Handle<T> {
        self.write().insert(value)
    }

    /// Removes and returns the value for `handle`, or `None` if stale/removed.
    ///
    /// One-shot convenience that locks for write internally. The write guard is
    /// released before the removed value is dropped by the caller, so a reentrant
    /// `Drop` on the removed value is safe against the deadlock class described
    /// in the [reentrancy section](Self#reentrancy).
    pub fn remove(&self, handle: Handle<T>) -> Option<T> {
        self.write().remove(handle)
    }

    /// Whether `handle` currently resolves to a live value.
    ///
    /// One-shot convenience that locks for read internally. Note that under
    /// concurrency a `true` result may be stale by the time the caller acts on it;
    /// acting on a stale handle can only ever produce `None` at the point of use,
    /// never resolve to a wrong live value within roughly `2^31` reuse cycles of
    /// that slot. Callers who need an atomic check-then-act use [`write`](Self::write);
    /// callers who only need an atomic check-then-**read** can use the cheaper
    /// [`read`](Self::read) instead — either way, one held guard instead of two
    /// separate lock acquisitions.
    #[must_use]
    pub fn contains(&self, handle: Handle<T>) -> bool {
        self.read().contains(handle)
    }

    /// Number of live values (I4).
    ///
    /// One-shot convenience that locks for read internally. Note that under
    /// concurrency the count is a momentary snapshot, not a stable property.
    #[must_use]
    pub fn len(&self) -> usize {
        self.read().len()
    }

    /// Whether the region holds no live values (I4).
    ///
    /// One-shot convenience that locks for read internally. Note that under
    /// concurrency this is a momentary snapshot, not a stable property.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.read().is_empty()
    }

    /// Removes every value, invalidating all outstanding handles.
    ///
    /// One-shot convenience that locks for write internally.
    /// The partial-clear-under-panic contract is [`Region::clear`]'s — see there.
    ///
    /// # Reentrancy hazard
    ///
    /// If `T::Drop` attempts to acquire a read or write lock on the same `SyncRegion`,
    /// a deadlock will occur (the lock is already held for write). Prefer extracting
    /// values to a temporary container and dropping them after the lock is released.
    pub fn clear(&self) {
        self.write().clear();
    }

    /// Clones the value for `handle` out without leaving the caller holding a guard,
    /// or `None` if stale/removed. One-shot convenience that locks for read internally.
    ///
    /// Prefer this over [`read`](Self::read) when you only need a by-value copy
    /// and don't want to hold the guard across other work. Note that the `T::clone`
    /// call itself runs under the read lock (this is unavoidable due to borrowing
    /// semantics), so for expensive-Clone payloads every call extends the lock hold
    /// by the full clone duration and delays any writer arriving during that window
    /// by up to that much (measured ~1.5–1.8 ms worst-case writer stall for a 4 MiB
    /// payload). For such payloads, store `Arc<T>` instead so the "clone" is a cheap
    /// refcount bump.
    ///
    /// See the [reentrancy section](Self#reentrancy) for the deadlock hazard
    /// when `T::clone` re-enters the same `SyncRegion`.
    pub fn get_cloned(&self, handle: Handle<T>) -> Option<T>
    where
        T: Clone,
    {
        self.read().get(handle).cloned()
    }
}

impl<T> From<SyncRegion<T>> for Region<T> {
    fn from(sr: SyncRegion<T>) -> Self {
        sr.into_inner()
    }
}

impl<T> Default for SyncRegion<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> std::fmt::Debug for SyncRegion<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Pattern borrowed from std::sync::RwLock's own Debug impl: try_read()
        // first, then discriminate *why* it failed. `try_read()` returns
        // `Err` for two structurally different reasons that must not be
        // collapsed into the same placeholder:
        //   - `WouldBlock`: another thread genuinely holds the lock right
        //     now. There is no data to show — "<locked>" is the only honest
        //     answer.
        //   - `Poisoned(_)`: a writer panicked while holding the write guard,
        //     but the lock itself is free right now (poisoning does not hold
        //     the lock). The container is intact and usable (see the
        //     poisoning-policy doc above), so this branch recovers the data
        //     via `into_inner()` and reports `poisoned: true` — matching how
        //     `std::sync::RwLock`'s own `Debug` renders a poisoned lock
        //     (`RwLock { data: .., poisoned: true, .. }`), verified by direct
        //     comparison against `std`'s actual output.
        match self.inner.try_read() {
            Ok(guard) => f.debug_struct("SyncRegion").field("inner", &guard).finish(),
            Err(TryLockError::Poisoned(e)) => f
                .debug_struct("SyncRegion")
                .field("inner", &*e.into_inner())
                .field("poisoned", &true)
                .finish(),
            Err(TryLockError::WouldBlock) => f
                .debug_struct("SyncRegion")
                .field("inner", &"<locked>")
                .finish(),
        }
    }
}
