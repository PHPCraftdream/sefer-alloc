//! Phase 12.3 -- the alloc face: [`SeferAlloc`], an `unsafe impl GlobalAlloc`
//! over the global heap registry (Phase 12.2) via raw-pointer TLS (Phase 12.3).
//!
//! This is the **drop-in face** -- the campaign's victory deliverable. One
//! substrate (the segment-backed, self-hosted, registry-resident heap
//! allocator), two faces: the `Handle` face (typed, generational,
//! relocatable) and this `alloc` face (raw `*mut u8`, drop-in
//! `#[global_allocator]` replacement).
//!
//! ## Phase 12.3 rewiring
//!
//! Previously (Phase 11) this face routed through
//! `RefCell<Option<Heap>>` via `with_heap_try`. That binding ABORTED under
//! libtest's reentrant harness: `RefCell::try_borrow_mut` returns `Err` on
//! a reentrant borrow → the alloc face returned null → std aborted.
//!
//! Phase 12.3 replaces that with [`tls_heap::current`](super::tls_heap::current):
//! a raw `Cell<*mut HeapCore>` TLS cache (no borrow state to fail) over the
//! global [`HeapRegistry`](crate::registry::HeapRegistry). The heap lives in
//! a registry slot (not in TLS); thread exit abandons + recycles the slot
//! (not drops the heap). The alloc face is therefore **reentrancy-safe**
//! (M5) and **never-null** (M10): [`current`] returns a non-null pointer in
//! every case (cached slot, fresh claim, or the process-global fallback
//! heap).
//!
//! ## M5 (reentrancy-freedom) -- how it is upheld
//!
//! The whole point (§4 M5, §8 of `ALLOC_PLAN.md`): when WE are the global
//! allocator, ANY use of `Vec`/`Box`/`HashSet`/`std::alloc`/`format!` on the
//! alloc path would recurse infinitely. This module contains NONE of those.
//! `current()` is a plain thread-local load + null check. `bind_slow` claims
//! a registry slot (which bootstraps via the OS aperture, never `std::alloc`);
//! the only `std::alloc` touch is the `Box<AtomicPtr<u8>>` TFS handle under
//! `alloc-xthread`, installed on the bind path (outside the registry
//! bootstrap). The `HeapCore` alloc/dealloc paths are pure safe integer
//! arithmetic + the `node` seam (intrusive pointer r/w). No `std` collection
//! is reachable from here.
//!
//! ## No-panic -- how it is upheld
//!
//! A panic in `#[global_allocator]` aborts the process (§8 of `ALLOC_PLAN.md`).
//! Every entry point here returns null on failure and NEVER panics:
//! - `alloc`: `current()` → `&mut HeapCore` → `HeapCore::alloc` (returns
//!   null on OOM). If `current()` itself yields the fallback (TLS teardown),
//!   the fallback's `with_heap` returns `None` only on true OOM → null.
//! - `dealloc`: `current()` → `HeapCore::dealloc`. If TLS is torn down, the
//!   fallback's `with_heap` deallocs under the spinlock; a torn-down-TLS
//!   dealloc still routes correctly (the segment's owner routes via the
//!   header). On any failure this is a no-op (the block is leaked safely).
//! - `realloc`: `alloc` + copy + `dealloc`, all null-returning.
//! - `alloc_zeroed`: `alloc` + zero-fill.
//!
//! [`current`]: super::tls_heap::current

// The crate is `#![deny(unsafe_code)]` with `alloc-global` on (see `src/lib.rs`);
// this is the documented alloc-face seam. `allow` lifts the crate-level
// deny for this file only -- `unsafe` anywhere else in the crate is a hard
// error. The ONLY `unsafe` here is the `unsafe impl GlobalAlloc` (the trait
// is `unsafe`) plus the `// SAFETY:`-annotated pointer handoff to HeapCore.
#![allow(unsafe_code)]

use core::alloc::{GlobalAlloc, Layout};

use super::fallback;
#[cfg(not(feature = "alloc-decommit"))]
use super::tls_heap::current_for_alloc;
#[cfg(feature = "alloc-decommit")]
use super::tls_heap::current_for_alloc_with_config;
use super::tls_heap::CurrentHeap;
use super::AllocStats;

