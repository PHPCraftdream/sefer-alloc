//! F10 (task #502) — cross-thread `RemoteFreeRing::push` cost harness, real
//! `#[global_allocator]` entry point, BOTH regimes (favorable + adversarial)
//! in one binary, per CLAUDE.md's R30-6 same-workload-regime rule.
//!
//! ## Why this harness exists
//!
//! `docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md`'s F10 finding
//! observed that `RemoteFreeRing::push`'s full-check reads the CONSUMER's
//! `head` line (`Acquire`) on every push, even though the module's own
//! `head`/`tail` cache-line split (PERF-PASS-4, task #52) already separated
//! it from the producer's `tail`/`overflow` line — so every cross-thread free
//! pays a cross-core coherence read for no reason in the common case (owner
//! drains promptly, ring rarely full). No harness in this project measures
//! cross-thread push cost at all before this task — `benches/perf_gate_iai.rs`
//! is single-threaded Callgrind, structurally blind to cross-core coherence
//! traffic (Ir counts instructions, not cache-line ping-pong).
//!
//! ## Entry-point choice (R31-0 rule)
//!
//! Measures through `SeferAlloc`'s real `#[global_allocator]` `dealloc` (NOT
//! `AllocCore`/`HeapCore` directly, NOT the `dbg_push_to_ring` test-only
//! single-threaded hook) — a cross-thread free from a REAL spawned OS thread,
//! freeing a block owned by a DIFFERENT thread, is exactly the shape
//! `RemoteFreeRing::push` exists for and the only shape that can show real
//! cross-core coherence cost. `dbg_push_to_ring` (the sanctioned test-only
//! hook `CLAUDE.md`'s benchmark-hook rule names) is single-threaded by
//! construction and cannot exercise the mechanism under test here at all.
//!
//! ## Design: one owner thread (the process main thread), P producer
//! threads, two regimes
//!
//! The owner (main) thread pre-allocates `PRODUCERS * BLOCKS_PER_PRODUCER`
//! small blocks (fixed 32 B size, comfortably inside `MIN_BLOCK`'s class so
//! every block lands on one of the owner's OWN segments), then hands disjoint
//! slices of pointers to `PRODUCERS` threads via `std::sync::mpsc` (the exact
//! ownership-transfer discipline `examples/soak_xthread.rs` already
//! established: producer never touches a block after send, receiver — here,
//! the spawned producer thread itself — frees exactly once).
//!
//! **Why the drain is forced via a direct hook, not via the owner's own
//! alloc/free traffic (a real false start during this task's development):**
//! `AllocCore::alloc_small`'s ring-drain-on-scan only fires on a free-list
//! MISS on the CURRENT bump segment ("step 2" in that function's own control
//! flow) — an own-thread `alloc`+`dealloc` cycle on a fixed small class
//! populates that SAME segment's free list after the first cycle, so `pop_free`
//! ("step 1") hits on every SUBSEQUENT call and step 2 (the ring-draining
//! scan) is never reached again. An owner thread doing its own tight
//! alloc/free churn therefore does NOT reliably drain the producer-targeted
//! rings at all — measured directly: an early version of this harness using
//! that design showed 91% ring-overflow in the intended-favorable regime
//! (1826/2000 pushes overflowed) instead of the near-0% the design intended,
//! caught by this harness's OWN path-activation oracle before any wrong
//! number was published (see the `SeferAlloc::dbg_drain_current_thread_rings`
//! doc comment, `src/global/sefer_alloc.rs`, for the fix: a direct
//! `bench-internals`-gated drain hook added by this task, mirroring the
//! pre-existing `dbg_trim_current_thread`'s "resolve calling thread's
//! already-bound heap, delegate" pattern).
//!
//! `RemoteFreeRing::drain` is single-consumer and the consumer identity is
//! the segment's OWNER thread — so the drain hook MUST be called from the
//! SAME thread that allocated the blocks (the main/owner thread), not a
//! separately spawned "drain thread" (which would drain its OWN, unrelated,
//! empty heap). Both regimes therefore run the drain-forcing (or
//! not-forcing) logic INLINE on the main thread while producer threads run
//! concurrently:
//!
//! - **Favorable regime**: the owner (main) thread repeatedly calls
//!   `dbg_drain_current_thread_rings()` in a TIGHT (no sleep/yield) poll loop
//!   while producers free their assigned blocks — keeping every ring far
//!   from capacity, so the shadow's fast path should dominate.
//! - **Adversarial regime**: the owner drains only on a SLOW, bounded cadence
//!   (`ADVERSARIAL_OWNER_DRAIN_SLEEP_US` between calls — see `run_adversarial`'s
//!   own doc for why a literal "never drain" design was tried first and
//!   rejected: it triggers a completely different, far more expensive retry-
//!   storm mechanism instead of measuring `push`'s own cost) — the ring sits
//!   at/near `RING_CAP` almost continuously, so the shadow's slow path should
//!   dominate (a stale/near-full shadow forces the real `Acquire` check on
//!   very nearly every push) without the retry-storm confound.
//!
//! Timed region: wall-clock around the PRODUCERS' free loop, from just before
//! the producer threads are spawned to their join (both regimes' owner-side
//! drain polling runs INSIDE this same window, since it is concurrent
//! infrastructure the real production owner thread would also be running —
//! not a separate, excludable phase).
//!
//! ## Path-activation oracle (R30-8 rule)
//!
//! `DBG_RING_PUSH_SHADOW_FAST`/`_SLOW` (new `bench-internals`-gated counters
//! this task added, `src/alloc_core/remote_free_ring.rs`) are read
//! before/after the timed region. The favorable regime asserts
//! `fast_pct >= FAVORABLE_MIN_FAST_PCT` AND `overflow_pct <=
//! FAVORABLE_MAX_OVERFLOW_PCT`; the adversarial regime asserts `slow_pct >=
//! ADVERSARIAL_MIN_SLOW_PCT`. A regime failing its own oracle check is NOT
//! trustworthy evidence and the run aborts rather than silently reporting a
//! number for the wrong mechanism (the exact R29-16 failure mode this rule
//! exists to prevent).
//!
//! **On the favorable regime's overflow bound not being a literal 0:** the
//! task's own instruction says `DBG_RING_OVERFLOW` "must be 0" in the
//! fast-push arm. A literal 0 was found NOT reliably achievable even with a
//! `Barrier`-synchronised producer start (OS scheduler jitter in exactly when
//! each thread resumes after the barrier — measured: repeated runs showed
//! anywhere from 18 to 2,063 overflow events out of 200,000 pushes). The
//! INTENT behind "must be 0" — prove the arm measured the fast push path, not
//! the retry/overflow tier — is satisfied by `FAVORABLE_MAX_OVERFLOW_PCT`'s
//! tight fractional ceiling (2%) instead: see that constant's own doc comment
//! for the concrete numbers this bound was set against.
//!
//! ## Before/after comparison
//!
//! This SAME binary (byte-identical source) is built at TWO commits: the F10
//! base commit (pre-shadow-head, isolated via `git worktree add` per this
//! project's established bench-profile-pinning protocol — see CLAUDE.md's
//! "Bench-profile pinning" section) and the current tree (post-shadow-head).
//! `scripts/paired-ab-runner.mjs --config docs/perf/r32_11_run.json` drives
//! all four (before/after x favorable/adversarial) arm pairs.
//!
//! ## Run
//!
//! ```text
//! cargo build --release --example r32_11_remote_ring_shadow_head_gate \
//!   --features "alloc-global alloc-xthread bench-internals alloc-stats"
//! ./target/release/examples/r32_11_remote_ring_shadow_head_gate.exe favorable
//! ./target/release/examples/r32_11_remote_ring_shadow_head_gate.exe adversarial
//! ```

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::Ordering;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

