//! Heap-located route pin cache (X-pin (а), `sol_ox.md` §6): a direct-mapped
//! per-`HeapCore` array of `base -> (pinned Entry descriptor, incarnation)`
//! pairs, the `HeapCore`-resident sibling of `route_pin_cache.rs`'s TLS cache.
//!
//! ## Why the pins live in `HeapCore` (design decision, documented with cost)
//!
//! The TLS cache (`route_pin_cache.rs`) pays, on every foreign-free hit:
//! `CACHE.try_with` (TLS dtor-state check + cell load, ~3–5 Ir) +
//! `try_borrow_mut` (`RefCell` borrow-flag RMW, ~2–3 Ir) + a LINEAR search
//! over `PIN_CACHE_SLOTS = 4` slots (~2 Ir per non-matching slot). The
//! `HeapCore` cache pays NONE of that: `HeapCore::dealloc_routing` already
//! holds `&mut self`, so the hit path is ONE shift/AND index computation plus
//! ONE slot probe — the direct-mapped load replaces both the TLS resolution
//! (~6–8 Ir) and the linear scan. This is why the hit path needs no TLS heap
//! resolution at all: the owner's `&mut HeapCore` is already in hand at the
//! sole call site (`dealloc_routing`), and `publish_foreign` (the static
//! entry used by ForeignNoBind threads, where no `HeapCore` is owned) keeps
//! using the TLS cache unchanged. No `RefCell`, no TLS dtor-state check, no
//! reentrant-borrow miss on the heap-thread hit path.
//!
//! Single-writer: a `HeapCore`'s pins are touched ONLY by its owning thread
//! (through `dealloc_routing`, which the owner reaches exclusively; the same
//! invariant `tls_heap.rs` documents for the whole core) and by
//! `trim_for_recycle` (owner thread, before the lease release) — never by a
//! foreign thread, never by maintenance.
//!
//! ## Direct mapping and retention cap (RSS budget)
//!
//! Index: `(base >> 22) & (HEAP_PIN_SLOTS - 1)` — one slot per 4 MiB segment
//! base, `HEAP_PIN_SLOTS = 16`. The base `route_pin_cache.rs` comment prices
//! one retained (possibly dead) pin at ≈ 1.16 MiB / 4 slots ≈ 0.29 MiB
//! (Entry + sidecar with up to 1024 class-leaves of 256 `AtomicU8`); 16
//! unconstrained slots could retain ≈ 4.6 MiB on a thread that frees across
//! ≥ 5 distinct bases and then goes idle — over the ≤ +1 MiB idle-retention
//! budget. The cache therefore enforces `MAX_LIVE_PINS = 4` live pins
//! (≈ 1.16 MiB — exactly the TLS cache's retention envelope): inserts beyond
//! the cap first sweep dead pins, then evict live pins round-robin. With
//! ≤ 4 concurrently-live foreign bases (the measured locality working set)
//! there are no evictions and the direct-mapped index never conflicts.
//!
//! ## Sweep / trim semantics (preserved from the TLS cache)
//!
//! Dead (unlinked) pins are swept only on the cold path (a miss, i.e. a
//! directory-lookup-price path, and every insert), never on the hit path —
//! `linked()` is an Acquire load that can touch a foreign cache line. All
//! pins are released in `HeapCore::trim_for_recycle` BEFORE the lease's
//! `LIVE → FREE` CAS (`tls_heap.rs` `LeaseReleaseGuard::drop` order), so a
//! recycled slot never hands its successor stale refcounts.
//!
//! Publication protocol: IDENTICAL to the TLS cache (`linked()` Acquire →
//! `incarnation` → `span_contains_addr` → `publish_small_borrowed`); only the
//! storage moved. Diagnostic counters are SHARED with the TLS cache
//! (`PIN_CACHE_HITS`/`MISSES`/... are the subsystem totals across both
//! backends) so the existing oracle prints one honest hit_frac.
// The no-op `count!` arm (not `internals ∧ bench-internals`) never expands
// its argument, so `route_pin_cache::` paths resolve only in the counting
// configuration — hence the conditional import of the module path (the
// established pattern from route_pin_cache.rs itself).
#[cfg(all(feature = "internals", feature = "bench-internals"))]
use super::route_pin_cache;
use super::route_pin_cache::{count, CachedPublication};
use crate::registry::segment_route::RoutePin;

/// Slot count (direct-mapped, power of two). See the module doc for the
/// retention-cap interaction.
const HEAP_PIN_SLOTS: usize = 16;

/// Live-pin retention cap: ≈ 4 × 0.29 MiB ≈ 1.16 MiB, the same idle-retention
/// envelope as the 4-slot TLS cache (≤ +1 MiB budget; see module doc).
const MAX_LIVE_PINS: usize = 4;

struct HeapPinSlot {
    base: usize,
    incarnation: u64,
    pin: Option<RoutePin>,
}

/// The `HeapCore`-resident cache. Owner-private (single-writer); no interior
/// mutability — the owner holds `&mut self`.
pub(crate) struct HeapRoutePinCache {
    slots: [HeapPinSlot; HEAP_PIN_SLOTS],
    /// Round-robin cursor for retention-cap evictions (see `insert`).
    next_victim: usize,
}

impl HeapRoutePinCache {
    pub(crate) const EMPTY: Self = Self {
        slots: [const {
            HeapPinSlot {
                base: 0,
                incarnation: 0,
                pin: None,
            }
        }; HEAP_PIN_SLOTS],
        next_victim: 0,
    };

