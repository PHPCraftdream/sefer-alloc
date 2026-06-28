//! RSS-probe harness for `SeferMalloc` — measures resident-set-size over time
//! under a sustained, **asymmetric** cross-thread free pattern designed to
//! stress the [`RemoteFreeRing`](sefer_alloc::alloc_core::remote_free_ring).
//!
//! ## Purpose (task #53)
//!
//! `examples/malloc_macro.rs` measures throughput but explicitly marks RSS as
//! `N/A`.  This harness fills that gap: it tracks RSS (bytes resident in RAM)
//! across a producer/consumer split where producers ONLY allocate and one
//! consumer ONLY frees, maximising the cross-thread-free pressure on the ring.
//! It also demonstrates the `alloc-decommit` recovery ratio when `alloc-decommit`
//! is enabled.
//!
//! ## What is measured — and what is NOT
//!
//! - **Measured directly:** RSS (platform-specific, see `mod rss` below),
//!   cumulative allocs/frees, in-flight estimate (`allocs - frees`).
//! - **NOT measured directly:** `RemoteFreeRing` overflow.  The ring's `overflow`
//!   field is `pub(crate)` internal state; breaking encapsulation to expose it
//!   would require a source change.  The in-flight estimate (`allocs - frees`)
//!   is the best indirect proxy available without invasive instrumentation.
//!   If a direct overflow counter is needed, add a `pub fn overflow_count()` to
//!   `RemoteFreeRing` and re-export it — **left for user approval**.
//!
//! ## Run
//!
//! ```text
//! # Smoke (10 s, default):
//! cargo run --release --example rss_probe --features "alloc-global alloc-xthread"
//!
//! # With decommit (compare RSS recovery ratio):
//! cargo run --release --example rss_probe \
//!     --features "alloc-global alloc-xthread alloc-decommit"
//!
//! # Longer run / more producers (env vars):
//! SEFER_RSS_SECONDS=60 SEFER_RSS_PRODUCERS=8 \
//!     cargo run --release --example rss_probe --features "alloc-global alloc-xthread"
//! ```

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::semicolon_if_nothing_returned
)]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use sefer_alloc::SeferMalloc;

// ---------------------------------------------------------------------------
// Global allocator
// ---------------------------------------------------------------------------

#[global_allocator]
static GLOBAL: SeferMalloc = SeferMalloc::new();

// ---------------------------------------------------------------------------
// mod rss — platform-specific RSS query, no external deps
// ---------------------------------------------------------------------------

mod rss {
    /// Return the current process RSS in bytes, or `None` if unsupported.
    pub fn current_bytes() -> Option<u64> {
        #[cfg(target_os = "linux")]
        return linux_rss();

        #[cfg(target_os = "windows")]
        return windows_rss();

        // macOS and others: unsupported (would need task_info syscall or
        // /proc which doesn't exist there).
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        return None;
    }

