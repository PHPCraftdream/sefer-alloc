//! Phase 11 -- `SeferAlloc` as `#[global_allocator]` vs `mimalloc` and the
//! system allocator. Quick criterion profile per the short-scenario policy:
//! `sample_size(10)` and short warm/measurement times. Honest verdict in
//! `docs/ALLOC_BENCH.md`.
//!
//! This bench exercises REAL Rust allocation patterns through the
//! `#[global_allocator]` face: `Vec` push/grow churn (which calls
//! `alloc`/`dealloc`/`realloc` under the hood), `Box` new/drop, and varied
//! sizes. We compare three configurations:
//!
//! 1. **SeferAlloc** (installed as the process's `#[global_allocator]`).
//! 2. **mimalloc** (via the `mimalloc` crate's `GlobalAlloc` impl, called
//!    directly -- NOT installed globally, to allow a head-to-head in one binary).
//! 3. **System** allocator (called directly).
//!
//! For (2) and (3) we call the `GlobalAlloc` methods directly to avoid
//! replacing the global allocator mid-process (which SeferAlloc already
//! occupies). This is an honest apples-to-apples comparison of the alloc/dealloc
//! hot path.
//!
//! ## Two confounds fixed here (performance review §4.1 items 2-3, §10 Stage B.2)
//!
//! 1. **Cross-group TLS heap state.** `criterion_main!` runs every
//!    `benchmark_group()` function below in the SAME process, on the SAME
//!    thread. `SeferAlloc::new()` is a zero-cost constructor -- it does NOT
//!    create an independent heap; every call on this thread resolves to the
//!    SAME per-thread `HeapCore` via TLS (see `src/global/tls_heap.rs`'s
//!    fast path). Without a reset, an EARLIER group's leftover tcache/pool/
//!    large-cache/segment state is still resident when a LATER group starts,
//!    biasing that group's numbers. Fixed by calling
//!    `SeferAlloc::dbg_trim_current_thread` (a `#[doc(hidden)]` test/bench
//!    hook delegating to the production `HeapCore::trim_for_recycle`
//!    teardown-trim primitive) at the start of every group that measures
//!    SeferAlloc, so each group starts from a comparable near-empty
//!    baseline. Chosen over fresh-subprocess-per-group isolation: this bench
//!    runs in a couple of minutes total (short-scenario policy, see
//!    `CLAUDE.md`), and an in-process trim is a handful of extra segment
//!    releases per group -- negligible next to spawning `cargo bench`'s
//!    harness N times, and it keeps `scripts/bench-table.mjs`'s single-
//!    invocation contract (one `cargo bench` run, one stdout to parse)
//!    intact.
//! 2. **Fixed SeferAlloc/mimalloc/System order.** Every 3-arm block
//!    previously registered SeferAlloc, then mimalloc, then System, in that
//!    exact order, every time -- so any monotonic host drift (frequency
//!    scaling, thermal throttling, background load) would systematically
//!    bias whichever arm always runs first/last rather than averaging out.
//!    Fixed by [`arm_rotation`]: a rotation index computed once per process
//!    (seeded from `SystemTime`, varying run to run -- this is bench-harness
//!    setup, not a hot allocation path, so a wall-clock seed is fine here
//!    even though the crate avoids that pattern in `alloc`/`dealloc`) that
//!    picks one of the 3! = 6 permutations of `[SeferAlloc, mimalloc,
//!    System]`, advanced deterministically per `(group, size)` call so
//!    consecutive registrations within one run also rotate. Simpler to
//!    reason about than per-call true randomization while still breaking
//!    "always the same arm benefits/suffers from drift".

#![cfg(feature = "alloc-global")]
#![allow(
    clippy::cast_possible_truncation,
    clippy::needless_pass_by_value,
    clippy::semicolon_if_nothing_returned
)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use sefer_alloc::{AllocCore, SeferAlloc};

/// Representative small-to-medium sizes for the churn bench.
const SIZES: &[usize] = &[16, 64, 256, 1024];

/// Number of alloc/dealloc pairs per iteration.
const OPS: usize = 1024;

/// Working-set size for the churn bench: how many live blocks are maintained.
/// 256 is small enough to fit in a future tcache, large enough to be meaningful.
const CHURN_WORKING_SET: usize = 256;

/// Deterministic, dependency-free PRNG (xorshift64*). Fixed seed for
/// reproducible benchmark runs. No external `rand` crate needed.
struct XorShift64(u64);

impl XorShift64 {
    const fn new(seed: u64) -> Self {
        // Avoid the all-zero fixed point.
        Self(seed | 1)
    }

    #[inline]
    fn next_usize(&mut self) -> usize {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D) as usize
    }
}

/// One of the three allocator arms compared throughout this bench file.
/// Used only to describe/permute REGISTRATION ORDER (confound 2, see the
/// module doc) -- not a `GlobalAlloc` abstraction; each arm's actual bench
/// closure is still written by hand against its concrete type
/// (`SeferAlloc` / `mimalloc::MiMalloc` / `System`), so the hot path being
/// measured is byte-identical to before this fix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arm {
    Sefer,
    Mimalloc,
    System,
}

/// All 3! = 6 permutations of the three arms, fixed and enumerable so
/// [`arm_rotation`] can pick one deterministically by index.
const ARM_PERMUTATIONS: [[Arm; 3]; 6] = [
    [Arm::Sefer, Arm::Mimalloc, Arm::System],
    [Arm::Sefer, Arm::System, Arm::Mimalloc],
    [Arm::Mimalloc, Arm::Sefer, Arm::System],
    [Arm::Mimalloc, Arm::System, Arm::Sefer],
    [Arm::System, Arm::Sefer, Arm::Mimalloc],
    [Arm::System, Arm::Mimalloc, Arm::Sefer],
];

/// Confound 2 fix (module doc item 2): a monotonically advancing rotation
/// index, ONE GLOBAL COUNTER shared by every `(group, size)` 3-arm block in
/// this file. Each call returns the next permutation of
/// [`ARM_PERMUTATIONS`], so consecutive registrations rotate through all 6
/// orders instead of always running SeferAlloc -> mimalloc -> System.
///
/// Seeded once per process from `SystemTime` (wall-clock is fine here -- this
/// is bench-harness registration code that runs a handful of times per
/// process, not the `alloc`/`dealloc` hot path the rest of the crate
/// deliberately keeps free of wall-clock reads) so the STARTING permutation
/// also varies run to run, rather than every invocation of `cargo bench`
/// beginning at the same fixed order.
fn arm_rotation() -> [Arm; 3] {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicUsize = AtomicUsize::new(usize::MAX);

    // Lazily seed the counter from the wall clock on first use; every
    // subsequent call just advances it. `compare_exchange` avoids a data
    // race if criterion ever called this from more than one thread (it
    // doesn't today -- `criterion_main!` runs groups sequentially on the
    // main thread -- but the seed-once logic is cheap to make robust anyway).
    let mut seed = COUNTER.load(Ordering::Relaxed);
    if seed == usize::MAX {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as usize)
            .unwrap_or(0);
        let init = now % ARM_PERMUTATIONS.len();
        match COUNTER.compare_exchange(usize::MAX, init, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => seed = init,
            Err(actual) => seed = actual,
        }
    }
    let idx = seed % ARM_PERMUTATIONS.len();
    COUNTER.store((seed + 1) % ARM_PERMUTATIONS.len(), Ordering::Relaxed);
    ARM_PERMUTATIONS[idx]
}

/// Register a 3-arm `bench_function` triple in the rotated order returned by
/// [`arm_rotation`], instead of the fixed SeferAlloc -> mimalloc -> System
/// order. `sefer_fn`/`mi_fn`/`sys_fn` are `FnMut(&mut criterion::Bencher)`
/// closures, one per arm, built by the caller exactly as before (same body,
/// same captures) -- this helper only changes the ORDER `group.bench_function`
/// is called in, not what each closure measures.
fn bench_three_arms_rotated<'a>(
    group: &mut criterion::BenchmarkGroup<'a, criterion::measurement::WallTime>,
    sefer_name: String,
    mut sefer_fn: impl FnMut(&mut criterion::Bencher) + 'a,
    mi_name: String,
    mut mi_fn: impl FnMut(&mut criterion::Bencher) + 'a,
    sys_name: String,
    mut sys_fn: impl FnMut(&mut criterion::Bencher) + 'a,
) {
    for arm in arm_rotation() {
        match arm {
            Arm::Sefer => group.bench_function(sefer_name.clone(), &mut sefer_fn),
            Arm::Mimalloc => group.bench_function(mi_name.clone(), &mut mi_fn),
            Arm::System => group.bench_function(sys_name.clone(), &mut sys_fn),
        };
    }
}