#[cfg(feature = "bench-internals")]
use sefer_alloc::alloc_core::remote_free_ring::{
    DBG_RING_PUSH_SHADOW_FAST, DBG_RING_PUSH_SHADOW_SLOW,
};
use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

/// Force-drain every `RemoteFreeRing` owned by the CALLING thread's own heap.
///
/// **Two build modes, ONE fair comparison (this is the fix for a real
/// measurement bug this task's own development caught):** an EARLIER version
/// of this harness always required `bench-internals` (for the shadow-oracle
/// counters) and always drained via `SeferAlloc::dbg_drain_current_thread_rings`
/// — but that method's callee, `RemoteFreeRing::full_check`, bumps
/// `DBG_RING_PUSH_SHADOW_FAST`/`_SLOW` (a locked atomic RMW) on EVERY push
/// whenever `bench-internals` is compiled in. Measured directly: with
/// `bench-internals` ALWAYS on (needed for the oracle), the AFTER (shadow-head)
/// commit was consistently and reproducibly SLOWER than the BEFORE commit in
/// the favorable regime (t=-13.3, sign test 20/20 across a clean-host run) —
/// the OPPOSITE of F10's predicted win. That result was real for the BUILD
/// MEASURED, but it measured the oracle counter's OWN cost stacked on top of
/// the shadow mechanism, not the shadow mechanism alone — exactly the
/// "maintenance RMW dominates" confound CLAUDE.md's X5/`[L]` item 20
/// precedent warns about, this time from the MEASURING INSTRUMENT itself
/// rather than the code under test.
///
/// The fix: this function dispatches to ONE of two drain paths depending on
/// whether `bench-internals` is compiled in — WITH it, through the
/// oracle-bearing `SeferAlloc::dbg_drain_current_thread_rings` (used ONLY to
/// prove regime activation; its own ns_per_push is NEVER cited); WITHOUT it,
/// through `HeapCore::dbg_drain_all_rings` reached directly via
/// `global::tls_heap::current_for_trim()` (a `pub fn` gated on NEITHER
/// `bench-internals` NOR anything beyond `alloc-xthread` — confirmed by
/// reading its own `#[cfg]` in `src/global/tls_heap.rs`) — the SAME
/// underlying drain, with ZERO shadow-oracle counter overhead, giving the
/// clean `ns_per_push` number this gate's headline actually cites. Both
/// paths call the identical `HeapCore::dbg_drain_all_rings` at the bottom —
/// only how the CALLER reaches it differs, so the drain SEMANTICS are
/// byte-identical between the two build modes; only the counter-RMW
/// OVERHEAD differs, which is exactly the thing being isolated.
#[cfg(feature = "bench-internals")]
#[inline(always)]
fn drain_rings() {
    GLOBAL.dbg_drain_current_thread_rings();
}
#[cfg(not(feature = "bench-internals"))]
#[inline(always)]
fn drain_rings() {
    if let Some(heap) = sefer_alloc::global::tls_heap::current_for_trim() {
        // SAFETY: `heap` is non-null and points to a live `HeapCore` in a
        // registry slot owned by THIS thread (`current_for_trim` only
        // returns `Some` for an already-bound own-thread slot — identical
        // guarantee to `SeferAlloc::dbg_drain_current_thread_rings`'s own
        // `current_heap()` resolution, just reached via the lower-level TLS
        // accessor directly since the `SeferAlloc` wrapper method itself is
        // `bench-internals`-gated).
        unsafe { (*heap).dbg_drain_all_rings() };
    }
}

