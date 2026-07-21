// R10-2 shared phased workload for the `paired_ab_medium_off` /
// `paired_ab_medium_on` process-level A/B/B/A judge binaries.
//
// ## Why this file exists (and why it is `include!`d, not a real module)
//
// This is the medium-classes production-gate counterpart to
// `examples/_shared/paired_ab_workload.rs`. That earlier file drives a mixed
// small-size (16 B–1024 B) churn to compare SeferAlloc against mimalloc /
// System. THIS file drives a phased 256 KiB–1 MiB workload to compare
// SeferAlloc built WITHOUT `medium-classes` (`production`) against the SAME
// source built WITH `medium-classes` (`production,medium-classes`) — the
// promotion-decision question R9-3 (`docs/perf/R9_3_MEDIUM_CLASSES_PRODUCTION_GATES.md`)
// deferred to a methodologically clean wall-clock judge.
//
// `include!` (not a shared crate module) is used for exactly the same reason
// `paired_ab_workload.rs` documents: Cargo examples are independent
// compilation units with no shared `examples/`-support crate in this project,
// and duplicating the workload body across two wrappers would risk the
// two-binaries-silently-drift-apart failure mode. Both wrappers
// (`paired_ab_medium_off.rs`, `paired_ab_medium_on.rs`) `include!` this file
// verbatim, so the workload code is byte-for-byte identical in both binaries —
// the ONLY difference is which Cargo feature set the binary was compiled with.
//
// ## What this measures (that R8-9 / R9-3 did NOT)
//
// R8-9 measured the feature's TARGET range (256 KiB–1 MiB) through `AllocCore`
// directly (`benches/medium_size_sweep.rs`, single-process, single-run per
// config). R9-3 measured the UNAFFECTED small-size path (16–1024 B) via iai
// (deterministic Ir) + one noisy criterion run. NEITHER measured the phased
// wall-clock of a realistic medium-range workload via INDEPENDENT process
// launches with paired statistics — which is what this file + the
// `paired-ab-runner.mjs` A/B/B/A protocol deliver:
//
//   - **Alloc phase** — allocate `WS_LEN` simultaneously-live objects (each >
//     the old ~253 KiB SMALL_MAX, so the baseline arm routes them through the
//     dedicated 4 MiB Large path), write the first 16 bytes (page touch +
//     dead-code fence), hold them.
//   - **Free phase** — free every held object.
//   - **Realloc phase** — allocate `WS_LEN` objects at 256 KiB (untimed
//     setup), realloc-grow each through `REALLOC_STEPS` (timed), free the
//     grown objects (untimed teardown).
//
// Each phase is timed independently (`Instant`-bounded across `ROUNDS`
// rounds) and emitted as its own `RESULT <phase>_ns=<n>` line, so the runner
// can pair each phase independently. The alloc + free phases expose the
// magazine-vs-OS-round-trip difference (the Large path's cost); the realloc
// phase exposes the move-leg-vs-in-place-grow difference (the realloc path's
// cost that R9-3 §4.3 flagged at +173.9% Ir but never measured as real
// wall-clock).
//
// ## Working set: why `WS_LEN = 16` (> `LARGE_CACHE_SLOTS = 8`)
//
// `src/alloc_core/alloc_core.rs:81` defines `LARGE_CACHE_SLOTS = 8` — the
// Large-segment free-cache holds up to 8 recently-freed dedicated 4 MiB spans
// to amortise the OS `VirtualFree`/`VirtualAlloc` round-trip on the Large
// alloc/free path. If the working set were ≤ 8 objects, the baseline
// (non-medium-classes) arm would recycle every freed span from the cache on
// the next alloc (a ~ns fast path) and the measurement would capture the
// Large cache's warm-reuse behaviour, NOT the actual class-routing difference
// this gate exists to measure. `WS_LEN = 16` (2× the cache) forces 8 of the
// 16 allocs per round to miss the cache and hit the OS — enough real Large
// work to expose the difference without making each process launch
// impractically slow. The Large cache is active in BOTH arms (it is gated on
// `alloc-decommit`, which is part of `production`), so this is a fair
// comparison of "baseline Large-path + 8-slot cache" vs "medium-classes small
// path + 8-slot cache", not "cache on vs cache off".
//
// ## Determinism
//
// The PRNG (xorshift64*, fixed seed) is copied from `paired_ab_workload.rs` /
// `benches/global_alloc.rs` so the exact same pseudo-random index sequence
// drives both arms. The size sequence is a fixed cyclic walk over the six
// medium classes (not PRNG-driven) so both arms allocate the exact same
// sizes in the exact same order.

use std::alloc::Layout;
use std::hint::black_box;
use std::time::Instant;

