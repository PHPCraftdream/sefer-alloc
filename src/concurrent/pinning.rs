#![allow(deprecated)]
//! [`PinnedRunner`] — a thread-per-core driver for [`ShardedRegion<T>`]
//! (Phase 7c, `pinning`), plus the OS-affinity helpers it wraps.
//!
//! The locality apex of the sharded tier. The idea: pin worker thread *i* to
//! core *i* and explicitly [`bind`](ShardedRegion::bind_current_thread_to_shard)
//! it to shard *i*, so each thread deterministically owns the shard whose data
//! lives in its own core's cache. The hot path then has **no lock** and **no
//! cross-shard contention** — writers in different shards never meet — which is
//! also why this composes with async runtimes without "lock across `.await`"
//! hazards (there is no lock to hold).
//!
//! [`core_affinity`](::core_affinity) is a safe wrapper over the OS affinity
//! syscalls (`sched_setaffinity` on Linux, `SetThreadAffinityMask` on Windows,
//! `thread_policy_set` on macOS). This module adds **zero `unsafe`** of its own.
//!
//! ## Integrating with current-thread-per-core async runtimes
//!
//! The pattern is identical for [`glommio`], [`monoio`], and
//! [`tokio-rt-multi-thread`] pinned runtimes, and for a hand-rolled
//! `thread-per-core` executor:
//!
//! 1. At runtime startup, pin the worker thread to its core via
//!    [`PinnedRunner::pin_current_thread_to_core`] (best-effort — some OSes /
//!    sandboxes refuse affinity; the runner runs correctly either way).
//! 2. Bind that worker to the matching shard once via
//!    [`ShardedRegion::bind_current_thread_to_shard`] (deterministic routing —
//!    the auto round-robin claim cannot guarantee a specific id).
//! 3. Spawn tasks on that worker; every `insert`/`get_with`/`remove` from the
//!    worker routes to its bound shard with no lock on the hot path.
//!
//! ```text
//!   core 0 ── worker 0 ── shard 0   (cache-local: data + compute on core 0)
//!   core 1 ── worker 1 ── shard 1
//!   core 2 ── worker 2 ── shard 2
//!   ...
//! ```
//!
//! Cross-shard reads (`get_with`) and cross-thread removes (`remote_evict`)
//! still work — they route by `handle.shard` and are lock-free — they just are
//! not the hot path you are optimizing.
//!
//! ## Honest verdict (workload-dependent)
//!
//! Pinning is **not** a guaranteed win. It helps when:
//! - the per-shard working set is cache-hot (repeated reads/writes to the same
//!   slots benefit from L1/L2 residency), and
//! - shard access is truly thread-local (a worker mostly touches its own shard).
//!
//! It does little (and may add spawn cost) when:
//! - the workload is read-heavy with random cross-shard handles (every read is
//!   already lock-free; locality is already good), or
//! - the working set far exceeds per-core cache (memory-bandwidth-bound), or
//! - the OS scheduler already places threads well on an idle machine.
//!
//! Measure with `benches/pinned_write.rs` for your own workload before
//! committing to the pinned topology.
//!
//! [`glommio`]: https://docs.rs/glommio
//! [`monoio`]: https://docs.rs/monoio
//! [`tokio-rt-multi-thread`]: https://docs.rs/tokio
//! [`ShardedRegion<T>`]: crate::concurrent::ShardedRegion

use std::sync::Arc;
use std::thread::scope;

use core_affinity::CoreId;

use crate::concurrent::ShardedRegion;

/// A thread-per-core runner for [`ShardedRegion<T>`]: spawns one worker thread
/// per core, pins thread *i* to core *i*, binds it to shard *i*, and runs a
/// per-worker closure. Zero `unsafe` — [`core_affinity`] is a safe wrapper.
///
/// Construct with [`PinnedRunner::new`] (which probes the host's available
/// cores); the runner's worker count is `min(cores, region.shard_count())` so a
/// region with fewer shards than cores is not over-subscribed. Use
/// [`PinnedRunner::with_workers`] to cap the worker count explicitly.
///
/// ## Example
///
/// ```no_run
/// # #[cfg(feature = "pinning")] {
/// use sefer_alloc::{PinnedRunner, ShardedRegion};
/// use std::sync::Arc;
///
/// let region = Arc::new(ShardedRegion::<u64>::new());
/// let runner = PinnedRunner::new(&region).expect("no cores available");
/// runner.run(&region, |shard_id, region: &ShardedRegion<u64>| {
///     let h = region.insert(u64::from(shard_id)).unwrap();
///     assert_eq!(h.shard(), shard_id); // deterministic: shard == worker == core
/// });
/// # }
/// ```
///
/// ## Pinning is best-effort
///
/// On some OSes / sandboxes the affinity syscall is refused (containers
/// without `CAP_SYS_NICE`, some CI runners, etc.). The runner STILL runs
/// correctly in that case — it just cannot guarantee the thread stayed on the
/// chosen core. The shard BINDING (which is what makes routing deterministic)
/// is always honored, so tests assert routing, not affinity.
#[derive(Debug, Clone)]
pub struct PinnedRunner {
    /// The resolved core ids (one per worker), in enumeration order. Worker
    /// `i` is pinned to `cores[i]` and bound to shard `i`.
    cores: Vec<CoreId>,
}

