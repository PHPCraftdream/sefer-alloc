//! Ph3a: the Small-block **issue transaction** witness.
//!
//! [`IssueTransaction`] is the type-level half of the private
//! `prepare -> commit` protocol every Small issuance path already follows:
//! a witness exists only after every fallible sidecar preparation for
//! `(segment_index, base, offset, class)` succeeded (retained promotions are
//! zero-filled copies a later retry may reuse), and only
//! [`commit`](IssueTransaction::commit) publishes the block. Between the two
//! the caller mutates freelist head, alloc bitmap, bump cursor, live credit,
//! directory bits and its output buffer — the ordering the sidecar audit
//! already found correct, now made explicit by the type system instead of by
//! convention at five call sites.
//!
//! # `IssueRun` — the batch twin (B4-R3)
//!
//! [`IssueRun`] is the run-level twin for whole-batch refills (one run = one
//! refill batch in one segment, used by `try_drain_freelist_batch` and
//! `try_carve_batch`). It hoists the per-block prepare/commit chain:
//!
//! - The fatal route guards (live slot, matching root, materialised Small
//!   sidecar) run ONCE per run, in `prepare_small_issue_run`.
//! - Preparedness is a per-LEAF property, not a per-block one: a promoted
//!   256-granule leaf stays promoted for every granule in it, and per-granule
//!   class stores overwrite, so once the leaf backing exists any granule in
//!   it can be written. Therefore the per-block prepare reduces to a
//!   same-leaf check (`RoutedIssueRun::prepare_for`) that only calls the
//!   fallible promotion on a leaf CHANGE — `Err`/rollback OOM semantics are
//!   unchanged (callers still run the whole prepare loop before ANY
//!   mutation, C6).
//! - The per-block commit reduces to one Release class store
//!   (`RoutedIssueRun::commit`); on a leaf CHANGE it re-resolves the leaf,
//!   and a NEVER-promoted leaf aborts (release-mode trap) — a skipped
//!   prepare cannot silently store, mirroring the scalar commit's
//!   abort-on-violation contract.
//! - Borrow soundness: the run borrows the table (`&'a SegmentTable` ->
//!   `&'a RouteSlots` -> the route's sidecar leaves), so the borrow checker
//!   itself excludes route removal / `&mut self` table work for the run's
//!   entire lifetime.
//!
//! The unrouted (standalone, route-less) branch preserves today's semantics
//! exactly: `prepare_for` is an unconditional tautology and `commit` a
//! documented no-op. The producer protocol and its memory orderings (leaf
//! pointer Release-publish, class byte Release store) are untouched.
//!
//! Protocol type only — ADR
//! `docs/design/2026-10-01-adr-physical-boundary-and-progress.md` §Матрица:
//! `IssueTransaction`/`IssueTicket` never become `pub unsafe`; tests observe
//! the protocol through numeric `dbg_*` observers under `bench-internals`.
//! This file is safe code: it owns no memory and dereferences no pointer; the
//! `base` it carries is an address token handed straight back to
//! [`SegmentTable`]'s own guarded accessors (stamped `segment_id` /
//! `base_at` / route-root comparison), which are what make the address sound
//! to use.

use crate::alloc_core::segment::segment_table::segment_table_impl::SegmentTable;
use crate::alloc_core::segment_header::SegmentHeader;

/// Witness that sidecar preparation for `(index, base, offset, class)` is
/// complete. Built only by the prepare side of the protocol —
/// `RouteSlots::prepare_small_issue` under `alloc-global`, or the standalone
/// (route-less) branch of `SegmentTable::prepare_small_issue` — and consumed
/// only by [`commit`](Self::commit).
///
/// Dropping an unconsumed witness is NOT a rollback and NOT an issue: the
/// promotions it stands for are retained by the sidecar and stay valid for a
/// later retry, which is exactly the pre-OOM state every caller's rollback
/// assertion relies on. No `Drop` impl, so a dropped witness cannot silently
/// turn an unissued block into a free one.
#[derive(Debug)]
pub(crate) struct IssueTransaction {
    /// Registry slot of the issuing segment (`segment_id_at(base)`).
    /// [`commit`](Self::commit) re-checks it against the segment's stamped id
    /// in debug builds; release builds leave it documentary because
    /// `SegmentTable::issue_small` re-derives and re-validates it there.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub(super) index: usize,
    /// Segment base the transaction issues into.
    pub(super) base: *mut u8,
    /// Segment-relative block offset.
    pub(super) offset: u32,
    /// Size class of the block.
    pub(super) class: u8,
}

impl IssueTransaction {
    /// Witness for a routed prepare: `index`/`base`/`offset`/`class` passed
    /// `RouteSlots`' guards (live slot, matching root, materialised sidecar)
    /// and the sidecar promotion for them succeeded.
    #[cfg(feature = "alloc-global")]
    #[cfg_attr(not(debug_assertions), inline(always))]
    pub(super) fn prepared(index: usize, base: *mut u8, offset: u32, class: u8) -> Self {
        Self {
            index,
            base,
            offset,
            class,
        }
    }

