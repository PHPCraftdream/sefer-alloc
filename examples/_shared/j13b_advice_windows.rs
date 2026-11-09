//! Native quiescent capture boundaries; producer joins and System bookkeeping are caller premises.
use super::advice_sample::AdviceSample;
use super::run_stats::RunStats;
use super::window_kernel::WindowKernel;
use super::workload::TouchPlan;
use aligned_vmem::{
    AdviceCapture as Capture, AdviceEvent, AdviceLifetimeSnapshot as Totals, AdviceReplay,
    AdviceReplaySnapshot,
};
use sefer_alloc::{AllocCore, SeferAlloc};
use std::io;
use std::time::Instant;

fn error(message: &'static str) -> io::Error {
    io::Error::other(message)
}
fn elapsed(start: Instant) -> io::Result<u64> {
    u64::try_from(start.elapsed().as_nanos()).map_err(|_| error("advice observer time overflow"))
}
fn capacity_bytes<T>(capacity: usize) -> io::Result<usize> {
    capacity
        .checked_mul(core::mem::size_of::<T>())
        .ok_or_else(|| error("advice observer capacity overflow"))
}

#[derive(Debug)]
struct Start {
    span_clock: Instant,
    before: AdviceReplaySnapshot,
    unscoped_records: u64,
    slot_bytes: usize,
    static_bytes: usize,
    tls_bytes: usize,
    snapshot_bytes: usize,
    replay_index_bytes: usize,
    capture_ns: u64,
    replay_ns: u64,
    observer_ns: u64,
}

/// One controller per disposable child, armed before the first payload mapping.
/// It owns only inert metadata, never mappings. Failed/unclosed windows cannot
/// be reset or repaired into supported rows. No API authenticates quiescence.
#[derive(Debug)]
pub(crate) struct AdviceWindows {
    started: bool,
    active: Option<Start>,
    last_end: u64,
    failure: Option<&'static str>,
}

impl Default for AdviceWindows {
    fn default() -> Self {
        Self::new()
    }
}

impl AdviceWindows {
    pub(crate) const fn new() -> Self {
        Self {
            started: false,
            active: None,
            last_end: 0,
            failure: None,
        }
    }

    pub(crate) fn begin(&mut self) -> io::Result<()> {
        // Reject models before any clock, capture or payload operation.
        if !Capture::native_available() {
            return Err(error("advice windows require native Linux/Android capture"));
        }
        if let Some(message) = self.failure {
            return Err(error(message));
        }
        if self.active.is_some() {
            self.failure = Some("previous advice window is not closed");
            return Err(error("previous advice window is not closed"));
        }
        let result = self.begin_inner();
        if result.is_err() {
            self.failure = Some("advice window start failed; evidence unsupported");
        }
        result
    }

    fn begin_inner(&mut self) -> io::Result<()> {
        let wall = Instant::now();
        if !self.started {
            if AllocCore::dbg_thp_advised_bytes_total() != 0 {
                return Err(error(
                    "advice capture started after accepted payload advice",
                ));
            }
            Capture::begin_native().map_err(error)?;
            self.started = true;
        }
        let capture_clock = Instant::now();
        let capture = Capture::snapshot().map_err(error)?;
        let capture_ns = elapsed(capture_clock)?;
        let replay_clock = Instant::now();
        let projection = AdviceReplay::window(&capture, capture.boundary).map_err(error)?;
        let replay_ns = elapsed(replay_clock)?;
        if projection.before.advice.process[Totals::TOTAL]
            != AllocCore::dbg_thp_advised_bytes_total()
        {
            return Err(error(
                "advice start total differs from native accepted counter",
            ));
        }
        let mut start = Start {
            span_clock: wall,
            before: projection.before,
            unscoped_records: capture
                .boundary
                .checked_sub(self.last_end)
                .ok_or_else(|| error("advice window cursor decreased"))?,
            slot_bytes: capture.slot_storage_bytes,
            static_bytes: capture.total_storage_bytes,
            tls_bytes: capture.tls_context_bytes_per_thread,
            snapshot_bytes: capacity_bytes::<AdviceEvent>(capture.records.capacity())?,
            replay_index_bytes: projection.replay_index_capacity_bytes,
            capture_ns,
            replay_ns,
            observer_ns: 0,
        };
        drop(capture);
        start.observer_ns = elapsed(wall)?;
        self.active = Some(start);
        Ok(())
    }

