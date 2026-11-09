//! Current-runtime Auto phase/mixed windows with independent own-process OS epochs.
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]
use super::control_snapshot::ControlSnapshot;
use super::measured_window::MeasuredWindow;
use super::os_probe::OsProbe;
use super::owner_training::OwnerTraining;
use super::payload_actors::PayloadActors;
use super::run_stats::RunStats;
use super::window_execution::WindowExecution;
use super::window_kernel::WindowKernel;
use super::workload::{self, Cell, Phase, Shape, TouchPlan, MAX_WORKER_LIVE_BYTES};
use sefer_alloc::{AllocCore, LargeThpMode, SeferAlloc};
use std::io;
use std::sync::Mutex;
use std::time::Instant;

fn error(message: &'static str) -> io::Error {
    io::Error::other(message)
}
fn elapsed(start: Instant) -> io::Result<u64> {
    u64::try_from(start.elapsed().as_nanos()).map_err(|_| error("window elapsed overflow"))
}
fn configured(allocator: &SeferAlloc, cell: &Cell) -> io::Result<usize> {
    let tuple = AllocCore::dbg_thp_tuple();
    let cache = allocator
        .dbg_current_large_cache_budget()
        .ok_or_else(|| error("window actor cache readback missing"))?;
    if cache != 0
        || tuple.mode != LargeThpMode::Auto
        || tuple.min_bytes != cell.minimum_request
        || AllocCore::dbg_thp_tuple_conflicts() != 0
        || allocator.stats().config_conflicts != 0
    {
        return Err(error("window actor configuration or conflict mismatch"));
    }
    Ok(cache)
}
fn plans(cell: &Cell) -> (Vec<TouchPlan>, Vec<TouchPlan>) {
    let primary = vec![
        TouchPlan {
            bytes: cell.bytes,
            shape: cell.shape
        };
        cell.blocks
    ];
    let secondary = cell
        .co_resident
        .map_or_else(Vec::new, |(shape, bytes, blocks)| {
            vec![TouchPlan { bytes, shape }; blocks]
        });
    (primary, secondary)
}

/// Sole payload allocator, System bookkeeping and quiescent process boundaries are
/// caller premises. Does not admit runtime rows or support historical/static arms.
pub(crate) struct AdaptiveWindows;

impl AdaptiveWindows {
    pub(crate) fn run(
        allocator: &SeferAlloc,
        scenario: &str,
        page: usize,
    ) -> io::Result<Vec<MeasuredWindow>> {
        let cells = workload::plan(scenario).map_err(error)?;
        Self::execute(allocator, &cells, page)
    }