/// Direct-alloc bench: alloc + dealloc OPS blocks of `layout` through `alloc`.
/// This bypasses Vec overhead and measures the raw hot path.
fn bench_direct_alloc<A: GlobalAlloc>(alloc: &A, layout: Layout) {
    let mut ptrs: [*mut u8; OPS] = [core::ptr::null_mut(); OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid alignment.
        *slot = unsafe { alloc.alloc(layout) };
    }
    black_box(&ptrs);
    for &ptr in &ptrs {
        // SAFETY: ptr was allocated by `alloc` with the same layout.
        if !ptr.is_null() {
            unsafe { alloc.dealloc(ptr, layout) };
        }
    }
}

/// Pre-fill a working set of `working_set` live blocks for the churn benches.
/// This is the COLD phase (first-touch page faults, BinTable carve) that F7
/// pulled OUT of the timed region: the churn benches time only the
/// steady-state `churn_step` loop via `iter_batched`, so the cold prefill and
/// the teardown no longer contaminate the reported ns/op.
fn churn_prefill<A: GlobalAlloc>(alloc: &A, layout: Layout, working_set: usize) -> Vec<*mut u8> {
    let mut live: Vec<*mut u8> = Vec::with_capacity(working_set);
    for _ in 0..working_set {
        // SAFETY: layout has non-zero size and valid alignment.
        let p = unsafe { alloc.alloc(layout) };
        live.push(p);
    }
    live
}

/// Teardown: free every block in the working set.
///
/// PERF-PASS-1 (task #49, G3/A2a): historically this was called from INSIDE
/// the `iter_batched` routine closure at each call site below
/// (`churn_step(...); churn_teardown(...)`), which `iter_batched` DOES time —
/// it only excludes `setup`'s time, not arbitrary code the routine closure
/// itself runs. At 1024B under `alloc-decommit` this made teardown ~85% of
/// the reported "churn" ns/op (~183us of ~208us total, per the churn-reuse
/// review's phase-split measurement), because freeing the last live block in
/// a non-current small segment triggers the decommit->release->re-reserve
/// cycle. `bench_global_alloc_churn`/`_write` below now use
/// [`ChurnTeardownGuard`] instead: the routine closure returns a guard
/// wrapping `live`, and `iter_batched` times only the guard's construction
/// (trivial move) — the guard's `Drop` impl runs `churn_teardown` OUTSIDE the
/// timed region, exactly matching how `churn_prefill` (setup) is already
/// excluded. One variant, [`bench_global_alloc_churn_with_teardown`], is kept
/// DELIBERATELY unconverted (still times teardown inline) as a diagnostic
/// signal for later Mechanism-2 (task #51) work — not a bug, see its own doc
/// comment.
fn churn_teardown<A: GlobalAlloc>(alloc: &A, layout: Layout, live: &[*mut u8]) {
    for &p in live {
        if !p.is_null() {
            // SAFETY: `p` was allocated by `alloc` with the same layout.
            unsafe { alloc.dealloc(p, layout) };
        }
    }
}

/// PERF-PASS-1 (task #49, G3/A2a): drop-guard that frees a churn working set
/// OUTSIDE criterion's timed region. `iter_batched`'s routine closure returns
/// this guard; criterion times only the closure's execution (including the
/// guard's cheap construction/move), then drops the returned value AFTER
/// stopping the clock for that iteration. See `churn_teardown`'s doc comment
/// for why this matters at 1024B under `alloc-decommit`.
struct ChurnTeardownGuard<'a, A: GlobalAlloc> {
    alloc: &'a A,
    layout: Layout,
    live: Vec<*mut u8>,
}

impl<'a, A: GlobalAlloc> Drop for ChurnTeardownGuard<'a, A> {
    fn drop(&mut self) {
        churn_teardown(self.alloc, self.layout, &self.live);
    }
}

/// Churn bench: maintain a working set of `working_set` live blocks; each of
/// `ops` iterations frees a pseudo-random block and allocates a replacement.
/// This is the steady-state pattern a per-thread magazine (tcache) wins on:
/// freed blocks re-enter the cache and are re-allocated without round-tripping
/// the BinTable. Fixed PRNG seed = 0xCAFE for reproducibility. This loop's
/// prefill is untimed (F7 fix). At most call sites below (`bench_global_alloc_churn`
/// and `_write`), teardown ALSO runs outside the timed region — the routine
/// closure returns a [`ChurnTeardownGuard`] instead of calling
/// `churn_teardown` inline, so `iter_batched` times only `ops` churn pairs,
/// and the reported ns/op divides cleanly by `ops` with no teardown skew. The
/// ONE exception is [`bench_global_alloc_churn_with_teardown`], which calls
/// `churn_teardown` directly inside the timed routine as a DELIBERATE
/// diagnostic (see its own doc comment and [`churn_teardown`]'s) — only that
/// variant's timed region does `ops` churn pairs plus `CHURN_WORKING_SET`
/// (256) teardown frees, so only ITS reported ns/op includes teardown cost.
fn churn_step<A: GlobalAlloc>(alloc: &A, layout: Layout, live: &mut [*mut u8], ops: usize) {
    let working_set = live.len();
    let mut rng = XorShift64::new(0xCAFE);

    // Churn: free a random slot, alloc a replacement.
    for _ in 0..ops {
        let idx = rng.next_usize() % working_set;
        let old = live[idx];
        if !old.is_null() {
            // SAFETY: `old` was allocated by `alloc` with the same layout.
            unsafe { alloc.dealloc(old, layout) };
        }
        // SAFETY: layout has non-zero size and valid alignment.
        live[idx] = unsafe { alloc.alloc(layout) };
    }

    black_box(&live);
}

/// Write-prefill: like `churn_prefill` but writes the first 16 bytes of each
/// freshly allocated block (see `churn_step_write` for the rationale). Cold
/// phase, pulled OUT of the timed region (F7).
fn churn_prefill_write<A: GlobalAlloc>(
    alloc: &A,
    layout: Layout,
    working_set: usize,
) -> Vec<*mut u8> {
    let mut live: Vec<*mut u8> = Vec::with_capacity(working_set);
    for _ in 0..working_set {
        // SAFETY: layout has non-zero size and valid alignment.
        let p = unsafe { alloc.alloc(layout) };
        if !p.is_null() {
            // SAFETY: `p` is a freshly allocated block of `layout` (size >= 16
            // for every bench size), so the first 16 bytes are in bounds and
            // writable. `write_volatile` prevents the store being elided.
            unsafe { core::ptr::write_volatile(p.cast::<u64>(), 0xA5A5_A5A5_A5A5_A5A5) };
            unsafe { core::ptr::write_volatile(p.cast::<u64>().add(1), 0xA5A5_A5A5_A5A5_A5A5) };
        }
        live.push(p);
    }
    live
}

/// Writing-churn bench: an EXACT clone of `churn_step` except that immediately
/// after every non-null `alloc` it writes the first 16 bytes (two u64 words at
/// offset 0 and 8) of the freshly allocated block. This dirties word1 (bytes
/// 8..16 — the magazine M2 double-free-guard key location) so the realistic
/// write-to-what-you-allocate pattern is measured, instead of leaving a stale
/// key that forces a slow-path scan on every free. Fixed PRNG seed = 0xCAFE
/// (identical to the non-writing bench) for reproducibility. Only this loop is
/// timed (F7).
fn churn_step_write<A: GlobalAlloc>(alloc: &A, layout: Layout, live: &mut [*mut u8], ops: usize) {
    let working_set = live.len();
    let mut rng = XorShift64::new(0xCAFE);

    // Churn: free a random slot, alloc a replacement, write into it.
    for _ in 0..ops {
        let idx = rng.next_usize() % working_set;
        let old = live[idx];
        if !old.is_null() {
            // SAFETY: `old` was allocated by `alloc` with the same layout.
            unsafe { alloc.dealloc(old, layout) };
        }
        // SAFETY: layout has non-zero size and valid alignment.
        let p = unsafe { alloc.alloc(layout) };
        if !p.is_null() {
            // SAFETY: `p` is a freshly allocated block of `layout` (size >= 16
            // for every bench size), so the first 16 bytes are in bounds and
            // writable. `write_volatile` prevents the store being elided.
            unsafe { core::ptr::write_volatile(p.cast::<u64>(), 0xA5A5_A5A5_A5A5_A5A5) };
            unsafe { core::ptr::write_volatile(p.cast::<u64>().add(1), 0xA5A5_A5A5_A5A5_A5A5) };
        }
        live[idx] = p;
    }

    black_box(&live);
}

