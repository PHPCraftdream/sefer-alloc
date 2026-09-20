//! Object lifecycle of [`AllocCore`] (mechanical split of the former flat
//! `alloc_core.rs`, here merged from the former `ctor.rs` + `drop.rs` pair).
//!
//! Construction: the `impl AllocCore { .. }` block for `new`,
//! `new_with_config`, `live_config_matches`, and `new_inner`, the
//! `LargeCacheDecayConfig::from_config` constructor, and the
//! `bench-internals`-gated `DBG_RESERVATION_OWNER_ID_COUNTER` source.
//! Teardown: the `Drop` implementation that releases every OS reservation,
//! plus the `base_add` node-seam offset helper. Pure code movement; no
//! behavior changed.

use crate::alloc_core::alloc_core::AllocCore;
use crate::alloc_core::node::Node;
use crate::alloc_core::os;
use crate::alloc_core::segment_header::SegmentHeader;

use super::bootstrap;
#[cfg(feature = "alloc-decommit")]
use super::counters::LargeCacheHitCounter;
#[cfg(feature = "alloc-decommit")]
use super::{LargeCacheDecayConfig, LARGE_CACHE_SLOTS};
#[cfg(feature = "alloc-decommit")]
use crate::alloc_core::large_cache_mode::LargeCacheMode;
#[cfg(feature = "numa-aware")]
use crate::alloc_core::numa;
#[cfg(feature = "alloc-decommit")]
use crate::alloc_core::os::SEGMENT;
#[cfg(feature = "numa-aware")]
use crate::alloc_core::segment_header::SegmentMeta;
#[cfg(feature = "alloc-segment-directory")]
use crate::alloc_core::size_classes::SMALL_CLASS_COUNT;

/// R31-15 (task #486): process-wide monotonic source for
/// [`AllocCore::dbg_reservation_owner_id`]. `bench-internals`-gated, same
/// discipline as the field it populates — this static does not exist at all
/// in a `production` build.
#[cfg(feature = "bench-internals")]
static DBG_RESERVATION_OWNER_ID_COUNTER: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

#[cfg(feature = "alloc-decommit")]
impl LargeCacheDecayConfig {
    /// Build the decay config from a resolved [`LargeCacheConfig`].
    ///
    /// [`LargeCacheConfig`]: crate::alloc_core::large_cache_config::LargeCacheConfig
    fn from_config(cfg: &crate::alloc_core::large_cache_config::LargeCacheConfig) -> Self {
        Self {
            decay_rate_bp: cfg.resolved_decay_rate_bp(),
            decay_interval: cfg.resolved_decay_interval(),
            headroom_bytes: cfg.resolved_headroom_bytes(),
        }
    }
}

impl AllocCore {
    /// Bootstrap the allocator using default large-cache configuration.
    ///
    /// Equivalent to `AllocCore::new_with_config(LargeCacheConfig::DEFAULT)`.
    /// Returns `None` only if the OS refuses the primordial reservation (OOM at
    /// startup).
    ///
    /// `AllocCore` intentionally does **not** implement `Default` (R3-C). Both
    /// `new()` and `new_with_config()` return `Option<Self>` because the very
    /// first thing construction does is a real multi-MiB OS memory reservation
    /// for the primordial segment, which can fail under memory pressure /
    /// OOM / `rlimit`. A `Default` impl would have to hide that fallibility
    /// behind an `.expect(...)` panic, but generic code across the ecosystem
    /// treats `T::default()` / `T: Default` as a conventionally-cheap,
    /// infallible operation (e.g. `Option::<T>::unwrap_or_default()`,
    /// `#[derive(Default)]`, `mem::take`, `resize_with(Default::default)`) —
    /// none of those call sites expect a multi-MiB syscall plus a latent
    /// panic. Keeping the construction fallibility explicit (callers write
    /// `AllocCore::new().expect("...")` themselves) is the deliberate design,
    /// not an oversight; do not re-add a `Default` impl.
    #[must_use]
    #[inline]
    pub fn new() -> Option<Self> {
        #[cfg(feature = "alloc-decommit")]
        return Self::new_with_config(
            crate::alloc_core::large_cache_config::LargeCacheConfig::DEFAULT,
        );
        #[cfg(not(feature = "alloc-decommit"))]
        return Self::new_inner();
    }

