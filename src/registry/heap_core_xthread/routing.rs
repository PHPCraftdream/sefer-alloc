//! Address-only dispatch. Foreign producers touch only independently pinned sidecars.
use super::route_pin_cache::{self, CachedPublication};
use crate::alloc_core::os;
use crate::registry::heap_core::HeapCore;
use crate::registry::segment_route::{RouteDirectory, RouteKind};
use core::alloc::Layout;

impl HeapCore {
    #[inline(always)]
    #[allow(unsafe_code)]
    pub(crate) fn dealloc_routing(&mut self, ptr: *mut u8, layout: Layout) {
        if let Some(owned) = self.core.owned_block_of(ptr) {
            #[cfg(feature = "fastbin")]
            self.dealloc_own_thread_with_base(owned, layout);
            #[cfg(not(feature = "fastbin"))]
            self.dealloc_own_thread(owned.block(), layout, owned.root());
            return;
        }
        // X-pin (а): the owner's `&mut self` is in hand — probe THIS heap's
        // direct-mapped pin cache (no TLS resolution, no `RefCell`, no
        // linear scan; see `heap_route_pin_cache.rs`'s module doc for the
        // ~6-8 Ir TLS-path comparison). The whole foreign leg stays in ONE
        // `#[cold]` call so the inlined own-thread body of `dealloc_routing`
        // keeps its original shape (mt-xp2: the inline probe arm cost the
        // own free ~+4 Ir in mstress / +3 Ir in thrash64 via code bloat).
        #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
        {
            self.publish_foreign_heap(ptr, layout);
        }
        #[cfg(not(all(feature = "alloc-global", feature = "alloc-xthread")))]
        {
            // SAFETY: reached only from the allocator's unique current-instance free.
            unsafe { Self::publish_foreign(ptr, layout) };
        }
    }

    /// X-pin (а): the heap-owner foreign-free leg, cold exactly like the
    /// static `publish_foreign` (same rationale — keep it out of the inlined
    /// own-free body). Hit path probes this heap's direct-mapped cache; a
    /// miss inserts into the SAME cache via `PinSink::Heap`.
    #[cold]
    #[allow(unsafe_code)]
    fn publish_foreign_heap(&mut self, ptr: *mut u8, layout: Layout) {
        // SAFETY: reached only from the allocator's unique current-instance
        // free (the `dealloc_routing` contract); the unique ownership is
        // forwarded verbatim to `publish_cached`.
        if let CachedPublication::Published(published) =
            unsafe { self.route_pins.publish_cached(ptr) }
        {
            Self::publish_count_tail(published);
            return;
        }
        let mut sink = PinSink::Heap(self);
        // SAFETY: caller transfers the current allocation once.
        let published = unsafe { Self::publish_foreign_uncached(&mut sink, ptr, layout) };
        Self::publish_count_tail(published);
    }

    /// # Safety
    /// `ptr` and `layout` name one current issued allocation, transferred once.
    #[allow(unsafe_code)]
    // Cold relative to the own-thread free in `dealloc_routing`: keeps the
    // cache lookup out of that inlined body (iai: ~+1 Ir per own free without).
    #[cold]
    pub(crate) unsafe fn publish_foreign(ptr: *mut u8, layout: Layout) {
        // SAFETY: this function's `# Safety` contract is the unique transfer
        // of the current issued allocation instance; that unique ownership
        // is forwarded verbatim to `publish_cached`, which hands it to
        // `publish_small_borrowed` for terminal publication. The foreign-free
        // path frees a block the caller does not own — the transfer-once
        // contract is what authorizes it (same contract as the slow
        // publication below).
        let published = match unsafe { route_pin_cache::publish_cached(ptr) } {
            CachedPublication::Published(published) => published,
            CachedPublication::Miss => {
                let mut sink = PinSink::Tls;
                // SAFETY: caller transfers the current allocation once.
                unsafe { Self::publish_foreign_uncached(&mut sink, ptr, layout) }
            }
        };
        Self::publish_count_tail(published);
        #[cfg(all(miri, feature = "internals", feature = "bench-internals"))]
        if published {
            // Only the independent, pre-initialized static test gate is touched.
            // The actual Box Drop and GlobalAlloc dealloc frames stay active.
            crate::registry::segment_route::TerminalPublicationGate::pause_after_publication(
                ptr.addr(),
            );
        }
    }