    // -----------------------------------------------------------------------
    // Linux: parse /proc/self/status for "VmRSS: <N> kB"
    // -----------------------------------------------------------------------
    #[cfg(target_os = "linux")]
    fn linux_rss() -> Option<u64> {
        let text = std::fs::read_to_string("/proc/self/status").ok()?;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                // rest is like "   123456 kB"
                let kb: u64 = rest
                    .split_whitespace()
                    .next()?
                    .parse()
                    .ok()?;
                return Some(kb * 1024);
            }
        }
        None
    }

    // -----------------------------------------------------------------------
    // Windows: GetProcessMemoryInfo via hand-rolled extern (no windows-sys dep)
    // -----------------------------------------------------------------------
    #[cfg(target_os = "windows")]
    fn windows_rss() -> Option<u64> {
        use std::os::raw::c_ulong;

        // PROCESS_MEMORY_COUNTERS from psapi.h — only the fields we need.
        // The struct is 40 bytes on both 32-bit and 64-bit Windows
        // (PagefileUsage / PeakPagefileUsage are SIZE_T = pointer-sized,
        // but WorkingSetSize at offset 16 is also SIZE_T).
        // We use usize for SIZE_T fields (matches pointer width).
        #[repr(C)]
        struct ProcessMemoryCounters {
            cb: c_ulong,           // 0: struct size (DWORD)
            page_fault_count: c_ulong, // 4
            peak_working_set_size: usize, // 8 (SIZE_T)
            working_set_size: usize,     // 16 (SIZE_T) ← RSS
            quota_peak_paged_pool_usage: usize, // 24
            quota_paged_pool_usage: usize,      // 32
            quota_peak_non_paged_pool_usage: usize, // 40
            quota_non_paged_pool_usage: usize,      // 48
            pagefile_usage: usize,                  // 56
            peak_pagefile_usage: usize,             // 64
        }

        // SAFETY: these are plain Win32 API declarations with correct
        // signatures and calling conventions.  We link against the
        // psapi import library (ships with every Windows SDK / MSVC
        // toolchain).  `GetCurrentProcess` returns a pseudo-handle
        // (always valid, never NULL).  The output struct is stack-local,
        // fully initialised to zero before the call.
        extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn GetProcessMemoryInfo(
                process: *mut std::ffi::c_void,
                ppsmemcounters: *mut ProcessMemoryCounters,
                cb: c_ulong,
            ) -> i32; // BOOL
        }

        // Link psapi — required for GetProcessMemoryInfo on Windows.
        // (On MSVC this is done by the #[link] attribute; on MinGW the
        // linker finds it via the import library.)
        #[cfg_attr(target_env = "msvc", link(name = "psapi"))]
        extern "C" {}

        let mut pmc = ProcessMemoryCounters {
            cb: std::mem::size_of::<ProcessMemoryCounters>() as c_ulong,
            page_fault_count: 0,
            peak_working_set_size: 0,
            working_set_size: 0,
            quota_peak_paged_pool_usage: 0,
            quota_paged_pool_usage: 0,
            quota_peak_non_paged_pool_usage: 0,
            quota_non_paged_pool_usage: 0,
            pagefile_usage: 0,
            peak_pagefile_usage: 0,
        };

        // SAFETY: `GetCurrentProcess()` always succeeds (pseudo-handle);
        // `&mut pmc` is valid for `size_of::<ProcessMemoryCounters>()` bytes;
        // `pmc.cb` is set to that same size as required by the API.
        let ok = unsafe {
            GetProcessMemoryInfo(
                GetCurrentProcess(),
                &mut pmc as *mut ProcessMemoryCounters,
                pmc.cb,
            )
        };
        if ok != 0 {
            Some(pmc.working_set_size as u64)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Block — heap-allocated byte slice handed across threads
// ---------------------------------------------------------------------------

/// A live allocation: raw pointer + the exact `Layout` it was allocated with.
/// Logically owned by exactly one thread at a time (producers transfer ownership
/// to the consumer via the channel; the consumer is then the sole freer).
struct Block {
    ptr: *mut u8,
    layout: Layout,
}

// SAFETY: `Block` is only ever sent across threads as an ownership transfer.
// The producer sets its slot to empty before sending; the consumer is the
// unique owner after receiving it.  No aliasing across threads.
unsafe impl Send for Block {}

// ---------------------------------------------------------------------------
// PRNG — deterministic xorshift64* (no dep, fixed seed → reproducible)
// ---------------------------------------------------------------------------

struct Xorshift64(u64);

impl Xorshift64 {
    fn new(seed: u64) -> Self {
        Self(seed | 1) // avoid all-zero fixed point
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

}

// ---------------------------------------------------------------------------
// Size picker — xorshift mix with optional large-block fraction
// ---------------------------------------------------------------------------

/// Pick an allocation size using xorshift mix.
/// `large_frac` ∈ [0.0, 1.0] controls fraction of large (8–256 KiB) blocks.
#[inline]
fn pick_size(rng: &mut Xorshift64, large_frac: f64) -> usize {
    let r = rng.next_u64();
    // Threshold: if r (0..2^64) < large_frac * 2^64, pick a large block.
    let threshold = (large_frac * (u64::MAX as f64)) as u64;
    if r < threshold {
        // Large: 8 KiB – 256 KiB (power-of-two-ish)
        let shift = (rng.next_u64() % 6) as u32; // 0..5 → 8K, 16K, 32K, 64K, 128K, 256K
        8 * 1024 * (1usize << shift)
    } else {
        // Small-skewed: 16 – 512 B
        16 + (r >> 10) as usize % (512 - 16)
    }
}

#[inline]
fn layout_for(size: usize) -> Layout {
    Layout::from_size_align(size.max(1), 8).unwrap()
}

// ---------------------------------------------------------------------------
// Shared counters
// ---------------------------------------------------------------------------

struct Counters {
    allocs: AtomicU64,
    frees: AtomicU64,
}

impl Counters {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
        }
    }
}

// ---------------------------------------------------------------------------
// Producer thread — allocates, NEVER frees; sends every block to consumer
// ---------------------------------------------------------------------------

