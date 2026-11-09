//! Training executor results, not admitted runtime rows or campaign measurements.
use super::control_snapshot::ControlSnapshot;
use super::run_stats::RunStats;
use super::training_observation::TrainingObservation;
use super::workload::{Shape, TrainingState};

#[derive(Debug)]
pub(crate) struct TrainingRun {
    pub requested_bytes: usize,
    pub shape: Shape,
    pub group: usize,
    pub resolved_tuple_words: [u64; 5],
    pub cache_budgets: [usize; 2],
    pub state: TrainingState,
    pub setup_stats: RunStats,
    pub payload_stats: RunStats,
    /// Includes actor startup/setup/shutdown, not outer OS probes or child time.
    pub wall_ns: u64,
    pub before: ControlSnapshot,
    pub after: ControlSnapshot,
    pub observations: Vec<TrainingObservation>,
}