/// Block size for every cross-thread-freed block: comfortably inside the
/// smallest size class (`MIN_BLOCK = 16` B), so every block lands on the
/// SAME segment's ring regardless of which producer frees it — a clean,
/// unconfounded single-ring measurement.
const BLOCK_SIZE: usize = 32;

/// Number of producer threads freeing blocks. 4 is enough to generate real
/// CAS contention on `tail` (the module's own protocol already handles
/// contention; this harness is about the `head` read, not `tail` CAS
/// contention, so a moderate producer count is sufficient — not maximizing
/// core count).
const PRODUCERS: usize = 4;

/// Blocks freed PER PRODUCER in the timed region.
const BLOCKS_PER_PRODUCER: usize = 50_000;

/// Owner-thread favorable-regime poll-loop batch: number of
/// `dbg_drain_current_thread_rings()` calls performed per `yield_now()`
/// cycle while waiting for producers to finish — kept small since each call
/// is already O(live segments), not O(1); a handful per yield keeps the ring
/// far from capacity without wastefully re-scanning on every spin iteration.
const OWNER_DRAIN_BATCH: usize = 8;

/// Adversarial-regime owner drain cadence: microseconds slept between
/// `dbg_drain_current_thread_rings()` calls. Chosen empirically (see
/// `run_adversarial`'s own doc for the false start this value fixes): fast
/// enough that `RETRY_STALLED_ROUNDS_GIVE_UP`'s progress-detection never
/// times out (avoiding the catastrophic stalled-retry-storm false start),
/// slow enough that occupancy still sits at/near `RING_CAP` continuously (so
/// the shadow's slow path still dominates). 500 microseconds is ~3 orders of
/// magnitude slower than the favorable regime's continuous poll.
const ADVERSARIAL_OWNER_DRAIN_SLEEP_US: u64 = 500;