fn producer_thread(
    tx: Sender<Block>,
    stop: Arc<AtomicBool>,
    counters: Arc<Counters>,
    seed: u64,
    large_frac: f64,
) {
    let a = SeferMalloc::new();
    let mut rng = Xorshift64::new(seed);

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        let size = pick_size(&mut rng, large_frac);
        let layout = layout_for(size);

        // SAFETY: layout has non-zero size and valid alignment (power of 2 ≥ 8).
        let ptr = unsafe { a.alloc(layout) };
        if ptr.is_null() {
            // OOM — back off briefly and retry
            thread::sleep(Duration::from_millis(1));
            continue;
        }

        // Touch first byte so the page is actually faulted in (RSS counts it).
        // SAFETY: ptr is valid for at least `layout.size()` bytes, non-null.
        unsafe { ptr.write(0xA5) };

        counters.allocs.fetch_add(1, Ordering::Relaxed);

        let block = Block { ptr, layout };
        // Send to the consumer.  If the channel is disconnected (consumer
        // exited early on a panic) — free locally to avoid a leak.
        if tx.send(block).is_err() {
            // SAFETY: we own this block; it was allocated above and not yet freed.
            unsafe { a.dealloc(ptr, layout) };
        }
    }
}

// ---------------------------------------------------------------------------
// Consumer thread — ONLY frees; drains the channel until `stop` + channel empty
// ---------------------------------------------------------------------------

fn consumer_thread(
    rx: Receiver<Block>,
    stop: Arc<AtomicBool>,
    counters: Arc<Counters>,
    cooldown: Duration,
) {
    let a = SeferMalloc::new();

    // Phase 1: run until producers signal stop AND channel is drained
    loop {
        match rx.recv_timeout(Duration::from_millis(5)) {
            Ok(block) => {
                // SAFETY: unique owner — the producer transferred ownership;
                // it is freed exactly once here.
                unsafe { a.dealloc(block.ptr, block.layout) };
                counters.frees.fetch_add(1, Ordering::Relaxed);
            }
            Err(_) => {
                // Timeout or disconnect
                if stop.load(Ordering::Relaxed) {
                    break;
                }
            }
        }
    }

    // Phase 2: cooldown — sleep so `alloc-decommit` has time to return
    // empty segments' pages to the OS before we take the final RSS snapshot.
    thread::sleep(cooldown);
}

// ---------------------------------------------------------------------------
// Monitor / CSV printer (runs on main thread)
// ---------------------------------------------------------------------------

