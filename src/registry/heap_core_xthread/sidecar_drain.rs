//! Exclusive heap-owner sweep of terminal route-sidecar publications.

use crate::registry::heap_core::HeapCore;

impl HeapCore {
    /// Called by cold trim while the caller still owns the heap. Foreign
    /// publication never borrows this core or accesses reservation metadata.
    pub(crate) fn drain_sidecar_ingress(&mut self) -> usize {
        self.core.drain_sidecar_ingress()
    }

    /// Reclaims only descriptor-claimed Large frees on actual cold paths.
    pub(crate) fn drain_large_sidecar_ingress(&mut self) -> usize {
        self.core.drain_large_sidecar_ingress()
    }

    /// Publish a current issued Small block through the real descriptor.
    ///
    /// # Safety
    /// `ptr` is a live Small/Primordial allocation issued by this heap and
    /// ownership is transferred exactly once; do not access or free it afterward.
    #[cfg(all(feature = "bench-internals", feature = "internals"))]
    #[doc(hidden)]
    #[allow(unsafe_code)] // Forward the same unique-transfer producer obligation.
    pub unsafe fn dbg_publish_small_sidecar_free(&mut self, ptr: *mut u8) -> bool {
        // SAFETY: this method requires the delegated current-issue unique transfer.
        unsafe { self.core.dbg_publish_small_sidecar_free(ptr) }
    }

    #[cfg(all(feature = "bench-internals", feature = "internals"))]
    #[doc(hidden)]
    pub fn dbg_drain_sidecar_ingress(&mut self) -> usize {
        self.drain_sidecar_ingress()
    }

    #[cfg(all(feature = "bench-internals", feature = "internals"))]
    #[doc(hidden)]
    pub fn dbg_background_maintenance_step(&mut self, budget: usize) -> (usize, usize) {
        self.background_maintenance_step(budget)
    }

    #[cfg(all(feature = "bench-internals", feature = "internals"))]
    #[doc(hidden)]
    pub fn dbg_background_cursor(&self) -> (usize, usize) {
        self.background_cursor
    }

    /// Probe the real logical-retirement primitive with a synthetic detached
    /// record, including an already free or magazine-resident block. No route
    /// publication occurs and reservation finalization is left to cold trim.
    ///
    /// # Safety
    /// `ptr` identifies a physical Small/Primordial block in a currently mapped
    /// segment owned exclusively by this heap, and `class` is its issued class.
    /// If still user-live, this call transfers its unique ownership on success;
    /// the caller must not use or free it afterward. Free/magazine records may
    /// be probed only while their reservation and block geometry remain live.
    #[cfg(all(feature = "bench-internals", feature = "internals"))]
    #[doc(hidden)]
    #[allow(unsafe_code)] // Synthetic record probes retain the real owner/geometry contract.
    pub unsafe fn dbg_reclaim_sidecar_record_for_test(&mut self, ptr: *mut u8, class: u8) -> bool {
        let address = ptr.addr();
        let key = address & !(crate::alloc_core::os::SEGMENT - 1);
        let base = self
            .segment_bases()
            .find(|base| base.addr() == key)
            .unwrap_or_else(|| std::process::abort());
        let offset = (address - base.addr()) as u32;
        crate::alloc_core::AllocCore::reclaim_sidecar_record(base, offset, class)
    }
}
