//! Actual owner-return training; registered targets/caps are never shortened.
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]
use super::control_interval::ControlInterval;
use super::control_snapshot::ControlSnapshot;
use super::payload_actors::PayloadActors;
use super::run_stats::RunStats;
use super::training_observation::TrainingObservation;
use super::training_run::TrainingRun;
use super::window_kernel::WindowKernel;
use super::workload::{
    Shape, TouchPlan, Training, TrainingState, MAX_WORKER_LIVE_BYTES, TRAINING_CAP,
};
use sefer_alloc::{AllocCore, LargeThpMode, LargeThpRequestGroup, SeferAlloc};
use std::io;
use std::time::Instant;

fn error(message: &'static str) -> io::Error {
    io::Error::other(message)
}
fn counts(state: TrainingState) -> (usize, usize) {
    match state {
        TrainingState::Collecting { eligible, valid }
        | TrainingState::Complete {
            eligible, valid, ..
        }
        | TrainingState::Insufficient {
            eligible, valid, ..
        }
        | TrainingState::InvalidShare { eligible, valid } => (eligible, valid),
    }
}
fn fraction(numerator: u64, denominator: u64) -> io::Result<f64> {
    let numerator = u32::try_from(numerator).map_err(|_| error("bounded numerator overflow"))?;
    let denominator =
        u32::try_from(denominator).map_err(|_| error("bounded denominator overflow"))?;
    if denominator == 0 || numerator > denominator {
        return Err(error("invalid bounded fraction"));
    }
    Ok(f64::from(numerator) / f64::from(denominator))
}
fn group_of(bytes: usize) -> io::Result<usize> {
    let bytes = u64::try_from(bytes).map_err(|_| error("request conversion overflow"))?;
    match LargeThpRequestGroup::of_bytes(bytes) {
        LargeThpRequestGroup::Under1MiB => Ok(0),
        LargeThpRequestGroup::MiB1To8 => Ok(1),
        LargeThpRequestGroup::MiB8To32 => Ok(2),
        LargeThpRequestGroup::MiB32To128 => Ok(3),
        LargeThpRequestGroup::MiB128To512 => Ok(4),
        LargeThpRequestGroup::Over512MiB => Ok(5),
        _ => Err(error("unsupported request group")),
    }
}

/// Current-runtime Auto backend only; not a historical or static-reference adapter.
/// Caller supplies the sole payload allocator; process bookkeeping must use System.
pub(crate) struct OwnerTraining;

