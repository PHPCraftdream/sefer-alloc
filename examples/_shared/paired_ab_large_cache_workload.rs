// R10-5 shared warm-recycle workload for the `paired_ab_large_cache_off` /
// `paired_ab_large_cache_on` process-level A/B/B/A judge binaries.
//
// ## Why this file exists (and why it is `include!`d, not a real module)
//
// This is the warm-Large-cache-hit gate's counterpart to R10-2's
// `examples/_shared/paired_ab_medium_workload.rs`. R10-2 drove a working set
// deliberately LARGER than `LARGE_CACHE_SLOTS` (8) to force the baseline arm
// into Large-cache MISSES (real OS churn) — the right design for measuring
// "medium-classes vs the cold Large path". THIS file drives the OPPOSITE
// pattern: a working set deliberately SMALLER than `LARGE_CACHE_SLOTS`,
// churned in a tight alloc/free/alloc steady-state loop so the baseline arm's
// Large allocations consistently HIT the warm cache — measuring "small-path
// freelist recycle vs the Large path's BEST case (warm cache)", which is the
// comparison R9-4 §2.4's "~90 µs → ~60 ns" consolation-prize claim needs to
// survive to stand up.
//
// `include!` (not a shared crate module) is used for exactly the same reason
// `paired_ab_medium_workload.rs` documents: Cargo examples are independent
// compilation units with no shared `examples/`-support crate in this project,
// and duplicating the workload body across two wrappers would risk the
// two-binaries-silently-drift-apart failure mode. Both wrappers
// (`paired_ab_large_cache_off.rs`, `paired_ab_large_cache_on.rs`) `include!`
// this file verbatim, so the workload code is byte-for-byte identical in both
// binaries — the ONLY difference is which Cargo feature set the binary was
// compiled with.
//
// ## What this measures (the warm-vs-warm comparison R9-4 did NOT make)
//
// R9-4 §2.4 cited R8-9 §4.3's ~90 µs Large free + ~90 µs re-alloc cost
// (a whole-segment `VirtualFree` + `VirtualAlloc` round-trip) vs the small
// path's ~60 ns freelist push/pop. But under `production` the Large-segment
// free-cache (`OPT-E`, `LARGE_CACHE_SLOTS = 8`, gated on `alloc-decommit`)
// is ACTIVE: a warm Large-cache HIT recycles a recently-freed dedicated span
// via cheap in-process bookkeeping (header rewrite + table re-register —
// pages stay committed, NO recommit, NO syscall; see
// `src/alloc_core/alloc_core_large.rs` lines 158-166), NOT the full
// `VirtualFree`+`VirtualAlloc` round-trip. So R9-4's ~90 µs number is the
// Large-cache MISS cost, not the typical WARM-cache recycle cost most
// steady-state programs actually see.
//
// This workload isolates the WARM-cache steady state:
//   - **Warm-up (untimed):** `WARMUP_ROUNDS` alloc/free cycles at `WS_LEN`
//     objects populate the Large cache (baseline) / the small-path freelist
//     (treatment) so the timed region starts in genuine steady state.
//   - **Timed steady state:** one `Instant` pair around `ROUNDS` alloc/free
//     cycles. Each cycle allocates `WS_LEN` objects at `size_bytes` (baseline:
// Large-cache HIT; treatment: freelist pop) then frees them (baseline:
// cache deposit; treatment: freelist push). `WS_LEN = 6` is deliberately
// below `LARGE_CACHE_SLOTS = 8` so every baseline alloc after warm-up hits
// the warm cache — the comparison this gate exists to make.
//
// ## The cache-hit PROOF (why `large_cache_hits` is emitted as a RESULT line)
//
// The whole point of this gate is to NOT repeat R9-4's methodology gap
// (comparing against the Large path's worst case). So the workload reads
// `SeferAlloc::stats().large_cache_hits` after the timed region and emits it
// as a RESULT line. Under the baseline arm this MUST read ~`WS_LEN * ROUNDS`
// (every steady-state alloc hit the warm cache); under the treatment arm it
// MUST read 0 (the small path never touches the Large cache). If the baseline
// reading were NOT large, the comparison would be warm-vs-cold again — the
// exact gap this gate exists to close. This requires building BOTH arms with
// the `alloc-stats` feature (NOT part of `production`) so the per-hit counter
// is live; the driver (`scripts/r10_5_large_cache_gate.mjs`) passes
// `--features "production alloc-stats"` / `"production medium-classes-wide
// alloc-stats"`. The per-hit increment is a single Relaxed load+store (NOT a
// `lock xadd`; see `alloc_core_large.rs` lines 129-157) on the owning thread —
// negligible against the µs-scale Large-path work and asymmetric only in that
// the baseline arm pays it (the treatment arm's small path does not increment
// it, because it does not hit the Large cache).
//
// ## Determinism
//
// The PRNG (xorshift64*, fixed seed) is copied from `paired_ab_workload.rs` /
// `paired_ab_medium_workload.rs` so both arms see the same pseudo-random
// sequence if any index randomisation is needed. The allocation size is a
// single fixed value per process launch (selected by the wrapper via argv),
// not PRNG-driven, so both arms allocate the exact same size.

