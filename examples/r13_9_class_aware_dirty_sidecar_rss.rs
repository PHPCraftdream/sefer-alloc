//! R13-9 (task #279) THROWAWAY measurement harness — NOT a shipping artifact.
//!
//! Measures the ACTUAL process RSS/commit-charge cost of materialising the
//! `class-aware-dirty` per-(segment,class) sidecar (`alloc_core::dirty_by_class::
//! PerClassDirty`) across N heaps, as a companion to this task's production
//! promotion-gate measurement (`docs/perf/R13_9_CLASS_AWARE_DIRTY_PRODUCTION_GATE.md`).
//!
//! `docs/perf/R12_7_CLASS_AWARE_DIRTY_ROUTING_GATE.md` (§3.1) states the
//! sidecar is "6.1 KiB per materialised heap" — that number is the RAW
//! `size_of::<PerClassDirty>()` (`SMALL_CLASS_COUNT * WORDS_PER_CLASS * 8`
//! bytes = 49 * 16 * 8 = 6,272 bytes with the default 49-class table). The
//! sidecar is reserved via `aligned_vmem::leak_zeroed_pages`, which rounds
//! its request UP to a whole number of 4 KiB pages
//! (`dirty_by_class.rs::PER_CLASS_DIRTY_SIZE`) — so the actual COMMITTED
//! footprint per materialised heap is 2 pages = 8,192 bytes = 8.0 KiB, not
//! 6.1 KiB. This harness confirms that arithmetic against REAL process RSS
//! deltas (not just the `size_of` computation) for N = 4, 8, 16 heaps, each
//! forced to materialise its own sidecar via one genuine cross-thread free
//! (the ONLY way `ensure_per_class_dirty` is reached in production code —
//! see `registry::heap_core_xthread::set_dirty_bit_for_segment`).
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example r13_9_class_aware_dirty_sidecar_rss --features "production class-aware-dirty alloc-stats"
//! ```
//!
//! Requires `class-aware-dirty` (the sidecar does not exist without it) and
//! `alloc-xthread`/`alloc-segment-directory` (both pulled in transitively by
//! `class-aware-dirty`'s own `Cargo.toml` feature dependency list).

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-segment-directory",
    feature = "class-aware-dirty"
))]
#![allow(clippy::cast_precision_loss)]

use std::alloc::Layout;
use std::thread;

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};

mod rss {
    /// Return the current process RSS in bytes, or `None` if unsupported.
    /// Byte-for-byte copy of `examples/rss_probe.rs`'s `mod rss` — kept local
    /// (not extracted to a shared module) per this project's existing
    /// per-example throwaway-harness convention (see `r13_6_large_cache_hit_rate_measure.rs`,
    /// `r12_4_reserved_capacity_measure.rs`, none of which share code either).
    pub fn current_bytes() -> Option<u64> {
        #[cfg(target_os = "linux")]
        return linux_rss();

        #[cfg(target_os = "windows")]
        return windows_rss();

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        return None;
    }

    #[cfg(target_os = "linux")]
    fn linux_rss() -> Option<u64> {
        let text = std::fs::read_to_string("/proc/self/status").ok()?;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
                return Some(kb * 1024);
            }
        }
        None
    }

    #[cfg(target_os = "windows")]
    fn windows_rss() -> Option<u64> {
        use std::os::raw::c_ulong;

        #[repr(C)]
        struct ProcessMemoryCounters {
            cb: c_ulong,
            page_fault_count: c_ulong,
            peak_working_set_size: usize,
            working_set_size: usize,
            quota_peak_paged_pool_usage: usize,
            quota_paged_pool_usage: usize,
            quota_peak_non_paged_pool_usage: usize,
            quota_non_paged_pool_usage: usize,
            pagefile_usage: usize,
            peak_pagefile_usage: usize,
        }

        // SAFETY: plain Win32 API declarations with correct signatures and
        // calling conventions (identical to `examples/rss_probe.rs`'s proven
        // usage); `GetCurrentProcess` returns a pseudo-handle (always valid);
        // the output struct is stack-local and fully initialised before the
        // call.
        extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn GetProcessMemoryInfo(
                process: *mut std::ffi::c_void,
                ppsmemcounters: *mut ProcessMemoryCounters,
                cb: c_ulong,
            ) -> i32;
        }

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

        // SAFETY: `&mut pmc` is valid for `size_of::<ProcessMemoryCounters>()`
        // bytes; `pmc.cb` is set to that same size as required by the API.
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