fn bench_global_alloc(c: &mut Criterion) {
    let mut group = c.benchmark_group("global_alloc");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    let sefer = SeferAlloc::new();
    let mi = mimalloc::MiMalloc;
    let sys = System;

    // Confound 1 fix (module doc item 1): this is the FIRST group
    // `criterion_main!` runs, so the trim here is a no-op on a fresh process
    // (nothing to reset yet) -- kept anyway so `bench_global_alloc` is
    // self-consistent with every other group and safe to reorder/re-invoke
    // independently (e.g. via a criterion `--bench` filter).
    sefer.dbg_trim_current_thread();

    // `&sefer`/`&mi`/`&sys` themselves are `Copy` (shared references), so
    // `move` closures below capture these BY VALUE (a pointer copy) instead
    // of borrowing `sefer`/`mi`/`sys` for the group's whole lifetime `'a` --
    // that lets each loop iteration build fresh closures without fighting
    // the borrow checker over `layout` (loop-local, shorter-lived than `'a`)
    // needing to be captured by value too.
    let sefer_ref = &sefer;
    let mi_ref = &mi;
    let sys_ref = &sys;

    for &size in SIZES {
        let layout = Layout::from_size_align(size, 8).unwrap();

        // --- SeferAlloc / mimalloc / System (each called directly through its
        // GlobalAlloc impl -- a true apples-to-apples comparison of the
        // alloc/dealloc hot path; SeferAlloc is NOT installed as the bench
        // binary's global allocator, so we must call it directly).
        // Confound 2 fix (module doc item 2): registration order is rotated
        // per `(group, size)` via `bench_three_arms_rotated` instead of
        // always SeferAlloc -> mimalloc -> System. ---
        bench_three_arms_rotated(
            &mut group,
            format!("SeferAlloc/{size}B"),
            move |b| b.iter(|| bench_direct_alloc(sefer_ref, layout)),
            format!("mimalloc/{size}B"),
            move |b| b.iter(|| bench_direct_alloc(mi_ref, layout)),
            format!("System/{size}B"),
            move |b| b.iter(|| bench_direct_alloc(sys_ref, layout)),
        );
    }

    // --- Real-world pattern: Vec<i64> push/grow churn ---
    // Honest geometric growth: capacity doubles (4, 8, 16, … 512) exactly as
    // `Vec` does, so this exercises realloc + many small allocs as the Vec
    // grows — capacity sequence 4, 8, 16, 32, 64, 128, 256, 512 per closure
    // call: 8 allocs total (the initial 4-element alloc, plus 7 growth
    // reallocs — each an alloc-new + copy-old + dealloc-old), NOT a single
    // jump straight to the final 4 KiB. `old_layout` is
    // tracked honestly across steps (mirroring the System arm) so every
    // dealloc matches the layout its block was allocated with.
    const VEC_PUSHES: usize = 512;
    // Confound 2 fix: rotated registration order (see the module doc and
    // `bench_direct_alloc`'s call site above) — same three closures as
    // before, just no longer always registered SeferAlloc -> mimalloc ->
    // System.
    bench_three_arms_rotated(
        &mut group,
        "Vec_push/SeferAlloc".to_string(),
        |b| {
            b.iter(|| {
                // Manual Vec growth through SeferAlloc's GlobalAlloc directly, so
                // the measurement is SeferAlloc (not the bench binary's default
                // global allocator) — symmetric with the mimalloc/System arms below.
                let mut ptr: *mut i64 = core::ptr::null_mut();
                let mut cap: usize = 0;
                let mut len: usize = 0;
                for i in 0..VEC_PUSHES {
                    if len == cap {
                        let new_cap = if cap == 0 { 4 } else { cap * 2 };
                        let new_layout = Layout::array::<i64>(new_cap).unwrap();
                        // SAFETY: realloc-like growth through SeferAlloc.
                        let new_ptr = unsafe { sefer.alloc(new_layout) };
                        if !new_ptr.is_null() && !ptr.is_null() {
                            let old_layout = Layout::array::<i64>(cap).unwrap();
                            unsafe {
                                core::ptr::copy_nonoverlapping(ptr, new_ptr as *mut i64, len);
                                sefer.dealloc(ptr as *mut u8, old_layout);
                            }
                        }
                        ptr = new_ptr as *mut i64;
                        cap = new_cap;
                    }
                    // SAFETY: ptr is valid for `cap` elements if non-null.
                    if !ptr.is_null() {
                        unsafe { ptr.add(len).write(i as i64) };
                    }
                    len += 1;
                }
                black_box(ptr);
                black_box(len);
                if !ptr.is_null() {
                    let final_layout = Layout::array::<i64>(cap.max(1)).unwrap();
                    unsafe { sefer.dealloc(ptr as *mut u8, final_layout) };
                }
            })
        },
        "Vec_push/mimalloc".to_string(),
        |b| {
            b.iter(|| {
                // mimalloc is NOT the global allocator here (SeferAlloc is), so we
                // manually replicate Vec growth via mimalloc's GlobalAlloc.
                let mut ptr: *mut i64 = core::ptr::null_mut();
                let mut cap: usize = 0;
                let mut len: usize = 0;
                for i in 0..VEC_PUSHES {
                    if len == cap {
                        let new_cap = if cap == 0 { 4 } else { cap * 2 };
                        let new_layout = Layout::array::<i64>(new_cap).unwrap();
                        // SAFETY: realloc-like growth through mimalloc.
                        let new_ptr = unsafe { mi.alloc(new_layout) };
                        if !new_ptr.is_null() && !ptr.is_null() {
                            let old_layout = Layout::array::<i64>(cap).unwrap();
                            unsafe {
                                core::ptr::copy_nonoverlapping(ptr, new_ptr as *mut i64, len);
                                mi.dealloc(ptr as *mut u8, old_layout);
                            }
                        }
                        ptr = new_ptr as *mut i64;
                        cap = new_cap;
                    }
                    // SAFETY: ptr is valid for `cap` elements if non-null.
                    if !ptr.is_null() {
                        unsafe { ptr.add(len).write(i as i64) };
                    }
                    len += 1;
                }
                black_box(ptr);
                black_box(len);
                if !ptr.is_null() {
                    let final_layout = Layout::array::<i64>(cap.max(1)).unwrap();
                    unsafe { mi.dealloc(ptr as *mut u8, final_layout) };
                }
            })
        },
        "Vec_push/System".to_string(),
        |b| {
            b.iter(|| {
                let mut ptr: *mut i64 = core::ptr::null_mut();
                let mut cap: usize = 0;
                let mut len: usize = 0;
                for i in 0..VEC_PUSHES {
                    if len == cap {
                        let new_cap = if cap == 0 { 4 } else { cap * 2 };
                        let new_layout = Layout::array::<i64>(new_cap).unwrap();
                        // SAFETY: realloc-like growth through System.
                        let new_ptr = unsafe { sys.alloc(new_layout) };
                        if !new_ptr.is_null() && !ptr.is_null() {
                            let old_layout = Layout::array::<i64>(cap.max(1)).unwrap();
                            unsafe {
                                core::ptr::copy_nonoverlapping(ptr, new_ptr as *mut i64, len);
                                sys.dealloc(ptr as *mut u8, old_layout);
                            }
                        }
                        ptr = new_ptr as *mut i64;
                        cap = new_cap;
                    }
                    if !ptr.is_null() {
                        unsafe { ptr.add(len).write(i as i64) };
                    }
                    len += 1;
                }
                black_box(ptr);
                black_box(len);
                if !ptr.is_null() {
                    let final_layout = Layout::array::<i64>(cap.max(1)).unwrap();
                    unsafe { sys.dealloc(ptr as *mut u8, final_layout) };
                }
            })
        },
    );

    group.finish();
}