    /// Shared counter tail of the foreign publication paths (X-pin (а):
    /// factored out so the heap-cache hit arm and the static TLS path count
    /// identically).
    #[inline]
    #[allow(unsafe_code)]
    fn publish_count_tail(published: bool) {
        #[cfg(feature = "bench-internals")]
        if published {
            crate::alloc_core::B4_FOREIGN_PUBLISH_OK
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
        if !published {
            crate::alloc_core::FOREIGN_OR_UNROUTABLE_FREES
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Slow path: full directory lookup and (possibly cached) publication.
    /// The freshly looked-up cacheable pin is inserted into the caller's
    /// [`PinSink`]: the owning heap's direct-mapped cache (X-pin (а)) or the
    /// ForeignNoBind thread's TLS cache — the storage move only; the protocol
    /// is unchanged.
    ///
    /// # Safety
    /// `ptr` and `layout` name one current issued allocation, transferred once.
    #[cold]
    #[allow(unsafe_code)]
    unsafe fn publish_foreign_uncached(
        sink: &mut PinSink<'_>,
        ptr: *mut u8,
        layout: Layout,
    ) -> bool {
        let Some(pin) = RouteDirectory::global().lookup(ptr) else {
            // The shared tail in `publish_foreign` counts this miss exactly
            // once via `published == false`.
            sink.evict(ptr.addr() & !(os::SEGMENT - 1));
            return false;
        };
        // Lookup has released its shard lock. A ready, idle fallback can
        // reclaim synchronously without service startup or a future allocation.
        // The independent pin remains valid even if this free releases its root.
        if pin.owner() == crate::alloc_core::segment_header::OWNER_ID_FALLBACK as usize
            && crate::global::fallback::try_with_heap(|heap| {
                // SAFETY: caller transfers the current allocation once, and
                // try_with_heap grants exclusive fallback owner mutation.
                unsafe { heap.dealloc(ptr, layout) };
            })
            .is_some()
        {
            // The free was handled synchronously; no unroutable-free bump.
            return true;
        }
        // A busy fallback cannot be waited on or recursively borrowed. Publish
        // through the same already-pinned sidecar used by every other owner.
        match pin.kind() {
            RouteKind::Large => {
                // SAFETY: caller uniquely transfers the current issued instance.
                unsafe { pin.publish_large() }
            }
            RouteKind::Small | RouteKind::Primordial => {
                let key = os::segment_base_of(ptr.addr());
                let offset = (ptr.addr() - key) as u32;
                let incarnation = pin.incarnation();
                let cacheable =
                    pin.owner() != crate::alloc_core::segment_header::OWNER_ID_FALLBACK as usize;
                // SAFETY: small routes retain segment alignment. Only a numeric
                // offset is transferred; no bytes of ptr or its slack are touched.
                // The caller's unique ownership of the current issued Small/
                // Primordial allocation instance is transferred onward into
                // `publish_small_borrowed` (and, via the cached fast path,
                // into `publish_cached`) for terminal publication — a bare
                // address grants no right to free the block.
                // The pin is borrowed, not consumed, so it can be cached below.
                let published = unsafe { pin.publish_small_borrowed(offset) };
                if cacheable {
                    // Retaining the pin (refs +1) until eviction, trim, or
                    // thread exit is the design: the descriptor and sidecar
                    // survive the segment reservation's release. The sink
                    // decides the storage (owner heap cache vs TLS cache).
                    sink.insert(pin, key, incarnation);
                }
                published
            }
        }
    }
}

/// X-pin (а): where `publish_foreign_uncached` stores a freshly looked-up
/// cacheable pin. `Heap` = the owning thread's `HeapCore`-resident direct-
/// mapped cache (the `&mut HeapCore` is already held by `dealloc_routing`);
/// `Tls` = the `ForeignNoBind` thread's TLS cache (unchanged behavior).
pub(crate) enum PinSink<'a> {
    Heap(&'a mut HeapCore),
    Tls,
}

impl PinSink<'_> {
    fn insert(
        &mut self,
        pin: crate::registry::segment_route::RoutePin,
        base: usize,
        incarnation: u64,
    ) {
        match self {
            PinSink::Heap(heap) => heap.route_pins.insert(pin, base, incarnation),
            PinSink::Tls => route_pin_cache::cache_insert(pin, base, incarnation),
        }
    }

    fn evict(&mut self, base: usize) {
        match self {
            // The heap cache is the owner's: evict the same slot the TLS
            // path would (a failed lookup means the route is gone).
            PinSink::Heap(heap) => heap.route_pins.evict_base(base),
            PinSink::Tls => route_pin_cache::cache_evict(base),
        }
    }
}