    /// Same path for CI-sized geometry. Every result records the actual cell and
    /// flags any departure from the full registered plan; never campaign evidence.
    pub(crate) fn execute(
        allocator: &SeferAlloc,
        cells: &[Cell],
        page: usize,
    ) -> io::Result<Vec<MeasuredWindow>> {
        if !cfg!(all(target_os = "linux", not(miri))) {
            return Err(error("adaptive windows require native Linux OS epochs"));
        }
        let first = cells
            .first()
            .ok_or_else(|| error("empty adaptive scenario"))?;
        if !matches!(first.scenario, "phased" | "mixed") {
            return Err(error("unsupported adaptive scenario"));
        }
        let canonical = workload::plan(first.scenario).map_err(error)?;
        if cells.len() != canonical.len() {
            return Err(error("incomplete adaptive window sequence"));
        }
        for (cell, reference) in cells.iter().zip(&canonical) {
            // Only actual request geometry may differ for bounded CI fixtures.
            let mut normalized = *cell;
            normalized.bytes = reference.bytes;
            normalized.blocks = reference.blocks;
            normalized.bucket = reference.bucket;
            normalized.minimum_request = reference.minimum_request;
            normalized.co_resident = match (cell.co_resident, reference.co_resident) {
                (Some((shape, _, _)), Some((_, bytes, blocks))) => Some((shape, bytes, blocks)),
                (None, None) => None,
                _ => return Err(error("adaptive co-resident shape mismatch")),
            };
            if normalized != *reference || cell.blocks == 0 || cell.blocks > 128 {
                return Err(error("adaptive window ordering or shape mismatch"));
            }
            if let Some((_, _, blocks)) = cell.co_resident {
                if blocks == 0 || blocks > 128 {
                    return Err(error("invalid co-resident block count"));
                }
            }
            let (mut primary, secondary) = plans(cell);
            primary.extend(secondary);
            WindowKernel::validate(&primary, page)?;
            if cell.minimum_request > cell.bytes
                || cell
                    .co_resident
                    .is_some_and(|(_, bytes, _)| cell.minimum_request > bytes)
            {
                return Err(error("adaptive request is below its configured minimum"));
            }
            if matches!(cell.phase, Phase::TrainSparse | Phase::TrainDense)
                && cell
                    .bytes
                    .checked_mul(cell.blocks)
                    .is_none_or(|live| live > MAX_WORKER_LIVE_BYTES)
            {
                return Err(error("training geometry exceeds registered live bound"));
            }
        }
        let dense = cells
            .iter()
            .find(|cell| cell.phase == Phase::TrainDense)
            .ok_or_else(|| error("dense training window missing"))?;
        let sparse = cells
            .iter()
            .find(|cell| cell.phase == Phase::TrainSparse)
            .ok_or_else(|| error("sparse training window missing"))?;
        let group = |bytes: usize| -> io::Result<sefer_alloc::LargeThpRequestGroup> {
            let bytes =
                u64::try_from(bytes).map_err(|_| error("window request conversion overflow"))?;
            Ok(sefer_alloc::LargeThpRequestGroup::of_bytes(bytes))
        };
        let dense_group = group(dense.bytes)?;
        let sparse_group = group(sparse.bytes)?;
        if first.scenario == "mixed" && dense_group == sparse_group
            || first.scenario == "phased"
                && (dense.bytes != sparse.bytes || dense.bucket != sparse.bucket)
        {
            return Err(error("adaptive training group relationship mismatch"));
        }
        for cell in cells {
            let trained = match cell.phase {
                Phase::TrainSparse | Phase::TrainedSparse => sparse,
                Phase::Measure | Phase::TrainDense | Phase::TrainedDense | Phase::CoResident => {
                    dense
                }
            };
            if cell.bytes != trained.bytes
                || cell.bucket != trained.bucket
                || cell.minimum_request != dense.minimum_request
                || cell
                    .co_resident
                    .is_some_and(|(_, bytes, _)| bytes != sparse.bytes)
            {
                return Err(error(
                    "adaptive measurement does not match its training geometry",
                ));
            }
        }
        let registered_geometry = cells == canonical.as_slice();
        let mut windows = Vec::with_capacity(cells.len());
        for cell in cells {
            let full = Instant::now();
            let os = OsProbe::begin()?;
            let (execution, live) = match cell.phase {
                Phase::TrainSparse | Phase::TrainDense => {
                    let run = OwnerTraining::run(allocator, cell.bytes, cell.shape, page)?;
                    if AllocCore::dbg_thp_tuple().min_bytes != cell.minimum_request {
                        return Err(error("training window resolved minimum mismatch"));
                    }
                    (WindowExecution::Training(run), None)
                }
                Phase::Measure | Phase::TrainedSparse | Phase::TrainedDense | Phase::CoResident => {
                    let (run, live) = Self::batch(allocator, cell, page, &os)?;
                    (run, Some(live))
                }
            };
            // Both executors join every actor before the terminal process probes.
            let os = os.finish()?;
            windows.push(MeasuredWindow {
                cell: *cell,
                registered_geometry,
                os,
                live,
                full_window_ns: elapsed(full)?,
                execution,
            });
        }
        Ok(windows)
    }