fn bench_global_alloc_churn(c: &mut Criterion) {
    let mut group = c.benchmark_group("global_alloc_churn");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    let sefer = SeferAlloc::new();
    let mi = mimalloc::MiMalloc;
    let sys = System;

    // Confound 1 fix: reset SeferAlloc's per-thread heap to a comparable
    // baseline before this group's measurements begin, so leftover
    // segment/tcache/pool/cache state from an EARLIER group in the same
    // `criterion_main!` run (e.g. `global_alloc`'s Cold-direct arm, which
    // runs before this group) does not bias "Churn, non-writing" — see the
    // module doc's confound 1 writeup.
    sefer.dbg_trim_current_thread();
    let sefer_ref = &sefer;
    let mi_ref = &mi;
    let sys_ref = &sys;

    for &size in SIZES {
        let layout = Layout::from_size_align(size, 8).unwrap();

        // F7 + PERF-PASS-1 (task #49, G3/A2a): `iter_batched` times ONLY the
        // steady-state churn loop (`OPS` op-pairs) plus the trivial guard
        // construction. Prefill (cold, first-touch) is the untimed setup and
        // teardown now runs in `ChurnTeardownGuard::drop`, which `iter_batched`
        // does NOT time (only the routine closure's own execution is timed) —
        // so the reported ns/op divides by exactly `OPS`, with neither the
        // ~25% cold-phase skew F7 fixed nor the ~85%-at-1024B teardown skew
        // this pass fixes. Confound 2 fix: rotated registration order (see
        // module doc) instead of fixed SeferAlloc -> mimalloc -> System.
        bench_three_arms_rotated(
            &mut group,
            format!("SeferAlloc/{size}B"),
            move |b| {
                b.iter_batched(
                    || churn_prefill(sefer_ref, layout, CHURN_WORKING_SET),
                    |mut live| {
                        churn_step(sefer_ref, layout, &mut live, OPS);
                        ChurnTeardownGuard {
                            alloc: sefer_ref,
                            layout,
                            live,
                        }
                    },
                    BatchSize::SmallInput,
                )
            },
            format!("mimalloc/{size}B"),
            move |b| {
                b.iter_batched(
                    || churn_prefill(mi_ref, layout, CHURN_WORKING_SET),
                    |mut live| {
                        churn_step(mi_ref, layout, &mut live, OPS);
                        ChurnTeardownGuard {
                            alloc: mi_ref,
                            layout,
                            live,
                        }
                    },
                    BatchSize::SmallInput,
                )
            },
            format!("System/{size}B"),
            move |b| {
                b.iter_batched(
                    || churn_prefill(sys_ref, layout, CHURN_WORKING_SET),
                    |mut live| {
                        churn_step(sys_ref, layout, &mut live, OPS);
                        ChurnTeardownGuard {
                            alloc: sys_ref,
                            layout,
                            live,
                        }
                    },
                    BatchSize::SmallInput,
                )
            },
        );
    }

    group.finish();
}

/// PERF-PASS-1 (task #49, G3/A2a): DELIBERATE diagnostic variant of
/// `bench_global_alloc_churn` that keeps teardown INSIDE the timed
/// `iter_batched` routine (the pre-fix behavior). This is not a leftover bug —
/// it is kept on purpose as the Mechanism-2 (task #51) signal: the gap
/// between this bench's ns/op and `global_alloc_churn`'s ns/op at the same
/// size IS the segment decommit/release/re-reserve lifecycle cost the
/// churn-reuse review measured (~183us of ~208us at 1024B under
/// `alloc-decommit`). Do not "fix" this bench to exclude teardown — that
/// would remove the only local signal for that cost class until task #51
/// lands Mechanism-2.
fn bench_global_alloc_churn_with_teardown(c: &mut Criterion) {
    let mut group = c.benchmark_group("global_alloc_churn_with_teardown");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    let sefer = SeferAlloc::new();
    let mi = mimalloc::MiMalloc;
    let sys = System;

    // Confound 1 fix — see `bench_global_alloc_churn`'s identical call.
    sefer.dbg_trim_current_thread();
    let sefer_ref = &sefer;
    let mi_ref = &mi;
    let sys_ref = &sys;

    for &size in SIZES {
        let layout = Layout::from_size_align(size, 8).unwrap();

        // Confound 2 fix: rotated registration order (see module doc).
        bench_three_arms_rotated(
            &mut group,
            format!("SeferAlloc/{size}B"),
            move |b| {
                b.iter_batched(
                    || churn_prefill(sefer_ref, layout, CHURN_WORKING_SET),
                    |mut live| {
                        churn_step(sefer_ref, layout, &mut live, OPS);
                        churn_teardown(sefer_ref, layout, &live);
                    },
                    BatchSize::SmallInput,
                )
            },
            format!("mimalloc/{size}B"),
            move |b| {
                b.iter_batched(
                    || churn_prefill(mi_ref, layout, CHURN_WORKING_SET),
                    |mut live| {
                        churn_step(mi_ref, layout, &mut live, OPS);
                        churn_teardown(mi_ref, layout, &live);
                    },
                    BatchSize::SmallInput,
                )
            },
            format!("System/{size}B"),
            move |b| {
                b.iter_batched(
                    || churn_prefill(sys_ref, layout, CHURN_WORKING_SET),
                    |mut live| {
                        churn_step(sys_ref, layout, &mut live, OPS);
                        churn_teardown(sys_ref, layout, &live);
                    },
                    BatchSize::SmallInput,
                )
            },
        );
    }

    group.finish();
}

fn bench_global_alloc_churn_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("global_alloc_churn_write");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    let sefer = SeferAlloc::new();
    let mi = mimalloc::MiMalloc;
    let sys = System;

    // Confound 1 fix — see `bench_global_alloc_churn`'s identical call.
    sefer.dbg_trim_current_thread();
    let sefer_ref = &sefer;
    let mi_ref = &mi;
    let sys_ref = &sys;

    for &size in SIZES {
        let layout = Layout::from_size_align(size, 8).unwrap();

        // F7 + PERF-PASS-1 (task #49, G3/A2a): time ONLY the churn loop (see
        // the non-writing group above) — teardown moved to the untimed
        // `ChurnTeardownGuard::drop`. Confound 2 fix: rotated registration
        // order (see module doc).
        bench_three_arms_rotated(
            &mut group,
            format!("SeferAlloc/{size}B"),
            move |b| {
                b.iter_batched(
                    || churn_prefill_write(sefer_ref, layout, CHURN_WORKING_SET),
                    |mut live| {
                        churn_step_write(sefer_ref, layout, &mut live, OPS);
                        ChurnTeardownGuard {
                            alloc: sefer_ref,
                            layout,
                            live,
                        }
                    },
                    BatchSize::SmallInput,
                )
            },
            format!("mimalloc/{size}B"),
            move |b| {
                b.iter_batched(
                    || churn_prefill_write(mi_ref, layout, CHURN_WORKING_SET),
                    |mut live| {
                        churn_step_write(mi_ref, layout, &mut live, OPS);
                        ChurnTeardownGuard {
                            alloc: mi_ref,
                            layout,
                            live,
                        }
                    },
                    BatchSize::SmallInput,
                )
            },
            format!("System/{size}B"),
            move |b| {
                b.iter_batched(
                    || churn_prefill_write(sys_ref, layout, CHURN_WORKING_SET),
                    |mut live| {
                        churn_step_write(sys_ref, layout, &mut live, OPS);
                        ChurnTeardownGuard {
                            alloc: sys_ref,
                            layout,
                            live,
                        }
                    },
                    BatchSize::SmallInput,
                )
            },
        );
    }

    group.finish();
}

