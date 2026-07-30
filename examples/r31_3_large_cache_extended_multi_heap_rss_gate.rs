//! R31-3 (task #466, step 3) — multi-heap RSS accounting for
//! `large-cache-extended`: retention is PER HEAP (`AllocCore` is owner-only,
//! neither `Send` nor `Sync` — no process-wide coordination between heaps,
//! per `large_cache_config.rs`'s `DEFAULT_EXTENDED_BUDGET_BYTES` doc), so a
//! single-heap number cannot simply be multiplied by heap count — this gate
//! measures ACTUAL process RSS across 1/8/32 concurrently-claimed heaps,
//! mirroring `examples/r29_13_large_cache_retention_gate.rs`'s
//! subprocess-per-arm/thread-per-heap methodology exactly, but sweeping
//! `large-cache-extended` OFF vs ON (compile-time feature, hence TWO
//! binaries via `include!`, not a runtime arm) at the shipped FINITE default
//! budget (`DEFAULT_EXTENDED_BUDGET_BYTES` = 256 MiB/heap, R17-9) rather than
//! R29-13's `headroom_bytes` sweep.
//!
//! ## Why this exists
//!
//! `docs/perf/R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md` §3 measured
//! RSS/commit retention for exactly ONE heap (a bare `AllocCore`, not even
//! through the registry). Task #466 (R31-3)'s brief calls this out as unmet:
//! "measure actual RSS across e.g. 1/8/32 heaps, do not just extrapolate the
//! single-heap number by multiplication; extrapolation can be wrong if
//! there's shared/amortized state." This gate is that direct measurement.
//!
//! ## Workload — overflows the base 8 slots, stays cheap at 32 heaps
//!
//! Each heap allocates+frees `LARGE_OBJ_COUNT` (16) distinct Large sizes,
//! genuinely touched (committed, not just reserved) — enough to overflow the
//! base 8 slots (forcing the ON arm's sidecar to materialise; the OFF arm's
//! base cache FIFO-evicts the 8 oldest) without the geometric-doubling
//! explosion `r14_5_large_cache_extended_rss_measure.rs`'s own module doc
//! warns about ("doubling 40 times explodes past any realistic/addressable
//! range"). Sizes are linearly spaced, `LARGE_OBJ_STRIDE_SEGMENTS` (1)
//! segment apart, starting near the safely-Large floor — totalling ~272
//! MiB/heap (exceeds the 256 MiB default budget, proving admission genuinely
//! ran) — deliberately kept SMALL (not the "one per base slot, deliberately
//! huge" design R29-13's fixed-8-count 34 MiB/object workload uses) because
//! THIS gate's thread count reaches 32 concurrently live heaps at once, and
//! ~272 MiB/heap x 32 threads must stay within a "few GiB, seconds not
//! minutes" budget per CLAUDE.md's "short scenario by default" rule — an
//! early draft using 40-slot-filling, up-to-1-GiB-per-object sizes
//! (mirroring `r14_5_large_cache_extended_rss_measure.rs`'s single-heap
//! derivation verbatim) ballooned to ~40 GiB aggregate commit at 32 threads
//! and was rejected as too heavy before this smaller design replaced it.
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example r31_3_large_cache_extended_multi_heap_rss_gate --features "production alloc-stats bench-internals"
//! cargo run --release --example r31_3_large_cache_extended_multi_heap_rss_gate --features "production alloc-stats bench-internals large-cache-extended"
//! ```

#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use core::alloc::Layout;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use sefer_alloc::registry::{bootstrap, config_conflicts_total, HeapCore, HeapRegistry};
use sefer_alloc::{AllocCore, LargeCacheConfig, SegmentLayout};

const THREAD_COUNTS: &[usize] = &[1, 8, 32];
const REPETITIONS: usize = 3;

/// 16 distinct Large sizes — overflows the base 8 slots by exactly 8 (forces
/// the ON arm's sidecar to materialise; the OFF arm's base cache FIFO-evicts
/// its 8 oldest entries), while staying small enough that
/// `LARGE_OBJ_COUNT * max_size * THREAD_COUNTS.max()` stays in the low
/// hundreds-of-MiB-to-few-GiB range, not tens of GiB (see module doc for the
/// rejected 40-slot/up-to-1-GiB design this replaced).
const LARGE_OBJ_COUNT: usize = 16;

