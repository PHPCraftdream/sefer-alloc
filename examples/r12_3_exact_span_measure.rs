//! R12-3 THROWAWAY measurement harness — NOT a shipping artifact.
//!
//! Measures the RSS/commit-charge cost of a single Large allocation at each
//! of the R12-3 review's 5 control sizes (260 KiB, 512 KiB, 1 MiB, 1.75 MiB,
//! 4 MiB), using `proc-memstat::snapshot()` (same-instant RSS + commit-charge
//! self-probe) immediately before and after each `alloc` + a full-span
//! write-touch (so every committed page is actually faulted in and counted
//! by RSS, not just reserved).
//!
//! Build/run WITHOUT the feature (baseline, today's `n_segments * SEGMENT`
//! rounding):
//!   cargo run --release --example r12_3_exact_span_measure --features production
//!
//! Build/run WITH the feature (experimental exact-span sizing):
//!   cargo run --release --example r12_3_exact_span_measure --features "production,exact-span-large"
//!
//! Each run allocates ONE fresh `AllocCore` — process starts cold, so the
//! first Large alloc's delta is a clean "this many bytes came from this one
//! allocation" measurement, unclouded by prior segments/caching.

use core::alloc::Layout;
use proc_memstat::snapshot;
use sefer_alloc::{AllocCore, SegmentLayout};

const KIB: u64 = 1024;
const MIB: u64 = 1024 * 1024;

fn label_and_size(name: &str, bytes: u64) -> (String, usize) {
    (name.to_string(), bytes as usize)
}

fn main() {
    let exact_span = cfg!(feature = "exact-span-large");
    println!("=== R12-3 exact-span-large measurement ===");
    println!(
        "feature exact-span-large: {}",
        if exact_span { "ON" } else { "OFF" }
    );
    println!();
    println!(
        "{:>10} | {:>14} | {:>14} | {:>14} | {:>14} | {:>10}",
        "size", "rss_before_KB", "rss_after_KB", "rss_delta_KB", "commit_delta_KB", "amplif_x"
    );

    let sizes = [
        label_and_size("260KiB", 260 * KIB),
        label_and_size("512KiB", 512 * KIB),
        label_and_size("1MiB", MIB),
        label_and_size("1.75MiB", (7 * MIB) / 4),
        label_and_size("4MiB", 4 * MIB),
    ];

    for (name, size) in sizes {
        // Fresh AllocCore per size — isolates each measurement from the
        // large_cache / prior segments of earlier iterations in this loop.
        let mut ac = AllocCore::new().expect("primordial");
        let layout = Layout::from_size_align(size, 8).unwrap();

        let before = snapshot();
        let ptr = ac.alloc(layout);
        assert!(!ptr.is_null(), "OOM allocating {name}");
        // Touch the ENTIRE physical span the allocator committed for this
        // segment (span_usable), not just the requested `size` — Windows RSS
        // (working-set) only counts pages that were actually FAULTED IN by a
        // touch, so touching only `size` bytes would undercount whatever
        // extra headroom the rounding policy reserved+committed beyond the
        // request (exactly the amplification this harness exists to make
        // visible). `dbg_span_usable_of` reads the header's stable physical
        // span MEASURED FROM THE SEGMENT BASE (not from `ptr`, which sits
        // past the header at `hdr_aligned`); `SegmentLayout::segment_base_of`
        // recovers that base, and the touch length is `span_usable` minus
        // the payload's offset from it — the exact number of committed bytes
        // remaining from `ptr` onward.
        let span_usable = ac.dbg_span_usable_of(ptr);
        let base = SegmentLayout::segment_base_of(ptr as usize);
        let payload_off = (ptr as usize) - base;
        let touch_len = span_usable.saturating_sub(payload_off);
        // SAFETY: `ptr` is valid for `touch_len` bytes — `span_usable` is the
        // segment's own physically-committed span measured from `base`, and
        // `touch_len = span_usable - (ptr - base)` is exactly the remaining
        // committed span from `ptr` onward, so this write stays within the
        // segment's committed pages.
        unsafe {
            ptr.write_bytes(0xAB, touch_len);
        }
        let after = snapshot();

        let rss_delta = after.rss.saturating_sub(before.rss);
        let commit_delta = after.commit.saturating_sub(before.commit);
        let amplif = rss_delta as f64 / size as f64;

        println!(
            "{:>10} | {:>14} | {:>14} | {:>14} | {:>14} | {:>10.2}",
            name,
            before.rss / 1024,
            after.rss / 1024,
            rss_delta / 1024,
            commit_delta / 1024,
            amplif
        );

        // SAFETY (R6-MS-1/2): `ptr` is a live allocation from this AllocCore
        // made with `layout`, freed exactly once here.
        unsafe { ac.dealloc(ptr, layout) };
        drop(ac);
    }

    println!();
    println!("amplif_x = rss_delta_bytes / requested_size_bytes (1.0x = no waste)");
}