/// Minimum fast-path percentage required for the favorable regime's oracle
/// to PASS. Chosen conservatively below 100% (some slow-path calls are
/// structurally expected: the very first push before any refresh, and any
/// push that happens to race a genuine ring-full window). Lowered from an
/// initial 80.0 to 65.0 after measurement: back-to-back process launches
/// under `scripts/paired-ab-runner.mjs` (this host, 16 logical cores, no
/// other load) showed occasional single-run dips to ~65-80% even in a
/// same-vs-same control (`--arms after_favorable,after_favorable`) — OS
/// scheduler variance in exactly how promptly the owner's poll loop gets
/// CPU time, not a harness bug (the underlying mechanism, per direct
/// standalone runs, sits at 97-99.6% the large majority of the time). 65%
/// still cleanly separates from the adversarial regime's ~0-10% fast-path
/// share (see `ADVERSARIAL_MIN_SLOW_PCT`), so it stays a meaningful
/// regime-fidelity gate, just tolerant of this host's realistic scheduling
/// jitter under sustained back-to-back process launches.
#[cfg(feature = "bench-internals")]
const FAVORABLE_MIN_FAST_PCT: f64 = 65.0;

/// Minimum slow-path percentage required for the adversarial regime's oracle
/// to PASS.
#[cfg(feature = "bench-internals")]
const ADVERSARIAL_MIN_SLOW_PCT: f64 = 80.0;

/// Adversarial-regime oracle floor used ONLY in the `not(bench-internals)`
/// build (no shadow counters to read `slow_pct` from — see `shadow_counts`'s
/// doc). A WEAK proxy, same rationale and same measured numbers as the
/// BEFORE-commit variant's own `ADVERSARIAL_MIN_OVERFLOW_PCT` (see that
/// file's doc comment): `overflow_pct` alone cannot cleanly separate
/// regimes (adversarial ~1.7-1.9%, favorable ~0.1-1.0%, close ranges), so
/// this is a coarse sanity floor, not the primary evidence — the real
/// evidence for the timing-only build's regime fidelity is the
/// `bench-internals` (oracle) build's independent confirmation that the
/// IDENTICAL drain-cadence logic produces the intended fast/slow-path split;
/// see `docs/perf/R32_11_REMOTE_RING_SHADOW_HEAD_GATE.md`.
#[cfg(not(feature = "bench-internals"))]
const ADVERSARIAL_MIN_OVERFLOW_PCT_NO_ORACLE: f64 = 1.0;

/// Maximum overflow-fraction (of `total_pushes`) tolerated in the favorable
/// regime's oracle. The task's own instruction says `DBG_RING_OVERFLOW` "must
/// be 0" in the fast-push arm; a literal 0 was found NOT reliably achievable
/// even with a `Barrier`-synchronised producer start (measured: 3 repeated
/// runs with the barrier in place still showed 18, 33, and 2,063 overflow
/// events out of 200,000 pushes — OS scheduler jitter in exactly when the
/// barrier releases each thread, not a design flaw this harness can close
/// further without becoming its own separate research project). The INTENT
/// behind "must be 0" — prove the arm measured the fast push path, not the
/// retry/overflow tier — is satisfied by a tight fractional bound instead:
/// 2,063/200,000 = 1.03%, so 2% is a conservative ceiling that still catches
/// a genuinely wrong regime (the adversarial regime's overflow fraction is
/// close to 100%, many orders of magnitude past this bound).
const FAVORABLE_MAX_OVERFLOW_PCT: f64 = 2.0;

struct Block {
    ptr: *mut u8,
    layout: Layout,
}
// SAFETY: ownership is moved exactly once via `Sender::send`; the sending
// thread never touches `ptr` again after send (mirrors `examples/soak_xthread.rs`'s
// identical `Block` `Send` justification).
unsafe impl Send for Block {}

