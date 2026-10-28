//! Per-thread route pin cache for the foreign-free terminal-publication path.
//!
//! What is cached: a per-thread direct-mapped array of
//! `base -> (pinned Entry descriptor, incarnation)` pairs. Only Small and
//! Primordial NON-fallback routes are ever inserted, so the hot path
//! publishes small directly without reading `kind()`.
//!
//! Why the pin can be held long-term: a route's `Entry` descriptor and its
//! sidecar are independent System allocations that survive the release of the
//! segment reservation they describe (see `routing.rs`'s existing comment on
//! the foreign path). Holding a counted `RoutePin` therefore keeps the
//! descriptor + sidecar bytes alive after the segment is gone, which is
//! exactly what the cache relies on. The cache never keeps the segment
//! RESERVATION itself alive — only the directory descriptor and sidecar.
//!
//! Safety protocol: a cached pin publishes only after `linked()` (an Acquire
//! load of `Entry::linked`) observes the route still linked. In
//! `RouteDirectory::remove` the unlink from the shard index runs FIRST, under
//! the shard lock, and only then a Release `linked = false` store publishes
//! the unlink — so observing `linked == true` does NOT prove the route is
//! still linked: a concurrent remove may already have unlinked it while this
//! thread races the check/publish window. What actually makes the cached
//! publication safe in a freed segment is the un-published block's live
//! credit: a segment is never finalized while its live-count is non-zero
//! (see `src/alloc_core/small/alloc_core_small_reclaim.rs`'s owner-reclaim
//! decrement and `alloc_core_small_pool_impl.rs`'s `live_count == 0`
//! finalization gate). The outstanding pin reference keeps only the route
//! descriptor and sidecar alive; the live credit keeps the reservation alive
//! until owner reclaim. `linked` is not a delivery guarantee — it is the
//! mechanism by which the cache gives up on dead pins and falls back to a
//! fresh lookup; a stale `true` can only cause a publication the owner-reclaim
//! protocol already serializes against.
//!
//! Thread exit: the `thread_local!` `Drop` releases every retained pin count
//! through the existing `EntryHandle::drop` protocol (`refs.fetch_sub`), the
//! ONLY release path — no new refcount code exists here.
//!
//! Ghost builds record `pin-cache/held` (+1 when a pin enters a slot, -1 when
//! it leaves), keyed by segment base; `tests/loom_b4_r1_pin_cache.rs` checks it
//! against `Entry::refs`.
use crate::registry::segment_route::RoutePin;
use core::cell::RefCell;

const PIN_CACHE_SLOTS: usize = 4;

struct PinCacheSlot {
    base: usize,
    incarnation: u64,
    pin: Option<RoutePin>,
}

struct PinCache {
    slots: [PinCacheSlot; PIN_CACHE_SLOTS],
    next_victim: usize,
}

impl PinCache {
    const EMPTY: Self = Self {
        slots: [const {
            PinCacheSlot {
                base: 0,
                incarnation: 0,
                pin: None,
            }
        }; PIN_CACHE_SLOTS],
        next_victim: 0,
    };
}

// Diagnostic counting macro, defined before first textual use (textual
// scoping of `macro_rules!`).
#[cfg(all(feature = "internals", feature = "bench-internals"))]
macro_rules! count {
    ($counter:expr) => {
        $counter.fetch_add(1, core::sync::atomic::Ordering::Relaxed)
    };
}
#[cfg(not(all(feature = "internals", feature = "bench-internals")))]
macro_rules! count {
    ($counter:expr) => {};
}

impl PinCache {
    /// Drop every cached pin whose route has been unlinked (`linked ==
    /// false`). This is the memory-reclamation sweep: an idle thread can
    /// otherwise hold up to `PIN_CACHE_SLOTS` × (Entry + sidecar + up to
    /// 1024 class-leaves of 256 `AtomicU8`) ≈ 1.16 MiB of dead descriptors
    /// alive until eviction or thread exit. Only called on cold paths (see
    /// the call sites): `linked()` is an Acquire load that can touch a
    /// foreign cache line, so a hit-path sweep would cost the very lookup
    /// this cache exists to avoid — a miss/insert is by definition already
    /// a directory-lookup-price path, where one extra Acquire-load per slot
    /// is noise. Dead pins drop through the existing `EntryHandle` protocol;
    /// no new refcount code. `PIN_CACHE_SWEEPS` counts each pin reclaimed
    /// by a sweep (deliberately NOT `UNLINK_FALLBACKS`, which counts
    /// on-hit unlink fallbacks).
    fn sweep_dead_slots(&mut self) {
        for slot in &mut self.slots {
            if slot.pin.as_ref().is_some_and(|pin| !pin.linked()) {
                slot.pin = None;
                crate::ghost::ghost_event!("pin-cache/held", slot.base, -1);
                count!(PIN_CACHE_SWEEPS);
            }
        }
    }
}

