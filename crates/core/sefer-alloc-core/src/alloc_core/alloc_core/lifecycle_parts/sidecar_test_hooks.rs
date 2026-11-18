//! Test-only access to the real terminal publication and owner sweep.

use super::AllocCore;

impl AllocCore {
    /// Construct a real descriptor-backed test core before its first issue.
    /// It is not a registry slot; controlled probes publish through RoutePin.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[allow(unsafe_code)] // Sentinel-owned standalone construction (see contract on `new_with_owner`).
    pub fn dbg_new_routed_for_test() -> Option<Self> {
        // SAFETY: `owner` is the literal `OWNER_ID_NONE` sentinel — outside the
        // registry slot space, so no registry slot exists and no ingress cell is
        // armed; the result is the standalone descriptor-backed mode. The core is
        // `!Send`/`!Sync`, stays self-owned here, and its sidecar is drained only
        // through this instance's own hooks — no lease is manufactured and no
        // registry slot is claimed.
        unsafe { Self::new_with_owner(crate::alloc_core::segment_header::OWNER_ID_NONE) }
    }

    /// Configured counterpart for real terminal publication/pool probes.
    #[cfg(feature = "alloc-decommit")]
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[allow(unsafe_code)] // Sentinel-owned standalone construction (see contract on `new_with_config_for_owner`).
    pub fn dbg_new_routed_with_config_for_test(
        config: crate::alloc_core::large_cache_config::LargeCacheConfig,
    ) -> Option<Self> {
        // SAFETY: `owner` is the literal `OWNER_ID_NONE` sentinel — same
        // standalone terms as `dbg_new_routed_for_test`: no registry
        // slot, no ingress cell, `!Send`/`!Sync` self-owned core, sidecar drained
        // only via this instance's own hooks; no lease is manufactured.
        unsafe {
            Self::new_with_config_for_owner(
                config,
                crate::alloc_core::segment_header::OWNER_ID_NONE,
            )
        }
    }

    /// Publishes a real issued Small allocation through its terminal descriptor.
    ///
    /// # Safety
    /// `ptr` is a live Small/Primordial allocation issued by this core. This
    /// transfers its unique ownership once; do not use or free it afterward.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[allow(unsafe_code)] // Test producer has the same unique-transfer contract as RoutePin.
    pub unsafe fn dbg_publish_small_sidecar_free(&mut self, ptr: *mut u8) -> bool {
        let Some((base, _)) = self.canonical_block_of(ptr) else {
            return false;
        };
        let Some(pin) = crate::alloc_core::segment_route::RouteDirectory::global().lookup(ptr)
        else {
            return false;
        };
        let offset = (ptr.addr() - base.addr()) as u32;
        // SAFETY: the caller transfers this current issued allocation exactly
        // once, and canonical_block_of resolves its canonical reservation root.
        unsafe { pin.publish_small(offset) }
    }

    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn dbg_drain_sidecar_ingress(&mut self) -> usize {
        self.drain_sidecar_ingress()
    }

    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn dbg_drain_large_sidecar_ingress(&mut self) -> usize {
        self.drain_large_sidecar_ingress()
    }

    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn dbg_bounded_sidecar_step(
        &mut self,
        cursor: &mut (usize, usize),
        budget: usize,
    ) -> (usize, usize) {
        self.drain_sidecar_ingress_bounded(cursor, budget)
    }
}