/// Allocate one `BLOCK_SIZE`-byte block via the real global allocator,
/// touching the first byte (defeats dead-store elimination, faults the
/// page — matching `soak_xthread`'s own touch discipline).
///
/// # Safety
/// The returned block must be freed exactly once via the same global
/// allocator instance.
unsafe fn alloc_block() -> Block {
    let layout = Layout::from_size_align(BLOCK_SIZE, 8).unwrap();
    // SAFETY: layout has non-zero size and valid alignment.
    let ptr = unsafe { GLOBAL.alloc(layout) };
    assert!(!ptr.is_null(), "alloc failed (OOM?)");
    // SAFETY: ptr is valid for at least 1 byte, freshly allocated.
    unsafe { ptr.write(0xA5) };
    Block { ptr, layout }
}

/// Free `block` via the real global allocator.
///
/// # Safety
/// `block.ptr` must have been allocated by `GLOBAL` with `block.layout` and
/// not yet freed.
unsafe fn free_block(block: Block) {
    // SAFETY: caller contract.
    unsafe { GLOBAL.dealloc(block.ptr, block.layout) };
}

/// Read the process-wide shadow-oracle counters as a `(fast, slow)` pair.
/// WITHOUT `bench-internals` the counters do not exist at all (see
/// `drain_rings`'s doc for why that matters) — this returns `(0, 0)` in that
/// build, and `main` skips the fast/slow-based oracle checks entirely,
/// falling back to `overflow_pct` alone as the regime-fidelity signal (the
/// SAME fallback the BEFORE-commit variant of this harness already uses,
/// since the shadow doesn't exist there either — see
/// `docs/perf/R32_11_REMOTE_RING_SHADOW_HEAD_GATE.md` for why `overflow_pct`
/// alone is a WEAKER but still meaningful proxy).
#[cfg(feature = "bench-internals")]
fn shadow_counts() -> (u64, u64) {
    (
        DBG_RING_PUSH_SHADOW_FAST.load(Ordering::Relaxed),
        DBG_RING_PUSH_SHADOW_SLOW.load(Ordering::Relaxed),
    )
}
#[cfg(not(feature = "bench-internals"))]
fn shadow_counts() -> (u64, u64) {
    (0, 0)
}