/// Linear spacing between consecutive sizes, in SEGMENTs — keeps every
/// generated size a genuinely distinct large-cache slot (each pairwise
/// difference is a whole SEGMENT, well above the allocator's own
/// SEGMENT-rounding granularity) without approaching `LARGE_OBJ_COUNT`
/// sizes' worth of geometric-doubling growth. `1` (not `2`+) keeps the
/// per-heap total (`sum` of 16 sizes starting near the safely-Large floor,
/// ~2 MiB, stepping 1 SEGMENT = 2 MiB) around 256-300 MiB — comfortably
/// exceeding the 256 MiB default budget (proving admission genuinely ran)
/// while keeping the 32-thread aggregate in the single-digit-GiB range, not
/// tens of GiB.
const LARGE_OBJ_STRIDE_SEGMENTS: usize = 1;

fn large_test_sizes(n: usize) -> Vec<usize> {
    let segment = SegmentLayout::SEGMENT;
    let small_max_class = AllocCore::dbg_small_class_count() - 1;
    let small_max = AllocCore::dbg_block_size(small_max_class);
    let floor = (2 * small_max).div_ceil(segment).max(1) * segment;
    let stride = LARGE_OBJ_STRIDE_SEGMENTS * segment;
    (0..n).map(|i| floor + i * stride).collect()
}

fn config() -> LargeCacheConfig {
    // Deliberately the DEFAULT config (no explicit `.budget_bytes(..)`
    // override) — this is the shipped-default question, not a custom
    // sweep: OFF resolves budget_bytes=None (base cache, unaffected by
    // R17-9); ON resolves Some(DEFAULT_EXTENDED_BUDGET_BYTES) = 256 MiB.
    LargeCacheConfig::new()
}

fn fill_large_objects(heap: &mut HeapCore, sizes: &[usize]) -> Vec<(*mut u8, Layout)> {
    let mut live = Vec::with_capacity(sizes.len());
    for &bytes in sizes {
        let layout = Layout::from_size_align(bytes, 8).unwrap();
        let p = heap.alloc(layout);
        if p.is_null() {
            eprintln!("OOM at {bytes} bytes -- stopping early (host memory pressure)");
            break;
        }
        // Touch every 4 KiB page so the reservation is genuinely committed.
        let page = 4096usize;
        let mut off = 0usize;
        // SAFETY: `p` is a fresh allocation of `layout.size()` bytes from
        // `heap`, not yet freed; each written offset is `< layout.size()`.
        unsafe {
            while off < layout.size() {
                p.add(off).write_volatile(0xAB);
                off += page;
            }
        }
        live.push((p, layout));
    }
    live
}

fn teardown_large_objects(heap: &mut HeapCore, live: &[(*mut u8, Layout)]) {
    for &(p, layout) in live {
        if !p.is_null() {
            // SAFETY: `p` was allocated by `heap` with `layout` above, not yet freed.
            unsafe { heap.dealloc(p, layout) };
        }
    }
}

fn snapshot_kib() -> (u64, u64) {
    let m = proc_probe::snapshot();
    (m.rss / 1024, m.commit / 1024)
}

fn parse_env_usize(name: &str) -> usize {
    let raw = std::env::var(name)
        .unwrap_or_else(|e| panic!("{name} env var required in child mode ({e})"));
    raw.parse::<usize>()
        .unwrap_or_else(|e| panic!("{name}={raw:?} not a valid usize ({e})"))
}

