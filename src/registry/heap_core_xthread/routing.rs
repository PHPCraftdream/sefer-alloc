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
        // SAFETY: reached only from the allocator's unique current-instance free.
        unsafe { Self::publish_foreign(ptr, layout) };
    }

    /// # Safety
    /// `ptr` and `layout` name one current issued allocation, transferred once.
    #[allow(unsafe_code)]
    pub(crate) unsafe fn publish_foreign(ptr: *mut u8, layout: Layout) {
        let published = match route_pin_cache::publish_cached(ptr) {
            CachedPublication::Published(published) => published,
            CachedPublication::Miss => {
                // SAFETY: caller transfers the current allocation once.
                unsafe { Self::publish_foreign_uncached(ptr, layout) }
            }
        };
        if !published {
            crate::alloc_core::FOREIGN_OR_UNROUTABLE_FREES
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
        #[cfg(all(miri, feature = "internals", feature = "bench-internals"))]
        if published {
            // Only the independent, pre-initialized static test gate is touched.
            // The actual Box Drop and GlobalAlloc dealloc frames stay active.
            crate::registry::segment_route::TerminalPublicationGate::pause_after_publication(
                ptr.addr(),
            );
        }
    }

    /// Slow path: full directory lookup and (possibly cached) publication.
    ///
    /// # Safety
    /// `ptr` and `layout` name one current issued allocation, transferred once.
    #[cold]
    #[allow(unsafe_code)]
    unsafe fn publish_foreign_uncached(ptr: *mut u8, layout: Layout) -> bool {
        let Some(pin) = RouteDirectory::global().lookup(ptr) else {
            // The shared tail in `publish_foreign` counts this miss exactly
            // once via `published == false`.
            route_pin_cache::cache_evict(ptr.addr() & !(os::SEGMENT - 1));
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
                // The pin is borrowed, not consumed, so it can be cached below.
                let published = unsafe { pin.publish_small_borrowed(offset) };
                if cacheable {
                    // Retaining the pin (refs +1) until eviction or thread
                    // exit is the design: the descriptor and sidecar survive
                    // the segment reservation's release.
                    route_pin_cache::cache_insert(pin, key, incarnation);
                }
                published
            }
        }
    }
}
