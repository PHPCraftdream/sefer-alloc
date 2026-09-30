//! Address-only dispatch. Foreign producers touch only independently pinned sidecars.
use crate::alloc_core::os;
use crate::registry::heap_core::HeapCore;
use crate::registry::segment_route::{RouteDirectory, RouteKind};
use core::alloc::Layout;

impl HeapCore {
    #[inline(always)]
    #[allow(unsafe_code)]
    pub(crate) fn dealloc_routing(&mut self, ptr: *mut u8, layout: Layout) {
        if let Some((base, block)) = self.core.canonical_block_of(ptr) {
            #[cfg(feature = "fastbin")]
            self.dealloc_own_thread_with_base(block, layout, base);
            #[cfg(not(feature = "fastbin"))]
            self.dealloc_own_thread(block, layout, base);
            return;
        }
        // SAFETY: reached only from the allocator's unique current-instance free.
        unsafe { Self::publish_foreign(ptr, layout) };
    }

    /// # Safety
    /// `ptr` and `layout` name one current issued allocation, transferred once.
    #[allow(unsafe_code)]
    #[cold]
    pub(crate) unsafe fn publish_foreign(ptr: *mut u8, layout: Layout) {
        let Some(pin) = RouteDirectory::global().lookup(ptr) else {
            crate::alloc_core::FOREIGN_OR_UNROUTABLE_FREES
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            return;
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
            return;
        }
        // A busy fallback cannot be waited on or recursively borrowed. Publish
        // through the same already-pinned sidecar used by every other owner.
        #[cfg(all(miri, feature = "internals", feature = "bench-internals"))]
        let publication_address = ptr.addr();
        let published = match pin.kind() {
            RouteKind::Large => {
                // SAFETY: caller uniquely transfers the current issued instance.
                unsafe { pin.publish_large() }
            }
            RouteKind::Small | RouteKind::Primordial => {
                let key = os::segment_base_of(ptr.addr());
                let offset = (ptr.addr() - key) as u32;
                // SAFETY: small routes retain segment alignment. Only a numeric
                // offset is transferred; no bytes of ptr or its slack are touched.
                unsafe { pin.publish_small(offset) }
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
                publication_address,
            );
        }
    }
}