fn run_child() {
    let thread_count = parse_env_usize("R31_3_THREAD_COUNT");
    let repetition = parse_env_usize("R31_3_REPETITION");
    let extended = cfg!(feature = "large-cache-extended");

    let conflicts_before = config_conflicts_total();
    let sizes = large_test_sizes(LARGE_OBJ_COUNT);

    let go = Arc::new(AtomicBool::new(false));
    let hold_ready = Arc::new(AtomicU64::new(0));
    let release = Arc::new(AtomicBool::new(false));

    let resolved_budget_is_default: Arc<Vec<AtomicBool>> =
        Arc::new((0..thread_count).map(|_| AtomicBool::new(false)).collect());
    let used_post_teardown: Arc<Vec<AtomicU64>> =
        Arc::new((0..thread_count).map(|_| AtomicU64::new(0)).collect());
    let slots_occupied: Arc<Vec<AtomicU64>> =
        Arc::new((0..thread_count).map(|_| AtomicU64::new(0)).collect());
    #[cfg(feature = "large-cache-extended")]
    let extension_materialised: Arc<Vec<AtomicBool>> =
        Arc::new((0..thread_count).map(|_| AtomicBool::new(false)).collect());

    let (rss_before_kib, commit_before_kib) = snapshot_kib();

    let mut handles = Vec::with_capacity(thread_count);
    for i in 0..thread_count {
        let go = Arc::clone(&go);
        let hold_ready = Arc::clone(&hold_ready);
        let release = Arc::clone(&release);
        let resolved_budget_is_default = Arc::clone(&resolved_budget_is_default);
        let used_post_teardown = Arc::clone(&used_post_teardown);
        let slots_occupied = Arc::clone(&slots_occupied);
        #[cfg(feature = "large-cache-extended")]
        let extension_materialised = Arc::clone(&extension_materialised);
        let sizes = sizes.clone();

        handles.push(thread::spawn(move || {
            let heap_ptr = HeapRegistry::claim_with_config(config());
            assert!(
                !heap_ptr.is_null(),
                "HeapRegistry::claim_with_config returned null at thread {i}"
            );
            // SAFETY: `heap_ptr` was just returned by `claim_with_config` and
            // is owned by THIS thread until we `recycle` it below.
            let heap: &mut HeapCore = unsafe { &mut *heap_ptr };

            // SELF-VERIFICATION (R26-4 config-identity rule): the resolved
            // budget must match what THIS build's `LargeCacheConfig::DEFAULT`
            // actually resolves to — `None` (unbounded) when the feature is
            // OFF, `Some(256 MiB)` when it is ON (R17-9's
            // `DEFAULT_EXTENDED_BUDGET_BYTES`). Read back via
            // `AllocCore::dbg_large_cache_budget` (through `HeapCore`'s
            // `Deref`), not assumed.
            let resolved = heap.dbg_large_cache_budget();
            let expected_default = if extended {
                Some(256usize * 1024 * 1024)
            } else {
                None
            };
            let matches_default = resolved == expected_default;
            resolved_budget_is_default[i].store(matches_default, Ordering::Release);

            while !go.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(1));
            }

            let live = fill_large_objects(heap, &sizes);
            teardown_large_objects(heap, &live);

            let used = heap.dbg_large_cache_used() as u64;
            used_post_teardown[i].store(used, Ordering::Release);
            let base_occupied = heap
                .dbg_large_cache_slot_sizes()
                .iter()
                .filter(|s| s.is_some())
                .count() as u64;
            #[cfg(feature = "large-cache-extended")]
            {
                let ext_occupied = heap
                    .dbg_large_cache_extended_slot_sizes()
                    .iter()
                    .filter(|s| s.is_some())
                    .count() as u64;
                slots_occupied[i].store(base_occupied + ext_occupied, Ordering::Release);
                extension_materialised[i].store(
                    heap.dbg_large_cache_extension_materialised(),
                    Ordering::Release,
                );
            }
            #[cfg(not(feature = "large-cache-extended"))]
            {
                slots_occupied[i].store(base_occupied, Ordering::Release);
            }

            hold_ready.fetch_add(1, Ordering::Release);
            while !release.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(2));
            }
            // SAFETY: `heap_ptr` was returned by `claim_with_config` above and
            // has not yet been recycled.
            unsafe { HeapRegistry::recycle(heap_ptr) };
        }));
    }

    go.store(true, Ordering::Release);
    let mut peak_rss_kib: u64 = 0;
    let mut peak_commit_kib: u64 = 0;
    let poll_interval = Duration::from_millis(15);
    while hold_ready.load(Ordering::Acquire) < thread_count as u64 {
        let (r, c) = snapshot_kib();
        peak_rss_kib = peak_rss_kib.max(r);
        peak_commit_kib = peak_commit_kib.max(c);
        thread::sleep(poll_interval);
    }
    for _ in 0..5 {
        let (r, c) = snapshot_kib();
        peak_rss_kib = peak_rss_kib.max(r);
        peak_commit_kib = peak_commit_kib.max(c);
        thread::sleep(poll_interval);
    }

    let (rss_post_kib, commit_post_kib) = snapshot_kib();

    let used_post_teardown_sum: u64 = used_post_teardown
        .iter()
        .map(|a| a.load(Ordering::Acquire))
        .sum();
    let used_post_teardown_max = used_post_teardown
        .iter()
        .map(|a| a.load(Ordering::Acquire))
        .max()
        .unwrap_or(0);
    let slots_occupied_max = slots_occupied
        .iter()
        .map(|a| a.load(Ordering::Acquire))
        .max()
        .unwrap_or(0);
    #[cfg(feature = "large-cache-extended")]
    let extension_materialised_count = extension_materialised
        .iter()
        .filter(|a| a.load(Ordering::Acquire))
        .count() as u64;
    #[cfg(not(feature = "large-cache-extended"))]
    let extension_materialised_count = 0u64;

    let conflicts_delta = config_conflicts_total().saturating_sub(conflicts_before);

    // SELF-VERIFICATION re-check: every worker resolved the expected default budget.
    for (i, slot) in resolved_budget_is_default.iter().enumerate() {
        assert!(
            slot.load(Ordering::Acquire),
            "R31-3 child (extended={extended}, threads={thread_count}, rep={repetition}): \
             worker {i} did not resolve the expected default budget — isolation contract broken"
        );
    }
    assert_eq!(
        conflicts_delta, 0,
        "R31-3 child (extended={extended}, threads={thread_count}, rep={repetition}): \
         CONFIG_CONFLICTS delta = {conflicts_delta} (expected 0 in a fresh process)"
    );

    // ADMISSION proof: LARGE_OBJ_COUNT (16) distinct large sizes total ~272
    // MiB/heap, comfortably exceeding even the 256 MiB default budget, so
    // every arm must show nonzero post-teardown large_cache_used_bytes
    // (something got admitted before any budget-driven eviction could empty
    // it entirely).
    assert!(
        used_post_teardown_max > 0,
        "ADMISSION FAILED: used_post_teardown_max=0 (extended={extended}, threads={thread_count}, \
         rep={repetition}) — no large span was ever cached; this arm's RSS number is vacuous"
    );

    proc_probe::emit_u64("extended", extended as u64);
    proc_probe::emit_u64("thread_count", thread_count as u64);
    proc_probe::emit_u64("repetition", repetition as u64);
    proc_probe::emit_u64("config_conflicts_delta", conflicts_delta);
    proc_probe::emit_u64("large_obj_count", LARGE_OBJ_COUNT as u64);
    proc_probe::emit_u64("rss_before_kib", rss_before_kib);
    proc_probe::emit_u64("commit_before_kib", commit_before_kib);
    proc_probe::emit_u64("peak_rss_kib", peak_rss_kib);
    proc_probe::emit_u64("peak_commit_kib", peak_commit_kib);
    proc_probe::emit_u64("used_post_teardown_sum", used_post_teardown_sum);
    proc_probe::emit_u64("used_post_teardown_max", used_post_teardown_max);
    proc_probe::emit_u64("slots_occupied_max", slots_occupied_max);
    proc_probe::emit_u64("extension_materialised_count", extension_materialised_count);
    proc_probe::emit_u64("rss_post_kib", rss_post_kib);
    proc_probe::emit_u64("commit_post_kib", commit_post_kib);

    println!(
        "OK extended={extended} threads={thread_count} rep={repetition} \
         cfg_conflicts_delta={conflicts_delta} used_post_teardown_max={used_post_teardown_max} \
         used_post_teardown_sum={used_post_teardown_sum} slots_occupied_max={slots_occupied_max} \
         extension_materialised_count={extension_materialised_count} \
         peak_rss_kib={peak_rss_kib} rss_post_kib={rss_post_kib} commit_post_kib={commit_post_kib}"
    );

    release.store(true, Ordering::Release);
    for (i, h) in handles.into_iter().enumerate() {
        h.join().unwrap_or_else(|e| {
            panic!(
                "R31-3 child (extended={extended}, thread={i}, rep={repetition}): \
                 worker thread panicked: {e:?}"
            )
        });
    }
}