    /// Bootstrap the allocator with a user-supplied large-cache configuration.
    ///
    /// All `LargeCacheConfig` fields use their documented defaults when `None`.
    /// Returns `None` only if the OS refuses the primordial reservation (OOM at
    /// startup).
    ///
    /// Use this when you want to set the cache knobs at compile time without
    /// environment variables (runnable form in `tests/large_cache_config_knobs.rs`):
    ///
    /// ```text
    /// use sefer_alloc::{AllocCore, LargeCacheConfig, LargeCacheMode};
    ///
    /// let cfg = LargeCacheConfig::new()
    ///     .budget_bytes(512 * 1024 * 1024)
    ///     .headroom_bytes(64 * 1024 * 1024)
    ///     .decay_interval_ms(200)
    ///     .decay_rate_percent(25)
    ///     .mode(LargeCacheMode::Lazy);
    ///
    /// let ac = AllocCore::new_with_config(cfg).expect("primordial");
    /// drop(ac);
    /// ```
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    #[inline]
    pub fn new_with_config(
        config: crate::alloc_core::large_cache_config::LargeCacheConfig,
    ) -> Option<Self> {
        let mut core = Self::new_inner()?;
        core.large_cache_budget_bytes = config.resolved_budget_bytes();
        core.decay_config = LargeCacheDecayConfig::from_config(&config);
        // R3-B (round3, решение №2): `LargeCacheMode` now carries only the
        // `Lazy` variant — the unimplemented `Background`/`Both` variants were
        // removed from the enum entirely ("make invalid states
        // unrepresentable"). The old T5 panic-match that rejected them is gone
        // with the variants: that match was itself reachable lazily through
        // `GlobalAlloc::alloc` (first-bind materialises the heap) and so
        // conflicted with the never-panics entry-point guarantee. With only
        // `Lazy` representable there is nothing left to validate or branch on
        // here — just store the resolved mode. See
        // `docs/reviews/2026-07-12-round3-remediation-plan.md`.
        core.large_cache_mode = config.resolved_mode();
        // Mechanism 2 (task #51); RAD-3 (E2, task #56): resolve the
        // empty-small-segment pool cap. The effective cap is the tighter of
        // the two bounds the user actually controls:
        //   - the configured segment count,
        //   - the byte ceiling expressed in whole segments (each is `SEGMENT`).
        // A `0` in EITHER config knob disables the pool (min → 0), matching the
        // `SmallSegmentPoolConfig` contract. NO third `.min(POOL_MAX_SLOTS)`
        // term — the old compile-time array bound is gone (the pool's storage
        // is now the intrusive `SegmentHeader::pool_next`/`pool_prev` list,
        // which has no fixed capacity), so a caller who asks for
        // `.pool_segments(64)` genuinely GETS a cap of 64 (subject only to the
        // byte budget), not a silent clamp to 4.
        let pool_cfg = config.resolved_pool();
        let by_segments = pool_cfg.resolved_pool_segments();
        let by_bytes = pool_cfg.resolved_pool_byte_cap() / SEGMENT;
        core.pool_cap = by_segments.min(by_bytes);
        Some(core)
    }