/// PERF-4 (task #14) — the decommit→recycle segment-churn shape. This is the
/// wall-clock companion to the `seg_cycle_decommit_256k` iai bench (the
/// deterministic judge). It drives a NON-primordial small segment through
/// empty→decommit→recycle→re-reserve on every round — the exact path the
/// shamir-db sweep flagged (0.3.0 vs 0.2.1, "many short-lived small segments
/// cycling quickly") and which PERF-4's release-follows fast path optimizes.
///
/// Geometry (see the iai bench for the full rationale): the largest small size
/// class is SMALL_MAX = 258,752 B (≈253 KiB) and one 4 MiB segment holds 15
/// such blocks (16 fit in 4 MiB, but the primordial reserves one block's worth
/// for its self-hosted registry and every fresh small segment loses a block to
/// per-segment metadata → 15 usable). `SEG_BATCH` (34) fills the primordial (15),
/// then a SECOND small segment (15), then opens a THIRD (4), so the SECOND
/// segment is NON-current when the whole batch is freed → it goes empty while not
/// the carve target → `decommit_empty_segment` + `recycle`. Note: a batch that
/// only just spills into the second segment (say 18) does NOT decommit — that
/// segment is still the current carve target, which is excluded from decommit;
/// the batch MUST reach a THIRD segment (≥ 31 blocks) to leave the second one
/// non-current. Under `alloc-decommit` this is the decommit path; without it,
/// the same shape still measures the reserve/carve/release churn. Compared
/// head-to-head vs mimalloc and System.
fn bench_segment_decommit_cycle<A: GlobalAlloc>(alloc: &A, layout: Layout) {
    const SEG_BATCH: usize = 34;
    let mut ptrs: [*mut u8; SEG_BATCH] = [core::ptr::null_mut(); SEG_BATCH];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid alignment.
        *slot = unsafe { alloc.alloc(layout) };
    }
    black_box(&ptrs);
    for &ptr in &ptrs {
        if !ptr.is_null() {
            // SAFETY: ptr was allocated by `alloc` with the same layout and is
            // freed exactly once; freeing the whole batch empties the
            // non-primordial second segment → decommit → recycle.
            unsafe { alloc.dealloc(ptr, layout) };
        }
    }
}

fn bench_global_segment_decommit_cycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("segment_decommit_cycle");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    let sefer = SeferAlloc::new();
    let mi = mimalloc::MiMalloc;
    let sys = System;

    // The largest small size class EXACTLY (SMALL_MAX = 258,752 B ≈ 253 KiB) @
    // align 8: this MUST route to the Small path for the decommit trigger to
    // fire. A literal 256 KiB (262,144 B) exceeds SMALL_MAX (258,752 B) and
    // silently falls through to the dedicated-segment Large path, where
    // `dec_live_and_maybe_decommit` bails on `kind != Small` and
    // `decommit_empty_segment_for_release` is NEVER reached — the whole point
    // of the bench. One 4 MiB segment holds 15 usable such blocks (see
    // `bench_segment_decommit_cycle`'s doc comment for the full 15/15/4 batch
    // breakdown); `SEG_BATCH` (34) must reach a THIRD segment to leave the
    // second one non-current, which is what actually triggers decommit.
    let layout = Layout::from_size_align(258_752, 8).unwrap();

    // Confound 1 fix — see `bench_global_alloc_churn`'s identical call. This
    // group deliberately fills/empties several 4 MiB segments, so leftover
    // segment-table state from an earlier group is exactly the kind of
    // carryover that would bias the decommit-trigger geometry here.
    sefer.dbg_trim_current_thread();

    // Confound 2 fix: rotated registration order (see module doc).
    bench_three_arms_rotated(
        &mut group,
        "SeferAlloc/253KiB".to_string(),
        |b| b.iter(|| bench_segment_decommit_cycle(&sefer, layout)),
        "mimalloc/253KiB".to_string(),
        |b| b.iter(|| bench_segment_decommit_cycle(&mi, layout)),
        "System/253KiB".to_string(),
        |b| b.iter(|| bench_segment_decommit_cycle(&sys, layout)),
    );

    group.finish();
}

/// PERF-PASS-1 (task #49, G3/A2b) — the canonical Mechanism-2 (task #51)
/// judge. Reproduces the churn-reuse review's "criterion-shape probe" that
/// isolated the 1024B churn blow-up to the empty-small-segment
/// decommit->release->re-reserve lifecycle (not the reuse path itself, which
/// measures 29-30ns/op flat): `N_WORKING_SETS` (64) independent working sets,
/// each `WORKING_SET_LEN` live pointers, are pre-built in `iter_batched`'s
/// untimed setup; the timed routine frees and reallocates EVERY block of
/// EVERY working set once (one full free+realloc oscillation per block, in
/// working-set order), simulating a working set that repeatedly crosses a
/// segment boundary and empties/re-fills non-current segments. Teardown of
/// all `N_WORKING_SETS` sets happens in the untimed `Drop` portion via
/// [`WorkingSetCycleGuard`] (same pattern as `ChurnTeardownGuard` above), so
/// the reported ns/op is the oscillation cost alone, not lifecycle teardown.
///
/// Only `SeferAlloc` is measured here (unlike the other groups in this file):
/// the point of this bench is `SeferAlloc`'s specific decommit/reuse
/// lifecycle, which mimalloc/System don't share the shape of, and the
/// `AllocStats` delta reporting below (`stats()`) is SeferAlloc-specific.
const N_WORKING_SETS: usize = 64;

/// PERF-PASS-1 (task #49, G3/A2b): drop-guard analogous to
/// `ChurnTeardownGuard`, but owning `N_WORKING_SETS` independent working
/// sets. `Drop` frees every live pointer across every set, outside the timed
/// `iter_batched` region.
struct WorkingSetCycleGuard<'a> {
    alloc: &'a SeferAlloc,
    layout: Layout,
    sets: Vec<Vec<*mut u8>>,
}

impl<'a> Drop for WorkingSetCycleGuard<'a> {
    fn drop(&mut self) {
        for set in &self.sets {
            churn_teardown(self.alloc, self.layout, set);
        }
    }
}

/// Pre-build `N_WORKING_SETS` working sets of `working_set_len` live blocks
/// each. Untimed `iter_batched` setup — mirrors `churn_prefill` but produces
/// many independent sets instead of one, so the timed routine can cycle each
/// set through a free+realloc oscillation without the sets sharing state.
fn working_set_cycle_prefill(
    alloc: &SeferAlloc,
    layout: Layout,
    working_set_len: usize,
) -> Vec<Vec<*mut u8>> {
    (0..N_WORKING_SETS)
        .map(|_| churn_prefill(alloc, layout, working_set_len))
        .collect()
}

/// Timed routine: for every working set, free-then-reallocate every block
/// once (in place, same index) — one full oscillation across a potential
/// segment boundary per block, across all `N_WORKING_SETS` sets. This is the
/// shape that, per the churn-reuse review's probe, reproduces 20
/// decommit+release+re-reserve cycles at 1024B with 0 such cycles at
/// 16/64/256B (whose footprint stays inside the primordial segment).
fn working_set_cycle_step(alloc: &SeferAlloc, layout: Layout, sets: &mut [Vec<*mut u8>]) {
    for set in sets.iter_mut() {
        for slot in set.iter_mut() {
            if !slot.is_null() {
                // SAFETY: `*slot` was allocated by `alloc` with `layout`.
                unsafe { alloc.dealloc(*slot, layout) };
            }
            // SAFETY: layout has non-zero size and valid alignment.
            *slot = unsafe { alloc.alloc(layout) };
        }
    }
    black_box(&sets);
}

fn bench_working_set_cycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("working_set_cycle");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    let sefer = SeferAlloc::new();

    // Confound 1 fix — see `bench_global_alloc_churn`'s identical call. Only
    // SeferAlloc is measured in this group (no mimalloc/System arm, so no
    // confound-2 rotation applies here — see the group's own doc comment).
    sefer.dbg_trim_current_thread();

    for &size in SIZES {
        let layout = Layout::from_size_align(size, 8).unwrap();

        // dbg_segments_released_total / dbg_decommit_count (via `stats()`)
        // are process-wide monotonic counters (see
        // `src/alloc_core/alloc_core.rs` `dbg_decommit_count`,
        // `dbg_segments_released_total`) — not resettable, so we snapshot
        // before/after the whole `bench_function` call (all `sample_size`
        // iterations) and report the delta as a diagnostic, not a strict
        // per-iteration measurement. `decommit_calls` reads 0 unless
        // `alloc-decommit` is compiled in; `segments_released_total` is
        // always compiled. If `alloc-decommit` is off, only the segment
        // release counter (still meaningful: recycle/release without decommit
        // also fires it) is expected to move.
        let before = sefer.stats();
        group.bench_function(format!("SeferAlloc/{size}B"), |b| {
            b.iter_batched(
                || working_set_cycle_prefill(&sefer, layout, CHURN_WORKING_SET),
                |mut sets| {
                    working_set_cycle_step(&sefer, layout, &mut sets);
                    WorkingSetCycleGuard {
                        alloc: &sefer,
                        layout,
                        sets,
                    }
                },
                BatchSize::SmallInput,
            )
        });
        let after = sefer.stats();

        eprintln!(
            "working_set_cycle/SeferAlloc/{size}B: decommit_calls delta = {}, \
             segments_released_total delta = {}",
            after.decommit_calls.saturating_sub(before.decommit_calls),
            after
                .segments_released_total
                .saturating_sub(before.segments_released_total),
        );
    }

    group.finish();
}