    #[inline]
    fn index(base: usize) -> usize {
        (base >> 22) & (HEAP_PIN_SLOTS - 1)
    }

    fn live(&self) -> usize {
        self.slots.iter().filter(|s| s.pin.is_some()).count()
    }

    /// Drop every cached pin whose route has been unlinked. Cold path only
    /// (see module doc); identical rationale to `PinCache::sweep_dead_slots`.
    fn sweep_dead_slots(&mut self) {
        for slot in &mut self.slots {
            if slot.pin.as_ref().is_some_and(|pin| !pin.linked()) {
                slot.pin = None;
                count!(route_pin_cache::PIN_CACHE_SWEEPS);
            }
        }
    }

    /// Release every pin (trim-for-recycle path: runs on the owning thread
    /// BEFORE the lease `LIVE → FREE` CAS). Drops go through the existing
    /// `EntryHandle` protocol; no new refcount code.
    pub(crate) fn release_all(&mut self) {
        let retained = self.live() as u64;
        for slot in &mut self.slots {
            slot.base = 0;
            slot.incarnation = 0;
            slot.pin = None;
        }
        self.next_victim = 0;
        #[cfg(not(all(feature = "internals", feature = "bench-internals")))]
        let _ = retained;
        #[cfg(all(feature = "internals", feature = "bench-internals"))]
        route_pin_cache::PIN_CACHE_THREAD_RELEASES
            .fetch_add(retained, core::sync::atomic::Ordering::Relaxed);
    }

    /// Fast path: publish `ptr` through a cached pin for its segment base.
    /// Direct-mapped: one probe, no linear search, no TLS.
    ///
    /// # Safety
    /// The caller transfers the unique ownership of the CURRENT issued Small
    /// or Primordial allocation instance at `ptr` for terminal publication
    /// (the foreign-free contract of `HeapCore::publish_foreign`). A bare
    /// address establishes no ownership.
    #[allow(unsafe_code)]
    pub(crate) unsafe fn publish_cached(&mut self, ptr: *mut u8) -> CachedPublication {
        let addr = ptr.addr();
        let base = addr & !(crate::alloc_core::os::SEGMENT - 1);
        let idx = Self::index(base);
        if self.slots[idx].pin.is_none() {
            // Cold path: no pin for this base. Sweep dead (unlinked) pins so
            // retention stays bounded even between inserts.
            self.sweep_dead_slots();
            count!(route_pin_cache::PIN_CACHE_MISSES);
            return CachedPublication::Miss;
        }
        let slot = &mut self.slots[idx];
        let pin = slot.pin.as_ref().expect("checked Some above");
        if !pin.linked() {
            count!(route_pin_cache::PIN_CACHE_UNLINK_FALLBACKS);
            slot.pin = None;
            count!(route_pin_cache::PIN_CACHE_MISSES);
            return CachedPublication::Miss;
        }
        if pin.incarnation() != slot.incarnation {
            // Defense in depth; the field is immutable so this never fires.
            slot.pin = None;
            count!(route_pin_cache::PIN_CACHE_MISSES);
            return CachedPublication::Miss;
        }
        if !pin.span_contains_addr(addr) {
            count!(route_pin_cache::PIN_CACHE_MISSES);
            return CachedPublication::Miss;
        }
        let offset = (addr - base) as u32;
        // SAFETY: the caller transfers the current issued allocation once —
        // identical to `publish_foreign`'s contract; address lookup alone
        // does not establish ownership.
        let published = unsafe { pin.publish_small_borrowed(offset) };
        count!(route_pin_cache::PIN_CACHE_HITS);
        CachedPublication::Published(published)
    }

    /// Drop any cached pin for `base` (the caller's fresh lookup failed, so
    /// the route is gone — mirror of `route_pin_cache::cache_evict`).
    pub(crate) fn evict_base(&mut self, base: usize) {
        let idx = Self::index(base);
        if self.slots[idx].base == base && self.slots[idx].pin.take().is_some() {
            count!(route_pin_cache::PIN_CACHE_EVICTIONS);
        }
    }

    /// Insert a freshly looked-up pin (Small/Primordial NON-fallback routes
    /// only, enforced by the caller). Direct-mapped at `index(base)`; enforces
    /// `MAX_LIVE_PINS` (sweep first, then round-robin live eviction).
    pub(crate) fn insert(&mut self, pin: RoutePin, base: usize, incarnation: u64) {
        // Cold path (the caller just ran a full directory lookup): sweep dead
        // pins before inserting; see `sweep_dead_slots`.
        self.sweep_dead_slots();
        let idx = Self::index(base);
        if self.slots[idx].base == base {
            if self.slots[idx].pin.take().is_some() {
                count!(route_pin_cache::PIN_CACHE_EVICTIONS);
            }
        } else {
            while self.live() >= MAX_LIVE_PINS {
                // Round-robin live eviction. Terminates: `live() >= 1` here.
                loop {
                    let victim = self.next_victim;
                    self.next_victim = (victim + 1) % HEAP_PIN_SLOTS;
                    if self.slots[victim].pin.take().is_some() {
                        count!(route_pin_cache::PIN_CACHE_EVICTIONS);
                        break;
                    }
                }
            }
            self.slots[idx].base = base;
        }
        self.slots[idx].incarnation = incarnation;
        self.slots[idx].pin = Some(pin);
        count!(route_pin_cache::PIN_CACHE_INSERTS);
    }
}