/// The drop-in `GlobalAlloc` face over the `sefer-alloc` segment substrate,
/// routed through the global heap registry (Phase 12.2) via raw-pointer TLS
/// (Phase 12.3).
///
/// Install it as your process's global allocator (simple form — uses all
/// large-cache defaults):
///
/// ```no_run
/// # #[cfg(feature = "alloc-global")]
/// # {
/// use sefer_alloc::SeferAlloc;
///
/// #[global_allocator]
/// static A: SeferAlloc = SeferAlloc::new();
/// # }
/// ```
///
/// Or configure the large-cache knobs at compile time (requires the
/// `alloc-decommit` feature):
///
/// ```no_run
/// # #[cfg(all(feature = "alloc-global", feature = "alloc-decommit"))]
/// # {
/// use sefer_alloc::{SeferAlloc, LargeCacheConfig, LargeCacheMode};
///
/// const CONFIG: LargeCacheConfig = LargeCacheConfig::new()
///     .budget_bytes(512 * 1024 * 1024)
///     .headroom_bytes(64 * 1024 * 1024)
///     .decay_interval_ms(200)
///     .decay_rate_percent(25)
///     .mode(LargeCacheMode::Lazy);
///
/// #[global_allocator]
/// static GLOBAL: SeferAlloc = SeferAlloc::with_config(CONFIG);
/// # }
/// ```
///
/// Each thread gets its own heap slot in the global registry (lazily claimed
/// on first allocation via the raw-pointer TLS binding -- no `RefCell`, no
/// reentrant-borrow failure). `alloc`/`dealloc`/`realloc`/`alloc_zeroed`
/// route through the per-thread heap's segment-centric `BinTable` free lists
/// (the Phase 12.1 hot path). With `alloc-xthread`, cross-thread `dealloc`
/// routes through the Phase 10 Treiber stack, now stamped from the
/// registry-resident heap (12.3 owner stamping).
///
/// Thread exit abandons the heap's segments back to the registry (a no-op
/// stub in 12.3 -- segments leak until 12.4 adoption; bounded and sound) and
/// recycles the slot for reuse. A primordial fallback heap (§2.3) serves
/// the pre-TLS / post-teardown windows, so the face is **never-null for a
/// serviceable request** (M10).
///
/// This is the **alloc face** of one substrate; the **handle face**
/// (`Region<T>` / `Handle<T>`) is the typed, generational view over the same
/// governed memory. See `docs/ALLOC_PLAN.md` §3 "The two faces".
///
/// # Multi-thread safety — read this before enabling `alloc-global` alone
///
/// **`alloc-global` without `alloc-xthread` is a footgun in any
/// multi-threaded program.** Without `alloc-xthread` there is no
/// ownership-checked routing path for a cross-thread free (no owner stamp,
/// no per-segment `RemoteFreeRing`): a block allocated on thread A and freed
/// on thread B has nowhere sound to go. In this configuration a
/// cross-thread `dealloc` degrades to a **leak of that block**, not a data
/// race — the fallback/registry paths never write into a foreign thread's
/// private free lists — but any workload that regularly frees on a
/// different thread than it allocated (thread pools, work-stealing queues,
/// producer/consumer channels) will leak monotonically under
/// `alloc-global` alone. The companion `fastbin` feature *requires*
/// `alloc-xthread` for exactly this reason (enforced by a `compile_error!`
/// in `lib.rs` — see `Cargo.toml`'s `fastbin = ["alloc-global",
/// "alloc-xthread"]`).
///
/// For any real multi-threaded deployment, build with at least
/// `["alloc-global", "alloc-xthread"]`, or use the `production` feature
/// bundle (`alloc-global + alloc-xthread + alloc-decommit + fastbin`), which
/// is the combination this crate is tested and tuned for. See
/// `docs/INTEGRATION.md` for the full feature matrix.
///
/// # `std`-only
///
/// `SeferAlloc` (and the whole `alloc-global` / `alloc-core` stack) requires
/// `std`: it uses thread-local storage for the per-thread heap binding and
/// `std::time::Instant` in the large-cache decay clock (`alloc-decommit`).
/// It cannot be used in a `no_std` build. The `Region<T>` / `Handle<T>` core
/// (this crate's other face) is `no_std` + `alloc`-only and unaffected by
/// this restriction — see the crate-level docs.
pub struct SeferAlloc {
    /// Large-cache configuration stored at static-init time. Plumbed into
    /// each per-thread `AllocCore` on the first TLS bind for that thread.
    ///
    /// Only present under `alloc-decommit`; without that feature the struct
    /// remains a ZST.
    #[cfg(feature = "alloc-decommit")]
    config: crate::alloc_core::LargeCacheConfig,
}