    /// Witness for a standalone core with NO route: prepare there is a
    /// tautology and `SegmentTable::issue_small` a documented no-op, so only
    /// `base`/`offset` matter — the index is still the segment's real slot,
    /// because a lying one would make `commit`'s witness re-check fire.
    #[cfg_attr(not(debug_assertions), inline(always))]
    pub(super) fn unrouted(base: *mut u8, offset: u32) -> Self {
        Self {
            index: SegmentHeader::segment_id_at(base) as usize,
            base,
            offset,
            class: u8::MAX,
        }
    }

    /// Publish the prepared block. Abort-on-violation is preserved verbatim
    /// from the inline call this replaces: a route root that differs from
    /// `base`, a missing `prepared(offset, class)` bit, or a failed
    /// `issue_small` each abort — a commit cannot silently half-issue.
    #[cfg_attr(not(debug_assertions), inline(always))]
    pub(crate) fn commit(self, table: &SegmentTable) {
        #[cfg(debug_assertions)]
        debug_assert_eq!(
            self.index,
            SegmentHeader::segment_id_at(self.base) as usize,
            "commit: witness index must still be its segment's stamped id",
        );
        table.issue_small(self.base, self.offset, usize::from(self.class));
    }
}

/// Run-level twin of [`IssueTransaction`] for whole-batch refills: see the
/// module docs for the soundness argument behind the per-run / per-leaf
/// hoist. Built only by `SegmentTable::prepare_small_issue_run`; consumed by
/// [`prepare_for`](Self::prepare_for) / [`commit`](Self::commit).
pub(crate) struct IssueRun<'a> {
    /// Registry slot of the issuing segment (`segment_id_at(base)`);
    /// debug-only witness, same contract as `IssueTransaction::index`.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    index: usize,
    /// Segment base the run issues into (debug witness address token).
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    base: *mut u8,
    /// Routed half; `None` for a standalone core with NO route, where the
    /// per-block protocol is a tautology (see `IssueTransaction::unrouted`).
    #[cfg(feature = "alloc-global")]
    routed: Option<super::route_slots::RoutedIssueRun<'a>>,
    /// Ties `'a` to the table borrow when the routed half is compiled out.
    _table: core::marker::PhantomData<&'a ()>,
}

impl<'a> IssueRun<'a> {
    /// Run for a routed refill batch: `index`/`base` passed the fatal route
    /// guards once in `RouteSlots::prepare_small_issue_run`.
    #[cfg(feature = "alloc-global")]
    #[cfg_attr(not(debug_assertions), inline(always))]
    pub(super) fn routed(
        index: usize,
        base: *mut u8,
        inner: super::route_slots::RoutedIssueRun<'a>,
    ) -> Self {
        Self {
            index,
            base,
            routed: Some(inner),
            _table: core::marker::PhantomData,
        }
    }

    /// Run for a standalone core with NO route: per-block prepare is a
    /// tautology and commit a documented no-op; the index is still the
    /// segment's real slot (see `IssueTransaction::unrouted`).
    #[cfg_attr(not(debug_assertions), inline(always))]
    pub(super) fn unrouted(base: *mut u8) -> Self {
        Self {
            index: SegmentHeader::segment_id_at(base) as usize,
            base,
            #[cfg(feature = "alloc-global")]
            routed: None,
            _table: core::marker::PhantomData,
        }
    }

    /// Fallible per-block half. Unrouted: always `true` (today's unrouted
    /// prepare is an unconditional tautology — no bounds checks, preserved
    /// exactly). Routed: leaf-change promotion; `false` is the spill OOM.
    #[cfg_attr(not(debug_assertions), inline(always))]
    pub(crate) fn prepare_for(&mut self, offset: u32) -> bool {
        #[cfg(feature = "alloc-global")]
        if let Some(inner) = &mut self.routed {
            return inner.prepare_for(offset);
        }
        #[cfg(not(feature = "alloc-global"))]
        let _ = offset;
        true
    }

    /// Infallible per-block half: one Release class store on the routed path,
    /// a documented no-op unrouted.
    #[cfg_attr(not(debug_assertions), inline(always))]
    pub(crate) fn commit(&mut self, offset: u32) {
        #[cfg(debug_assertions)]
        debug_assert_eq!(
            self.index,
            SegmentHeader::segment_id_at(self.base) as usize,
            "batch commit: run index must still be its segment's stamped id",
        );
        #[cfg(feature = "alloc-global")]
        if let Some(inner) = &mut self.routed {
            inner.commit(offset);
        }
        #[cfg(not(feature = "alloc-global"))]
        let _ = offset;
    }
}