// ---------------------------------------------------------------------------
// RSS / commit-charge probes — thin KiB wrappers over the `proc-probe` crate's
// re-export of `proc-memstat`'s same-instant `snapshot()` (bytes). Identical
// to `paired_ab_workload.rs`'s wrappers — kept here so this file is
// self-contained when `include!`d.
// ---------------------------------------------------------------------------

fn rss_kib() -> u64 {
    proc_probe::snapshot().rss / 1024
}

fn commit_kib() -> u64 {
    proc_probe::snapshot().commit / 1024
}

// ---------------------------------------------------------------------------
// Constants — the working set and phased structure
// ---------------------------------------------------------------------------

const KIB: usize = 1024;

/// Working-set size: number of simultaneously-live medium objects per round.
/// Deliberately 2× `LARGE_CACHE_SLOTS` (8) — see module doc.
const WS_LEN: usize = 16;

/// The six exact medium-class sizes (`src/alloc_core/size_classes.rs` EXTRAS
/// block) — 256 KiB / 320 KiB / 384 KiB / 512 KiB / 768 KiB / 1 MiB. The
/// alloc phase cycles through these so both arms see the same size mix. Every
/// one of these exceeds the OLD ~253 KiB `SMALL_MAX` (so the baseline routes
/// them Large) and is ≤ the NEW 1 MiB `SMALL_MAX` (so medium-classes routes
/// them small).
const MEDIUM_SIZES: &[usize] = &[
    256 * KIB,
    320 * KIB,
    384 * KIB,
    512 * KIB,
    768 * KIB,
    1024 * KIB,
];

/// The realloc phase starts every object at 256 KiB (the smallest medium
/// class). This is a realistic starting size for a growable buffer that will
/// be realloc-grown into the medium range.
const REALLOC_BASE: usize = 256 * KIB;

/// The realloc-grow sequence: 256 KiB → 384 KiB → 512 KiB → 768 KiB. Each
/// step crosses a medium-class boundary under medium-classes (forcing a
/// move-leg: alloc + copy + dealloc), while under the baseline every step is
/// an in-place Large grow within the dedicated 4 MiB span (header update,
/// ~0 cost). This is the exact asymmetry R9-3 §4.3 flagged at +173.9% Ir.
const REALLOC_STEPS: &[usize] = &[384 * KIB, 512 * KIB, 768 * KIB];

/// Rounds: each round is one full alloc+free cycle (alloc phase) or one
/// full setup+realloc+teardown cycle (realloc phase). Chosen so each phase's
/// total wall-clock is comfortably multi-millisecond (giving `Instant` enough
/// resolution headroom) without inflating per-process launch time past what
/// the runner's `pairs` × 4-launch block budget comfortably handles.
const ROUNDS: usize = 20;

/// Alignment for all allocations. Matches `paired_ab_workload.rs`.
const ALIGN: usize = 8;

/// Sentinel write value (same pattern as `paired_ab_workload.rs`).
const TOUCH: u64 = 0xA5A5_A5A5_A5A5_A5A5;

// ---------------------------------------------------------------------------
// PRNG — deterministic xorshift64* (copied from paired_ab_workload.rs /
// benches/global_alloc.rs, fixed seed) so both arms see the same pseudo-random
// sequence if any index randomisation is needed. Currently the size sequence
// is a fixed cyclic walk (no PRNG needed), but the PRNG is retained for
// future extension and for the `black_box` anti-optimisation fence.
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
// ---------------------------------------------------------------------------

/// Allocate `size` bytes, write the first 16 bytes (volatile), return the
/// pointer. Panics on allocation failure (the probe is invalid if alloc
/// fails — 16 objects should never exhaust the address space).
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

/// Free a pointer allocated with `alloc_one(size)` / `realloc_one(_, size)`.
fn dealloc_one(p: *mut u8, size: usize) {
    if p.is_null() {
        return;
    }
    let layout = Layout::from_size_align(size, ALIGN).unwrap();
    // SAFETY: `p` was allocated with this exact `layout` (same size, same
    // align) by `alloc_one` or `realloc_one`, and is freed exactly once.
    unsafe { std::alloc::dealloc(p, layout) };
}

/// Realloc-grow `p` from `old_size` to `new_size`. Writes the first 8 bytes
/// of the result (volatile) to fence the realloc. Panics on failure.
fn realloc_one(p: *mut u8, old_size: usize, new_size: usize) -> *mut u8 {
    let old_layout = Layout::from_size_align(old_size, ALIGN).unwrap();
    // SAFETY: `p` was allocated with `old_layout` (or grown to `old_size` by
    // a prior `realloc_one`), and `new_size > 0`, satisfying
    // `GlobalAlloc::realloc`'s preconditions.
    let q = unsafe { std::alloc::realloc(p, old_layout, new_size) };
    assert!(
        !q.is_null(),
        "realloc({old_size} -> {new_size}) failed — probe is invalid"
    );
    // SAFETY: `q` is a freshly allocated/relocated block of at least 8 bytes
    // (every size in this workload is >= 256 KiB); `write_volatile` prevents
    // the store (and thus the realloc) being optimized away.
    unsafe { std::ptr::write_volatile(q.cast::<u64>(), TOUCH) };
    q
}