impl PinnedRunner {
    /// Probes the host's available cores via [`core_affinity::get_core_ids`]
    /// and returns a runner whose worker count is
    /// `min(cores_available, region.shard_count())`.
    ///
    /// Returns `None` if [`core_affinity::get_core_ids`] returns `None` (the
    /// host refused to enumerate cores) — in that case a thread-per-core
    /// topology is not constructible; fall back to the unpinned
    /// [`ShardedRegion`] (still correct, just not cache-pinned).
    #[must_use]
    pub fn new<T>(region: &ShardedRegion<T>) -> Option<Self> {
        let cores = core_affinity::get_core_ids()?;
        let n = cores.len().min(region.shard_count());
        let cores = cores.into_iter().take(n.max(1)).collect();
        Some(Self { cores })
    }

    /// Like [`new`](Self::new) but caps the worker count at `max_workers`
    /// (useful when you want fewer workers than cores, e.g. to leave cores free
    /// for other work). `max_workers` is clamped to `>= 1` and to
    /// `region.shard_count()`.
    #[must_use]
    pub fn with_workers<T>(region: &ShardedRegion<T>, max_workers: usize) -> Option<Self> {
        let cores = core_affinity::get_core_ids()?;
        let n = cores
            .len()
            .min(region.shard_count())
            .min(max_workers.max(1));
        let cores = cores.into_iter().take(n.max(1)).collect();
        Some(Self { cores })
    }

    /// The number of workers this runner will spawn (also the number of cores
    /// it will attempt to pin, and the number of shards it will bind).
    #[must_use]
    pub fn worker_count(&self) -> usize {
        self.cores.len()
    }

    /// Pins the CALLING thread to `core_id` via [`core_affinity::set_for_current`].
    ///
    /// Returns whether pinning succeeded. Best-effort: on some OSes the
    /// affinity syscall is refused; the caller should still proceed (shard
    /// binding does not depend on the OS honoring affinity).
    #[must_use]
    pub fn pin_current_thread_to_core(core_id: CoreId) -> bool {
        core_affinity::set_for_current(core_id)
    }

    /// Enumerates the host's available cores via [`core_affinity::get_core_ids`].
    ///
    /// Returns `None` if the host refused to enumerate. This is the raw probe;
    /// [`new`](Self::new) / [`with_workers`](Self::with_workers) use it
    /// internally and cap to the region's shard count.
    #[must_use]
    pub fn available_cores() -> Option<Vec<CoreId>> {
        core_affinity::get_core_ids()
    }

    /// Spawns one worker thread per core, pins thread *i* to core *i* (via
    /// [`core_affinity::set_for_current`]), binds it to shard *i* (via
    /// [`ShardedRegion::bind_current_thread_to_shard`]), invokes `f(shard_id,
    /// region)` on it, and joins all workers.
    ///
    /// `f` receives the shard id (0-based, equal to the worker index and the
    /// core id's position in the enumeration) and a `&ShardedRegion<T>` (the
    /// same region, shared across workers). The closure runs once per worker.
    ///
    /// Pinning is best-effort: if the OS refuses the affinity, the worker still
    /// runs and the shard binding still routes deterministically.
    ///
    /// # Bounds
    ///
    /// - `T: Send + Sync` so the shared `&ShardedRegion<T>` is `Send` across the
    ///   worker threads. A `ShardedRegion<T>` is `Send + Sync` iff `T` is
    ///   (mirroring [`EpochRegion<T>`](crate::concurrent::EpochRegion), whose
    ///   `AtomicSlot<T>` is `Send + Sync` iff `T` is).
    /// - `F: Fn(...) + Sync` and `R: Send` so the (shared) closure and each
    ///   worker's return value can cross the thread boundary.
    ///
    /// # Panics
    ///
    /// Propagates a panic from any worker (the scope is joined; a panicking
    /// worker aborts the join, mirroring `std::thread::scope` semantics).
    pub fn run<T, F, R>(&self, region: &ShardedRegion<T>, f: F)
    where
        T: Send + Sync,
        F: Fn(u16, &ShardedRegion<T>) -> R + Sync,
        R: Send,
    {
        // The closure is `Sync` so every spawned thread can hold a shared `&F`.
        // `R: Send` so a worker's return value could be collected (we discard
        // it here; collectors that need returns should wrap their own channel).
        let f = &f;
        scope(|s| {
            for (i, &core) in self.cores.iter().enumerate() {
                // SAFETY: none — `core_affinity` is a safe wrapper. The shard
                // id fits `u16` because `ShardedRegion` caps `shard_count` at
                // `u16::MAX`, and we capped workers to `shard_count`.
                let shard_id = u16::try_from(i).expect(
                    "worker index fits u16: PinnedRunner caps workers to \
                     region.shard_count() which is itself <= u16::MAX",
                );
                s.spawn(move || {
                    // Best-effort pin: ignored if the OS refuses. Done FIRST so
                    // the bind + closure run on the intended core when honored.
                    let _ = Self::pin_current_thread_to_core(core);
                    // Deterministic routing bind: shard == worker == core.
                    // The result is ignored: `shard_id` is always in range here
                    // (worker index < shard_count by construction), so this
                    // always returns true; the `let _` acknowledges that.
                    let _ = region.bind_current_thread_to_shard(shard_id);
                    // Discard the return; collectors wrap their own sync.
                    let _ = f(shard_id, region);
                });
            }
        });
    }

    /// Convenience: [`run`](Self::run) over an `Arc<ShardedRegion<T>>` without
    /// forcing the caller to reborrow. Identical semantics; the `Arc` is simply
    /// deref'd for the duration of the run (the region itself is shared, not
    /// cloned per worker — the shard binding is per-thread TLS state, not
    /// per-`Arc`).
    pub fn run_arc<T, F, R>(&self, region: &Arc<ShardedRegion<T>>, f: F)
    where
        T: Send + Sync,
        F: Fn(u16, &ShardedRegion<T>) -> R + Sync,
        R: Send,
    {
        self.run(region, f);
    }
}
