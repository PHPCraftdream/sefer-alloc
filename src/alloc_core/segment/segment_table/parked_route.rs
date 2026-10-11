#[cfg(feature = "alloc-global")]
use crate::registry::segment_route::RouteRegistration;

/// Owner-held registration detached from the active table, not the directory.
/// Without alloc-global this is a ZST and every operation is inert.
pub(crate) struct ParkedRoute(#[cfg(feature = "alloc-global")] Option<RouteRegistration<'static>>);

impl ParkedRoute {
    pub(crate) fn none() -> Self {
        Self(
            #[cfg(feature = "alloc-global")]
            None,
        )
    }
    #[cfg(feature = "alloc-global")]
    pub(super) fn new(route: RouteRegistration<'static>) -> Self {
        Self(Some(route))
    }
    #[cfg(feature = "alloc-global")]
    pub(super) fn take_matching(
        &mut self,
        base: *mut u8,
        payload: *mut u8,
    ) -> Option<RouteRegistration<'static>> {
        let route = self.0.take()?;
        if route.root() == base && route.payload_addr() == payload.addr() {
            Some(route)
        } else {
            drop(route);
            None
        }
    }
    #[cfg(feature = "alloc-decommit")]
    pub(crate) fn cache_consumed(&self) {
        #[cfg(feature = "alloc-global")]
        if let Some(route) = &self.0 {
            let state = route.large_state().unwrap_or_else(|| std::process::abort());
            if !route.cache_large_consumed(state.generation()) {
                std::process::abort();
            }
        }
    }
    #[cfg(feature = "alloc-decommit")]
    pub(crate) fn discard_if_payload_changed(&mut self, payload: *mut u8) {
        #[cfg(feature = "alloc-global")]
        if self
            .0
            .as_ref()
            .is_some_and(|route| route.payload_addr() != payload.addr())
        {
            drop(self.0.take());
        }
        #[cfg(not(feature = "alloc-global"))]
        let _ = payload;
    }
    #[cfg(feature = "alloc-decommit")]
    pub(crate) fn begin_reuse(&mut self) -> Option<u64> {
        #[cfg(feature = "alloc-global")]
        if let Some(route) = &self.0 {
            if let Some(generation) = route.begin_large_reuse() {
                return Some(generation);
            }
            let state = route.large_state().unwrap_or_else(|| std::process::abort());
            if crate::alloc_core::segment_header::next_large_generation(state.generation())
                .is_some()
            {
                std::process::abort();
            }
            // Route generation exhausted: retire descriptor and register fresh.
            drop(self.0.take());
        }
        None
    }
    #[cfg_attr(
        not(any(feature = "alloc-decommit", feature = "alloc-xthread")),
        allow(dead_code)
    )]
    pub(crate) fn release_before_unmap(self) {
        // `self` drops here: the registration is unlinked before the caller unmaps.
    }
}