use std::alloc::Layout;
use std::hint::black_box;
use std::time::Instant;

// ---------------------------------------------------------------------------
// RSS / commit-charge probes — thin KiB wrappers over the `proc-probe` crate's
// re-export of `proc-memstat`'s same-instant `snapshot()` (bytes). Identical
// to `paired_ab_medium_workload.rs`'s wrappers — kept here so this file is
// self-contained when `include!`d.
// ---------------------------------------------------------------------------

fn rss_kib() -> u64 {
    proc_probe::snapshot().rss / 1024
}

fn commit_kib() -> u64 {
    proc_probe::snapshot().commit / 1024
}

// ---------------------------------------------------------------------------
// Constants — the working set and steady-state structure
// ---------------------------------------------------------------------------

const KIB: usize = 1024;

/// Working-set size: number of simultaneously-live objects per round.
/// Deliberately BELOW `LARGE_CACHE_SLOTS` (8) — see module doc. `6` leaves a
/// 2-slot safety margin so the baseline arm's every post-warm-up alloc hits
/// the warm cache (no eviction pressure), which is the comparison this gate
/// exists to make. A `WS_LEN > 8` would force cache misses (R10-2's design,
/// a DIFFERENT question); a `WS_LEN <= 8` that exactly equals 8 risks the last
/// deposit evicting the oldest slot, so 6 is the clean safe choice.
const WS_LEN: usize = 6;

/// Untimed warm-up rounds: each is one full alloc+free cycle at `WS_LEN`.
/// After `WARMUP_ROUNDS` cycles the Large cache (baseline) / small freelist
/// (treatment) is populated, so the timed region starts in genuine steady
/// state. `3` is enough to guarantee the cache is warm regardless of the
/// baseline's initial cold state (round 1 reserves, round 2+ recycles).
const WARMUP_ROUNDS: usize = 3;

/// Timed steady-state rounds: each is one full alloc+free cycle at `WS_LEN`.
/// Chosen so the single timed `Instant` pair spans multi-milliseconds (giving
/// `Instant` ample resolution headroom) at the expected ~hundreds-of-ns per
/// warm recycle op: 3000 rounds × 12 ops × ~200 ns ≈ 7 ms.
const ROUNDS: usize = 3000;

/// Alignment for all allocations. Matches `paired_ab_medium_workload.rs`.
const ALIGN: usize = 8;

/// Sentinel write value (same pattern as `paired_ab_medium_workload.rs`).
const TOUCH: u64 = 0xA5A5_A5A5_A5A5_A5A5;

// ---------------------------------------------------------------------------
// PRNG — deterministic xorshift64* (copied from paired_ab_medium_workload.rs /
// benches/global_alloc.rs, fixed seed) so both arms see the same pseudo-random
// sequence if any index randomisation is needed. Currently the workload is a
// fixed cyclic churn (no PRNG needed), but the PRNG is retained for the
// `black_box` anti-optimisation fence and for future extension.
// ---------------------------------------------------------------------------

struct XorShift64(u64);

