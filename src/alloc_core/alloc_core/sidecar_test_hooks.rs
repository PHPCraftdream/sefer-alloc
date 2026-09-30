//! Test-only access to the real terminal publication and owner sweep.

use super::AllocCore;

impl AllocCore {
    /// Construct a real descriptor-backed test core before its first issue.
    /// It is not a registry slot; controlled probes publish through RoutePin.
    #[doc(hidden)]
    pub fn dbg_new_routed_for_test() -> Option<Self> {
        Self::new_with_owner(crate::alloc_core::segment_header::OWNER_ID_NONE)
    }

    /// Configured counterpart for real terminal publication/pool probes.
    #[cfg(feature = "alloc-decommit")]
    #[doc(hidden)]
    pub fn dbg_new_routed_with_config_for_test(
        config: crate::alloc_core::large_cache_config::LargeCacheConfig,
    ) -> Option<Self> {
        Self::new_with_config_for_owner(config, crate::alloc_core::segment_header::OWNER_ID_NONE)
    }

    /// Publishes a real issued Small allocation through its terminal descriptor.
    ///
    /// # Safety
    /// `ptr` is a live Small/Primordial allocation issued by this core. This
    /// transfers its unique ownership once; do not use or free it afterward.
    #[doc(hidden)]
    #[allow(unsafe_code)] // Test producer has the same unique-transfer contract as RoutePin.
    pub unsafe fn dbg_publish_small_sidecar_free(&mut self, ptr: *mut u8) -> bool {
        let Some((base, _)) = self.canonical_block_of(ptr) else {
            return false;
        };
        let Some(pin) = crate::registry::segment_route::RouteDirectory::global().lookup(ptr) else {
            return false;
        };
        let offset = (ptr.addr() - base.addr()) as u32;
        // SAFETY: the caller transfers this current issued allocation exactly
        // once, and canonical_block_of resolves its canonical reservation root.
        unsafe { pin.publish_small(offset) }
    }

    #[doc(hidden)]
    pub fn dbg_drain_sidecar_ingress(&mut self) -> usize {
        self.drain_sidecar_ingress()
    }

    #[doc(hidden)]
    pub fn dbg_drain_large_sidecar_ingress(&mut self) -> usize {
        self.drain_large_sidecar_ingress()
    }

    #[doc(hidden)]
    pub fn dbg_bounded_sidecar_step(
        &mut self,
        cursor: &mut (usize, usize),
        budget: usize,
    ) -> (usize, usize) {
        self.drain_sidecar_ingress_bounded(cursor, budget)
    }
}
