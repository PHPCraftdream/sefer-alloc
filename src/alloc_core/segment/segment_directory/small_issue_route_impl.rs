//! R6-T40 OPT05: the borrowed small-issue route context, resolved ONCE per
//! batch. Physically placed beside `RouteSlots`' directory to keep the
//! `segment_table` folder at its baseline entry count; logically this is the
//! `segment_table::small_issue_route` module (declared via `#[path]` in
//! `segment_table/mod.rs`), so `super` below is the `segment_table` module.
//!
//! The routed halves compile only under `alloc-global`; on a standalone core
//! the type exists (the [`SegmentTable::small_issue_route`] proxy signature is
//! configuration-independent) but is never constructed and its methods are
//! no-op stubs — standalone tables have no routes and keep the existing
//! `IssueTransaction`-based fallback at the call sites.

use super::segment_table_impl::SegmentTable;
#[cfg(feature = "alloc-global")]
use crate::alloc_core::fatal::fatal;
#[cfg(feature = "alloc-global")]
use crate::alloc_core::segment_header::SegmentHeader;
#[cfg(feature = "alloc-global")]
use crate::registry::segment_route::{RouteRegistration, SmallSidecar};

/// Borrowed owner-local context: one resolved route plus its materialised
/// sidecar, cached so a batch's per-block `prepare`/`commit` calls skip the
/// per-block slot resolution. Constructed ONLY by
/// `RouteSlots::small_issue_route` after the same guards the scalar prepare
/// has always enforced (live slot, matching root, present sidecar).
#[cfg(feature = "alloc-global")]
pub(crate) struct SmallIssueRoute<'a> {
    pub(in crate::alloc_core::segment::segment_table) route: &'a RouteRegistration<'static>,
    pub(in crate::alloc_core::segment::segment_table) sidecar: &'a SmallSidecar,
}

/// Standalone twin: never constructed (no routes exist), present only so the
/// proxy's return type is defined in every alloc-core configuration.
#[cfg(not(feature = "alloc-global"))]
pub(crate) struct SmallIssueRoute<'a> {
    _never: core::marker::PhantomData<&'a ()>,
}

impl SmallIssueRoute<'_> {
    /// Prepare half: run the sidecar promotion for `(offset, class)`.
    /// `false` is the uncommitted spill OOM. A failed promotion WITHOUT a
    /// sidecar is the same broken invariant the scalar path aborts on —
    /// identical fatal bytes.
    #[cfg(feature = "alloc-global")]
    #[inline]
    pub(crate) fn prepare(&self, offset: u32, class: u8) -> bool {
        let prepared = self.route.prepare_small(offset, class);
        if !prepared && self.route.small_sidecar().is_none() {
            fatal(b"route_slots: prepare_small failed without sidecar\n");
        }
        prepared
    }

    #[cfg(not(feature = "alloc-global"))]
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
    #[inline]
    pub(crate) fn prepare(&self, _offset: u32, _class: u8) -> bool {
        true
    }

    /// Commit half: publish one prepared block. Byte-identical to the
    /// scalar `RouteSlots::issue_small` checks — the sidecar's
    /// `prepared(offset, class)` bit must hold and `issue_small` must
    /// succeed; any violation aborts with the existing commit fatal bytes.
    #[cfg(feature = "alloc-global")]
    #[inline]
    pub(crate) fn commit(&self, offset: u32, class: u8) {
        if !self.sidecar.prepared(offset, class) || !self.route.issue_small(offset, class) {
            fatal(b"route_slots: small issue commit rejected\n");
        }
    }

    #[cfg(not(feature = "alloc-global"))]
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
    #[inline]
    pub(crate) fn commit(&self, _offset: u32, _class: u8) {}
}

impl SegmentTable {
    /// R6-T40 OPT05: resolve the owner's small route ONCE per batch.
    /// `None` = standalone (route-less) table: callers keep the existing
    /// `IssueTransaction` unrouted fallback. Routed tables validate `base`
    /// against the table slot (same `base_at` guard and fatal bytes as the
    /// scalar prepare) and delegate to `RouteSlots::small_issue_route`.
    #[cfg_attr(not(debug_assertions), inline(always))]
    pub(crate) fn small_issue_route(&self, base: *mut u8) -> Option<SmallIssueRoute<'_>> {
        #[cfg(feature = "alloc-global")]
        if let Some(routes) = &self.routes {
            let index = SegmentHeader::segment_id_at(base) as usize;
            if self.base_at(index) != base {
                fatal(b"segment_table: prepare base is not the segment base\n");
            }
            return Some(routes.small_issue_route(index, base));
        }
        let _ = base;
        None
    }
}