struct ChildMetrics {
    map: std::collections::HashMap<String, u64>,
}

impl ChildMetrics {
    fn get(&self, k: &str) -> u64 {
        *self
            .map
            .get(k)
            .unwrap_or_else(|| panic!("child RESULT missing {k}"))
    }
}

fn parse_child_stdout(stdout: &str) -> ChildMetrics {
    let mut map = std::collections::HashMap::new();
    for line in stdout.lines() {
        let rest = match line.strip_prefix("RESULT ") {
            Some(r) => r,
            None => continue,
        };
        for tok in rest.split_whitespace() {
            if let Some((k, v)) = tok.split_once('=') {
                if let Ok(n) = v.parse::<u64>() {
                    map.insert(k.to_string(), n);
                }
            }
        }
    }
    ChildMetrics { map }
}

fn run_one_child(thread_count: usize, repetition: usize) -> ChildMetrics {
    let exe = std::env::current_exe().unwrap_or_else(|e| panic!("current_exe: {e}"));
    let mut cmd = std::process::Command::new(&exe);
    cmd.env("R31_3_THREAD_COUNT", thread_count.to_string())
        .env("R31_3_REPETITION", repetition.to_string())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit());
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("spawning child: {e}"));
    if !output.status.success() {
        panic!(
            "R31-3 child (threads={thread_count}, rep={repetition}) failed with status {:?}; \
             see stderr above",
            output.status.code()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    print!("{stdout}");
    parse_child_stdout(&stdout)
}

fn median(v: &mut [u64]) -> u64 {
    if v.is_empty() {
        return 0;
    }
    v.sort_unstable();
    v[v.len() / 2]
}

fn run_orchestrator() {
    let extended = cfg!(feature = "large-cache-extended");
    println!(
        "=== R31-3 large-cache-extended MULTI-HEAP RSS gate — subprocess-per-arm isolation ==="
    );
    println!(
        "extended={extended} threads={THREAD_COUNTS:?} x {REPETITIONS} reps; LARGE_OBJ_COUNT={LARGE_OBJ_COUNT}"
    );
    println!();

    let mut all: Vec<ChildMetrics> = Vec::new();
    for &tc in THREAD_COUNTS {
        for rep in 0..REPETITIONS {
            eprintln!("--- arm extended={extended} threads={tc} rep={rep}/{REPETITIONS} ---");
            let m = run_one_child(tc, rep);
            assert_eq!(
                m.get("thread_count"),
                tc as u64,
                "child reported wrong thread_count"
            );
            assert_eq!(
                m.get("repetition"),
                rep as u64,
                "child reported wrong repetition"
            );
            assert_eq!(
                m.get("config_conflicts_delta"),
                0,
                "child reported non-zero config_conflicts_delta"
            );
            all.push(m);
        }
    }

    println!();
    println!("=== aggregated (median of {REPETITIONS} reps; min..max in parens) ===");
    println!(
        "{:>8} {:>24} {:>24} {:>14} {:>10} {:>10}",
        "threads", "peak_rss_kib", "rss_post_kib", "used_max", "slots_occ", "ext_mat",
    );
    for &tc in THREAD_COUNTS {
        let cell: Vec<&ChildMetrics> = all
            .iter()
            .filter(|m| m.get("thread_count") == tc as u64)
            .collect();
        let pick = |k: &str| -> Vec<u64> { cell.iter().map(|m| m.get(k)).collect() };
        let mut peak = pick("peak_rss_kib");
        let mut post = pick("rss_post_kib");
        let mut usedmax = pick("used_post_teardown_max");
        let mut slots = pick("slots_occupied_max");
        let mut extmat = pick("extension_materialised_count");
        let (peak_m, post_m, usedmax_m, slots_m, extmat_m) = (
            median(&mut peak),
            median(&mut post),
            median(&mut usedmax),
            median(&mut slots),
            median(&mut extmat),
        );
        let fmt = |v: &mut Vec<u64>, m: u64| format!("{m} ({}..{})", v[0], v[v.len() - 1]);
        println!(
            "{:>8} {:>24} {:>24} {:>14} {:>10} {:>10}",
            tc,
            fmt(&mut peak, peak_m),
            fmt(&mut post, post_m),
            usedmax_m,
            slots_m,
            extmat_m,
        );
    }

    println!();
    println!("=== CSV (one row per child) ===");
    let cols = [
        "extended",
        "thread_count",
        "repetition",
        "config_conflicts_delta",
        "process_identity",
        "large_obj_count",
        "peak_rss_kib",
        "peak_commit_kib",
        "used_post_teardown_sum",
        "used_post_teardown_max",
        "slots_occupied_max",
        "extension_materialised_count",
        "rss_post_kib",
        "commit_post_kib",
    ];
    println!("# {}", cols.join(","));
    for m in &all {
        let row: Vec<String> = cols
            .iter()
            .map(|c| {
                if *c == "process_identity" {
                    "subprocess".to_string()
                } else if *c == "extended" {
                    (extended as u64).to_string()
                } else {
                    m.get(c).to_string()
                }
            })
            .collect();
        println!("{}", row.join(","));
    }

    println!();
    println!(
        "NOTE: each (thread_count, rep) cell ran in its OWN freshly-spawned OS process \
         (subprocess-per-arm isolation, PER HeapRegistry-instance heap — mirrors R29-13's \
         proven methodology). Every arm hard-asserted the resolved large-cache budget matches \
         this build's expected DEFAULT (None when large-cache-extended is OFF, Some(256 MiB) \
         when ON, R17-9's DEFAULT_EXTENDED_BUDGET_BYTES) AND config_conflicts_delta == 0 (R26-4 \
         config identity). Every arm hard-asserted used_post_teardown_max > 0 (admission \
         proven) — LARGE_OBJ_COUNT (16) distinct large sizes total ~272 MiB/heap, exceeding the \
         256 MiB default budget either way."
    );
}

fn main() {
    let _ = bootstrap::ensure();
    let child_mode = std::env::var_os("R31_3_THREAD_COUNT").is_some();
    if child_mode {
        run_child();
        return;
    }
    run_orchestrator();
}