    /// Compare this heap's live (resolved) cache/pool policy against a
    /// requested [`LargeCacheConfig`]. Returns `true` when the resolved
    /// values match what [`new_with_config`](Self::new_with_config) would
    /// apply from `requested`.
    ///
    /// Used by `HeapRegistry::claim_with_config` (task #95 / N2) to detect
    /// that a recycled slot's pre-existing policy silently overrides a
    /// different config passed by a later claimant. Comparing **resolved**
    /// values (not the raw `Option` fields of `LargeCacheConfig`) means two
    /// configs that resolve identically — e.g. `budget_bytes(None)` vs
    /// `LargeCacheConfig::DEFAULT` — are correctly treated as a match, so
    /// this only flags genuine policy differences, not stylistic builder
    /// variation.
    ///
    /// **Drift hazard:** the comparisons here MUST stay in sync with the
    /// fields [`new_with_config`](Self::new_with_config) sets. If a future
    /// change adds a new config-derived field to `AllocCore`, add the
    /// matching comparison here.
    #[cfg(feature = "alloc-decommit")]
    pub(crate) fn live_config_matches(
        &self,
        requested: &crate::alloc_core::large_cache_config::LargeCacheConfig,
    ) -> bool {
        if self.large_cache_budget_bytes != requested.resolved_budget_bytes() {
            return false;
        }
        if self.decay_config != LargeCacheDecayConfig::from_config(requested) {
            return false;
        }
        if self.large_cache_mode != requested.resolved_mode() {
            return false;
        }
        let pool_cfg = requested.resolved_pool();
        let by_segments = pool_cfg.resolved_pool_segments();
        let by_bytes = pool_cfg.resolved_pool_byte_cap() / SEGMENT;
        self.pool_cap == by_segments.min(by_bytes)
    }

