//! Ph3a: the Small-block **issue transaction** witness.
//!
//! [`IssueTransaction`] is the type-level half of the private
//! `prepare -> commit` protocol every Small issuance path already follows:
//! a witness exists only after every fallible sidecar preparation for
//! `(segment_index, base, offset, class)` succeeded (retained promotions are
//! uniform copies a later retry may reuse), and only
//! [`commit`](IssueTransaction::commit) publishes the block. Between the two
//! the caller mutates freelist head, alloc bitmap, bump cursor, live credit,
//! directory bits and its output buffer — the ordering the sidecar audit
//! already found correct, now made explicit by the type system instead of by
//! convention at five call sites.
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
/// only by [`commit`](Self::commit) / [`commit_prepared`](Self::commit_prepared).
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
    #[inline(always)]
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
    #[inline(always)]
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
    #[inline(always)]
    pub(crate) fn commit(self, table: &SegmentTable) {
        #[cfg(debug_assertions)]
        debug_assert_eq!(
            self.index,
            SegmentHeader::segment_id_at(self.base) as usize,
            "commit: witness index must still be its segment's stamped id",
        );
        table.issue_small(self.base, self.offset, usize::from(self.class));
    }

    /// Batch twin of [`commit`](Self::commit) for the whole-batch producers
    /// (`try_drain_freelist_batch`, `try_carve_batch`): those run ALL prepares
    /// before any mutation, so the prepared state for
    /// `(index, base, off, class_idx)` was established by the caller's
    /// preceding prepare loop. The commit re-validates it through the same
    /// aborting guards the scalar path uses — a violation aborts rather than
    /// issuing an unprepared block.
    ///
    /// `index` must be `base`'s stamped segment id; `commit` re-checks it in
    /// debug builds, and `issue_small` re-derives + re-validates it in all.
    #[inline(always)]
    pub(crate) fn commit_prepared(
        table: &SegmentTable,
        index: usize,
        base: *mut u8,
        off: u32,
        class_idx: usize,
    ) {
        Self::prepared(
            index,
            base,
            off,
            u8::try_from(class_idx).unwrap_or_else(|_| std::process::abort()),
        )
        .commit(table)
    }
}
