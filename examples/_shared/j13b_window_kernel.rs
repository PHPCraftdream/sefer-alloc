//! Real GlobalAlloc payload operations; no OS/model observations are fabricated.
use super::owned::OwnedBlock;
use super::run_stats::RunStats;
use super::workload::{touch_slots_with_page, Shape, TouchPlan, MAX_LIVE_BLOCKS, MAX_LIVE_BYTES};
use std::alloc::GlobalAlloc;
use std::io;
use std::time::Instant;

fn error(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn nanos(start: Instant) -> io::Result<u64> {
    u64::try_from(start.elapsed().as_nanos()).map_err(|_| error("elapsed nanoseconds overflow"))
}

fn number(value: usize) -> io::Result<u64> {
    u64::try_from(value).map_err(|_| error("requested byte/count overflow"))
}

/// Internal operation executor. Its allocator is the real payload receiver;
/// the eventual child uses System for bookkeeping, matching the original judge.
pub(crate) struct WindowKernel;

impl WindowKernel {
    pub(crate) fn validate(plans: &[TouchPlan], page: usize) -> io::Result<()> {
        if !page.is_power_of_two() || plans.is_empty() || plans.len() > MAX_LIVE_BLOCKS {
            return Err(error("invalid page geometry or live block count"));
        }
        let mut live = 0usize;
        for plan in plans {
            OwnedBlock::<std::alloc::System>::checked_layout(plan.bytes).map_err(error)?;
            live = live
                .checked_add(plan.bytes)
                .ok_or_else(|| error("live byte overflow"))?;
            if plan.shape == Shape::EdgeTwoPages
                && !page
                    .checked_mul(2)
                    .is_some_and(|minimum| plan.bytes >= minimum)
            {
                return Err(error("edge payload must contain two runtime pages"));
            }
            if matches!(plan.shape, Shape::Tail | Shape::Quarter) && plan.bytes < 4
                || plan.shape == Shape::SparseFivePercent && plan.bytes < 20
                || plan.shape == Shape::SparseTwoPercent && plan.bytes < 50
            {
                return Err(error("registered touch shape would write no bytes"));
            }
        }
        if live > MAX_LIVE_BYTES {
            return Err(error("aggregate requested live bytes exceed bound"));
        }
        Ok(())
    }

    pub(crate) fn touch<A: GlobalAlloc + Sync>(
        block: &mut OwnedBlock<'_, A>,
        from: usize,
        shape: Shape,
        page: usize,
    ) -> io::Result<usize> {
        let slots = block.slots();
        let tail = slots
            .get_mut(from..)
            .ok_or_else(|| error("touch start outside payload"))?;
        let written = touch_slots_with_page(tail, shape, page);
        // A benchmark compiler barrier keeps the actual mutable payload visible;
        // optimized native activation still needs a separate real-path oracle.
        std::hint::black_box(tail);
        if written == 0 {
            return Err(error("touch operation produced no writes"));
        }
        Ok(written)
    }

    /// Hold every validated block live, then touch and free in reverse order.
    pub(crate) fn batch<A: GlobalAlloc + Sync>(
        allocator: &A,
        plans: &[TouchPlan],
        page: usize,
    ) -> io::Result<RunStats> {
        Self::batch_observed(allocator, plans, page, |_| Ok(()))
    }

    fn batch_observed<A, P>(
        allocator: &A,
        plans: &[TouchPlan],
        page: usize,
        after_allocate: P,
    ) -> io::Result<RunStats>
    where
        A: GlobalAlloc + Sync,
        P: FnOnce(&A) -> io::Result<()>,
    {
        Self::validate(plans, page)?;
        let wall = Instant::now();
        let mut stats = RunStats::default();
        let mut blocks = Vec::new();
        blocks
            .try_reserve_exact(plans.len())
            .map_err(|_| error("batch metadata allocation failed"))?;
        for plan in plans {
            let start = Instant::now();
            let block = OwnedBlock::allocate(allocator, plan.bytes).map_err(error)?;
            let elapsed = nanos(start)?;
            stats.merge(RunStats {
                alloc_ns: elapsed,
                allocations: 1,
                requested_bytes_issued: number(plan.bytes)?,
                ..RunStats::default()
            })?;
            blocks.push(block);
        }
        // Resolution must be read on the actual worker after its allocator
        // claim exists, not inferred from an unbound thread's empty diagnostic.
        after_allocate(allocator)?;
        for (block, plan) in blocks.iter_mut().zip(plans) {
            let start = Instant::now();
            let written = Self::touch(block, 0, plan.shape, page)?;
            stats.merge(RunStats {
                touch_ns: nanos(start)?,
                bytes_written: number(written)?,
                ..RunStats::default()
            })?;
        }
        while let Some(block) = blocks.pop() {
            let start = Instant::now();
            drop(block);
            stats.merge(RunStats {
                free_ns: nanos(start)?,
                frees: 1,
                ..RunStats::default()
            })?;
        }
        drop(blocks);
        stats.wall_ns = nanos(wall)?;
        Ok(stats)
    }

    /// Four-worker registered churn uses real operations on each worker. Every
    /// started thread is explicitly joined before any worker error is returned.
    pub(crate) fn churn<A, P>(
        allocator: &A,
        plan: TouchPlan,
        workers: usize,
        operations_per_worker: usize,
        page: usize,
        preflight: P,
    ) -> io::Result<RunStats>
    where
        A: GlobalAlloc + Sync,
        P: Fn(usize, &A) -> io::Result<()> + Sync,
    {
        if workers == 0 || workers > 4 || operations_per_worker == 0 || operations_per_worker > 4096
        {
            return Err(error(
                "churn worker/operation count outside registered bounds",
            ));
        }
        Self::validate(&vec![plan; workers], page)?;
        let traffic = number(plan.bytes)?
            .checked_mul(number(workers)?)
            .and_then(|value| value.checked_mul(u64::try_from(operations_per_worker).ok()?))
            .ok_or_else(|| error("churn requested traffic overflow"))?;
        if traffic > 64 * 1024 * 1024 * 1024 {
            return Err(error(
                "churn exceeds registered 64 GiB requested traffic cap",
            ));
        }
        let wall = Instant::now();
        let mut first_error = None;
        let mut aggregate = RunStats::default();
        let start_gate = (
            std::sync::Mutex::new(None::<bool>),
            std::sync::Condvar::new(),
        );
        std::thread::scope(|scope| {
            let mut jobs = Vec::with_capacity(workers);
            let mut outcomes = Vec::with_capacity(workers);
            let preflight = &preflight;
            let start_gate = &start_gate;
            for worker in 0..workers {
                let job = std::thread::Builder::new().spawn_scoped(scope, move || {
                    let mut state = start_gate
                        .0
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    while state.is_none() {
                        state = start_gate
                            .1
                            .wait(state)
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                    }
                    let run = *state;
                    drop(state);
                    if run != Some(true) {
                        return Err(io::Error::other("worker startup cancelled"));
                    }
                    let mut stats = Self::batch_observed(
                        allocator,
                        std::slice::from_ref(&plan),
                        page,
                        |actual| preflight(worker, actual),
                    )?;
                    for _ in 1..operations_per_worker {
                        stats.merge(Self::batch(allocator, std::slice::from_ref(&plan), page)?)?;
                    }
                    Ok::<_, io::Error>(stats)
                });
                match job {
                    Ok(job) => jobs.push(job),
                    Err(failure) => {
                        first_error = Some(failure);
                        break;
                    }
                }
            }
            *start_gate
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(first_error.is_none());
            start_gate.1.notify_all();
            // Do not drop a panic/error payload or return through ? while
            // another started worker remains unjoined.
            for job in jobs {
                outcomes.push(job.join());
            }
            for outcome in outcomes {
                let result = match outcome {
                    Ok(result) => result,
                    Err(_) => Err(io::Error::other("churn worker panicked")),
                };
                match result {
                    Ok(stats) => {
                        if let Err(failure) = aggregate.merge(stats) {
                            if first_error.is_none() {
                                first_error = Some(failure);
                            }
                        }
                    }
                    Err(failure) => {
                        if first_error.is_none() {
                            first_error = Some(failure);
                        }
                    }
                }
            }
        });
        if let Some(failure) = first_error {
            return Err(failure);
        }
        aggregate.wall_ns = nanos(wall)?;
        Ok(aggregate)
    }

    /// Real doubling realloc chain; only newly added payload bytes are touched.
    pub(crate) fn grow<A: GlobalAlloc + Sync>(
        allocator: &A,
        initial: usize,
        target: usize,
        page: usize,
    ) -> io::Result<RunStats> {
        Self::validate(
            &[TouchPlan {
                bytes: target,
                shape: Shape::Dense,
            }],
            page,
        )?;
        if initial == 0 || initial > target {
            return Err(error("invalid growth start/target"));
        }
        let wall = Instant::now();
        let start = Instant::now();
        let mut block = OwnedBlock::allocate(allocator, initial).map_err(error)?;
        let mut stats = RunStats {
            alloc_ns: nanos(start)?,
            allocations: 1,
            requested_bytes_issued: number(initial)?,
            ..RunStats::default()
        };
        let start = Instant::now();
        let written = Self::touch(&mut block, 0, Shape::Dense, page)?;
        stats.touch_ns = nanos(start)?;
        stats.bytes_written = number(written)?;
        while block.requested_bytes() < target {
            let old = block.requested_bytes();
            let next = old.checked_mul(2).unwrap_or(target).min(target);
            let start = Instant::now();
            block.reallocate(next).map_err(error)?;
            stats.merge(RunStats {
                alloc_ns: nanos(start)?,
                reallocations: 1,
                requested_bytes_issued: number(next)?,
                ..RunStats::default()
            })?;
            let start = Instant::now();
            let written = Self::touch(&mut block, old, Shape::Dense, page)?;
            stats.merge(RunStats {
                touch_ns: nanos(start)?,
                bytes_written: number(written)?,
                ..RunStats::default()
            })?;
        }
        let start = Instant::now();
        drop(block);
        stats.free_ns = nanos(start)?;
        stats.frees = 1;
        stats.wall_ns = nanos(wall)?;
        Ok(stats)
    }
}
