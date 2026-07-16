//! R7-A0 transition-zone probe: S = 32, 40, 48, 56, 63 at holes = 0 for
//! SMALL_MAX. Exists solely to fill the P5 threshold-choice data gap between
//! the main sweep's S = 16 and S = 64 points.

#![cfg(feature = "alloc-core")]
#![allow(clippy::cast_precision_loss)]

use std::alloc::Layout;
use std::time::{Duration, Instant};

use sefer_alloc::{AllocCore, SegmentLayout};

/// S values to probe in the 32..63 transition zone.
const S_VALUES: &[u32] = &[32, 40, 48, 56, 63];

const SCANS_PER_TRIAL: usize = 256;
const MAX_REPEATS: usize = 30;
const TARGET_TOTAL_BLOCKS: u64 = 200_000;

fn base_of(p: *mut u8) -> usize {
    SegmentLayout::segment_base_of(p as usize)
}

struct SegmentCapacity {
    primordial: usize,
    fresh: usize,
}

fn measure_capacity(class_idx: usize) -> SegmentCapacity {
    let mut core = AllocCore::new().expect("AllocCore::new");
    let block_size = AllocCore::dbg_block_size(class_idx);
    let layout = Layout::from_size_align(block_size, 8).unwrap();
    let mut counts = [0usize; 2];
    let mut bases: Vec<usize> = Vec::new();
    loop {
        let p = core.alloc(layout);
        assert!(!p.is_null());
        let b = base_of(p);
        let idx = match bases.iter().position(|&sb| sb == b) {
            Some(i) => i,
            None => {
                bases.push(b);
                bases.len() - 1
            }
        };
        if idx >= 2 {
            break;
        }
        counts[idx] += 1;
    }
    SegmentCapacity {
        primordial: counts[0],
        fresh: counts[1],
    }
}

fn adaptive_repeats(class_idx: usize, s: u32) -> usize {
    let block_size = AllocCore::dbg_block_size(class_idx) as u64;
    let per_seg = (SegmentLayout::SEGMENT as u64 / block_size).max(1);
    let per_trial = per_seg.saturating_mul(s as u64).max(1);
    let scaled = TARGET_TOTAL_BLOCKS / per_trial;
    scaled.clamp(1, MAX_REPEATS as u64) as usize
}

fn construct_and_measure(
    class_idx: usize,
    s: u32,
    cap: &SegmentCapacity,
) -> (Duration, Duration, Duration) {
    let block_size = AllocCore::dbg_block_size(class_idx);
    let layout = Layout::from_size_align(block_size, 8).unwrap();
    let repeats = adaptive_repeats(class_idx, s);
    let mut samples: Vec<Duration> = Vec::new();

    for _ in 0..repeats {
        let mut core = AllocCore::new().expect("AllocCore::new");
        let mut buckets: Vec<Vec<*mut u8>> = Vec::new();
        let mut seen_bases: Vec<usize> = Vec::new();
        let total = s as usize + 1;

        while seen_bases.len() < total {
            let idx = seen_bases.len();
            let this_cap = if idx == 0 { cap.primordial } else { cap.fresh };
            let mut bucket = Vec::with_capacity(this_cap);
            let mut this_base: Option<usize> = None;
            for _ in 0..this_cap {
                let p = core.alloc(layout);
                assert!(!p.is_null());
                let b = base_of(p);
                match this_base {
                    None => this_base = Some(b),
                    Some(est) => assert_eq!(b, est),
                }
                bucket.push(p);
            }
            seen_bases.push(this_base.unwrap());
            buckets.push(bucket);
        }

        // Pop off the extra "current" segment
        let _current = buckets.pop();
        seen_bases.pop();

        // Punch: target (last) gets one free, non-targets stay full
        let target = buckets.last_mut().unwrap();
        let victim = target.pop().unwrap();
        unsafe { core.dealloc(victim, layout) };

        // Time scans
        let t0 = Instant::now();
        for _ in 0..SCANS_PER_TRIAL {
            let found = core.alloc(layout);
            std::hint::black_box(found);
            assert!(!found.is_null());
            unsafe { core.dealloc(found, layout) };
        }
        let batch = t0.elapsed();
        samples.push(batch / (SCANS_PER_TRIAL as u32));
    }

    samples.sort_unstable();
    let sum: Duration = samples.iter().sum();
    let mean = sum / (samples.len() as u32);
    let p50 = samples[((samples.len() as f64 - 1.0) * 0.50).round() as usize];
    let p99 = samples[((samples.len() as f64 - 1.0) * 0.99)
        .round()
        .min(samples.len() as f64 - 1.0) as usize];
    (mean, p50, p99)
}

fn main() {
    let class_idx = AllocCore::dbg_small_class_count() - 1;
    let cap = measure_capacity(class_idx);
    eprintln!(
        "directory_threshold_probe: S=32..63 transition zone, class=SMALL_MAX ({} B), holes=0%",
        SegmentLayout::SMALL_MAX
    );

    for &s in S_VALUES {
        let (mean, p50, p99) = construct_and_measure(class_idx, s, &cap);
        let slots_walked = s.saturating_sub(1);
        let per_slot_ns = if slots_walked > 0 {
            mean.as_secs_f64() * 1e9 / slots_walked as f64
        } else {
            0.0
        };
        eprintln!(
            "directory_threshold_probe: S={s:>4} slots_walked={slots_walked:>4} \
             mean={:>9.1}ns p50={:>9.1}ns p99={:>9.1}ns per_slot={per_slot_ns:.1}ns",
            mean.as_secs_f64() * 1e9,
            p50.as_secs_f64() * 1e9,
            p99.as_secs_f64() * 1e9,
        );
    }
}