/// RAD-3 (plan Phase 0(b) + Phase 3, E2) — `pool_cap_sweep`: a
/// spread-then-empty-then-drain harness (the pattern
/// `tests/small_segment_pool.rs`/`tests/regression_c3_unbounded_recycle.rs`
/// already use via `AllocCore` directly — spread allocations across many
/// distinct small segments, empty every segment but one survivor block each
/// via a cross-thread-free-shaped ring push, then drain), parameterized over
/// the small-segment pool's configured cap (`SmallSegmentPoolConfig::
/// pool_segments`). This is the judge for the E2 workstream: PASS-3's own
/// honest report (`docs/perf/IAI_BASELINE.md`, "Post-PERF-PASS-3 reference")
/// recorded 173/367 residual `decommit_calls` at 256 B/1024 B in
/// `working_set_cycle` because demand exceeds the (at the time of writing
/// that report) hard-capped 4-segment pool. This harness sweeps cap =
/// 0/1/4/8/16 and reports the `decommit_calls` delta at each cap, so a
/// before/after comparison across the "remove the silent clamp" change is
/// directly visible in the harness's own output rather than asserted from
/// memory.
///
/// **Why `AllocCore` directly, not `SeferAlloc` / `working_set_cycle`'s
/// shape.** An earlier version of this harness reused
/// `working_set_cycle_prefill`/`_step` verbatim (built-in-place oscillation of
/// 64 concurrent working sets through `SeferAlloc`). Measured against BOTH
/// the pre-fix code (RED baseline) and the post-fix code, that specific
/// access pattern turned out to be cap-INSENSITIVE at every swept value —
/// each pass's peak concurrent segment-empty count never exceeds what even
/// `cap=1` already absorbs, so it cannot distinguish "cap silently clamped to
/// 4" from "cap honestly resolved to 8/16" (both produce the SAME
/// `decommit_calls` delta in that shape). A harness that cannot go RED before
/// the fix is not a valid counterfactual — see this task's summary for the
/// measured numbers. This shape instead directly controls how many DISTINCT
/// segments empty out in a single scan (`SPREAD_TARGET_SEGMENTS`, well above
/// every swept cap), which is exactly the axis `pool_cap` bounds — a clean,
/// monotonic, cap-sensitive signal (verified: decommits strictly decrease as
/// cap rises from 0 through 32 against this task's fixed code).
///
/// **Why `AllocCore` (not `SeferAlloc`) — no TLS/thread plumbing needed.**
/// `AllocCore::new_with_config` builds a standalone allocator directly (no
/// registry/TLS bind), so — unlike the `SeferAlloc`-based approach, which
/// needed a fresh OS thread per cap to get a never-before-bound TLS slot —
/// this harness just constructs a fresh `AllocCore` per cap on the criterion
/// runner's own thread. `AllocCore` is `pub` (re-exported at the crate root),
/// so a bench (like `tests/small_segment_pool.rs`, an integration test) may
/// use it directly; the `dbg_*` seams used below
/// (`dbg_layout_class_for`/`dbg_push_to_ring`/`dbg_drain_all_rings`/
/// `dbg_decommit_count`) are the SAME `#[doc(hidden)] pub` test-only surface
/// `tests/small_segment_pool.rs` and `tests/regression_c3_unbounded_recycle.rs`
/// already rely on.
///
/// `pool_byte_cap` is set generously (256 MiB, i.e. 64 segments' worth) so
/// that only `pool_segments` — not the byte ceiling — constrains occupancy at
/// every swept cap; the point is to isolate the segment-count knob.
///
/// Gated on `alloc-xthread` too (not just `alloc-decommit`): the sweep uses
/// `dbg_push_to_ring`/`dbg_drain_all_rings`, which only exist under
/// `alloc-xthread` (the ring is a cross-thread-free mechanism). `alloc-decommit`
/// alone (without `alloc-xthread`) is a real, separately-buildable feature
/// combination in this crate (`alloc-decommit = ["alloc-core"]`, no
/// `alloc-xthread` dependency) — gating on `alloc-decommit` alone left this
/// code uncompilable under that combination even though `production` (which
/// always pulls in both) masked the gap in the project's own CI matrix.
#[cfg(all(feature = "alloc-decommit", feature = "alloc-xthread"))]
const POOL_CAP_SWEEP_VALUES: &[usize] = &[0, 1, 4, 8, 16];

/// Number of distinct small segments to spread allocations across before
/// emptying them all in one scan — comfortably above every value in
/// [`POOL_CAP_SWEEP_VALUES`] so the pool is genuinely saturated at each cap
/// (otherwise a low target could never exercise cap=16, for instance).
#[cfg(all(feature = "alloc-decommit", feature = "alloc-xthread"))]
const SPREAD_TARGET_SEGMENTS: usize = 40;

/// Spread allocations of `layout` across [`SPREAD_TARGET_SEGMENTS`] distinct
/// fresh small segments (one "survivor" block recorded per segment, mirroring
/// `tests/small_segment_pool.rs::spread_across_segments`), free every
/// non-survivor block, then push each survivor into its OWN segment's
/// cross-thread free ring (`dbg_push_to_ring`) and drain every ring in one
/// scan (`dbg_drain_all_rings`) — emptying every one of the
/// `SPREAD_TARGET_SEGMENTS` segments (except the current carve target and the
/// primordial segment) in a single call, exactly the scenario
/// `regression_c3_unbounded_recycle` exercises. Returns the
/// `dbg_decommit_count()` delta observed across that single drain call.
#[cfg(all(feature = "alloc-decommit", feature = "alloc-xthread"))]
fn pool_cap_sweep_spread_and_drain(cap: usize, size: usize) -> u64 {
    let config = sefer_alloc::LargeCacheConfig::new().pool(
        sefer_alloc::SmallSegmentPoolConfig::new()
            .pool_segments(cap)
            .pool_byte_cap(256 * 1024 * 1024),
    );
    let mut ac = sefer_alloc::AllocCore::new_with_config(config).expect("primordial reservation");
    let layout = Layout::from_size_align(size, 8).unwrap();
    let class_idx = ac
        .dbg_layout_class_for(layout)
        .expect("bench sizes are all small classes");

    const SEGMENT: usize = 4 * 1024 * 1024;
    // Scale the per-round block count to `size` so a round reliably advances
    // past at least one fresh segment regardless of block size: a 4 MiB
    // segment holds roughly `SEGMENT / size` blocks of this size (ignoring
    // metadata overhead, a slight under-count that only makes this MORE
    // generous), so `4 * (SEGMENT / size)` blocks per round crosses several
    // segment boundaries even at the smallest bench size (16 B, ~262K
    // blocks/segment) without inflating the round count needed at larger
    // sizes (1024 B, ~4K blocks/segment) into the millions.
    let round_blocks = 4 * (SEGMENT / size).max(1);
    let mut survivors: std::collections::HashMap<usize, *mut u8> = std::collections::HashMap::new();
    let mut all_ptrs: Vec<*mut u8> = Vec::new();
    let mut round = 0usize;
    while survivors.len() < SPREAD_TARGET_SEGMENTS && round < SPREAD_TARGET_SEGMENTS * 3 {
        for _ in 0..round_blocks {
            let p = ac.alloc(layout);
            assert!(!p.is_null(), "alloc null while spreading (round={round})");
            let seg_base = (p as usize) & !(SEGMENT - 1);
            survivors.entry(seg_base).or_insert(p);
            all_ptrs.push(p);
        }
        round += 1;
    }
    assert!(
        survivors.len() >= SPREAD_TARGET_SEGMENTS,
        "failed to spread across {SPREAD_TARGET_SEGMENTS} segments (only {})",
        survivors.len()
    );

    let survivor_set: std::collections::HashSet<usize> =
        survivors.values().map(|&p| p as usize).collect();
    for &p in &all_ptrs {
        if !survivor_set.contains(&(p as usize)) {
            // SAFETY (R6-MS-1/2): `p` was allocated by `ac.alloc` with `layout` above and
            // is freed exactly once (non-survivors are freed here; survivors
            // are freed only via the ring-push/drain path below).
            unsafe { ac.dealloc(p, layout) };
        }
    }
    for &p in survivors.values() {
        // SAFETY (R6-MS-4): `p` is a live allocation owned by `ac`; this push is
        // its SINGLE logical remote free — survivors are freed ONLY via this
        // ring-push/drain path (the non-survivors were dealloc'd above, and the
        // survivors were deliberately excluded from that dealloc loop), with no
        // re-issue before the `dbg_drain_all_rings` below. `class_idx` is the
        // block's actual class.
        unsafe { ac.dbg_push_to_ring(p, class_idx) };
    }

    let before = sefer_alloc::AllocCore::dbg_decommit_count();
    ac.dbg_drain_all_rings();
    let after = sefer_alloc::AllocCore::dbg_decommit_count();

    // Cleanup: the pool may still hold up to `cap` segments; force-drain so
    // this `AllocCore`'s `Drop` walks a clean table (no functional
    // requirement — `Drop` releases every registered segment either way —
    // this just keeps consecutive sweep iterations independent of leftover
    // pooled state from a prior cap's run within the same process).
    let _ = ac.dbg_drain_small_pool();

    after.saturating_sub(before)
}