impl Drop for PinCache {
    fn drop(&mut self) {
        // Measurement-only: a plain production Drop must neither read nor
        // increment anything; the pins themselves release through their own
        // `Drop for RoutePin` (the EntryHandle protocol) in every config.
        // The pins are released FIRST (dropping them here, before the field
        // drops would run), so `THREAD_RELEASES` is bumped only AFTER the
        // actual releases completed — leak-oracle tests can rely on the
        // counter never preceding the release it reports.
        let retained = self.slots.iter().filter(|slot| slot.pin.is_some()).count() as u64;
        for slot in &mut self.slots {
            crate::ghost::ghost_event!("pin-cache/held", slot.base, -i64::from(slot.pin.is_some()));
            slot.pin = None;
        }
        #[cfg(not(all(feature = "internals", feature = "bench-internals")))]
        let _ = retained;
        #[cfg(all(feature = "internals", feature = "bench-internals"))]
        PIN_CACHE_THREAD_RELEASES.fetch_add(retained, core::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(all(loom, test))]
loom::thread_local! {
    static CACHE: RefCell<PinCache> = RefCell::new(PinCache::EMPTY);
}

#[cfg(not(all(loom, test)))]
thread_local! {
    static CACHE: RefCell<PinCache> = const { RefCell::new(PinCache::EMPTY) };
}

// Test/diagnostic-only counters. Statics are always compiled, but increments
// exist only under `internals ∧ bench-internals` (measurement-only, the
// `LOCAL_REJECT_FREES` convention): a plain `production` build pays nothing
// on the hot path. Relaxed: diagnostic only.
#[cfg_attr(
    not(all(feature = "internals", feature = "bench-internals")),
    allow(dead_code)
)]
pub(crate) static PIN_CACHE_HITS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
#[cfg_attr(
    not(all(feature = "internals", feature = "bench-internals")),
    allow(dead_code)
)]
pub(crate) static PIN_CACHE_MISSES: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
#[cfg_attr(
    not(all(feature = "internals", feature = "bench-internals")),
    allow(dead_code)
)]
pub(crate) static PIN_CACHE_INSERTS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
#[cfg_attr(
    not(all(feature = "internals", feature = "bench-internals")),
    allow(dead_code)
)]
pub(crate) static PIN_CACHE_EVICTIONS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
#[cfg_attr(
    not(all(feature = "internals", feature = "bench-internals")),
    allow(dead_code)
)]
pub(crate) static PIN_CACHE_UNLINK_FALLBACKS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
#[cfg_attr(
    not(all(feature = "internals", feature = "bench-internals")),
    allow(dead_code)
)]
pub(crate) static PIN_CACHE_THREAD_RELEASES: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
#[cfg_attr(
    not(all(feature = "internals", feature = "bench-internals")),
    allow(dead_code)
)]
pub(crate) static PIN_CACHE_SWEEPS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// Outcome of a cached-pin publication attempt.
pub(crate) enum CachedPublication {
    /// Published (or the sidecar RMW rejected the publication).
    Published(bool),
    /// No usable cached pin; the caller must run the uncached path.
    Miss,
}

/// Fast path: publish `ptr` through a cached pin for its segment base, if a
/// still-linked route is cached. Never panics and never allocates; TLS
/// unavailability or a reentrant borrow degrades to [`CachedPublication::Miss`].
///
/// # Safety
/// The caller transfers the unique ownership of the CURRENT issued Small or
/// Primordial allocation instance at `ptr` for terminal publication (the
/// foreign-free contract of `HeapCore::publish_foreign`). A bare address
/// establishes no ownership: publishing a live allocation owned by someone
/// else is use-after-free by construction (the sidecar RMW reclaims it).
#[allow(unsafe_code)]
pub(crate) unsafe fn publish_cached(ptr: *mut u8) -> CachedPublication {
    let addr = ptr.addr();
    let base = addr & !(crate::alloc_core::os::SEGMENT - 1);
    CACHE
        .try_with(|cell| {
            let Ok(mut cache) = cell.try_borrow_mut() else {
                return CachedPublication::Miss;
            };
            let Some(slot) = cache
                .slots
                .iter_mut()
                .find(|slot| slot.pin.is_some() && slot.base == base)
            else {
                // Cold path: no slot for this base. Sweep dead (unlinked)
                // pins so a long-lived idle thread does not retain ~1.16 MiB
                // of stale descriptors; see `sweep_dead_slots`.
                cache.sweep_dead_slots();
                count!(PIN_CACHE_MISSES);
                return CachedPublication::Miss;
            };
            if let Some(pin) = slot.pin.as_ref() {
                if !pin.linked() {
                    // The route was unlinked; release the dead pin immediately
                    // (taking it out drops it via the existing EntryHandle
                    // protocol) and fall back to a fresh lookup.
                    count!(PIN_CACHE_UNLINK_FALLBACKS);
                    slot.pin = None;
                    crate::ghost::ghost_event!("pin-cache/held", slot.base, -1);
                    count!(PIN_CACHE_MISSES);
                    return CachedPublication::Miss;
                }
                if pin.incarnation() != slot.incarnation {
                    // Defense in depth; the field is immutable so this never
                    // fires today.
                    slot.pin = None;
                    crate::ghost::ghost_event!("pin-cache/held", slot.base, -1);
                    count!(PIN_CACHE_MISSES);
                    return CachedPublication::Miss;
                }
                if !pin.span_contains_addr(addr) {
                    // The fresh lookup path re-derives the same verdict and
                    // counts its own drop, so no extra counting here.
                    count!(PIN_CACHE_MISSES);
                    return CachedPublication::Miss;
                }
                let offset = (addr - base) as u32;
                // SAFETY: the caller transfers the current issued allocation
                // once — identical to `publish_foreign`'s contract; address
                // lookup alone does not establish ownership.
                let published = unsafe { pin.publish_small_borrowed(offset) };
                count!(PIN_CACHE_HITS);
                return CachedPublication::Published(published);
            }
            count!(PIN_CACHE_MISSES);
            CachedPublication::Miss
        })
        .unwrap_or(CachedPublication::Miss)
}

