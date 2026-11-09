//! Independent diagnostic loads; only caller-established quiescence closes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ControlSnapshot {
    pub ordinal: u64,
    pub controls: [[u64; 8]; 6],
    pub learning: [[u64; 5]; 6],
}

impl ControlSnapshot {
    #[cfg(all(
        feature = "alloc-core",
        feature = "alloc-decommit",
        feature = "bench-internals",
        not(feature = "numa-aware")
    ))]
    pub(crate) fn capture() -> Self {
        Self {
            ordinal: sefer_alloc::AllocCore::dbg_thp_control_ordinal(),
            controls: sefer_alloc::AllocCore::dbg_thp_control_telemetry(),
            learning: sefer_alloc::AllocCore::dbg_thp_learning_telemetry(),
        }
    }
}