impl XorShift64 {
    const fn new(seed: u64) -> Self {
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

// ---------------------------------------------------------------------------
// Allocation primitives — direct `std::alloc` calls routed through the
// installed `#[global_allocator]` (SeferAlloc in both arms). Each function
// writes the first 16 bytes (volatile) to (a) prove the allocation is "used"
// so the compiler cannot eliminate it, and (b) touch the committed pages.
// Identical to `paired_ab_medium_workload.rs`'s primitives.
// ---------------------------------------------------------------------------

/// Allocate `size` bytes, write the first 16 bytes (volatile), return the
/// pointer. Panics on allocation failure (the probe is invalid if alloc
/// fails — `WS_LEN` objects should never exhaust the address space).
fn alloc_one(size: usize) -> *mut u8 {
    let layout = Layout::from_size_align(size, ALIGN).unwrap();
    // SAFETY: `layout` has non-zero size and valid (power-of-two, <=
    // usize::MAX/2) alignment (8), satisfying `GlobalAlloc::alloc`'s
    // preconditions.
    let p = unsafe { std::alloc::alloc(layout) };
    assert!(!p.is_null(), "alloc({size}) failed — probe is invalid");
    touch16(p);
    p
}

/// Free a pointer allocated with `alloc_one(size)`.
fn dealloc_one(p: *mut u8, size: usize) {
    if p.is_null() {
        return;
    }
    let layout = Layout::from_size_align(size, ALIGN).unwrap();
    // SAFETY: `p` was allocated with this exact `layout` (same size, same
    // align) by `alloc_one`, and is freed exactly once.
    unsafe { std::alloc::dealloc(p, layout) };
}

/// Write the first 16 bytes (two u64 words) via `write_volatile` — proves the
/// allocation is real and touches the first cache line.
/// SAFETY: caller guarantees `p` points to at least 16 writable bytes (every
/// size in this workload is >= 1.5 MiB).
fn touch16(p: *mut u8) {
    unsafe {
        std::ptr::write_volatile(p.cast::<u64>(), TOUCH);
        std::ptr::write_volatile(p.cast::<u64>().add(1), TOUCH);
    }
}

// ---------------------------------------------------------------------------
// The warm-recycle steady-state driver — called by both wrapper binaries'
// `main`. Returns `(recycle_ns, size_bytes)` so the wrapper can emit the
// `RESULT` lines.
//
// Structure: WARMUP_ROUNDS untimed alloc/free cycles (populate the cache /
// freelist), then ONE `Instant` pair around ROUNDS timed alloc/free cycles.
// Each cycle allocates `WS_LEN` objects at `size_bytes`, writes the first 16
// bytes (page touch + dead-code fence), then frees them all. The single
// `Instant` pair (not per-round) keeps timing-overhead at ~2 QPC reads total
// (~150 ns out of a multi-ms region — 0.00x%), so the measured `recycle_ns`
// is dominated by the actual alloc/free work, not by the clock.
// ---------------------------------------------------------------------------

pub fn run_warm_recycle_workload(size_bytes: usize) -> (u128, usize) {
    // Burn one PRNG draw to make the XorShift64 struct "used" (anti-opt fence;
    // retained for parity with paired_ab_medium_workload.rs though the size
    // sequence is a fixed single value here).
    let _ = black_box(XorShift64::new(0xCAFE).next_usize());

    let mut live: Vec<(*mut u8, usize)> = Vec::with_capacity(WS_LEN);

    // ── Warm-up (untimed): populate the Large cache / small freelist ──────
    for _ in 0..WARMUP_ROUNDS {
        live.clear();
        for _ in 0..WS_LEN {
            let p = alloc_one(size_bytes);
            live.push((p, size_bytes));
        }
        for &(p, sz) in live.iter() {
            dealloc_one(p, sz);
        }
        live.clear();
    }
    black_box(live.as_ptr());

    // ── Timed steady state: one Instant pair around the full churn ────────
    let t = Instant::now();
    for _ in 0..ROUNDS {
        live.clear();
        for _ in 0..WS_LEN {
            let p = alloc_one(size_bytes);
            live.push((p, size_bytes));
        }
        for &(p, sz) in live.iter() {
            dealloc_one(p, sz);
        }
    }
    let recycle_ns = t.elapsed().as_nanos();
    black_box(live.as_ptr());
    live.clear();

    (recycle_ns, size_bytes)
}