/// Write the first 16 bytes (two u64 words) via `write_volatile` — proves the
/// allocation is real and touches the first cache line.
/// SAFETY: caller guarantees `p` points to at least 16 writable bytes (every
/// size in this workload is >= 256 KiB).
fn touch16(p: *mut u8) {
    unsafe {
        std::ptr::write_volatile(p.cast::<u64>(), TOUCH);
        std::ptr::write_volatile(p.cast::<u64>().add(1), TOUCH);
    }
}

// ---------------------------------------------------------------------------
// Phases — each is a standalone function so the timing boundary is explicit
// in `run_phased_workload` below.
// ---------------------------------------------------------------------------

/// Alloc phase: allocate `WS_LEN` objects at cyclically-selected medium
/// sizes, write the first 16 bytes, push into `live`. The caller times this
/// function's wall-clock.
fn alloc_phase(live: &mut Vec<(*mut u8, usize)>) {
    for i in 0..WS_LEN {
        let size = MEDIUM_SIZES[i % MEDIUM_SIZES.len()];
        let p = alloc_one(size);
        live.push((p, size));
    }
    black_box(live.as_ptr());
}

/// Free phase: free every object in `live` (at its tracked current size),
/// then clear. The caller times this function's wall-clock.
fn free_phase(live: &mut Vec<(*mut u8, usize)>) {
    for &(p, size) in live.iter() {
        dealloc_one(p, size);
    }
    black_box(live.as_ptr());
    live.clear();
}

/// Realloc phase (timed portion only): grow every object in `live` through
/// `REALLOC_STEPS`, updating each entry's tracked current size. The setup
/// (alloc at `REALLOC_BASE`) and teardown (free at final size) are done
/// OUTSIDE this function and are NOT timed — see `run_phased_workload`.
fn realloc_grow_phase(live: &mut [(*mut u8, usize)]) {
    for &step in REALLOC_STEPS {
        for entry in live.iter_mut() {
            let (p, cur) = *entry;
            let q = realloc_one(p, cur, step);
            *entry = (q, step);
        }
    }
    black_box(live.as_ptr());
}

// ---------------------------------------------------------------------------
// The phased workload driver — called by both wrapper binaries' `main`.
// Returns `(elapsed_ns, alloc_ns, free_ns, realloc_ns)` so the wrapper can
// emit one `RESULT` line per phase.
//
// Structure: alloc + free phases are interleaved per round (alloc a fresh
// pool, then free it — steady-state churn, bounded memory). The realloc
// phase runs separately after the alloc/free churn: each round does an
// untimed setup (alloc WS_LEN at REALLOC_BASE), a timed realloc-grow through
// REALLOC_STEPS, and an untimed teardown (free at final size).
// ---------------------------------------------------------------------------

pub fn run_phased_workload() -> (u128, u128, u128, u128) {
    // Burn one PRNG draw to make the XorShift64 struct "used" (anti-opt fence;
    // the PRNG is retained for future index randomisation but the current
    // size sequence is a fixed cyclic walk).
    let _ = black_box(XorShift64::new(0xCAFE).next_usize());

    let mut alloc_ns: u128 = 0;
    let mut free_ns: u128 = 0;
    let mut realloc_ns: u128 = 0;

    let mut live: Vec<(*mut u8, usize)> = Vec::with_capacity(WS_LEN);

    // ── Alloc + Free phases: interleaved per round ──────────────────────
    for _ in 0..ROUNDS {
        live.clear();

        let t_alloc = Instant::now();
        alloc_phase(&mut live);
        alloc_ns += t_alloc.elapsed().as_nanos();

        let t_free = Instant::now();
        free_phase(&mut live);
        free_ns += t_free.elapsed().as_nanos();
    }

    // ── Realloc phase: setup (untimed) → grow (timed) → teardown (untimed) ─
    for _ in 0..ROUNDS {
        // Untimed setup: alloc WS_LEN objects at REALLOC_BASE.
        live.clear();
        for _ in 0..WS_LEN {
            let p = alloc_one(REALLOC_BASE);
            live.push((p, REALLOC_BASE));
        }

        // Timed: realloc-grow through REALLOC_STEPS.
        let t_realloc = Instant::now();
        realloc_grow_phase(&mut live);
        realloc_ns += t_realloc.elapsed().as_nanos();

        // Untimed teardown: free at the final grown size.
        free_phase(&mut live);
    }

    let elapsed_ns = alloc_ns + free_ns + realloc_ns;
    (elapsed_ns, alloc_ns, free_ns, realloc_ns)
}