    pub(crate) fn batch(
        &mut self,
        allocator: &SeferAlloc,
        plans: &[TouchPlan],
        page: usize,
    ) -> io::Result<(RunStats, AdviceSample)> {
        self.begin()?;
        let stats = self.executed(WindowKernel::batch(allocator, plans, page))?;
        let advice = self.finish()?;
        Ok((stats, advice))
    }

    pub(crate) fn grow(
        &mut self,
        allocator: &SeferAlloc,
        initial: usize,
        target: usize,
        page: usize,
    ) -> io::Result<(RunStats, AdviceSample)> {
        self.begin()?;
        let stats = self.executed(WindowKernel::grow(allocator, initial, target, page))?;
        let advice = self.finish()?;
        Ok((stats, advice))
    }

    pub(crate) fn churn<P>(
        &mut self,
        allocator: &SeferAlloc,
        plan: TouchPlan,
        workers: usize,
        operations_per_worker: usize,
        page: usize,
        preflight: P,
    ) -> io::Result<(RunStats, AdviceSample)>
    where
        P: Fn(usize, &SeferAlloc) -> io::Result<()> + Sync,
    {
        self.begin()?;
        let stats = self.executed(WindowKernel::churn(
            allocator,
            plan,
            workers,
            operations_per_worker,
            page,
            preflight,
        ))?;
        let advice = self.finish()?;
        Ok((stats, advice))
    }

    pub(crate) fn abort(&mut self) {
        self.active = None;
        self.failure = Some("advice window aborted; evidence unsupported");
    }

    fn executed<T>(&mut self, result: io::Result<T>) -> io::Result<T> {
        if result.is_err() {
            self.abort();
        }
        result
    }

    pub(crate) fn finish(&mut self) -> io::Result<AdviceSample> {
        if let Some(message) = self.failure {
            return Err(error(message));
        }
        let Some(start) = self.active.take() else {
            self.failure = Some("advice window was not started");
            return Err(error("advice window was not started"));
        };
        let result = self.finish_inner(start);
        if result.is_err() {
            self.failure = Some("advice window finish failed; evidence unsupported");
        }
        result
    }

    fn finish_inner(&mut self, start: Start) -> io::Result<AdviceSample> {
        let wall = Instant::now();
        let capture_clock = Instant::now();
        let capture = Capture::snapshot().map_err(error)?;
        let capture_ns = elapsed(capture_clock)?;
        let replay_clock = Instant::now();
        let window = AdviceReplay::window(&capture, start.before.events).map_err(error)?;
        let replay_ns = elapsed(replay_clock)?;
        if window.before != start.before {
            return Err(error(
                "advice window history changed across its start boundary",
            ));
        }
        if window.after.advice.process[Totals::TOTAL] != AllocCore::dbg_thp_advised_bytes_total() {
            return Err(error(
                "advice terminal total differs from native accepted counter",
            ));
        }
        if capture.slot_storage_bytes != start.slot_bytes
            || capture.total_storage_bytes != start.static_bytes
            || capture.tls_context_bytes_per_thread != start.tls_bytes
        {
            return Err(error("advice capture storage identity changed"));
        }
        let snapshot_bytes = capacity_bytes::<AdviceEvent>(capture.records.capacity())?;
        let replay_indexes = window.replay_index_capacity_bytes;
        drop(capture);
        self.last_end = window.end;
        let observer_ns = elapsed(wall)?;
        let observed_interval_ns = elapsed(start.span_clock)?;
        Ok(AdviceSample {
            window,
            unscoped_records_before: start.unscoped_records,
            capture_slot_bytes: start.slot_bytes,
            capture_static_bytes: start.static_bytes,
            tls_scalar_bytes_per_thread: start.tls_bytes,
            snapshot_capacity_bytes: [start.snapshot_bytes, snapshot_bytes],
            replay_index_capacity_bytes: [start.replay_index_bytes, replay_indexes],
            replay_state_type_bytes: core::mem::size_of::<AdviceReplay>(),
            replay_index_descriptor_bytes: core::mem::size_of::<Vec<usize>>() + core::mem::size_of::<Vec<bool>>(),
            capture_ns: [start.capture_ns, capture_ns],
            replay_ns: [start.replay_ns, replay_ns],
            observer_wall_ns: [start.observer_ns, observer_ns],
            observed_interval_ns,
            cost_scope: "capture/replay/reconciliation/drop before record return; outer window/child includes return bookkeeping; logical capacities/types are not RSS or stack bounds",
        })
    }
}