    /// Inner bootstrap: reserve the primordial segment and hand-carve its
    /// self-hosted metadata. All feature-gated fields are set to their
    /// defaults here; `new_with_config` then overwrites the decommit knobs.
    #[inline]
    fn new_inner() -> Option<Self> {
        let prim = bootstrap::primordial()?;
        let primordial_base = prim.segment.as_ptr();
        // The primordial segment hosts the registry AND serves as the first
        // small segment (its remaining payload is free for small allocs).
        let small_cur = primordial_base;
        // We take ownership of the registry; the primordial Segment handle is
        // forgotten — its memory is freed by walking the registry in `drop`
        // (the registry records the reservation pointers, so we do not need
        // the Rust `Segment` handle to free it).
        core::mem::forget(prim.segment);
        // Phase C (numa-aware): the primordial segment was reserved by
        // `bootstrap::primordial()` via the plain OS path (it predates NUMA
        // awareness). Stamp the current thread's NUMA node into its header NOW
        // so that `find_segment_with_free` can treat it as a local segment.
        // On platforms without NUMA `current_node()` returns `NO_NODE`; the
        // field already holds `NO_NODE_RAW` (same value), so this is a no-op
        // in terms of visible effect — but it makes the invariant explicit.
        #[cfg(feature = "numa-aware")]
        {
            let my_node = numa::current_node();
            SegmentMeta::new(primordial_base).set_node_id(my_node);
        }
        Some(Self {
            table: prim.table,
            small_cur,
            // R11-5: cache starts empty; first call to `current_node_cached`
            // populates it. `HeapRegistry::claim` resets it to `None` on
            // every (re-)claim of a registry slot so a recycled slot never
            // hands a stale node to a new owning thread. Standalone
            // `AllocCore`s (tests) never claim/recycle, so the first-query
            // value persists for their lifetime — correct, since they are
            // single-threaded by construction. See `PHASE_NUMA_DESIGN.md` §4.1.
            #[cfg(feature = "numa-aware")]
            cached_numa_node: None,
            // R12-5: no hits served yet; the first `current_node_cached` call
            // is a miss regardless (cache is `None`), which itself resets
            // this counter to 0 on populate.
            #[cfg(feature = "numa-aware")]
            numa_node_hits_since_refresh: 0,
            #[cfg(feature = "alloc-decommit")]
            large_cache: [const { None }; LARGE_CACHE_SLOTS],
            #[cfg(feature = "alloc-decommit")]
            large_cache_occupied: 0,
            #[cfg(feature = "large-cache-extended")]
            large_cache_extension: core::ptr::null_mut(),
            #[cfg(feature = "alloc-decommit")]
            large_cache_budget_bytes: None,
            #[cfg(feature = "alloc-decommit")]
            large_cache_used_bytes: 0,
            #[cfg(feature = "alloc-decommit")]
            large_cache_seq: 0,
            #[cfg(feature = "alloc-decommit")]
            decay_config: LargeCacheDecayConfig {
                decay_rate_bp: crate::alloc_core::large_cache_config::DEFAULT_DECAY_RATE_PERCENT * 100,
                decay_interval: core::time::Duration::from_millis(
                    crate::alloc_core::large_cache_config::DEFAULT_DECAY_INTERVAL_MS,
                ),
                headroom_bytes: crate::alloc_core::large_cache_config::DEFAULT_HEADROOM_BYTES,
            },
            #[cfg(feature = "alloc-decommit")]
            last_decay_tick: None,
            #[cfg(feature = "alloc-decommit")]
            large_cache_decay_op_count: 0,
            #[cfg(feature = "alloc-decommit")]
            large_cache_mode: LargeCacheMode::Lazy,
            #[cfg(feature = "alloc-decommit")]
            large_cache_hits: LargeCacheHitCounter::new(0),
            // W3: unbound by default; `HeapRegistry::claim` redirects this to
            // the owning slot's counter for a registry-bound heap. Standalone
            // `AllocCore`s (tests) stay `None` and count into the owned field
            // above.
            #[cfg(feature = "alloc-decommit")]
            large_cache_hits_sink: None,
            // Mechanism 2 (task #51); RAD-3 (E2, task #56): empty pool.
            // `pool_cap` defaults to the production default here (as if
            // `SmallSegmentPoolConfig::DEFAULT`); `new_with_config` overwrites
            // it from the resolved config. The `new_inner`-only path (a
            // `not(alloc-decommit)`-style direct bootstrap) never reaches
            // this arm — every `alloc-decommit` build funnels construction
            // through `new_with_config`. `pool_head`/`pool_tail` start
            // `null` (empty list) — no fixed-size array to zero-init.
            #[cfg(feature = "alloc-decommit")]
            pool_head: core::ptr::null_mut(),
            #[cfg(feature = "alloc-decommit")]
            pool_tail: core::ptr::null_mut(),
            #[cfg(feature = "alloc-decommit")]
            pooled_count: 0,
            #[cfg(feature = "alloc-decommit")]
            pool_cap:
                crate::alloc_core::small_segment_pool_config::SmallSegmentPoolConfig::DEFAULT_POOL_SEGMENTS.min(
                    crate::alloc_core::small_segment_pool_config::SmallSegmentPoolConfig::DEFAULT_POOL_BYTE_CAP
                        / SEGMENT,
                ),
            #[cfg(feature = "alloc-decommit")]
            last_pool_decay_tick: None,
            #[cfg(feature = "alloc-segment-directory")]
            directory_sidecar: core::ptr::null_mut(),
            #[cfg(feature = "alloc-segment-directory")]
            directory_miss_streak: [0; SMALL_CLASS_COUNT],
            #[cfg(all(feature = "alloc-xthread", feature = "alloc-segment-directory"))]
            dirty_segments: None,
            #[cfg(feature = "class-aware-dirty")]
            dirty_by_class: None,
            #[cfg(feature = "class-aware-dirty")]
            sidecar_oom_latch: None,
            // R31-15 (task #486): stamp a fresh, process-wide-unique identity
            // for this AllocCore. See the field's own doc comment for why
            // this must be a monotonic counter rather than `&self`'s address.
            #[cfg(feature = "bench-internals")]
            dbg_reservation_owner_id: DBG_RESERVATION_OWNER_ID_COUNTER
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed),
        })
    }
}

