#[cfg(feature = "alloc-global")]
use crate::registry::segment_route::{GlobalRouteOwner, RouteRegistration};

/// Owner-held registration detached from the active table, not the directory.
/// Without alloc-global this is a ZST and every operation is inert.
pub(crate) struct ParkedRoute(#[cfg(feature = "alloc-global")] Option<GlobalRouteOwner>);

impl ParkedRoute {
    pub(crate) fn is_present(&self) -> bool {
        #[cfg(feature = "alloc-global")]
        {
            self.0.is_some()
        }
        #[cfg(not(feature = "alloc-global"))]
        {
            false
        }
    }

    pub(crate) fn none() -> Self {
        Self(
            #[cfg(feature = "alloc-global")]
            None,
        )
    }
    #[cfg(feature = "alloc-global")]
    pub(super) fn new(route: RouteRegistration<'static>) -> Self {
        Self(Some(route.into_global_owner()))
    }
    #[cfg(feature = "alloc-global")]
    #[inline(always)]
    pub(super) fn take_matching(
        &mut self,
        base: *mut u8,
        payload: *mut u8,
    ) -> Option<RouteRegistration<'static>> {
        let owner = self.0.take()?;
        if owner.matches_identity(base, payload) {
            Some(owner.into_registration())
        } else {
            drop(owner);
            None
        }
    }
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn cache_consumed(&self) {
        #[cfg(feature = "alloc-global")]
        if let Some(owner) = &self.0 {
            owner.with_large_state(|state| {
                if !state.cache_consumed(state.generation()) {
                    std::process::abort();
                }
            });
        }
    }
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn discard_if_identity_changed(&mut self, base: *mut u8, payload: *mut u8) {
        #[cfg(feature = "alloc-global")]
        if self
            .0
            .as_ref()
            .is_some_and(|owner| !owner.matches_identity(base, payload))
        {
            drop(self.0.take());
        }
        #[cfg(not(feature = "alloc-global"))]
        let _ = (base, payload);
    }
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn begin_reuse(&mut self) -> Option<u64> {
        #[cfg(feature = "alloc-global")]
        if let Some(owner) = &self.0 {
            let generation = owner.with_large_state(|state| {
                if let Some(generation) = state.begin_reuse() {
                    return Some(generation);
                }
                if crate::alloc_core::segment_header::next_large_generation(state.generation())
                    .is_some()
                {
                    std::process::abort();
                }
                None
            });
            if generation.is_some() {
                return generation;
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
