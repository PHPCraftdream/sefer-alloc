//! Actual payload execution evidence; no accepted-advice lifetime is inferred.
use super::control_snapshot::ControlSnapshot;
use super::run_stats::RunStats;
use super::training_run::TrainingRun;
use std::thread::ThreadId;

#[derive(Debug)]
pub(crate) enum WindowExecution {
    Training(TrainingRun),
    Batch {
        setup_stats: RunStats,
        payload_stats: RunStats,
        /// Actor startup/setup, live probes, payload operations and joined shutdown.
        executor_wall_ns: u64,
        resolved_tuple_words: [u64; 5],
        cache_budgets: [usize; 2],
        actor_threads: [ThreadId; 2],
        before: ControlSnapshot,
        after: ControlSnapshot,
        /// Whole-process readings while all batches are live, not bucket attribution.
        live_rss_bytes: u64,
        live_anon_huge_pages_bytes: u64,
    },
}