    fn batch(
        allocator: &SeferAlloc,
        cell: &Cell,
        page: usize,
        os: &OsProbe,
    ) -> io::Result<(WindowExecution, ControlSnapshot)> {
        let wall = Instant::now();
        let cache_readbacks = Mutex::new([None; 2]);
        let (mut result, live) = PayloadActors::with(
            allocator,
            2,
            page,
            |actor, actual| {
                let cache = configured(actual, cell)?;
                cache_readbacks
                    .lock()
                    .map_err(|_| error("window cache readback poisoned"))?[actor] = Some(cache);
                Ok(())
            },
            |actors| {
                let mut setup_stats = RunStats::default();
                let setup_ordinal = AllocCore::dbg_thp_control_ordinal();
                let mut threads = [None; 2];
                for (actor, thread) in threads.iter_mut().enumerate() {
                    let (stats, actual) = actors.allocate(
                        actor,
                        &[TouchPlan {
                            bytes: 32,
                            shape: Shape::Dense,
                        }],
                    )?;
                    *thread = Some(actual);
                    setup_stats.merge(stats)?;
                    setup_stats.merge(actors.free(actor)?.0)?;
                }
                if AllocCore::dbg_thp_control_ordinal() != setup_ordinal {
                    return Err(error("window setup reached an eligible Large path"));
                }
                let actor_threads = match threads {
                    [Some(a), Some(b)] if a != b => [a, b],
                    _ => return Err(error("window actor identities missing or duplicated")),
                };
                let cache_budgets = match *cache_readbacks
                    .lock()
                    .map_err(|_| error("window cache readback poisoned"))?
                {
                    [Some(a), Some(b)] => [a, b],
                    _ => return Err(error("window actor cache readback missing")),
                };
                let tuple = AllocCore::dbg_thp_tuple().pack();
                let before = ControlSnapshot::capture();
                let (primary, secondary) = plans(cell);
                let batches = [&primary, &secondary];
                let mut payload_stats = RunStats::default();
                for (actor, batch) in batches
                    .iter()
                    .enumerate()
                    .filter(|(_, batch)| !batch.is_empty())
                {
                    let (stats, thread) = actors.allocate(actor, batch)?;
                    if thread != actor_threads[actor] {
                        return Err(error("window allocation actor changed"));
                    }
                    payload_stats.merge(stats)?;
                }
                for (actor, _) in batches
                    .iter()
                    .enumerate()
                    .filter(|(_, batch)| !batch.is_empty())
                {
                    let (stats, thread) = actors.touch(actor)?;
                    if thread != actor_threads[actor] {
                        return Err(error("window touch actor changed"));
                    }
                    payload_stats.merge(stats)?;
                }
                // All batches remain live and all actor commands are acknowledged.
                // This is process RSS, including both buckets in co-resident windows.
                let live = ControlSnapshot::capture();
                if live.ordinal.checked_sub(before.ordinal) != Some(payload_stats.allocations)
                    || live.learning != before.learning
                    || (0..6).any(|group| live.controls[group][1..] != before.controls[group][1..])
                {
                    return Err(error("window payload retired before live OS probes"));
                }
                let (live_rss_bytes, live_anon_huge_pages_bytes) = os.live_memory()?;
                for (actor, _) in batches
                    .iter()
                    .enumerate()
                    .rev()
                    .filter(|(_, batch)| !batch.is_empty())
                {
                    let (stats, thread) = actors.free(actor)?;
                    if thread != actor_threads[actor] {
                        return Err(error("window retirement actor changed"));
                    }
                    payload_stats.merge(stats)?;
                }
                let after = ControlSnapshot::capture();
                if after.ordinal.checked_sub(before.ordinal) != Some(payload_stats.allocations)
                    || AllocCore::dbg_thp_tuple().pack() != tuple
                    || AllocCore::dbg_thp_tuple_conflicts() != 0
                    || allocator.stats().config_conflicts != 0
                {
                    return Err(error("window terminal eligibility/configuration mismatch"));
                }
                Ok((
                    WindowExecution::Batch {
                        setup_stats,
                        payload_stats,
                        executor_wall_ns: 0,
                        resolved_tuple_words: tuple,
                        cache_budgets,
                        actor_threads,
                        before,
                        after,
                        live_rss_bytes,
                        live_anon_huge_pages_bytes,
                    },
                    live,
                ))
            },
        )?;
        if let WindowExecution::Batch {
            after,
            executor_wall_ns,
            ..
        } = &mut result
        {
            if ControlSnapshot::capture() != *after {
                return Err(error("window shutdown changed terminal diagnostics"));
            }
            *executor_wall_ns = elapsed(wall)?;
        }
        Ok((result, live))
    }
}
