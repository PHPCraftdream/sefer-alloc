//! Bounded persistent payload owners; bookkeeping stays on the process allocator.
use super::owned::OwnedBlock;
use super::run_stats::RunStats;
use super::window_kernel::WindowKernel;
use super::workload::{Shape, TouchPlan, MAX_LIVE_BLOCKS, MAX_LIVE_BYTES};
use std::alloc::GlobalAlloc;
use std::io;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::thread::ThreadId;
use std::time::Instant;

type Batch<'a, A> = Vec<(OwnedBlock<'a, A>, Shape)>;

enum Command<'a, A: GlobalAlloc + Sync> {
    Allocate(Vec<TouchPlan>),
    Touch,
    Take,
    Put(Batch<'a, A>),
    Free,
}

struct Reply<'a, A: GlobalAlloc + Sync> {
    batch: Option<Batch<'a, A>>,
    stats: RunStats,
    thread: ThreadId,
}

struct Endpoint<'a, A: GlobalAlloc + Sync> {
    commands: SyncSender<Command<'a, A>>,
    replies: Receiver<io::Result<Reply<'a, A>>>,
}

fn error(message: &'static str) -> io::Error {
    io::Error::other(message)
}

fn elapsed(start: Instant) -> io::Result<u64> {
    u64::try_from(start.elapsed().as_nanos()).map_err(|_| error("actor elapsed overflow"))
}

fn count(value: usize) -> io::Result<u64> {
    u64::try_from(value).map_err(|_| error("actor count overflow"))
}

/// Synchronous bounded controller; payload never escapes its owned actor slots.
/// Dropping command endpoints closes workers; every started worker is then joined.
pub(crate) struct PayloadActors<'a, A: GlobalAlloc + Sync> {
    endpoints: Vec<Endpoint<'a, A>>,
    live: Vec<(usize, usize)>,
    page: usize,
    failed: bool,
}

