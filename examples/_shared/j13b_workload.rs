//! Bounded judge-only plan. No allocator adapter or workload runner is connected.
use std::{
    mem::MaybeUninit,
    sync::{
        mpsc::{sync_channel, SyncSender},
        Arc,
    },
    thread::{self, ThreadId},
};

pub const MAX_LIVE_BLOCKS: usize = 128;
pub const MAX_LIVE_BYTES: usize = 2 * 1024 * 1024 * 1024;
pub const MAX_SCENARIO_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub const MAX_WORKER_PLANS: usize = 1536;
pub const MAX_WORKER_LIVE_BYTES: usize = 512 * 1024 * 1024;
pub const TRAINING_TARGET: usize = 96;
pub const TRAINING_CAP: usize = 1536;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shape {
    Dense,
    Sparse,
    Stride,
    Tail,
    SparseFivePercent,
    SparseTwoPercent,
    Quarter,
    EdgeTwoPages,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Bucket {
    Below1,
    OneTo8,
    EightTo32,
    ThirtyTwoTo128,
    OneTwentyEightTo512,
    AtLeast512,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Measure,
    TrainSparse,
    TrainDense,
    TrainedSparse,
    TrainedDense,
    CoResident,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cell {
    pub id: &'static str,
    pub scenario: &'static str,
    /// Canonical cell id from `scripts/j13b_schema.mjs` (`CELL_MATRIX`). The Rust-only
    /// `dense` alias is not a registered cell; its value here is meaningless.
    pub cell_id: &'static str,
    /// Canonical window name from `CELL_WINDOWS` for this plan entry.
    pub window: &'static str,
    pub shape: Shape,
    pub bytes: usize,
    pub blocks: usize,
    pub minimum_request: usize,
    pub bucket: Bucket,
    pub phase: Phase,
    pub threads: usize,
    pub operations_per_thread: usize,
    pub growth_start_bytes: Option<usize>,
    pub co_resident: Option<(Shape, usize, usize)>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TouchPlan {
    pub bytes: usize,
    pub shape: Shape,
}
#[derive(Debug)]
pub struct OwnedBytes {
    slots: Vec<MaybeUninit<u8>>,
    plan: TouchPlan,
}
impl OwnedBytes {
    pub fn allocate(plan: TouchPlan) -> Result<Self, &'static str> {
        if plan.bytes == 0 || plan.bytes > MAX_LIVE_BYTES {
            return Err("block exceeds 2 GiB hard bound");
        }
        if plan.shape == Shape::EdgeTwoPages && plan.bytes < 8192 {
            return Err("two-page edge shape requires at least 8192 bytes");
        }
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(plan.bytes)
            .map_err(|_| "allocation failed")?;
        slots.resize_with(plan.bytes, MaybeUninit::uninit);
        Ok(Self { slots, plan })
    }
    /// Touches the block through the single shared write kernel [`touch_slots`].
    pub fn touch(&mut self) -> usize {
        touch_slots(&mut self.slots, self.plan.shape)
    }
}

/// Byte written by every touch; tests use a different sentinel.
pub const TOUCH_BYTE: u8 = 0x5a;
/// Registered `stride` step: every 16th 2 MiB region.
pub const STRIDE_STEP_BYTES: usize = 16 * 2 * 1024 * 1024;
/// Legacy preparation geometry. Runtime adapters pass the actual host page size
/// to `touch_slots_with_page` instead of using this literal.
pub const EDGE_BASE_PAGE_BYTES: usize = 4096;

/// One writable byte slot. `u8` is the safe, inspectable test implementation;
/// `MaybeUninit<u8>` is the implementation used by [`OwnedBytes`].
pub trait WriteSlot {
    fn put(&mut self, value: u8);
}
impl WriteSlot for u8 {
    fn put(&mut self, value: u8) {
        *self = value;
    }
}
impl WriteSlot for MaybeUninit<u8> {
    fn put(&mut self, value: u8) {
        MaybeUninit::write(self, value);
    }
}

/// Preparation entrypoint for the single shape-to-offset kernel. Returns the
/// number of slots written. Empty slices and undersized edge shapes write nothing.
pub fn touch_slots<T: WriteSlot>(slots: &mut [T], shape: Shape) -> usize {
    touch_slots_stepped(slots, shape, STRIDE_STEP_BYTES)
}

/// Same kernel with an explicit stride step so small buffers can exercise `Stride`.
pub fn touch_slots_stepped<T: WriteSlot>(slots: &mut [T], shape: Shape, step: usize) -> usize {
    touch_slots_kernel(slots, shape, step, EDGE_BASE_PAGE_BYTES)
}

/// Same write kernel with validated host page geometry for the edge shape.
pub fn touch_slots_with_page<T: WriteSlot>(slots: &mut [T], shape: Shape, page: usize) -> usize {
    if !page.is_power_of_two() {
        return 0;
    }
    touch_slots_kernel(slots, shape, STRIDE_STEP_BYTES, page)
}

fn touch_slots_kernel<T: WriteSlot>(
    slots: &mut [T],
    shape: Shape,
    step: usize,
    page: usize,
) -> usize {
    let n = slots.len();
    if n == 0 || step == 0 {
        return 0;
    }
    let mut count = 0;
    let mut put = |slots: &mut [T], i: usize| {
        slots[i].put(TOUCH_BYTE);
        count += 1;
    };
    match shape {
        Shape::Dense => (0..n).for_each(|i| put(slots, i)),
        Shape::Sparse => put(slots, 0),
        Shape::Stride => (0..n).step_by(step).for_each(|i| put(slots, i)),
        Shape::Tail => (n - n / 4..n).for_each(|i| put(slots, i)),
        Shape::SparseFivePercent => (0..n / 20).for_each(|i| put(slots, i)),
        Shape::SparseTwoPercent => (0..n / 50).for_each(|i| put(slots, i)),
        Shape::Quarter => (0..n / 4).for_each(|i| put(slots, i)),
        Shape::EdgeTwoPages => {
            if page.checked_mul(2).is_some_and(|minimum| n >= minimum) {
                put(slots, 0);
                put(slots, n - page);
            }
        }
    }
    count
}
fn mib(n: usize) -> usize {
    n * 1024 * 1024
}
#[allow(clippy::too_many_arguments)]
fn cell(
    id: &'static str,
    scenario: &'static str,
    shape: Shape,
    bytes: usize,
    blocks: usize,
    bucket: Bucket,
    phase: Phase,
    min: usize,
) -> Cell {
    Cell {
        id,
        scenario,
        cell_id: id,
        window: "initial",
        shape,
        bytes,
        blocks,
        minimum_request: min,
        bucket,
        phase,
        threads: 1,
        operations_per_thread: 1,
        growth_start_bytes: None,
        co_resident: None,
    }
}
fn at(mut c: Cell, cell_id: &'static str, window: &'static str) -> Cell {
    c.cell_id = cell_id;
    c.window = window;
    c
}
fn sized(id: &'static str, scenario: &'static str, bytes: usize, bucket: Bucket) -> Cell {
    cell(
        id,
        scenario,
        Shape::Dense,
        bytes,
        1,
        bucket,
        Phase::Measure,
        mib(8),
    )
}
pub fn plan(s: &str) -> Result<Vec<Cell>, &'static str> {
    use Bucket::*;
    use Phase::*;
    use Shape::*;
    let cells = match s {
        "dense" => vec![sized("dense-16m", "dense", mib(16), EightTo32)],
        "sparse" => vec![cell(
            "sparse-128x16m",
            "sparse",
            Sparse,
            mib(16),
            128,
            EightTo32,
            Measure,
            mib(8),
        )],
        "stride" => vec![cell(
            "stride-64x16m",
            "stride",
            Stride,
            mib(16),
            64,
            EightTo32,
            Measure,
            mib(8),
        )],
        "tail" => vec![cell(
            "tail-32x16m",
            "tail",
            Tail,
            mib(16),
            32,
            EightTo32,
            Measure,
            mib(8),
        )],
        "edge" => vec![cell(
            "edge-128m",
            "edge",
            EdgeTwoPages,
            mib(128),
            1,
            OneTwentyEightTo512,
            Measure,
            mib(8),
        )],
        "cycle" => vec![
            sized("cycle-1m", "cycle", mib(1), OneTo8),
            {
                let mut c = cell(
                    "cycle-128m",
                    "cycle",
                    Dense,
                    mib(128),
                    1,
                    OneTwentyEightTo512,
                    Measure,
                    mib(8),
                );
                c.growth_start_bytes = Some(mib(128));
                c
            },
            sized("cycle-1g", "cycle", mib(1024), AtLeast512),
        ],
        "grow" => {
            let mut c = cell(
                "grow-1g",
                "grow",
                Dense,
                mib(1024),
                1,
                AtLeast512,
                Measure,
                mib(8),
            );
            c.growth_start_bytes = Some(mib(1));
            vec![c]
        }
        "many" => vec![cell(
            "many-128x16m",
            "many",
            Dense,
            mib(16),
            128,
            EightTo32,
            Measure,
            mib(8),
        )],
        "phased" => vec![
            at(
                cell(
                    "phased-cold",
                    "phased",
                    Dense,
                    mib(16),
                    1,
                    EightTo32,
                    Measure,
                    mib(8),
                ),
                "phased",
                "initial",
            ),
            at(
                cell(
                    "phased-sparse-training",
                    "phased",
                    SparseFivePercent,
                    mib(16),
                    32,
                    EightTo32,
                    TrainSparse,
                    mib(8),
                ),
                "phased",
                "sparse-training",
            ),
            at(
                cell(
                    "phased-sparse-trained",
                    "phased",
                    SparseFivePercent,
                    mib(16),
                    1,
                    EightTo32,
                    TrainedSparse,
                    mib(8),
                ),
                "phased",
                "sparse-trained",
            ),
            at(
                cell(
                    "phased-dense-training",
                    "phased",
                    Dense,
                    mib(16),
                    32,
                    EightTo32,
                    TrainDense,
                    mib(8),
                ),
                "phased",
                "dense-training",
            ),
            at(
                cell(
                    "phased-dense-trained",
                    "phased",
                    Dense,
                    mib(16),
                    1,
                    EightTo32,
                    TrainedDense,
                    mib(8),
                ),
                "phased",
                "dense-trained",
            ),
        ],
        "mixed" => {
            let mut c = cell(
                "mixed-co-resident",
                "mixed",
                Dense,
                mib(16),
                8,
                EightTo32,
                CoResident,
                mib(8),
            );
            c.co_resident = Some((SparseTwoPercent, mib(64), 8));
            let c = at(c, "mixed", "co-resident");
            vec![
                at(
                    cell(
                        "mixed-dense-training",
                        "mixed",
                        Dense,
                        mib(16),
                        8,
                        EightTo32,
                        TrainDense,
                        mib(8),
                    ),
                    "mixed",
                    "dense-training",
                ),
                at(
                    cell(
                        "mixed-sparse-training",
                        "mixed",
                        SparseTwoPercent,
                        mib(64),
                        8,
                        ThirtyTwoTo128,
                        TrainSparse,
                        mib(8),
                    ),
                    "mixed",
                    "sparse-training",
                ),
                // dense 16 MiB x8 alone
                at(
                    cell(
                        "mixed-dense-only",
                        "mixed",
                        Dense,
                        mib(16),
                        8,
                        EightTo32,
                        TrainedDense,
                        mib(8),
                    ),
                    "mixed",
                    "dense-only",
                ),
                // sparse 64 MiB 2% x8 alone
                at(
                    cell(
                        "mixed-sparse-only",
                        "mixed",
                        SparseTwoPercent,
                        mib(64),
                        8,
                        ThirtyTwoTo128,
                        TrainedSparse,
                        mib(8),
                    ),
                    "mixed",
                    "sparse-only",
                ),
                c,
            ]
        }
        "churn" => {
            let mut c = cell(
                "churn-4x1024",
                "churn",
                Quarter,
                mib(16),
                4,
                EightTo32,
                Measure,
                mib(8),
            );
            c.threads = 4;
            c.operations_per_thread = 1024;
            vec![c]
        }
        "churn-small" => {
            let mut c = cell(
                "churn-small-4x4096",
                "churn-small",
                Quarter,
                mib(1),
                4,
                OneTo8,
                Measure,
                0,
            );
            c.threads = 4;
            c.operations_per_thread = 4096;
            vec![c]
        }
        _ => return Err("unknown scenario"),
    };
    let total = cells
        .iter()
        .try_fold(0usize, |a, c| a.checked_add(c.bytes.checked_mul(c.blocks)?))
        .ok_or("scenario total overflow")?;
    if total as u64 > MAX_SCENARIO_BYTES {
        return Err("scenario exceeds 8 GiB");
    };
    for c in &cells {
        let live = c.bytes.checked_mul(c.blocks).ok_or("live size overflow")?;
        if c.blocks > MAX_LIVE_BLOCKS || live > MAX_LIVE_BYTES {
            return Err("cell exceeds per-cell live bound");
        }
        if let Some((_, other_bytes, other_blocks)) = c.co_resident {
            let total = live
                .checked_add(
                    other_bytes
                        .checked_mul(other_blocks)
                        .ok_or("co-resident size overflow")?,
                )
                .ok_or("co-resident size overflow")?;
            if total > MAX_LIVE_BYTES {
                return Err("co-resident live set exceeds 2 GiB");
            }
        }
    }
    Ok(cells)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrainingState {
    Collecting {
        eligible: usize,
        valid: usize,
    },
    Complete {
        eligible: usize,
        valid: usize,
        /// `None` only when the caller supplied no share at all; a supplied but invalid
        /// share is reported as [`TrainingState::InvalidShare`], never dropped.
        attained_share: Option<f64>,
    },
    Insufficient {
        eligible: usize,
        valid: usize,
        attained_share: Option<f64>,
    },
    /// The terminal observation carried a non-finite or out-of-[0,1] share.
    InvalidShare {
        eligible: usize,
        valid: usize,
    },
}
#[derive(Debug)]
pub struct Training {
    state: TrainingState,
    sparse: bool,
}
impl Training {
    pub fn new(sparse: bool) -> Self {
        Self {
            state: TrainingState::Collecting {
                eligible: 0,
                valid: 0,
            },
            sparse,
        }
    }
    pub fn observe(
        &mut self,
        eligible: bool,
        observation: Option<f64>,
        share: Option<f64>,
    ) -> TrainingState {
        let TrainingState::Collecting {
            eligible: mut e,
            valid: mut v,
        } = self.state
        else {
            return self.state;
        };
        if eligible {
            e += 1;
            if observation.is_some_and(|o| {
                o.is_finite()
                    && (0.0..=1.0).contains(&o)
                    && if self.sparse { o < 0.1 } else { o > 0.9 }
            }) {
                v += 1
            }
        }
        let invalid_share = share.is_some_and(|x| !(x.is_finite() && (0.0..=1.0).contains(&x)));
        self.state = if v < TRAINING_TARGET && e < TRAINING_CAP {
            TrainingState::Collecting {
                eligible: e,
                valid: v,
            }
        } else if invalid_share {
            TrainingState::InvalidShare {
                eligible: e,
                valid: v,
            }
        } else if v >= TRAINING_TARGET {
            TrainingState::Complete {
                eligible: e,
                valid: v,
                attained_share: share,
            }
        } else {
            TrainingState::Insufficient {
                eligible: e,
                valid: v,
                attained_share: share,
            }
        };
        self.state
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    A,
    B,
}
/// Events emitted at the actual event points by the worker threads and RAII guards.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditEvent {
    Allocated {
        id: usize,
        thread: ThreadId,
    },
    Touched {
        id: usize,
        thread: ThreadId,
        count: usize,
    },
    /// Sent by the buffer's drop guard after the bytes were released.
    Dropped {
        id: usize,
        thread: ThreadId,
    },
    /// Sent by the worker's exit guard as the last thing the thread does.
    WorkerExited {
        role: Role,
        thread: ThreadId,
    },
}
/// Join result observed from `JoinHandle::join`; `Returned` carries the id the
/// finished thread itself returned, so it cannot exist without a real join.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JoinOutcome {
    Returned { thread: ThreadId },
    Panicked,
}
#[derive(Debug)]
pub struct MigrationReport {
    pub counts: Vec<usize>,
    pub events: Vec<AuditEvent>,
    pub a_thread: ThreadId,
    pub b_thread: ThreadId,
    pub a_join: JoinOutcome,
    pub b_join: JoinOutcome,
    /// Buffers whose drop guard had not run after both joins (must be 0).
    pub outstanding_buffers: usize,
}
#[derive(Debug)]
pub struct MigrationFailure {
    pub error: &'static str,
    pub report: MigrationReport,
}
#[derive(Debug)]
pub enum WorkerError {
    /// Rejected by bounds validation before any thread was spawned.
    Rejected(&'static str),
    /// A worker failed or panicked; both threads were joined and are reported.
    Failed(MigrationFailure),
}

struct DropAudit {
    id: usize,
    sink: SyncSender<AuditEvent>,
    _token: Arc<()>,
}
impl Drop for DropAudit {
    fn drop(&mut self) {
        let _ = self.sink.send(AuditEvent::Dropped {
            id: self.id,
            thread: thread::current().id(),
        });
    }
}
struct ExitAudit {
    role: Role,
    sink: SyncSender<AuditEvent>,
}
impl Drop for ExitAudit {
    fn drop(&mut self) {
        let _ = self.sink.send(AuditEvent::WorkerExited {
            role: self.role,
            thread: thread::current().id(),
        });
    }
}
/// Field order matters: `bytes` is released before the guard reports the drop.
struct AuditedBuffer {
    bytes: OwnedBytes,
    _guard: DropAudit,
}
enum Msg {
    Buffer { id: usize, buf: AuditedBuffer },
    Stop,
}
struct AOut {
    thread: ThreadId,
    counts: Vec<usize>,
    error: Option<&'static str>,
}
struct BOut {
    thread: ThreadId,
    error: Option<&'static str>,
}

fn validate_worker_plans(plans: &[TouchPlan]) -> Result<(), &'static str> {
    if plans.len() > MAX_WORKER_PLANS {
        return Err("worker plan count exceeds 1536");
    }
    let mut traffic = 0u64;
    for p in plans {
        if p.bytes == 0 || p.bytes > MAX_LIVE_BYTES {
            return Err("invalid worker plan size");
        }
        if p.bytes > MAX_WORKER_LIVE_BYTES {
            return Err("worker plan exceeds 512 MiB");
        }
        traffic = traffic
            .checked_add(p.bytes as u64)
            .ok_or("worker plan total overflow")?;
    }
    if traffic
        > (MAX_WORKER_PLANS as u64)
            .checked_mul(MAX_WORKER_LIVE_BYTES as u64)
            .ok_or("worker traffic bound overflow")?
    {
        return Err("worker plan traffic exceeds bound");
    }
    Ok(())
}

/// Persistent A/B pair; ownership goes A→B only, B touches/frees, each request is acknowledged.
pub fn migration_worker(plans: &[TouchPlan]) -> Result<MigrationReport, WorkerError> {
    run_pair(
        plans,
        |_, p| OwnedBytes::allocate(p),
        |_, buffer| Ok(buffer.touch()),
    )
}

/// Generic core of [`migration_worker`]. The hooks let a test inject allocation or
/// touch failures/panics into the real thread/channel/join machinery; nothing is
/// injected in the registered path. Both threads are always joined explicitly and
/// their outcomes are reported before any error is returned. The audit channel is
/// bounded at three events per plan plus the two exit events and never blocks.
pub fn run_pair<A, T>(
    plans: &[TouchPlan],
    alloc: A,
    touch: T,
) -> Result<MigrationReport, WorkerError>
where
    A: Fn(usize, TouchPlan) -> Result<OwnedBytes, &'static str> + Sync,
    T: Fn(usize, &mut OwnedBytes) -> Result<usize, &'static str> + Sync,
{
    validate_worker_plans(plans).map_err(WorkerError::Rejected)?;
    let token = Arc::new(());
    let (ev_tx, ev_rx) = sync_channel::<AuditEvent>(plans.len() * 3 + 4);
    let (tx, rx) = sync_channel::<Msg>(1);
    let (ack_tx, ack_rx) = sync_channel::<usize>(1);
    let (alloc, touch, tok) = (&alloc, &touch, &token);
    let (ev_a, ev_b) = (ev_tx.clone(), ev_tx.clone());
    let (a_id, b_id, ar, br) = thread::scope(|s| {
        let b = s.spawn(move || {
            // Declared first so it drops last: the exit event follows every other drop.
            let _exit = ExitAudit {
                role: Role::B,
                sink: ev_b.clone(),
            };
            let rx = rx;
            let ack_tx = ack_tx;
            let mut error = None;
            while let Ok(m) = rx.recv() {
                match m {
                    Msg::Buffer { id, mut buf } => match touch(id, &mut buf.bytes) {
                        Ok(count) => {
                            let _ = ev_b.send(AuditEvent::Touched {
                                id,
                                thread: thread::current().id(),
                                count,
                            });
                            drop(buf);
                            if ack_tx.send(count).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            error = Some(e);
                            break;
                        }
                    },
                    Msg::Stop => break,
                }
            }
            BOut {
                thread: thread::current().id(),
                error,
            }
        });
        let a = s.spawn(move || {
            let _exit = ExitAudit {
                role: Role::A,
                sink: ev_a.clone(),
            };
            let tx = tx;
            let ack_rx = ack_rx;
            let mut counts = Vec::with_capacity(plans.len());
            let mut error = None;
            for (id, plan) in plans.iter().copied().enumerate() {
                let bytes = match alloc(id, plan) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        error = Some(e);
                        break;
                    }
                };
                let buf = AuditedBuffer {
                    bytes,
                    _guard: DropAudit {
                        id,
                        sink: ev_a.clone(),
                        _token: Arc::clone(tok),
                    },
                };
                let _ = ev_a.send(AuditEvent::Allocated {
                    id,
                    thread: thread::current().id(),
                });
                if tx.send(Msg::Buffer { id, buf }).is_err() {
                    error = Some("worker B disconnected");
                    break;
                }
                match ack_rx.recv() {
                    Ok(count) => counts.push(count),
                    Err(_) => {
                        error = Some("worker B disconnected");
                        break;
                    }
                }
            }
            let _ = tx.send(Msg::Stop);
            AOut {
                thread: thread::current().id(),
                counts,
                error,
            }
        });
        let ids = (a.thread().id(), b.thread().id());
        // Both joins always run, whatever the first one returned.
        let ar = a.join();
        let br = b.join();
        (ids.0, ids.1, ar, br)
    });
    let events: Vec<AuditEvent> = ev_rx.try_iter().collect();
    drop(ev_tx);
    let outstanding_buffers = Arc::strong_count(&token) - 1;
    let (a_join, a_out) = match ar {
        Ok(o) => (JoinOutcome::Returned { thread: o.thread }, Some(o)),
        Err(_) => (JoinOutcome::Panicked, None),
    };
    let (b_join, b_out) = match br {
        Ok(o) => (JoinOutcome::Returned { thread: o.thread }, Some(o)),
        Err(_) => (JoinOutcome::Panicked, None),
    };
    let error = if a_out.is_none() {
        Some("worker A panicked")
    } else if b_out.is_none() {
        Some("worker B panicked")
    } else {
        b_out
            .as_ref()
            .and_then(|o| o.error)
            .or_else(|| a_out.as_ref().and_then(|o| o.error))
    };
    let report = MigrationReport {
        counts: a_out.map(|o| o.counts).unwrap_or_default(),
        events,
        a_thread: a_id,
        b_thread: b_id,
        a_join,
        b_join,
        outstanding_buffers,
    };
    match error {
        None => Ok(report),
        Some(error) => Err(WorkerError::Failed(MigrationFailure { error, report })),
    }
}