impl OwnerTraining {
    pub(crate) fn run(
        allocator: &SeferAlloc,
        bytes: usize,
        shape: Shape,
        page: usize,
    ) -> io::Result<TrainingRun> {
        if !cfg!(any(target_os = "linux", target_os = "android")) || cfg!(miri) {
            return Err(error("owner training requires native Linux residency"));
        }
        let sparse = match shape {
            Shape::Dense => false,
            Shape::SparseFivePercent | Shape::SparseTwoPercent => true,
            _ => return Err(error("unsupported registered training class")),
        };
        WindowKernel::validate(&[TouchPlan { bytes, shape }], page)?;
        if bytes > MAX_WORKER_LIVE_BYTES {
            return Err(error("training live payload exceeds 512 MiB"));
        }
        let group = group_of(bytes)?;
        let wall = Instant::now();
        let cache_readbacks = std::sync::Mutex::new([None; 2]);
        let mut result = PayloadActors::with(
            allocator,
            2,
            page,
            |actor, actual| {
                let tuple = AllocCore::dbg_thp_tuple();
                let cache = actual
                    .dbg_current_large_cache_budget()
                    .ok_or_else(|| error("training actor has no resolved cache configuration"))?;
                if cache != 0
                    || tuple.mode != LargeThpMode::Auto
                    || tuple.min_bytes > bytes
                    || AllocCore::dbg_thp_tuple_conflicts() != 0
                    || actual.stats().config_conflicts != 0
                {
                    return Err(error("training worker configuration or conflict mismatch"));
                }
                cache_readbacks
                    .lock()
                    .map_err(|_| error("training cache readback poisoned"))?[actor] = Some(cache);
                Ok(())
            },
            |actors| {
                let mut setup_stats = RunStats::default();
                let setup_ordinal = AllocCore::dbg_thp_control_ordinal();
                // Count both real 32-byte claims; no hidden priming or competing config.
                for actor in 0..2 {
                    setup_stats.merge(
                        actors
                            .allocate(
                                actor,
                                &[TouchPlan {
                                    bytes: 32,
                                    shape: Shape::Dense,
                                }],
                            )?
                            .0,
                    )?;
                    setup_stats.merge(actors.free(actor)?.0)?;
                }
                if AllocCore::dbg_thp_control_ordinal() != setup_ordinal {
                    return Err(error("setup claims reached an eligible Large path"));
                }
                let cache_budgets = match *cache_readbacks
                    .lock()
                    .map_err(|_| error("training cache readback poisoned"))?
                {
                    [Some(a), Some(b)] => [a, b],
                    _ => return Err(error("training actor cache readback missing")),
                };
                let resolved_tuple_words = AllocCore::dbg_thp_tuple().pack();
                let before = ControlSnapshot::capture();
                let mut after = before;
                let mut payload_stats = RunStats::default();
                let mut training = Training::new(sparse);
                let mut state = TrainingState::Collecting {
                    eligible: 0,
                    valid: 0,
                };
                // An arbitrary interval can intersect two partial global cohorts.
                let record_cap = TRAINING_CAP / 8 + 1;
                let mut observations = Vec::new();
                observations
                    .try_reserve_exact(record_cap)
                    .map_err(|_| error("training evidence allocation failed"))?;
                let mut last_selected_cohort = None;
                for step in 0..TRAINING_CAP {
                    let owner = step % 2;
                    let toucher = 1 - owner;
                    // Successful synchronous acknowledgements leave both actors idle:
                    // no payload is live and no ticket writer can race this reset.
                    AllocCore::dbg_thp_control_ticket_log_reset();
                    let start = ControlSnapshot::capture();
                    if start != after {
                        return Err(error("training diagnostics changed between requests"));
                    }
                    let (allocated, owner_thread) =
                        actors.allocate(owner, &[TouchPlan { bytes, shape }])?;
                    if allocated.allocations != 1 {
                        return Err(error("training request did not allocate exactly once"));
                    }
                    payload_stats.merge(allocated)?;
                    let (from, touch_thread) = actors.transfer(owner, toucher)?;
                    if from != owner_thread || touch_thread == owner_thread {
                        return Err(error("training ownership transfer did not change actor"));
                    }
                    let (touched, actual_toucher) = actors.touch(toucher)?;
                    if actual_toucher != touch_thread {
                        return Err(error("training touch actor changed"));
                    }
                    payload_stats.merge(touched)?;
                    if actors.transfer(toucher, owner)? != (touch_thread, owner_thread) {
                        return Err(error(
                            "training payload did not return to its original actor",
                        ));
                    }
                    let (freed, actual_owner) = actors.free(owner)?;
                    if actual_owner != owner_thread || freed.frees != 1 {
                        return Err(error("training retirement did not run on its owner"));
                    }
                    payload_stats.merge(freed)?;
                    after = ControlSnapshot::capture();
                    let (tickets, cursor) = AllocCore::dbg_thp_control_ticket_log()
                        .ok_or_else(|| error("unsupported training ticket capture"))?;
                    if cursor != 1
                        || ((tickets[0] >> 1) & 1 != 0)
                            != AllocCore::dbg_thp_control_decision_at(tickets[0] >> 2)
                    {
                        return Err(error("training actual ticket or selection mismatch"));
                    }
                    let interval =
                        ControlInterval::close_request(group, page, &start, &after, &tickets[..1])?;
                    if AllocCore::dbg_thp_tuple().pack() != resolved_tuple_words
                        || AllocCore::dbg_thp_tuple_conflicts() != 0
                        || allocator.stats().config_conflicts != 0
                    {
                        return Err(error("training configuration changed during a request"));
                    }
                    let observation = if interval.reasons[1] == 1 {
                        Some(fraction(interval.resident_pages, interval.sampled_pages)?)
                    } else {
                        None
                    };
                    let share = fraction(interval.target_share_q31, 1 << 31)?;
                    let previous = counts(state);
                    state = training.observe(true, observation, Some(share));
                    let current = counts(state);
                    if current.0 != step + 1
                        || current.1.checked_sub(previous.1)
                            != Some(usize::from(interval.class_valid(sparse)))
                        || matches!(state, TrainingState::InvalidShare { .. })
                    {
                        return Err(error("training class/count conversion failed closure"));
                    }
                    if tickets[0] & 2 != 0 {
                        let cohort = (tickets[0] >> 2) / 8;
                        if last_selected_cohort == Some(cohort) || observations.len() >= record_cap
                        {
                            return Err(error(
                                "training selected more than one control per cohort",
                            ));
                        }
                        last_selected_cohort = Some(cohort);
                        observations.push(TrainingObservation {
                            eligible_position: step + 1,
                            ticket: tickets[0],
                            owner_thread,
                            touch_thread,
                            e_q31: after.learning[group][0],
                            m_q31: after.learning[group][1],
                            interval,
                        });
                    }
                    if matches!(
                        state,
                        TrainingState::Complete { .. } | TrainingState::Insufficient { .. }
                    ) {
                        break;
                    }
                }
                if matches!(
                    state,
                    TrainingState::Collecting { .. } | TrainingState::InvalidShare { .. }
                ) {
                    return Err(error(
                        "training failed to reach a declared terminal criterion",
                    ));
                }
                Ok(TrainingRun {
                    requested_bytes: bytes,
                    shape,
                    group,
                    resolved_tuple_words,
                    cache_budgets,
                    state,
                    setup_stats,
                    payload_stats,
                    wall_ns: 0,
                    before,
                    after,
                    observations,
                })
            },
        )?;
        if ControlSnapshot::capture() != result.after {
            return Err(error(
                "training actor shutdown changed terminal diagnostics",
            ));
        }
        result.wall_ns = u64::try_from(wall.elapsed().as_nanos())
            .map_err(|_| error("training executor wall time overflow"))?;
        Ok(result)
    }
}