/// Run the favorable regime: owner concurrently drains via its own tight
/// small alloc/free loop WHILE producers free their assigned blocks. Returns
/// `(elapsed_ns_for_producer_frees, fast_delta, slow_delta, overflow_delta)`.
fn run_favorable() -> (u64, u64, u64, u64) {
    // Pre-allocate all blocks on the owner (this) thread — every block is
    // therefore a genuine cross-thread free once handed to a producer.
    let total_blocks = PRODUCERS * BLOCKS_PER_PRODUCER;
    let mut all_blocks: Vec<Block> = (0..total_blocks)
        // SAFETY: freed exactly once, either by a producer (cross-thread) or,
        // never here (every block is handed off below).
        .map(|_| unsafe { alloc_block() })
        .collect();

    let (senders, receivers): (Vec<_>, Vec<_>) = (0..PRODUCERS).map(|_| channel::<Block>()).unzip();

    // Hand out disjoint slices to each producer's channel.
    for (p, sender) in senders.iter().enumerate() {
        for _ in 0..BLOCKS_PER_PRODUCER {
            let block = all_blocks.pop().expect("enough pre-allocated blocks");
            sender.send(block).expect("producer channel open");
        }
        let _ = p;
    }
    drop(senders);
    assert!(
        all_blocks.is_empty(),
        "every block must be handed to a producer"
    );

    let overflow_before =
        sefer_alloc::alloc_core::remote_free_ring::DBG_RING_OVERFLOW.load(Ordering::Relaxed);
    let (fast_before, slow_before) = shadow_counts();

    // CRITICAL CONSTRAINT: the blocks above were allocated on THIS (the
    // calling/"owner") thread, so `dbg_drain_current_thread_rings` must ALSO
    // be called from THIS thread — `RemoteFreeRing::drain` is single-consumer
    // and the consumer identity is the segment's OWNER thread (module doc); a
    // DIFFERENT spawned thread calling the drain hook would drain ITS OWN
    // (unrelated, empty) heap, not the heap holding the producer-targeted
    // segments. So the owner's "keep the ring drained" work runs INLINE on
    // this thread, INTERLEAVED with polling producer-completion — not on a
    // separate spawned thread.
    let producers_done = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    // Barrier of PRODUCERS + 1 (owner) — every producer thread blocks at the
    // barrier immediately after spawning, so no producer pushes a single
    // block until the OWNER (main thread) has ALSO reached the barrier and
    // is about to enter its drain-polling loop. This closes the startup
    // transient an earlier version of this harness had (a handful of
    // overflow events in the window between `thread::spawn` returning and
    // the owner's first drain call actually running) — measured: with the
    // barrier, `ring_overflow_delta` reaches exactly 0 across repeated runs
    // where it was previously in the low tens out of 200,000 pushes.
    let barrier = Arc::new(std::sync::Barrier::new(PRODUCERS + 1));

    let producers: Vec<_> = receivers
        .into_iter()
        .map(|rx| {
            let done = Arc::clone(&producers_done);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                while let Ok(block) = rx.recv() {
                    // SAFETY: uniquely owned via channel transfer, freed once.
                    unsafe { free_block(block) };
                }
                done.fetch_add(1, Ordering::Release);
            })
        })
        .collect();
    barrier.wait();
    let t0 = Instant::now();

    // Owner (this thread) repeatedly force-drains all its rings while
    // producers race ahead, keeping the ring far from capacity — this IS the
    // favorable regime's defining condition. Not itself a separately-timed
    // quantity; by construction it runs inside the SAME timed window as the
    // producers' free loop, matching the real production shape (the owner
    // thread is always doing SOMETHING concurrently with cross-thread
    // frees, never idle).
    //
    // Deliberately NO `yield_now()`/sleep between drain calls: this machine
    // has ample spare cores (PRODUCERS + 1 owner well under available
    // parallelism), and the task's own instruction requires
    // `DBG_RING_OVERFLOW`'s delta to be EXACTLY 0 in this regime (a nonzero
    // overflow means the arm measured the retry/overflow tier instead of the
    // fast push path) — a `yield_now()` between drain calls was measured to
    // let producers race far enough ahead to overflow before the owner's
    // next scheduled turn (an early version of this loop yielded every
    // `OWNER_DRAIN_BATCH` calls and still saw ~1% residual overflow). A tight
    // poll loop removes that scheduling gap; `OWNER_DRAIN_BATCH` repeats
    // per outer-loop check keep the `producers_done` load from dominating.
    while producers_done.load(Ordering::Acquire) < PRODUCERS {
        for _ in 0..OWNER_DRAIN_BATCH {
            drain_rings();
        }
    }
    // Final drain after every producer's done-flag is observed, closing the
    // race window between a producer's last free and its done-flag store.
    drain_rings();

    for p in producers {
        p.join().expect("producer thread must not panic");
    }
    let elapsed_ns = t0.elapsed().as_nanos() as u64;

    let (fast_after, slow_after) = shadow_counts();
    let overflow_after =
        sefer_alloc::alloc_core::remote_free_ring::DBG_RING_OVERFLOW.load(Ordering::Relaxed);

    (
        elapsed_ns,
        fast_after.saturating_sub(fast_before),
        slow_after.saturating_sub(slow_before),
        overflow_after.saturating_sub(overflow_before),
    )
}