/// Entry point: sweep every cap in [`POOL_CAP_SWEEP_VALUES`] at every size in
/// [`SIZES`], reporting the `decommit_calls` delta observed during the single
/// drain call via `eprintln!` (matching `bench_working_set_cycle`'s reporting
/// style). Registered as a single trivial (one-iteration) criterion benchmark
/// purely so `cargo bench` runs it as part of the normal harness and its
/// `eprintln!` diagnostic lines land in the same captured output as
/// `working_set_cycle`'s — criterion does not otherwise offer a "run once,
/// print diagnostics" hook outside `bench_function`. Not a comparative
/// wall-clock judge; the sweep's signal is the `eprintln!` counter deltas,
/// not the criterion timing table (the spread/drain construction cost swamps
/// the drain itself, so the criterion timing column is not meaningful here).
#[cfg(all(feature = "alloc-decommit", feature = "alloc-xthread"))]
fn bench_pool_cap_sweep(c: &mut Criterion) {
    let mut group = c.benchmark_group("pool_cap_sweep");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(1));
    group.measurement_time(Duration::from_millis(1));

    for &size in SIZES {
        for &cap in POOL_CAP_SWEEP_VALUES {
            group.bench_function(format!("cap={cap}/{size}B"), |b| {
                b.iter(|| {
                    let delta = pool_cap_sweep_spread_and_drain(cap, size);
                    eprintln!(
                        "pool_cap_sweep/cap={cap}/{size}B: decommit_calls delta = {delta} \
                         (spread across {SPREAD_TARGET_SEGMENTS} segments, single drain)",
                    );
                    black_box(delta);
                });
            });
        }
    }

    group.finish();
}

/// Sizes swept by [`bench_batch_ceiling`] — the three sizes task #220 (R8-7)
/// asks for: 16 B (well below a page, many blocks/segment), 64 B, 256 B
/// (upper edge of "small bulk" before medium/large paths matter).
const BATCH_CEILING_SIZES: &[usize] = &[16, 64, 256];

/// Number of blocks allocated-then-freed per iteration — matches [`OPS`]
/// (`bench_direct_alloc`'s cold bulk pattern, module doc confound discussion
/// / `global_alloc.rs:192`) so the scalar and batch arms measure literally
/// the same amount of work (1024 blocks in, 1024 blocks out) and are
/// directly comparable at 1:1 op count.
const BATCH_CEILING_OPS: usize = OPS;

/// R9-9 follow-up — batch sizes swept by [`bench_batch_ceiling_followup`].
/// R8-7 measured only batch=1024 (via [`BATCH_CEILING_OPS`]); a realistic
/// caller (`calloc`-style bulk allocation, collection pre-sizing) is far
/// more likely to request 8-64 items at once, and the amortisation win from
/// batching (one classification / routing call instead of N) shrinks as the
/// batch size shrinks. 1024 is kept for continuity with R8-7's reported
/// numbers so the same `cargo bench` run reproduces the R8-7 data point.
const BATCH_CEILING_COUNTS: &[usize] = &[8, 16, 32, 64, BATCH_CEILING_OPS];

