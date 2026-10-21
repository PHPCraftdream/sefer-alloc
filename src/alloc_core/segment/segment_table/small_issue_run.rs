//! D1′ staged small-issue run: per-run slot validation, cheap per-block ops.

/// D1′ staged issue handle. The per-run fatal guards (route-slot index in
/// range, slot occupied, root == base, Small sidecar present, and the cheap
/// `base_at(segment_id_at(base)) == base` integrity check) run ONCE in
/// `SegmentTable::small_issue_run`;
/// the per-block operations below perform only the cheap sidecar work
/// (carve-clear pending RMW, single-writer start-bit load→or→Release store)
/// exactly at each block's own commit point — never merged across the run
/// (d1p_rev finding 3). Unrouted (standalone) cores get a no-op run, exactly
/// like the former per-issue no-op.
pub struct SmallIssueRun<'a> {
    // Zero-cost: keeps `'a` meaningful on the no-routes build (loom /
    // standalone) where the routed sidecar reference is compiled out.
    _anchor: core::marker::PhantomData<&'a ()>,
    #[cfg(feature = "alloc-global")]
    sidecar: Option<&'a crate::registry::segment_route::SmallSidecar>,
}

impl<'a> SmallIssueRun<'a> {
    /// No-op run for a standalone (route-less) `SegmentTable`.
    pub(crate) fn unrouted() -> Self {
        Self {
            _anchor: core::marker::PhantomData,
            #[cfg(feature = "alloc-global")]
            sidecar: None,
        }
    }

    /// Routed run: the caller has already run the fatal slot guards
    /// (`RouteSlots::small_issue_run`).
    #[cfg(feature = "alloc-global")]
    pub(super) fn routed(sidecar: &'a crate::registry::segment_route::SmallSidecar) -> Self {
        Self {
            _anchor: core::marker::PhantomData,
            sidecar: Some(sidecar),
        }
    }

    /// Per-block issue: set the record's "start was issued" bit at this
    /// block's own commit point (infallible; the class lives only in the
    /// owner state byte written next to it). The out-of-segment /
    /// misalignment fatal guard stays per block inside the sidecar.
    #[cfg_attr(not(debug_assertions), inline(always))]
    pub(crate) fn issue(&self, offset: u32) {
        #[cfg(feature = "alloc-global")]
        if let Some(sidecar) = self.sidecar {
            sidecar.issue(offset);
        }
        #[cfg(not(feature = "alloc-global"))]
        let _ = offset;
    }

    /// Per-block carve-clear (design §2.4): drop the fresh block's start
    /// granule's pending bit before its LIVE write. Only the RMW form is
    /// used, exactly as before.
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