/// Claim a heap, force ONE genuine cross-thread free into it from a helper
/// thread (the only production code path that calls
/// `ensure_per_class_dirty`), and return the heap so its sidecar stays
/// materialised for the RSS snapshot. Mirrors
/// `benches/r12_7_class_aware_dirty_wallclock.rs::run_round`'s owner/producer
/// shape, reduced to the minimum needed to trip ONE sidecar materialisation.
fn claim_heap_with_materialised_sidecar() -> *mut HeapCore {
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let heap_addr = heap as usize;

    // Class 40: comfortably above any materialisation-carve range used by
    // this crate's own directory-materialisation tests, mirroring the wallclock
    // bench's own `TARGET_CLASS`/`PRODUCER_CLASS_INDICES` choice.
    let class_idx = 40usize;
    let block_size = AllocCore::dbg_block_size(class_idx);
    let layout = Layout::from_size_align(block_size, 8).expect("valid layout");

    // Owner allocates one block...
    let p = unsafe { (*heap).alloc(layout) } as usize;
    assert!(p != 0, "owner alloc returned null");

    // ...a helper thread frees it remotely, which is the ONLY call path that
    // reaches `set_dirty_bit_for_segment` -> `ensure_per_class_dirty` and
    // therefore the only way the sidecar is EVER materialised in production.
    thread::spawn(move || {
        let _ = bootstrap::ensure();
        let target = heap_addr as *mut HeapCore;
        let ptr = p as *mut u8;
        // SAFETY: `ptr` is a live allocation from `heap`'s `layout`, made by
        // the owner above and not yet freed; freeing it from a different
        // thread than the owner is exactly the cross-thread-free path this
        // harness exists to exercise.
        unsafe { (*target).dealloc(ptr, layout) };
    })
    .join()
    .expect("remote-free helper thread must not panic");

    heap
}

fn main() {
    let _ = bootstrap::ensure();

    println!("=== R13-9 class-aware-dirty sidecar RSS measurement ===");
    println!(
        "PER_CLASS_DIRTY_WORDS (computed): SMALL_CLASS_COUNT={} * WORDS_PER_CLASS=16",
        AllocCore::dbg_small_class_count()
    );
    let raw_bytes = AllocCore::dbg_small_class_count() * 16 * 8;
    println!(
        "raw size_of::<PerClassDirty>(): {raw_bytes} bytes ({:.2} KiB)",
        raw_bytes as f64 / 1024.0
    );
    const PAGE: usize = 4096;
    let page_rounded = raw_bytes.div_ceil(PAGE) * PAGE;
    println!(
        "page-rounded (leak_zeroed_pages) footprint per materialised heap: \
         {page_rounded} bytes ({:.2} KiB, {} page(s))",
        page_rounded as f64 / 1024.0,
        page_rounded / PAGE
    );
    println!();

    let rss_available = rss::current_bytes().is_some();
    if !rss_available {
        println!("WARNING: RSS measurement unavailable on this platform; only the");
        println!("computed sidecar size above is reported.");
        return;
    }

    let counts: &[usize] = &[4, 8, 16];
    for &n in counts {
        // Fresh baseline snapshot right before this N's heaps are claimed —
        // avoids the FIRST arm's one-time process warm-up cost (loader,
        // primordial segment, thread-pool machinery) polluting later arms.
        let rss_before = rss::current_bytes().expect("checked available above");

        let mut heaps: Vec<*mut HeapCore> = Vec::with_capacity(n);
        for _ in 0..n {
            heaps.push(claim_heap_with_materialised_sidecar());
        }

        let rss_after = rss::current_bytes().expect("checked available above");
        let delta = rss_after.saturating_sub(rss_before);
        let expected_min = (page_rounded * n) as u64;

        println!(
            "N={n:<3} heaps  RSS before={:>10.2} KiB  after={:>10.2} KiB  \
             delta={:>8.2} KiB  ({:.2} KiB/heap)  [sidecar-only floor: {:.2} KiB]",
            rss_before as f64 / 1024.0,
            rss_after as f64 / 1024.0,
            delta as f64 / 1024.0,
            delta as f64 / 1024.0 / n as f64,
            expected_min as f64 / 1024.0,
        );

        // Recycle so the NEXT N's arm starts from a comparable (not
        // monotonically growing) heap population -- registry slots are
        // reused, but per this crate's documented lazy-materialisation
        // discipline the SIDECAR POINTER itself, once materialised, is never
        // un-materialised for the process lifetime (only the slot's logical
        // heap identity is recycled) -- this is disclosed in the report
        // rather than silently producing a monotonic delta across arms.
        for h in heaps {
            unsafe { HeapRegistry::recycle(h) };
        }
    }

    println!();
    println!(
        "NOTE: the sidecar itself (`RacyPtrCell<PerClassDirty>` -> \
         `leak_zeroed_pages`) is never freed once materialised (process-lifetime \
         leak by design, same discipline as `segment_directory`'s and \
         `HeapOverflow`'s sidecars) -- so RSS deltas across successive N arms in \
         THIS process are NOT simply additive/comparable to a fresh-process \
         run of each N in isolation. Each arm's per-heap KiB figure is the \
         reliable number; cross-arm process-level accumulation is a secondary, \
         expected artifact of slot reuse, not a leak bug."
    );
}