fn run_monitor(seconds: u64, counters: &Arc<Counters>) -> (Option<u64>, Vec<String>) {
    let mut peak_rss: Option<u64> = None;
    let mut rows: Vec<String> = Vec::new();

    println!(
        "{:>6}, {:>10}, {:>14}, {:>13}, {:>18}",
        "t_s", "rss_mb", "allocs_total", "frees_total", "in_flight_estimate"
    );

    for t in 0..=seconds {
        if t > 0 {
            thread::sleep(Duration::from_secs(1));
        }

        let rss = rss::current_bytes();
        let allocs = counters.allocs.load(Ordering::Relaxed);
        let frees = counters.frees.load(Ordering::Relaxed);
        let in_flight = allocs.saturating_sub(frees);

        let rss_mb = rss.map_or("N/A".to_string(), |b| {
            format!("{:.2}", b as f64 / 1_048_576.0)
        });

        let row = format!(
            "{:>6}, {:>10}, {:>14}, {:>13}, {:>18}",
            t, rss_mb, allocs, frees, in_flight
        );
        println!("{row}");
        rows.push(row);

        if let Some(b) = rss {
            peak_rss = Some(peak_rss.map_or(b, |p| p.max(b)));
        }
    }

    (peak_rss, rows)
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    // Read env-var tuning knobs
    let n_producers: usize = std::env::var("SEFER_RSS_PRODUCERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4)
        .max(1);

    let run_secs: u64 = std::env::var("SEFER_RSS_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10)
        .max(1);

    let large_frac: f64 = std::env::var("SEFER_RSS_LARGE_FRACTION")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.01_f64)
        .clamp(0.0, 1.0);

    let decommit_enabled = cfg!(feature = "alloc-decommit");

    println!("=== sefer-alloc RSS probe ===");
    println!(
        "producers={n_producers}  run={run_secs}s  large_frac={large_frac:.4}  \
         alloc-decommit={}",
        if decommit_enabled { "ON" } else { "OFF" }
    );
    println!();
    println!(
        "Asymmetric scenario: {n_producers} producer thread(s) allocate ONLY; \
         1 consumer thread frees ONLY."
    );
    println!(
        "All cross-thread frees route through RemoteFreeRing (one ring per segment)."
    );
    println!();

    // Check RSS availability before starting threads
    let rss_available = rss::current_bytes().is_some();
    if !rss_available {
        println!("WARNING: RSS measurement unavailable on this platform.");
        println!("  rss_mb column will show N/A.  Other counters still valid.");
        println!();
    }

    let stop_flag = Arc::new(AtomicBool::new(false));
    let counters = Arc::new(Counters::new());

    // Channel: producers → consumer (unbounded mpsc; back-pressure via RSS growth)
    let (tx, rx) = channel::<Block>();

    // Spawn consumer
    let consumer_handle = {
        let stop = Arc::clone(&stop_flag);
        let ctr = Arc::clone(&counters);
        let cooldown = Duration::from_secs(3);
        thread::spawn(move || consumer_thread(rx, stop, ctr, cooldown))
    };

    // Spawn producers
    let mut producer_handles = Vec::with_capacity(n_producers);
    for i in 0..n_producers {
        let tx_i = tx.clone();
        let stop = Arc::clone(&stop_flag);
        let ctr = Arc::clone(&counters);
        // Different seed per producer for varied size distributions
        let seed = 0x9E37_79B9_7F4A_7C15u64
            .wrapping_mul((i as u64).wrapping_add(1))
            .wrapping_add(0xDEAD_BEEF_CAFE_1234);
        let lf = large_frac;
        let h = thread::spawn(move || producer_thread(tx_i, stop, ctr, seed, lf));
        producer_handles.push(h);
    }

    // Drop the main-thread copy of tx so the consumer sees EOF when all
    // producer senders are dropped.
    drop(tx);

    // ---------------------------------------------------------------------------
    // Monitor loop: print CSV every second for `run_secs` seconds
    // ---------------------------------------------------------------------------
    let t_start = Instant::now();
    let (peak_rss, _rows) = run_monitor(run_secs, &counters);
    let _ = t_start; // used implicitly via run_monitor's internal sleep

    // Signal producers to stop
    stop_flag.store(true, Ordering::Relaxed);

    // Wait for producers to finish (they stop allocating once `stop` is set)
    for h in producer_handles {
        let _ = h.join();
    }

    // Consumer continues draining + cooldown in its thread; wait for it
    println!();
    println!("[cooldown 3s — giving alloc-decommit time to return empty segments]");
    let _ = consumer_handle.join();

    // Final RSS snapshot (after cooldown)
    let final_rss = rss::current_bytes();
    let final_allocs = counters.allocs.load(Ordering::Relaxed);
    let final_frees = counters.frees.load(Ordering::Relaxed);

    // ---------------------------------------------------------------------------
    // Summary
    // ---------------------------------------------------------------------------
    println!();
    println!("=== Summary ===");
    println!("  Total allocs : {final_allocs}");
    println!("  Total frees  : {final_frees}");
    println!(
        "  In-flight    : {} (allocs-frees; should be ~0 after drain)",
        final_allocs.saturating_sub(final_frees)
    );

    match (peak_rss, final_rss) {
        (Some(peak), Some(fin)) => {
            let peak_mb = peak as f64 / 1_048_576.0;
            let fin_mb = fin as f64 / 1_048_576.0;
            let ratio = fin as f64 / peak as f64;
            println!("  Peak RSS     : {peak_mb:.2} MB");
            println!("  Final RSS    : {fin_mb:.2} MB");
            println!("  Recovery ratio (final/peak): {ratio:.4}");
            if decommit_enabled {
                if ratio < 0.80 {
                    println!("  → alloc-decommit is returning pages to the OS (ratio < 0.80).");
                } else {
                    println!(
                        "  → ratio near 1.0 — decommit may need more time or \
                         segments still live."
                    );
                }
            } else {
                println!(
                    "  → alloc-decommit OFF: ratio expected ~1.0 \
                     (segments not returned to OS)."
                );
            }
        }
        _ => {
            println!(
                "  RSS measurement unavailable on this platform — \
                 peak/final/ratio not reported."
            );
            println!(
                "  Indirect overflow proxy: in-flight estimate above \
                 (should drop to ~0 after drain)."
            );
        }
    }

    println!();
    println!("NOTE: RemoteFreeRing overflow counter is internal (pub(crate)).");
    println!("  Overflow can only be measured INDIRECTLY via in_flight_estimate");
    println!("  (allocs - frees).  For a direct counter, add pub fn overflow_count()");
    println!("  to src/alloc_core/remote_free_ring.rs — left for user approval.");
    println!();
    println!("NOTE: Run with --features alloc-decommit to compare recovery ratio.");
}