impl SeferAlloc {
    /// Construct the allocator with default large-cache settings. This is a
    /// zero-cost `const` constructor — the per-thread heaps are lazily
    /// claimed on first use (not here), so this can be used in `static`
    /// initialisers without any allocation or OS calls.
    ///
    /// Equivalent to `SeferAlloc::with_config(LargeCacheConfig::DEFAULT)`.
    #[must_use]
    pub const fn new() -> Self {
        #[cfg(feature = "alloc-decommit")]
        {
            Self {
                config: crate::alloc_core::LargeCacheConfig::DEFAULT,
            }
        }
        #[cfg(not(feature = "alloc-decommit"))]
        {
            Self {}
        }
    }

    /// Construct the allocator with a user-supplied large-cache configuration.
    ///
    /// This is a `const fn` so it can be used in a `static` initialiser:
    ///
    /// ```no_run
    /// # #[cfg(all(feature = "alloc-global", feature = "alloc-decommit"))]
    /// # {
    /// use sefer_alloc::{SeferAlloc, LargeCacheConfig, LargeCacheMode};
    ///
    /// const CONFIG: LargeCacheConfig = LargeCacheConfig::new()
    ///     .budget_bytes(512 * 1024 * 1024)
    ///     .headroom_bytes(64 * 1024 * 1024)
    ///     .decay_interval_ms(200)
    ///     .decay_rate_percent(25)
    ///     .mode(LargeCacheMode::Lazy);
    ///
    /// #[global_allocator]
    /// static GLOBAL: SeferAlloc = SeferAlloc::with_config(CONFIG);
    /// # }
    /// ```
    ///
    /// The config is stored in the `SeferAlloc` struct and plumbed into
    /// each per-thread `AllocCore` when its TLS slot is first claimed.
    /// Threads created before the static is initialised will receive the
    /// default config — in practice this cannot happen because the `static`
    /// initialiser runs before any thread is started.
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    pub const fn with_config(config: crate::alloc_core::LargeCacheConfig) -> Self {
        Self { config }
    }
}

impl Default for SeferAlloc {
    fn default() -> Self {
        Self::new()
    }
}

impl SeferAlloc {
    /// Resolve the current heap for an allocation, threading the config into
    /// the TLS bind slow path when `alloc-decommit` is active.
    ///
    /// Under `not(alloc-decommit)` this delegates to the config-free
    /// [`current_for_alloc`]; under `alloc-decommit` it passes `&self.config`
    /// (a single pointer load, not a ~40-byte value copy) into
    /// [`current_for_alloc_with_config`]. The reference avoids
    /// materialising the config on the hot fast path — the dereference
    /// happens only on the cold `bind_slow` branch.
    #[inline(always)]
    fn current_heap(&self) -> CurrentHeap {
        #[cfg(feature = "alloc-decommit")]
        {
            current_for_alloc_with_config(&self.config)
        }
        #[cfg(not(feature = "alloc-decommit"))]
        {
            current_for_alloc()
        }
    }

    /// A cheap, process-wide diagnostic snapshot of this allocator's internal
    /// counters — cache-hit rates, cross-thread reclaim/overflow counts, and
    /// segment/heap totals. See [`AllocStats`] for what each field means and
    /// which feature flags it depends on.
    ///
    /// `stats()` is a handful of relaxed atomic loads: no locks, no
    /// allocation, no segment or heap walk. Safe to call on a metrics-scrape
    /// hot path.
    ///
    /// The counters are **process-wide**, not per-`SeferAlloc`-instance: if a
    /// process installs more than one `SeferAlloc` (unusual, but not
    /// forbidden), every instance's `stats()` reads the same process-global
    /// totals.
    ///
    /// ```no_run
    /// # #[cfg(feature = "alloc-global")]
    /// # {
    /// use sefer_alloc::SeferAlloc;
    ///
    /// #[global_allocator]
    /// static A: SeferAlloc = SeferAlloc::new();
    ///
    /// fn report() {
    ///     let stats = A.stats();
    ///     println!(
    ///         "segments live ~= {}, tcache hits = {}, ring overflows = {}",
    ///         stats.segments_reserved_total.saturating_sub(stats.segments_released_total),
    ///         stats.tcache_hits,
    ///         stats.ring_overflows,
    ///     );
    /// }
    /// # }
    /// ```
    #[must_use]
    pub fn stats(&self) -> AllocStats {
        AllocStats {
            #[cfg(feature = "alloc-decommit")]
            large_cache_hits: crate::alloc_core::AllocCore::dbg_large_cache_hits(),
            #[cfg(not(feature = "alloc-decommit"))]
            large_cache_hits: 0,

            #[cfg(feature = "alloc-decommit")]
            decommit_calls: crate::alloc_core::AllocCore::dbg_decommit_count(),
            #[cfg(not(feature = "alloc-decommit"))]
            decommit_calls: 0,

            #[cfg(feature = "alloc-xthread")]
            large_xthread_reclaimed: crate::registry::DBG_LARGE_XTHREAD_RECLAIMED
                .load(core::sync::atomic::Ordering::Relaxed),
            #[cfg(not(feature = "alloc-xthread"))]
            large_xthread_reclaimed: 0,

            #[cfg(feature = "fastbin")]
            tcache_hits: crate::registry::DBG_TCACHE_HITS
                .load(core::sync::atomic::Ordering::Relaxed),
            #[cfg(not(feature = "fastbin"))]
            tcache_hits: 0,

            #[cfg(feature = "alloc-xthread")]
            ring_overflows: crate::alloc_core::remote_free_ring::DBG_RING_OVERFLOW
                .load(core::sync::atomic::Ordering::Relaxed),
            #[cfg(not(feature = "alloc-xthread"))]
            ring_overflows: 0,

            segments_reserved_total: crate::alloc_core::AllocCore::dbg_segments_reserved_total(),
            segments_released_total: crate::alloc_core::AllocCore::dbg_segments_released_total(),

            heaps_claimed_high_water: crate::registry::heaps_claimed_high_water(),
        }
    }
}