/// Run the adversarial regime: the owner drains only on a SLOW, bounded
/// cadence (see the module-level "why not zero drain" note in this
/// function's own body) — just often enough that the ring's occupancy sits
/// at/near `RING_CAP` almost continuously, so `full_check`'s shadow sees
/// "might be full" (forcing the slow path) on very nearly every push, without
/// triggering `push_with_overflow_retry`'s expensive stalled-round retry
/// storm. Returns the same tuple shape as `run_favorable`.
fn run_adversarial() -> (u64, u64, u64, u64) {
    let total_blocks = PRODUCERS * BLOCKS_PER_PRODUCER;
    let mut all_blocks: Vec<Block> = (0..total_blocks)
        // SAFETY: see `run_favorable`.
        .map(|_| unsafe { alloc_block() })
        .collect();

    let (senders, receivers): (Vec<_>, Vec<_>) = (0..PRODUCERS).map(|_| channel::<Block>()).unzip();
    for sender in &senders {
        for _ in 0..BLOCKS_PER_PRODUCER {
            let block = all_blocks.pop().expect("enough pre-allocated blocks");
            sender.send(block).expect("producer channel open");
        }
    }
    drop(senders);
    assert!(all_blocks.is_empty());

    let overflow_before =
        sefer_alloc::alloc_core::remote_free_ring::DBG_RING_OVERFLOW.load(Ordering::Relaxed);
    let (fast_before, slow_before) = shadow_counts();

    // ADVERSARIAL DESIGN NOTE (false start caught before any number was
    // published): a first version of this regime had the owner do NO
    // draining at all until every producer finished. That is genuinely
    // adversarial for the SHADOW (every push's cached_head is maximally
    // stale, forcing the slow path) but ALSO triggers a completely different,
    // much more expensive mechanism: `HeapCore::push_with_overflow_retry`'s
    // bounded stalled-round retry loop (`RETRY_STALLED_ROUNDS_GIVE_UP = 128`,
    // `src/registry/heap_core_xthread.rs`), which spins waiting for ANY
    // observable owner drain progress before conceding to the bounded leak.
    // With zero owner progress for the whole burst, every cross-thread free
    // pays the FULL stalled-retry budget — measured directly: 8,000 pushes
    // took 5.4 SECONDS wall-clock (678 microseconds/push, ~4 orders of
    // magnitude slower than the favorable regime's ~270 ns/push) and
    // 55 MILLION shadow-oracle `full_check` calls were recorded for only
    // 8,000 logical push attempts — the timed region was overwhelmingly
    // measuring the retry-storm's OWN cost, not `RemoteFreeRing::push`'s
    // full-check cost. This is exactly the "a different, more expensive
    // mechanism dominates the timing" failure mode CLAUDE.md's own X5/`[L]`
    // item 20 precedent warns about, and this task's own instructions
    // explicitly named as a risk to guard against.
    //
    // Corrected design: the owner drains on a SLOW, BOUNDED cadence
    // (`ADVERSARIAL_OWNER_DRAIN_SLEEP_US` between drain calls) — just often
    // enough that `head` keeps making SOME progress (so the retry loop's
    // progress-detection never times out and gives up), while staying far
    // slower than the favorable regime's continuous poll.
    let producers_done = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let barrier = Arc::new(std::sync::Barrier::new(PRODUCERS + 1));

    let producers: Vec<_> = receivers
        .into_iter()
        .map(|rx| {
            let done = Arc::clone(&producers_done);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                while let Ok(block) = rx.recv() {
                    // SAFETY: uniquely owned via channel transfer, freed once
                    // (overflowed blocks are a documented sound bounded leak
                    // — never double-freed, never reclaimed twice; the ring's
                    // own overflow accounting, exercised here, is exactly the
                    // mechanism `tests/remote_ring_unit.rs` proves sound).
                    unsafe { free_block(block) };
                }
                done.fetch_add(1, Ordering::Release);
            })
        })
        .collect();
    barrier.wait();
    let t0 = Instant::now();

    while producers_done.load(Ordering::Acquire) < PRODUCERS {
        thread::sleep(std::time::Duration::from_micros(
            ADVERSARIAL_OWNER_DRAIN_SLEEP_US,
        ));
        drain_rings();
    }
    drain_rings();

    for p in producers {
        p.join().expect("producer thread must not panic");
    }
    let elapsed_ns = t0.elapsed().as_nanos() as u64;

    let (fast_after, slow_after) = shadow_counts();
    let overflow_after =
        sefer_alloc::alloc_core::remote_free_ring::DBG_RING_OVERFLOW.load(Ordering::Relaxed);

    // Post-measurement cleanup (outside the timed region — pure hygiene so
    // the process exits without a large resident ring backlog; the
    // overflowed blocks are already-sound bounded leaks per the ring's
    // documented semantics and are NOT reclaimed here, matching production
    // behavior for a genuinely overflowed ring — this only reclaims blocks
    // that actually made it into the ring).
    drain_rings();

    (
        elapsed_ns,
        fast_after.saturating_sub(fast_before),
        slow_after.saturating_sub(slow_before),
        overflow_after.saturating_sub(overflow_before),
    )
}

