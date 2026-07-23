//! R14-4 (task #289) THROWAWAY pad-target sweep — NOT a shipping artifact.
//!
//! Resolves the open question R11-3's design doc (§4.4) deliberately left
//! open: once a growing medium-class realloc is diverted to Large, how much
//! should the promoted request be PADDED beyond the caller's `new_size`?
//!
//! Candidates swept, all at the 256 KiB threshold:
//!   (a) fixed 2 MiB pad target (what R11-3's probe used, unjustified there)
//!   (b) `max(new_size, threshold * 2)` == max(new_size, 512 KiB) floor
//!   (c) `new_size` itself, no artificial padding at all
//!
//! ## Why this should mostly be a wash under `production`
//!
//! `AllocCore::alloc_large` (`src/alloc_core/alloc_core_large.rs`) rounds
//! every request up to a whole `SEGMENT` (4 MiB) multiple UNLESS the opt-in
//! `exact-span-large` feature is enabled — and `production` does NOT include
//! `exact-span-large` (see `Cargo.toml`'s `production = [...]` bundle). So
//! under the mainline `production,medium-classes` build this task ships
//! against, every candidate pad target below one SEGMENT (4 MiB) — (a), (b),
//! and (c), since every value in this sweep's growth sequence is <= 1 MiB —
//! gets rounded up to the SAME 4 MiB commit by `alloc_large` regardless. This
//! probe measures whether that reasoning holds: commit/RSS after promotion
//! should be identical across (a)/(b)/(c), and wall-clock should be
//! statistically indistinguishable (the promotion call pays the SAME
//! `alloc_large` cost regardless of the logical `size` argument, since
//! `usable` collapses to the same 4 MiB either way).
//!
//! **Build:** `cargo build --release --example r14_4_pad_target_probe --features "production,medium-classes,alloc-stats"`
//! **Run:**   `target/release/examples/r14_4_pad_target_probe <mode>`
//!   - `mode`: `fixed2mib` | `floor512kib` | `nopad`

use sefer_alloc::SeferAlloc;
use std::alloc::Layout;
use std::time::Instant;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const KIB: usize = 1024;
const ALIGN: usize = 8;
const THRESHOLD_KIB: usize = 256;
const TOUCH: u64 = 0xA5A5_A5A5_A5A5_A5A5;

const GROWTH_SEQUENCE_KIB: &[usize] = &[64, 96, 144, 216, 324, 486, 729, 1024];
const ROUNDS: usize = 30;
const WS_LEN: usize = 8;

fn touch(p: *mut u8) {
    unsafe {
        std::ptr::write_volatile(p.cast::<u64>(), TOUCH);
    }
}

fn alloc_one(size: usize) -> *mut u8 {
    let layout = Layout::from_size_align(size, ALIGN).unwrap();
    let p = unsafe { std::alloc::alloc(layout) };
    assert!(!p.is_null(), "alloc({size}) failed");
    touch(p);
    p
}

fn dealloc_one(p: *mut u8, size: usize) {
    let layout = Layout::from_size_align(size, ALIGN).unwrap();
    unsafe { std::alloc::dealloc(p, layout) };
}

fn realloc_one(p: *mut u8, old_size: usize, new_size: usize) -> *mut u8 {
    let old_layout = Layout::from_size_align(old_size, ALIGN).unwrap();
    let q = unsafe { std::alloc::realloc(p, old_layout, new_size) };
    assert!(!q.is_null(), "realloc({old_size} -> {new_size}) failed");
    touch(q);
    q
}

fn pad_target(mode: &str, new_size: usize) -> usize {
    match mode {
        "fixed2mib" => 2048 * KIB,
        "floor512kib" => new_size.max((THRESHOLD_KIB * 2) * KIB),
        "nopad" => new_size,
        other => panic!("unknown mode: {other}"),
    }
}

/// Diverted growth mirroring `try_promote_to_large`'s shape: on the first
/// step whose target crosses `THRESHOLD_KIB`, realloc to `pad_target(...)`;
/// subsequent steps ride the real OPT-G in-place-grow fast path as long as
/// they stay within the padded span.
fn diverted_growth(mode: &str) -> (*mut u8, usize) {
    let mut size = GROWTH_SEQUENCE_KIB[0] * KIB;
    let mut p = alloc_one(size);
    let mut promoted = false;
    let mut real_size = size;

    for &step_kib in &GROWTH_SEQUENCE_KIB[1..] {
        let new_size = step_kib * KIB;
        if !promoted && new_size >= THRESHOLD_KIB * KIB {
            let target = pad_target(mode, new_size);
            p = realloc_one(p, real_size, target);
            real_size = target;
            promoted = true;
        } else if promoted {
            if new_size > real_size {
                p = realloc_one(p, real_size, new_size);
                real_size = new_size;
            }
        } else {
            p = realloc_one(p, size, new_size);
            real_size = new_size;
        }
        size = new_size;
    }
    (p, real_size)
}

fn run_arm(mode: &str) {
    let mut total_ns = 0u128;

    {
        let mut live: Vec<(*mut u8, usize)> = Vec::with_capacity(WS_LEN);
        for _ in 0..WS_LEN {
            live.push(diverted_growth(mode));
        }
        for (p, size) in live {
            dealloc_one(p, size);
        }
    }

    let mut live_after: Vec<(*mut u8, usize)> = Vec::new();
    for round in 0..ROUNDS {
        let mut live: Vec<(*mut u8, usize)> = Vec::with_capacity(WS_LEN);
        let t0 = Instant::now();
        for _ in 0..WS_LEN {
            live.push(diverted_growth(mode));
        }
        total_ns += t0.elapsed().as_nanos();
        // Free the PREVIOUS round's objects now (not the one just allocated) so
        // only one round's working set (WS_LEN objects) is ever live at once —
        // mirrors the R11-3 probe's per-round churn shape. Keep only the FINAL
        // round's objects alive past the loop, to measure steady-state commit.
        if round + 1 < ROUNDS {
            std::mem::swap(&mut live_after, &mut live);
            for (p, size) in live {
                dealloc_one(p, size);
            }
        } else {
            live_after = live;
        }
    }

    let ops = (ROUNDS * WS_LEN) as u64;
    println!("RESULT mode={mode}");
    println!("RESULT {mode}_total_ns={total_ns}");
    println!("RESULT {mode}_ns_per_growth_seq={}", total_ns / ops as u128);

    let rss = proc_probe::snapshot().rss / 1024;
    let commit = proc_probe::snapshot().commit / 1024;
    println!("RESULT {mode}_rss_after_kib={rss}");
    println!("RESULT {mode}_commit_after_kib={commit}");

    for (p, size) in live_after {
        dealloc_one(p, size);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("nopad");
    run_arm(mode);
}