impl<'a, A: GlobalAlloc + Sync> PayloadActors<'a, A> {
    pub(crate) fn with<R, F, P>(
        allocator: &'a A,
        workers: usize,
        page: usize,
        preflight: P,
        body: F,
    ) -> io::Result<R>
    where
        F: FnOnce(&mut Self) -> io::Result<R>,
        P: Fn(usize, &A) -> io::Result<()> + Sync,
    {
        if !(1..=4).contains(&workers) || !page.is_power_of_two() {
            return Err(error("invalid actor count or page geometry"));
        }
        std::thread::scope(|scope| {
            let mut actors = Self {
                endpoints: Vec::with_capacity(workers),
                live: vec![(0, 0); workers],
                page,
                failed: false,
            };
            let mut jobs = Vec::with_capacity(workers);
            let mut outcomes = Vec::with_capacity(workers);
            let mut startup_error = None;
            let preflight = &preflight;
            for worker in 0..workers {
                let (commands, requests) = sync_channel(1);
                let (responses, replies) = sync_channel(1);
                match std::thread::Builder::new().spawn_scoped(scope, move || {
                    let mut held = None;
                    while let Ok(command) = requests.recv() {
                        let result = Self::perform(allocator, &mut held, command, page, || {
                            preflight(worker, allocator)
                        });
                        let failed = result.is_err();
                        if responses.send(result).is_err() || failed {
                            break;
                        }
                    }
                    // Cleanup runs on the holding actor, possibly foreign to the allocator owner.
                    drop(held);
                }) {
                    Ok(job) => {
                        jobs.push(job);
                        actors.endpoints.push(Endpoint { commands, replies });
                    }
                    Err(failure) => {
                        startup_error = Some(failure);
                        break;
                    }
                }
            }
            let result = if let Some(failure) = startup_error {
                Ok(Err(failure))
            } else {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(&mut actors)))
            };
            let session_failed = actors.failed;
            // Close both directions before joining: failed sends drop their owned
            // payload, and no worker can remain blocked waiting for controller I/O.
            drop(actors);
            for job in jobs {
                outcomes.push(job.join());
            }
            let worker_panicked = outcomes.iter().any(Result::is_err);
            drop(outcomes);
            if worker_panicked {
                return Err(error("payload actor panicked"));
            }
            if session_failed {
                return Err(error("payload actor session failed"));
            }
            result.map_err(|_| error("payload controller panicked"))?
        })
    }

    fn request(&mut self, worker: usize, command: Command<'a, A>) -> io::Result<Reply<'a, A>> {
        if self.failed || worker >= self.endpoints.len() {
            return Err(error("invalid or failed actor session"));
        }
        let endpoint = &self.endpoints[worker];
        let result = endpoint
            .commands
            .send(command)
            .map_err(|_| error("payload actor command disconnected"))
            .and_then(|()| {
                endpoint
                    .replies
                    .recv()
                    .map_err(|_| error("payload actor reply disconnected"))
            })
            .and_then(|result| result);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    pub(crate) fn allocate(
        &mut self,
        worker: usize,
        plans: &[TouchPlan],
    ) -> io::Result<(RunStats, ThreadId)> {
        WindowKernel::validate(plans, self.page)?;
        if worker >= self.live.len() || self.live[worker] != (0, 0) {
            return Err(error("allocation requires an empty actor slot"));
        }
        let bytes = plans
            .iter()
            .try_fold(0usize, |sum, plan| sum.checked_add(plan.bytes))
            .ok_or_else(|| error("actor live bytes overflow"))?;
        let total = self
            .live
            .iter()
            .try_fold((plans.len(), bytes), |(blocks, bytes), &(n, b)| {
                Some((blocks.checked_add(n)?, bytes.checked_add(b)?))
            })
            .ok_or_else(|| error("aggregate actor bounds overflow"))?;
        if total.0 > MAX_LIVE_BLOCKS || total.1 > MAX_LIVE_BYTES {
            return Err(error("aggregate actor live set exceeds bound"));
        }
        let reply = self.request(worker, Command::Allocate(plans.to_vec()))?;
        self.live[worker] = (plans.len(), bytes);
        Ok((reply.stats, reply.thread))
    }

    pub(crate) fn touch(&mut self, worker: usize) -> io::Result<(RunStats, ThreadId)> {
        let reply = self.request(worker, Command::Touch)?;
        Ok((reply.stats, reply.thread))
    }

    pub(crate) fn free(&mut self, worker: usize) -> io::Result<(RunStats, ThreadId)> {
        let reply = self.request(worker, Command::Free)?;
        self.live[worker] = (0, 0);
        Ok((reply.stats, reply.thread))
    }

    /// Transfer a whole live batch without touching or reconstructing pointers.
    /// Returning it to its original actor permits owner-terminal retirement;
    /// freeing it at the recipient is genuinely foreign retirement.
    pub(crate) fn transfer(&mut self, from: usize, to: usize) -> io::Result<(ThreadId, ThreadId)> {
        if from == to
            || from >= self.live.len()
            || to >= self.live.len()
            || self.live[from].0 == 0
            || self.live[to] != (0, 0)
        {
            return Err(error(
                "transfer requires distinct live and empty actor slots",
            ));
        }
        let source = self.request(from, Command::Take)?;
        let batch = source
            .batch
            .ok_or_else(|| error("actor omitted transfer ownership"))?;
        let live = self.live[from];
        self.live[from] = (0, 0);
        let destination = self.request(to, Command::Put(batch))?;
        self.live[to] = live;
        Ok((source.thread, destination.thread))
    }

    fn perform<P>(
        allocator: &'a A,
        held: &mut Option<Batch<'a, A>>,
        command: Command<'a, A>,
        page: usize,
        preflight: P,
    ) -> io::Result<Reply<'a, A>>
    where
        P: FnOnce() -> io::Result<()>,
    {
        let mut stats = RunStats::default();
        let mut returned = None;
        match command {
            Command::Allocate(plans) => {
                if held.is_some() {
                    return Err(error("actor already owns a batch"));
                }
                let mut blocks = Vec::new();
                blocks
                    .try_reserve_exact(plans.len())
                    .map_err(|_| error("actor metadata allocation failed"))?;
                for plan in plans {
                    let start = Instant::now();
                    let block = OwnedBlock::allocate(allocator, plan.bytes).map_err(error)?;
                    stats.merge(RunStats {
                        alloc_ns: elapsed(start)?,
                        allocations: 1,
                        requested_bytes_issued: count(plan.bytes)?,
                        ..RunStats::default()
                    })?;
                    blocks.push((block, plan.shape));
                }
                *held = Some(blocks);
                // A real heap claim now exists on the actor, not the controller.
                preflight()?;
            }
            Command::Touch => {
                let blocks = held
                    .as_mut()
                    .ok_or_else(|| error("touch requires a live actor batch"))?;
                for (block, shape) in blocks {
                    let start = Instant::now();
                    let written = WindowKernel::touch(block, 0, *shape, page)?;
                    stats.merge(RunStats {
                        touch_ns: elapsed(start)?,
                        bytes_written: count(written)?,
                        ..RunStats::default()
                    })?;
                }
            }
            Command::Take => {
                returned = Some(
                    held.take()
                        .ok_or_else(|| error("transfer requires a live actor batch"))?,
                );
            }
            Command::Put(blocks) => {
                if held.is_some() {
                    return Err(error("transfer recipient already owns a batch"));
                }
                *held = Some(blocks);
            }
            Command::Free => {
                let mut blocks = held
                    .take()
                    .ok_or_else(|| error("free requires a live actor batch"))?;
                while let Some((block, _)) = blocks.pop() {
                    let start = Instant::now();
                    drop(block);
                    stats.merge(RunStats {
                        free_ns: elapsed(start)?,
                        frees: 1,
                        ..RunStats::default()
                    })?;
                }
            }
        }
        Ok(Reply {
            batch: returned,
            stats,
            thread: std::thread::current().id(),
        })
    }
}