// SAFETY (the trait obligation): `GlobalAlloc` requires that `alloc`/
// `alloc_zeroed`/`realloc` return valid memory for the requested `Layout`
// (or null on failure), and that `dealloc` receives a pointer previously
// returned by an allocating method. We delegate to `HeapCore::alloc`/
// `dealloc`/`realloc`/`alloc_zeroed`, which uphold M1 (validity), M3 (no
// overlap), and M4 (alignment/size fidelity) -- verified by the Phase 8/9
// differential proptests and miri. `HeapCore` returns null on OOM (never
// panics -- the substrate panic sites were hardened in Phase 11). If the
// TLS heap is unavailable (thread teardown), `current()` returns the
// process-global fallback heap (never null); `dealloc` on the fallback is
// sound under the fallback's spinlock. M10 (never-null for serviceable
// requests) is upheld: the only null return is true OOM.
unsafe impl GlobalAlloc for SeferAlloc {
    #[inline(always)]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match self.current_heap() {
            // Fallback path (TLS torn down, registry exhausted, or true
            // fallback OOM): route through the fallback's spinlock-guarded
            // `with_heap`. `with_heap` returns `None` only on true OOM → we
            // surface null.
            CurrentHeap::Fallback => {
                fallback::with_heap(|h| h.alloc(layout)).unwrap_or(core::ptr::null_mut())
            }
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in
            // a registry slot. `current_heap` returned it for THIS thread;
            // the single-writer invariant (the CAS-won slot owner) makes
            // `&mut` access exclusive. `HeapCore::alloc` upholds the
            // GlobalAlloc contract (returns valid memory or null).
            CurrentHeap::Own(heap) => unsafe { (*heap).alloc(layout) },
        }
    }

    #[inline(always)]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        match self.current_heap() {
            CurrentHeap::Fallback => {
                // Fallback path: dealloc under the spinlock. A failure here
                // (true OOM at fallback init) is a safe no-op — the block
                // is leaked, never corrupted.
                let _ = fallback::with_heap(|h| h.dealloc(ptr, layout));
            }
            // SAFETY: as above; `dealloc` is a safe no-op on a
            // foreign/dangling pointer (M2 defence-in-depth), so even if
            // `ptr` was allocated on a different thread's heap, this routes
            // correctly (own-thread → BinTable; cross-thread → TFS under
            // `alloc-xthread`).
            CurrentHeap::Own(heap) => unsafe { (*heap).dealloc(ptr, layout) },
        }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        match self.current_heap() {
            CurrentHeap::Fallback => {
                fallback::with_heap(|h| h.alloc_zeroed(layout)).unwrap_or(core::ptr::null_mut())
            }
            // SAFETY: as in `alloc`.
            CurrentHeap::Own(heap) => unsafe { (*heap).alloc_zeroed(layout) },
        }
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        if ptr.is_null() {
            return core::ptr::null_mut();
        }
        match self.current_heap() {
            CurrentHeap::Fallback => fallback::with_heap(|h| h.realloc(ptr, old_layout, new_size))
                .unwrap_or(core::ptr::null_mut()),
            // SAFETY: as in `alloc`; `realloc` is alloc-new + copy +
            // dealloc-old, leaving the old allocation intact on OOM.
            CurrentHeap::Own(heap) => unsafe { (*heap).realloc(ptr, old_layout, new_size) },
        }
    }
}
