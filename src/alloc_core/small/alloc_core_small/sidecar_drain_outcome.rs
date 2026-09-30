/// Reservation disposition after one bounded owner-side sidecar cut.
pub(super) enum SidecarDrainOutcome {
    Skipped,
    #[cfg(feature = "alloc-decommit")]
    Decommitted {
        pooled: bool,
    },
    Drained,
}