/// # ⚠️ Quiescence pin (UBFIX-12 / L-8, 0.3.0) — read before adding any
/// `Sync`/cross-thread capability to `AllocCore`, or before making registry
/// heaps droppable
///
/// This `drop` walks every segment in `self.table` and releases its OS
/// reservation (`os::release_segment`) unconditionally — it does NOT perform
/// any handshake to prove no OTHER thread is concurrently pushing onto one of
/// these segments' cross-thread remote-free rings
/// ([`RemoteFreeRing`](super::remote_free_ring::RemoteFreeRing), the
/// `alloc-xthread` per-segment MPSC the segment header's `owner_thread_free`
/// stamp routes into) before unmapping. If such a push raced this `drop`, it
/// would write into memory that is either about to be, or has already been,
/// unmapped — a use-after-free / wild write on the remote thread's side.
///
/// **Today this is reachable-but-moot, not a live bug**, for two independent
/// reasons, EITHER of which is already sufficient on its own:
///
/// 1. **Registry heaps never reach this `drop`.** The `HeapRegistry`/
///    `HeapCore` substrate that `SeferAlloc`/TLS actually use lives for the
///    entire process (`HeapCore::new`'s `AllocCore` is never dropped by
///    `recycle` — `recycle` only flips the slot's state and pushes it onto
///    `free_slots` for reuse; see `HeapRegistry::recycle`). So the ONLY way
///    to reach `AllocCore::drop` today is constructing a STANDALONE
///    `AllocCore` directly (`AllocCore::new`/`::default`, bypassing the
///    registry entirely) and letting it go out of scope.
/// 2. **A standalone `AllocCore` cannot be shared across threads in the
///    first place.** `AllocCore` carries raw pointers (`table`, `small_cur`,
///    `large_cache` entries) and has no `unsafe impl Sync for AllocCore`
///    anywhere in this crate (verified by grep at the time of writing) — so
///    it is `!Sync` by the ordinary auto-trait rules, and a `&AllocCore`
///    cannot be handed to another thread to begin with. Without a live
///    `&AllocCore` on some OTHER thread, nothing can call the remote-free
///    routing that would push onto a segment's `RemoteFreeRing` while this
///    thread's `drop` is unmapping it — the race this note warns about has
///    no way to be constructed against a standalone `AllocCore` today.
///
/// Both conditions must be independently defeated before this becomes live:
/// (a) some future change makes registry heaps droppable (e.g. a
/// decommit-when-empty or heap-teardown policy that actually frees a
/// `HeapCore`'s `AllocCore`, not just recycles the slot), OR (b) some future
/// change adds `unsafe impl Sync for AllocCore` (or otherwise exposes a
/// standalone `AllocCore` for cross-thread sharing outside the registry).
/// If EITHER lands, this `drop` needs a quiescence handshake — e.g. draining
/// every segment's `RemoteFreeRing` under a happens-before edge that rules
/// out a concurrent remote push, or otherwise proving no other thread holds
/// a reference capable of routing a free into a segment this `drop` is about
/// to unmap — before it is safe to release segments unconditionally as it
/// does now. This note is the load-bearing reminder to add that handshake at
/// that time; do not remove or weaken it while working on (a) or (b) above.
impl Drop for AllocCore {
    #[allow(unsafe_code)] // R14-1 (task #286): calls the `unsafe fn deref_large_cache_extension_mut`
                          // boundary when `large-cache-extended` is on, right after
                          // `self.large_cache_extension` is proven non-null. Sound: the pointer
                          // was produced only by `reserve_large_cache_extension`
                          // (typed-initialised), `AllocCore`'s owner-only discipline (neither
                          // `Send` nor `Sync`) rules out a concurrent reader/writer, and no other
                          // reference to the sidecar is live across this call (this IS the
                          // teardown path — no other method runs concurrently with `drop`).
    fn drop(&mut self) {
        // OPT-E (alloc-decommit): release any large segments held in the
        // free-cache BEFORE walking the segment table. The cached entries are
        // NOT in the table (they were unregistered on deposit), so the normal
        // `table.bases()` walk below won't see them. We must release them
        // explicitly here or they would leak.
        #[cfg(feature = "alloc-decommit")]
        for slot in &mut self.large_cache {
            if let Some(cached) = slot.take() {
                os::release_segment(cached.reservation, cached.reservation_len);
            }
        }
        // R13-7 (task #277): the lazily-materialised extension sidecar holds
        // large-cache entries the SAME way the base array does (unregistered
        // from `self.table`, so the `table.bases()` walk below never sees
        // them) — its reservations must be released here too, or they leak
        // on drop. The sidecar's OWN VM reservation (the `leak_zeroed_pages`
        // page(s) backing the `LargeCacheExtension` struct itself) is NOT
        // released — it is leaked for the process lifetime by design, the
        // same discipline `directory_sidecar`/`dirty_by_class` already use
        // for their own sidecars (a bounded, one-time-per-heap cost, not a
        // growing leak).
        #[cfg(feature = "large-cache-extended")]
        if !self.large_cache_extension.is_null() {
            // SAFETY: see the `#[allow(unsafe_code)]` justification on
            // `Drop::drop` above.
            let ext = unsafe {
                crate::alloc_core::large_cache_extended::deref_large_cache_extension_mut(
                    self.large_cache_extension,
                )
            };
            for slot in &mut ext.slots {
                if let Some(cached) = slot.take() {
                    os::release_segment(cached.reservation, cached.reservation_len);
                }
            }
        }

        // Collect every live segment's `(reservation, reservation_len)` into a
        // fixed-size stack array FIRST, then free them all. We must NOT free
        // the primordial segment while still reading the registry — the
        // registry lives IN the primordial's payload, so freeing it would
        // unmap the array we're iterating over. Collecting up front (into a
        // stack array, no global-allocator involvement) breaks that aliasing.
        //
        // `self.table.bases()` already filters NULL (recycled) slots — those
        // segments were released by `recycle()` during their decommit cycle and
        // must NOT be freed again. Only non-NULL (live) segments are collected
        // and freed here.
        //
        // The array is bounded by MAX_SEGMENTS (4096 × 16 B = 64 KiB stack —
        // fine; a deeply-nested drop chain would be the only concern, and
        // AllocCore is a top-level owner).
        let mut to_free: [(*mut u8, usize); crate::alloc_core::segment_table::MAX_SEGMENTS] =
            [(core::ptr::null_mut(), 0usize); crate::alloc_core::segment_table::MAX_SEGMENTS];
        let mut n = 0usize;
        for base in self.table.bases() {
            if n >= crate::alloc_core::segment_table::MAX_SEGMENTS {
                break;
            }
            let hdr = SegmentHeader::read_at(base);
            // Every registered segment has a valid reservation recorded (set
            // at register-time). We free them all — including large segments
            // whose magic was zeroed by `dealloc` (they are still mapped and
            // still carry the reservation info in their header).
            to_free[n] = (hdr.reservation, hdr.reservation_len);
            n += 1;
        }
        // Now free every collected reservation. The primordial (whose payload
        // hosts the registry) is freed here alongside the rest — safe, because
        // we no longer read the registry.
        for &(reservation, reservation_len) in &to_free[..n] {
            os::release_segment(reservation, reservation_len);
        }
    }
}

// NOTE: `AllocCore` is intentionally NOT `Send` (nor `Sync`) in Phase 8.
// Phase 8 is single-threaded; `Send` is not needed. Phase 9 (per-thread
// heaps) will add `Send` at the heap layer (the segment substrate is
// `Send`-capable, but the claim belongs to the layer that owns the threading
// discipline, not the substrate itself). Adding it here would require an
// `unsafe impl` that has no place outside the two named `unsafe` seams.

/// `base + off` as `*mut u8`, routed through the `node` seam. The Cartographer
/// only ever passes offsets derived from the fixed [`SegLayout`] or the bump
/// cursor (both bounded by `SEGMENT`).
pub(in crate::alloc_core) fn base_add(base: *mut u8, off: usize) -> *mut u8 {
    Node::offset(base, off)
}