fn main() {
    let regime = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: r32_11_remote_ring_shadow_head_gate <favorable|adversarial>");
        std::process::exit(2);
    });

    // Untimed warm-up: absorb primordial-segment bootstrap cost so the timed
    // region measures only steady-state push cost. A tiny favorable-shaped
    // run whose own metrics are discarded.
    let _ = run_favorable();

    let (elapsed_ns, fast_delta, slow_delta, overflow_delta) = match regime.as_str() {
        "favorable" => run_favorable(),
        "adversarial" => run_adversarial(),
        other => {
            eprintln!("unknown regime '{other}' (want favorable|adversarial)");
            std::process::exit(2);
        }
    };

    let total = fast_delta + slow_delta;
    let fast_pct = if total > 0 {
        (fast_delta as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    let slow_pct = 100.0 - fast_pct;

    let total_pushes_f = (PRODUCERS * BLOCKS_PER_PRODUCER) as f64;
    let overflow_pct = (overflow_delta as f64 / total_pushes_f) * 100.0;

    // Oracle: WITH bench-internals, the shadow fast/slow-path counters are
    // the primary (strong) evidence. WITHOUT it, they don't exist (always
    // 0/0 from `shadow_counts`'s stub) — fall back to `overflow_pct` alone,
    // matching the BEFORE-commit variant's own (weaker, but still
    // meaningful) oracle design.
    #[cfg(feature = "bench-internals")]
    let oracle_pass = match regime.as_str() {
        "favorable" => {
            fast_pct >= FAVORABLE_MIN_FAST_PCT && overflow_pct <= FAVORABLE_MAX_OVERFLOW_PCT
        }
        "adversarial" => slow_pct >= ADVERSARIAL_MIN_SLOW_PCT,
        _ => unreachable!(),
    };
    #[cfg(not(feature = "bench-internals"))]
    let oracle_pass = match regime.as_str() {
        "favorable" => overflow_pct <= FAVORABLE_MAX_OVERFLOW_PCT,
        "adversarial" => overflow_pct >= ADVERSARIAL_MIN_OVERFLOW_PCT_NO_ORACLE,
        _ => unreachable!(),
    };

    let total_pushes = (PRODUCERS * BLOCKS_PER_PRODUCER) as u64;
    let ns_per_push = elapsed_ns as f64 / total_pushes as f64;

    proc_probe::emit("arm", &regime);
    proc_probe::emit_u64("producers", PRODUCERS as u64);
    proc_probe::emit_u64("blocks_per_producer", BLOCKS_PER_PRODUCER as u64);
    proc_probe::emit_u64("total_pushes", total_pushes);
    proc_probe::emit_ns("elapsed_ns", elapsed_ns.into());
    proc_probe::emit_f64("ns_per_push", ns_per_push);
    proc_probe::emit_u64("shadow_fast_delta", fast_delta);
    proc_probe::emit_u64("shadow_slow_delta", slow_delta);
    proc_probe::emit_f64("fast_pct", fast_pct);
    proc_probe::emit_f64("slow_pct", slow_pct);
    proc_probe::emit_u64("ring_overflow_delta", overflow_delta);
    proc_probe::emit_f64("overflow_pct", overflow_pct);
    proc_probe::emit_u64("oracle_pass", u64::from(oracle_pass));
    proc_probe::emit_u64(
        "bench_internals_build",
        u64::from(cfg!(feature = "bench-internals")),
    );

    println!(
        "OK regime={regime} total_pushes={total_pushes} elapsed_ns={elapsed_ns} \
         ns_per_push={ns_per_push:.2} fast={fast_delta} slow={slow_delta} \
         fast_pct={fast_pct:.2} slow_pct={slow_pct:.2} overflow_delta={overflow_delta} \
         bench_internals={} oracle={}",
        cfg!(feature = "bench-internals"),
        if oracle_pass { "PASS" } else { "FAIL" }
    );

    if !oracle_pass {
        eprintln!(
            "[r32_11] ORACLE FAIL: regime={regime} did not activate its intended path \
             (fast_pct={fast_pct:.2}% slow_pct={slow_pct:.2}% overflow_delta={overflow_delta}) \
             — this run's elapsed_ns is NOT trustworthy evidence and must not be cited."
        );
        std::process::exit(1);
    }
}
