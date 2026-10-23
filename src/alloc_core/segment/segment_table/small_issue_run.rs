//! Поблочная выдача через owner-capability проверенного маршрута.

/// Sidecar разрешается один раз; marker сохраняет borrow регистрации !Sync.
pub struct SmallIssueRun<'a> {
    _anchor: core::marker::PhantomData<&'a core::cell::Cell<()>>,
    #[cfg(feature = "alloc-global")]
    sidecar: Option<&'a crate::registry::segment_route::SmallSidecar>,
}

impl<'a> SmallIssueRun<'a> {
    pub(crate) fn unrouted() -> Self {
        Self {
            _anchor: core::marker::PhantomData,
            #[cfg(feature = "alloc-global")]
            sidecar: None,
        }
    }

    #[cfg(feature = "alloc-global")]
    pub(super) fn routed(
        route: &'a crate::registry::segment_route::RouteRegistration<'static>,
    ) -> Self {
        let sidecar = route.small_sidecar().unwrap_or_else(|| {
            crate::alloc_core::fatal::fatal(b"route_slots: small sidecar on non-Small route\n")
        });
        Self {
            _anchor: core::marker::PhantomData,
            sidecar: Some(sidecar),
        }
    }

    #[cfg_attr(not(debug_assertions), inline(always))]
    #[allow(unsafe_code)] // Вызов конструктора seam через owner-only borrow.
    pub(crate) fn issue(&self, offset: u32) {
        #[cfg(feature = "alloc-global")]
        if let Some(sidecar) = self.sidecar {
            // SAFETY: routed привязал lifetime к регистрации !Sync; marker
            // запрещает Send/Sync. Лишь этот owner конструирует и выдаёт блоки.
            unsafe { sidecar.issue(offset) };
        }
        #[cfg(not(feature = "alloc-global"))]
        let _ = offset;
    }

    #[cfg_attr(not(debug_assertions), inline(always))]
    pub(crate) fn clear_pending_start_bit(&self, offset: u32) {
        #[cfg(feature = "alloc-global")]
        if let Some(sidecar) = self.sidecar {
            sidecar.clear_pending_start_bit(offset);
        }
        #[cfg(not(feature = "alloc-global"))]
        let _ = offset;
    }
}