/// R8-7 (task #220) — **measure, don't design.** An external perf review
/// speculated that a public batch/scoped alloc API (`alloc_batch`/
/// `dealloc_batch`) could give 1.5-3x on bulk small-object patterns by
/// amortising TLS lookup / classification / routing over many blocks per
/// call. But no consumer of such an API exists in this repo today, and a
/// bench purpose-built around a not-yet-existing API would only prove the
/// mechanism works, not that adoption is worth it -- circular. So this bench
/// does NOT add any new public symbol. It measures the CEILING such an API
/// could deliver by calling the EXISTING internal batch primitives
/// (`AllocCore::refill_class_bump` / `AllocCore::flush_class`, already used
/// in production by `src/registry/heap_core_alloc.rs`'s magazine-miss refill
/// and `src/registry/heap_core_free.rs`'s magazine-overflow flush) directly,
/// exactly the way [`pool_cap_sweep_spread_and_drain`] above already calls
/// `AllocCore` directly, bypassing `SeferAlloc`/`GlobalAlloc`/TLS entirely.
///
/// ## Two arms, same 1024-block cold bulk workload
///
/// - **(a) Scalar**: `AllocCore::alloc`/`AllocCore::dealloc` in a loop --
///   [`BATCH_CEILING_OPS`] separate calls to allocate, then
///   [`BATCH_CEILING_OPS`] separate calls to free. This is the same cold
///   bulk shape `bench_direct_alloc` measures for `SeferAlloc`/mimalloc/
///   System (module doc confound discussion references `global_alloc.rs:192`
///   for this exact pattern), just against `AllocCore` directly so it is
///   apples-to-apples with arm (b) below (no TLS/registry overhead on either
///   side).
/// - **(b) Batch**: ONE `refill_class_bump` call fills a
///   `[*mut u8; BATCH_CEILING_OPS]` buffer in a single call (vs.
///   [`BATCH_CEILING_OPS`] separate `alloc` calls), then ONE `flush_class`
///   call frees the whole buffer in a single call (vs.
///   [`BATCH_CEILING_OPS`] separate `dealloc` calls). Both primitives
///   produce a byte-identical end state to the scalar per-block path (see
///   their doc comments in `src/alloc_core/alloc_core_small_magazine.rs`) --
///   only the call-count amortisation differs, which is exactly the
///   mechanism a public batch API would exploit.
///
/// The measured ratio (scalar ns/iter / batch ns/iter) at each size is the
/// CEILING a hypothetical public API could deliver -- a public API could not
/// beat this, since it would still have to route through these same
/// primitives plus its own argument validation. The verdict (GO/NO-GO on
/// designing a public `alloc_batch`/`dealloc_batch` signature) is recorded in
/// `docs/perf/R8_7_BATCH_CEILING_MEASUREMENT.md`, not in this file -- this
/// bench only produces the raw numbers.
fn bench_batch_ceiling(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_ceiling");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    for &size in BATCH_CEILING_SIZES {
        let layout = Layout::from_size_align(size, 8).unwrap();

        // ── (a) Scalar: AllocCore::alloc / AllocCore::dealloc, one call per
        //    block, BATCH_CEILING_OPS blocks per iteration. ──────────────
        group.bench_function(format!("scalar/{size}B"), |b| {
            b.iter_batched(
                || AllocCore::new().expect("primordial reservation"),
                |mut ac| {
                    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(BATCH_CEILING_OPS);
                    for _ in 0..BATCH_CEILING_OPS {
                        let p = ac.alloc(layout);
                        assert!(!p.is_null(), "scalar alloc returned null");
                        ptrs.push(p);
                    }
                    black_box(&ptrs);
                    for &p in &ptrs {
                        // SAFETY: `p` was allocated by `ac.alloc` with `layout`
                        // immediately above and is freed exactly once, here.
                        unsafe { ac.dealloc(p, layout) };
                    }
                    ac
                },
                BatchSize::SmallInput,
            );
        });

        // ── (b) Batch: ONE refill_class_bump call fills the whole buffer,
        //    ONE flush_class call frees the whole buffer. ─────────────────
        let class_idx = {
            let ac = AllocCore::new().expect("primordial reservation");
            ac.dbg_layout_class_for(layout)
                .expect("bench sizes are all small classes")
        };
        group.bench_function(format!("batch/{size}B"), |b| {
            b.iter_batched(
                || AllocCore::new().expect("primordial reservation"),
                |mut ac| {
                    let mut ptrs: Vec<*mut u8> = vec![core::ptr::null_mut(); BATCH_CEILING_OPS];
                    let filled = ac.refill_class_bump(class_idx, &mut ptrs);
                    assert_eq!(filled, BATCH_CEILING_OPS, "batch refill under-filled");
                    black_box(&ptrs);
                    // SAFETY (R6-MS-3): every entry of `ptrs` was produced by
                    // the `refill_class_bump` call immediately above -- a
                    // live, bitmap-allocated block of class `class_idx` owned
                    // by this same `ac`, each appearing exactly once in the
                    // slice, freed exactly once here, in this single call.
                    unsafe { ac.flush_class(class_idx, &ptrs) };
                    ac
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

/// R9-9 (follow-up to R8-7 / task #220) — extends the batch-ceiling
/// measurement along TWO axes the R8-7 report's external review flagged.
/// See `docs/perf/R9_9_BATCH_BENCH_FOLLOWUP.md` for the verdict; this bench
/// only produces the raw numbers. Measurement-only: no `src/` surface
/// touched, same `#[doc(hidden)]` primitives R8-7 used (`refill_class_bump`,
/// `flush_class`, `dbg_layout_class_for`), which all take arbitrary-length
/// `&mut [*mut u8]` / `&[*mut u8]` slices so no new primitive is needed for
/// the smaller batch sizes.
///
/// ## Axis 1 — batch-size sweep
///
/// R8-7 measured only batch=1024. This sweep adds N in {8, 16, 32, 64}
/// (1024 kept for continuity) at the same three sizes (16/64/256 B), so the
/// ratio table reveals whether the ceiling holds up at realistic small
/// batch sizes or degrades as the per-call amortisation thins out.
///
/// ## Axis 2 — third arm: real `SeferAlloc`/`GlobalAlloc` scalar path
///
/// R8-7's two arms BOTH bypass `SeferAlloc`/TLS/registry (calling
/// `AllocCore` directly) to isolate the batching mechanism. But a public
/// batch API built ON TOP OF `SeferAlloc` could ALSO amortise the TLS heap
/// lookup + registry dispatch that `SeferAlloc::alloc`/`GlobalAlloc::alloc`
/// pays on EVERY scalar call — which neither AllocCore-direct arm captures.
/// This third arm measures the real production entry point
/// (`sefer.alloc`/`sefer.dealloc` through `GlobalAlloc`, same pattern
/// `bench_direct_alloc` uses for the `global_alloc` group) at the same
/// (size, N) grid. The three-way comparison separates:
/// - (b vs a) "batching amortises routing work" — the R8-7 batch win.
/// - (c vs a) "real production scalar vs bare AllocCore scalar" — the
///   TLS/registry term. NOTE the warmth asymmetry: arm (c) reuses the
///   shared `SeferAlloc` heap across iterations (warm tcache after
///   criterion's warmup), whereas arms (a)/(b) use a fresh `AllocCore` per
///   iteration (cold carve). So (c vs a) measures TLS+registry overhead
///   NET of warm-tcache savings, not the overhead in isolation — see the
///   report's mechanism read for why that cut is still the useful one.
///
/// All three arms pre-size a `Vec<*mut u8>` of length N in the untimed
/// `iter_batched` setup and write/read it identically, so the only timed
/// difference between arms is the alloc/dealloc mechanism itself (the
/// `Vec`'s own alloc/free via the bench binary's default allocator runs
/// in setup / return-drop, outside the timed region — matching how
/// `bench_global_alloc_churn` already excludes `ChurnTeardownGuard`'s
/// drop from timing).
fn bench_batch_ceiling_followup(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_ceiling_followup");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    let sefer = SeferAlloc::new();
    // Confound 1 fix (module doc item 1): reset SeferAlloc's per-thread heap
    // so leftover state from earlier groups does not bias arm (c).
    sefer.dbg_trim_current_thread();

    for &size in BATCH_CEILING_SIZES {
        let layout = Layout::from_size_align(size, 8).unwrap();
        let class_idx = {
            let ac = AllocCore::new().expect("primordial reservation");
            ac.dbg_layout_class_for(layout)
                .expect("bench sizes are all small classes")
        };

        for &n in BATCH_CEILING_COUNTS {
            // ── (a) AllocCore scalar: N `alloc` + N `dealloc`, fresh
            //    `AllocCore` per iteration (untimed setup). Matches R8-7's
            //    arm (a) methodology, parameterised over N. ──────────────
            group.bench_function(format!("scalar_core/{size}B/n{n}"), |b| {
                b.iter_batched(
                    || {
                        (
                            AllocCore::new().expect("primordial reservation"),
                            vec![core::ptr::null_mut(); n],
                        )
                    },
                    |(mut ac, mut ptrs)| {
                        for slot in ptrs.iter_mut() {
                            let p = ac.alloc(layout);
                            assert!(!p.is_null(), "scalar_core alloc null");
                            *slot = p;
                        }
                        black_box(&ptrs);
                        for &p in &ptrs {
                            // SAFETY: `p` from `ac.alloc` above, freed once here.
                            unsafe { ac.dealloc(p, layout) };
                        }
                        (ac, ptrs)
                    },
                    BatchSize::SmallInput,
                );
            });

            // ── (b) AllocCore batch: ONE `refill_class_bump` + ONE
            //    `flush_class` for N blocks. Matches R8-7's arm (b),
            //    parameterised over N. ───────────────────────────────────
            group.bench_function(format!("batch_core/{size}B/n{n}"), |b| {
                b.iter_batched(
                    || {
                        (
                            AllocCore::new().expect("primordial reservation"),
                            vec![core::ptr::null_mut(); n],
                        )
                    },
                    |(mut ac, mut ptrs)| {
                        let filled = ac.refill_class_bump(class_idx, &mut ptrs);
                        assert_eq!(filled, n, "batch_core refill under-filled (n={n})");
                        black_box(&ptrs);
                        // SAFETY (R6-MS-3): every entry of `ptrs` was produced
                        // by the `refill_class_bump` call immediately above — a
                        // live, bitmap-allocated block of `class_idx` owned by
                        // this same `ac`, each appearing exactly once, freed
                        // once here, in this single call.
                        unsafe { ac.flush_class(class_idx, &ptrs) };
                        (ac, ptrs)
                    },
                    BatchSize::SmallInput,
                );
            });

            // ── (c) Real SeferAlloc/`GlobalAlloc` scalar: N `alloc` + N
            //    `dealloc` through the production `GlobalAlloc` impl (TLS
            //    heap lookup + registry dispatch every call), shared heap
            //    with warm tcache after warmup. Same pattern
            //    `bench_direct_alloc` uses for the `global_alloc` group,
            //    scaled to batch size N. ─────────────────────────────────
            group.bench_function(format!("scalar_sefer/{size}B/n{n}"), |b| {
                b.iter_batched(
                    || vec![core::ptr::null_mut(); n],
                    |mut ptrs| {
                        for slot in ptrs.iter_mut() {
                            // SAFETY: layout has non-zero size and valid alignment.
                            let p = unsafe { sefer.alloc(layout) };
                            assert!(!p.is_null(), "scalar_sefer alloc null");
                            *slot = p;
                        }
                        black_box(&ptrs);
                        for &p in &ptrs {
                            // SAFETY: `p` from `sefer.alloc` above, freed once here.
                            unsafe { sefer.dealloc(p, layout) };
                        }
                        ptrs
                    },
                    BatchSize::SmallInput,
                );
            });
        }
    }

    group.finish();
}

#[cfg(all(feature = "alloc-decommit", feature = "alloc-xthread"))]
criterion_group!(
    benches,
    bench_global_alloc,
    bench_global_alloc_churn,
    bench_global_alloc_churn_with_teardown,
    bench_global_alloc_churn_write,
    bench_global_segment_decommit_cycle,
    bench_working_set_cycle,
    bench_pool_cap_sweep,
    bench_batch_ceiling,
    bench_batch_ceiling_followup
);
#[cfg(not(all(feature = "alloc-decommit", feature = "alloc-xthread")))]
criterion_group!(
    benches,
    bench_global_alloc,
    bench_global_alloc_churn,
    bench_global_alloc_churn_with_teardown,
    bench_global_alloc_churn_write,
    bench_global_segment_decommit_cycle,
    bench_working_set_cycle,
    bench_batch_ceiling,
    bench_batch_ceiling_followup
);
criterion_main!(benches);
