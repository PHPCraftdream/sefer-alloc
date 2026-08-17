/// Whether this platform's backend actually HONORS the `initial_commit`
/// argument of [`reserve_aligned_lazy`](crate::api::reserve_aligned_lazy) — i.e. whether "lazy" is real here.
///
/// - `true` — the reservation commits exactly `initial_commit` bytes up front
///   and the tail stays reserved-but-uncommitted until you commit it. Touching
///   the tail before committing it faults.
/// - `false` — `initial_commit` is IGNORED and the whole span is committed by
///   the reserve call itself. Committing more is a well-formed no-op, and the
///   tail was writable all along.
///
/// A compile-time property of the backend, not a runtime observation: only the
/// Windows native backend performs a genuine two-phase
/// reserve-then-commit-prefix. The Unix backend has no separate reserve/commit
/// distinction at this granularity and delegates straight to the eager path;
/// miri models no RSS; and the `--cfg aligned_vmem_mock` backend deliberately
/// chains to the eager path, so a mocked (no-op) commit cannot leave the tail
/// unwritable.
///
/// Without this query the difference is invisible from outside the crate — it
/// was previously discoverable only by reading the backend source. It is the
/// third member of the same family as
/// [`Reservation::decommit_reclaims_and_zeroes`](crate::Reservation::decommit_reclaims_and_zeroes) and [`Reservation::is_huge`](crate::Reservation::is_huge):
/// where a platform difference or a best-effort outcome exists, this crate
/// exposes it as something you can branch on instead of a caveat in prose.
///
/// [`LazyReservation`](crate::lazy_reservation::LazyReservation) consults this internally, so its
/// [`committed_len`](crate::lazy_reservation::LazyReservation::committed_len) is already the platform
/// truth — you do not need this query merely to interpret it. Reach for it when
/// the DIFFERENCE itself is the subject: RSS accounting, a benchmark that must
/// not report a no-op as a saving, or deciding whether a lazy reservation buys
/// anything at all on the current target.
#[must_use]
#[cfg(feature = "lazy-commit")]
#[cfg_attr(docsrs, doc(cfg(feature = "lazy-commit")))]
pub const fn lazy_commit_is_honored() -> bool {
    cfg!(all(windows, not(miri), not(aligned_vmem_mock)))
}