/// Insert a freshly looked-up pin for `base` into the cache. Slots only ever
/// hold Small/Primordial NON-fallback routes (enforced by the caller).
/// On TLS unavailability or a reentrant borrow the pin is simply dropped
/// (releasing its references through the existing protocol).
pub(crate) fn cache_insert(pin: RoutePin, base: usize, incarnation: u64) {
    let inserted = CACHE.try_with(|cell| {
        let Ok(mut cache) = cell.try_borrow_mut() else {
            return Err(pin);
        };
        // Cold path (the caller just ran a full directory lookup): sweep
        // dead (unlinked) pins before inserting; see `sweep_dead_slots` for
        // why the sweep lives only on cold paths.
        cache.sweep_dead_slots();
        if let Some(slot) = cache.slots.iter_mut().find(|slot| slot.base == base) {
            if slot.pin.take().is_some() {
                crate::ghost::ghost_event!("pin-cache/held", slot.base, -1);
                count!(PIN_CACHE_EVICTIONS);
            }
            slot.incarnation = incarnation;
            slot.pin = Some(pin);
            crate::ghost::ghost_event!("pin-cache/held", slot.base, 1);
        } else {
            let victim = cache.next_victim;
            cache.next_victim = (victim + 1) % PIN_CACHE_SLOTS;
            let slot = &mut cache.slots[victim];
            if slot.pin.take().is_some() {
                crate::ghost::ghost_event!("pin-cache/held", slot.base, -1);
                count!(PIN_CACHE_EVICTIONS);
            }
            slot.base = base;
            slot.incarnation = incarnation;
            slot.pin = Some(pin);
            crate::ghost::ghost_event!("pin-cache/held", slot.base, 1);
        }
        count!(PIN_CACHE_INSERTS);
        Ok(())
    });
    // TLS unavailable: `Err` carries the pin; dropping it here releases its
    // references through the existing EntryHandle protocol.
    drop(inserted);
}

/// Drop any cached pin for `base` (the caller's fresh lookup already counted
/// its own MISS for this address).
pub(crate) fn cache_evict(base: usize) {
    let _ = CACHE.try_with(|cell| {
        let Ok(mut cache) = cell.try_borrow_mut() else {
            return;
        };
        if let Some(slot) = cache
            .slots
            .iter_mut()
            .find(|slot| slot.base == base && slot.pin.is_some())
        {
            slot.pin = None;
            crate::ghost::ghost_event!("pin-cache/held", slot.base, -1);
            count!(PIN_CACHE_EVICTIONS);
        }
    });
}

/// Test/diagnostic totals of the per-thread route pin cache
/// (hits, misses, inserts, evictions, unlink fallbacks, thread releases,
/// sweep-reclaimed pins).
#[cfg(all(feature = "internals", feature = "bench-internals"))]
pub(crate) fn dbg_totals() -> (u64, u64, u64, u64, u64, u64, u64) {
    use core::sync::atomic::Ordering::Relaxed;
    (
        PIN_CACHE_HITS.load(Relaxed),
        PIN_CACHE_MISSES.load(Relaxed),
        PIN_CACHE_INSERTS.load(Relaxed),
        PIN_CACHE_EVICTIONS.load(Relaxed),
        PIN_CACHE_UNLINK_FALLBACKS.load(Relaxed),
        PIN_CACHE_THREAD_RELEASES.load(Relaxed),
        PIN_CACHE_SWEEPS.load(Relaxed),
    )
}
