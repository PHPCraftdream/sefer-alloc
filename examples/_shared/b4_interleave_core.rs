// B4 J1′ native interleave judge — SHARED CORE, `include!`d verbatim into
// `examples/b4_interleave_judge/main.rs` AND
// `tests/b4_interleave_judge_correctness.rs` (`include!` = byte-identical
// judge code in both; no shared support crate).
//
// ## Design contract (B4 J1′, see docs/perf/B4_INTERLEAVE_JUDGE.md)
// ONE process, BOTH arms. `#[global_allocator]` of the judge binary is the
// counting `HarnessAlloc` (System underneath): zero allocations inside the
// measurement window. The arms (`SEFER`, `MI`) are reached ONLY through
// identically-shaped `#[inline(never)]` shims (`Layout::from_size_align_unchecked`
// inside) — the exact `__rust_alloc`/`realloc`/`dealloc` face of an installed
// allocator; kernels are black-boxed-`ShimTable` generic: ONE shared LOAD fn.
//
// `unsafe` (declared deviation): the arms' `GlobalAlloc` faces through the
// shims, timer reads, and payload reads/writes of a live SAME-set block;
// every block carries a `// SAFETY:` comment.
use core::arch::x86_64::{__cpuid_count, _mm_lfence, _rdtsc};
use core::cell::Cell;
use core::hint::black_box;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

// Harness allocator: System + allocation counters (the zero-in-window oracle).

/// Counting wrapper over [`System`]: the window asserts a zero counter delta
/// (the timed work touches ONLY the arms' allocators).
pub struct HarnessAlloc;

/// Total harness-side `alloc` calls since process start.
pub static HARNESS_ALLOCS: AtomicU64 = AtomicU64::new(0);
/// Total harness-side `dealloc` calls since process start.
pub static HARNESS_DEALLOCS: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for HarnessAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        HARNESS_ALLOCS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: passthrough to `System` with the caller-provided layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        HARNESS_DEALLOCS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: passthrough to `System`; caller upholds the contract.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        HARNESS_ALLOCS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: passthrough to `System` with matching old layout.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// Serialised TSC read: `lfence; rdtsc; lfence` (invariant-TSC checked).
#[inline(never)]
pub fn tsc_now() -> u64 {
    // SAFETY: `lfence` + `rdtsc` are plain timing instructions, no state.
    unsafe {
        _mm_lfence();
        let t = _rdtsc();
        _mm_lfence();
        t
    }
}

/// `true` iff CPUID leaf 0x80000007 EDX bit 8 (Invariant TSC) is set.
#[allow(unused_unsafe)] // `unsafe` is required on the MSRV, redundant later.
pub fn tsc_invariant() -> bool {
    // SAFETY: CPUID always exists on x86_64, no side effects.
    (unsafe { __cpuid_count(0x8000_0007, 0) }.edx & (1 << 8)) != 0
}

#[cfg(windows)]
mod qpc {
    // SAFETY (module): FFI matches the documented kernel32 signatures; the
    // allowed timer-read seam.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn QueryPerformanceCounter(lpCounter: *mut i64) -> i32;
        fn QueryPerformanceFrequency(lpFrequency: *mut i64) -> i32;
    }

    pub fn now() -> i64 {
        let mut v: i64 = 0;
        // SAFETY: valid out-pointer, documented kernel32 call.
        unsafe {
            QueryPerformanceCounter(&mut v);
        }
        v
    }

    pub fn freq() -> i64 {
        let mut v: i64 = 0;
        // SAFETY: valid out-pointer, documented kernel32 call.
        unsafe {
            QueryPerformanceFrequency(&mut v);
        }
        v
    }
}

/// Wall-clock ns for a portion: QPC window scaled by its tick share.
/// 0.0 when no QPC wall-clock (non-Windows).
pub fn portion_ns(q0: i64, q1: i64, ticks: u64, window_ticks: u64) -> f64 {
    if window_ticks == 0 {
        return 0.0;
    }
    #[cfg(windows)]
    {
        let freq = qpc::freq() as f64;
        if freq <= 0.0 {
            return 0.0;
        }
        let window_ns = (q1 - q0) as f64 * 1e9 / freq;
        window_ns * (ticks as f64 / window_ticks as f64)
    }
    #[cfg(not(windows))]
    {
        let _ = (q0, q1, ticks);
        0.0
    }
}

// The arms: statics + identically-shaped shim sets.

/// Sefer arm static (the allocator under test, `production` features).
pub static SEFER: sefer_alloc::SeferAlloc = sefer_alloc::SeferAlloc::new();
/// mimalloc arm static (the comparison baseline).
pub static MI: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Number of shim sets: 0=sefer_a, 1=sefer_b, 2=mi_a, 3=mi_b and, with the
/// `ablate` feature, 4=variant_a, 5=variant_b (the sefer-alloc COPY
/// generated under `.rush/variant/`). A/A modes use two sets against the
/// SAME static.
/// Slots 4/5 stay at zero without the feature (never incremented).
pub const SHIM_SLOTS: usize = 6;

// Shim-call counters per shim SET (see `SHIM_SLOTS` for the layout).
// v2: thread-local NON-atomic cells — the judge is single-threaded, and the
// `lock inc` of the former `AtomicU64::fetch_add` (~20 cycles/op) sat inside
// the measured window of BOTH arms (see docs/perf/B4_INTERLEAVE_JUDGE.md,
// section "v2"). Const-init: no lazy-TLS allocation on first access.
thread_local! {
    static SHIM_CALLS_TL: [Cell<u64>; SHIM_SLOTS] =
        const { [const { Cell::new(0) }; SHIM_SLOTS] };
}

/// Total shim calls of set `slot` on the CURRENT thread (same thread as the
/// shims: the judge and the correctness test are single-threaded).
pub fn shim_calls_total(slot: usize) -> u64 {
    SHIM_CALLS_TL.with(|c| c[slot].get())
}

/// One arm's entry points, in the exact shape of an installed allocator's
/// `__rust_alloc` / `__rust_realloc` / `__rust_dealloc` faces.
#[derive(Clone, Copy)]
pub struct ShimTable {
    pub name: &'static str,
    pub alloc: unsafe fn(size: usize, align: usize) -> *mut u8,
    pub realloc:
        unsafe fn(ptr: *mut u8, old_size: usize, old_align: usize, new_size: usize) -> *mut u8,
    pub dealloc: unsafe fn(ptr: *mut u8, size: usize, align: usize),
    pub call_slot: usize,
}
macro_rules! shim_set {
    ($alloc:ident, $realloc:ident, $dealloc:ident, $static:ident, $slot:expr, $label:literal) => {
        #[inline(never)]
        unsafe fn $alloc(size: usize, align: usize) -> *mut u8 {
            SHIM_CALLS_TL.with(|c| {
                let v = c[$slot].get();
                c[$slot].set(v + 1);
            });
            // SAFETY: kernel-validated size table; exactly the `__rust_alloc`
            // shape of the installed allocator.
            unsafe { $static.alloc(Layout::from_size_align_unchecked(size, align)) }
        }

        #[inline(never)]
        unsafe fn $realloc(
            ptr: *mut u8,
            old_size: usize,
            old_align: usize,
            new_size: usize,
        ) -> *mut u8 {
            SHIM_CALLS_TL.with(|c| {
                let v = c[$slot].get();
                c[$slot].set(v + 1);
            });
            // SAFETY: old layout matches the kernel's alloc in this shim set;
            // `__rust_realloc` shape.
            unsafe {
                $static.realloc(
                    ptr,
                    Layout::from_size_align_unchecked(old_size, old_align),
                    new_size,
                )
            }
        }

        #[inline(never)]
        unsafe fn $dealloc(ptr: *mut u8, size: usize, align: usize) {
            SHIM_CALLS_TL.with(|c| {
                let v = c[$slot].get();
                c[$slot].set(v + 1);
            });
            // SAFETY: ptr from the same shim set with this exact layout;
            // `__rust_dealloc` shape.
            unsafe { $static.dealloc(ptr, Layout::from_size_align_unchecked(size, align)) }
        }
    };
}

shim_set!(
    sefer_a_alloc,
    sefer_a_realloc,
    sefer_a_dealloc,
    SEFER,
    0,
    "sefer_a"
);
shim_set!(
    sefer_b_alloc,
    sefer_b_realloc,
    sefer_b_dealloc,
    SEFER,
    1,
    "sefer_b"
);
shim_set!(mi_a_alloc, mi_a_realloc, mi_a_dealloc, MI, 2, "mi_a");
shim_set!(mi_b_alloc, mi_b_realloc, mi_b_dealloc, MI, 3, "mi_b");

/// A vs B: sefer (production) vs mimalloc.
pub const SEFER_A_TABLE: ShimTable = ShimTable {
    name: "sefer",
    alloc: sefer_a_alloc,
    realloc: sefer_a_realloc,
    dealloc: sefer_a_dealloc,
    call_slot: 0,
};
pub const MI_A_TABLE: ShimTable = ShimTable {
    name: "mimalloc",
    alloc: mi_a_alloc,
    realloc: mi_a_realloc,
    dealloc: mi_a_dealloc,
    call_slot: 2,
};

/// A/A calibration tables: two identical-shape shim sets, SAME static.
pub const SEFER_B_TABLE: ShimTable = ShimTable {
    name: "sefer",
    alloc: sefer_b_alloc,
    realloc: sefer_b_realloc,
    dealloc: sefer_b_dealloc,
    call_slot: 1,
};
pub const MI_B_TABLE: ShimTable = ShimTable {
    name: "mimalloc",
    alloc: mi_b_alloc,
    realloc: mi_b_realloc,
    dealloc: mi_b_dealloc,
    call_slot: 3,
};

// ── Ablation arm (feature `ablate`): the sefer-alloc COPY, in-process ───────
// `sefer_alloc_variant` is the patched copy of the crate generated
// outside the repo tree under `.rush/variant/` (relative path
// dev-dependency). The judge drives its
// `GlobalAlloc` face through the SAME identically-shaped shim sets as the
// `sefer` arm; the oracle / balance / identity logic is untouched.

/// Variant arm static (the generated sefer-alloc copy, `production` set).
#[cfg(feature = "ablate")]
pub static VARIANT: sefer_alloc_variant::SeferAlloc = sefer_alloc_variant::SeferAlloc::new();

#[cfg(feature = "ablate")]
shim_set!(
    variant_a_alloc,
    variant_a_realloc,
    variant_a_dealloc,
    VARIANT,
    4,
    "variant_a"
);
#[cfg(feature = "ablate")]
shim_set!(
    variant_b_alloc,
    variant_b_realloc,
    variant_b_dealloc,
    VARIANT,
    5,
    "variant_b"
);

/// A/B ablation: sefer vs the generated variant (same-shape shim sets).
#[cfg(feature = "ablate")]
pub const VARIANT_A_TABLE: ShimTable = ShimTable {
    name: "variant",
    alloc: variant_a_alloc,
    realloc: variant_a_realloc,
    dealloc: variant_a_dealloc,
    call_slot: 4,
};
/// A/A ablation control: variant×variant (two sets against the SAME static).
#[cfg(feature = "ablate")]
pub const VARIANT_B_TABLE: ShimTable = ShimTable {
    name: "variant",
    alloc: variant_b_alloc,
    realloc: variant_b_realloc,
    dealloc: variant_b_dealloc,
    call_slot: 5,
};

/// Judge mode: which pair of shim tables the block runner drives.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Ab,      // sefer vs mimalloc (the actual judge)
    AaSefer, // sefer×sefer A/A calibration (control)
    AaMi,    // mimalloc×mimalloc A/A calibration (control)
    /// sefer vs the generated variant copy (b4-ablate; feature `ablate`).
    #[cfg(feature = "ablate")]
    AbVariant,
    /// variant×variant A/A ablation control (feature `ablate`).
    #[cfg(feature = "ablate")]
    AaVariant,
}
impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Ab => "ab",
            Mode::AaSefer => "aa_sefer",
            Mode::AaMi => "aa_mi",
            #[cfg(feature = "ablate")]
            Mode::AbVariant => "ab_variant",
            #[cfg(feature = "ablate")]
            Mode::AaVariant => "aa_variant",
        }
    }

    pub fn tables(self) -> (&'static ShimTable, &'static ShimTable) {
        match self {
            Mode::Ab => (&SEFER_A_TABLE, &MI_A_TABLE),
            Mode::AaSefer => (&SEFER_A_TABLE, &SEFER_B_TABLE),
            Mode::AaMi => (&MI_A_TABLE, &MI_B_TABLE),
            #[cfg(feature = "ablate")]
            Mode::AbVariant => (&SEFER_A_TABLE, &VARIANT_A_TABLE),
            #[cfg(feature = "ablate")]
            Mode::AaVariant => (&VARIANT_A_TABLE, &VARIANT_B_TABLE),
        }
    }

    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "ab" => Some(Mode::Ab),
            "aa_sefer" => Some(Mode::AaSefer),
            "aa_mi" => Some(Mode::AaMi),
            #[cfg(feature = "ablate")]
            "ab_variant" => Some(Mode::AbVariant),
            #[cfg(feature = "ablate")]
            "aa_variant" => Some(Mode::AaVariant),
            _ => None,
        }
    }

    /// The variant name recorded into the raw `variant` / `variant_name`
    /// fields. Under the `ablate` feature this is a property of the BINARY
    /// BUILD (the generated copy version), so it is written in EVERY mode;
    /// "-" only when built without `ablate`.
    pub fn variant(self) -> &'static str {
        #[cfg(feature = "ablate")]
        {
            let _ = self;
            VARIANT_NAME
        }
        #[cfg(not(feature = "ablate"))]
        {
            let _ = self;
            "-"
        }
    }

    /// Alias of [`Mode::variant`] kept under the explicit raw-field name.
    pub fn variant_name(self) -> &'static str {
        self.variant()
    }

    /// The sha256 identity of the generated variant copy (from
    /// `ab_variant_identity.rs`); written in every mode under the `ablate`
    /// feature, "-" otherwise.
    pub fn variant_sha256(self) -> &'static str {
        #[cfg(feature = "ablate")]
        {
            let _ = self;
            VARIANT_SHA256
        }
        #[cfg(not(feature = "ablate"))]
        {
            let _ = self;
            "-"
        }
    }
}

// Ablation variant identity (the patch name + sha256 of the generated copy
// under docs/perf/b4_ablate/patches/). The generator
// (scripts/b4_ablate_make_variant.mjs) writes the real identity OUTSIDE the
// repository tree, at `<repo>/../.rush/variant/current/src/
// ab_variant_identity.rs`, mirrored on EVERY generation (any variant,
// including `null`); it is `include!`-ed here so the judge records the ACTUAL
// variant (name + sha256 of the copy) into the raw header / raw records
// instead of a hardcoded constant. The `current/` component keeps the path
// STABLE and variant-independent. `CARGO_MANIFEST_DIR` is the sefer-alloc
// crate root both for the judge example and for test crates that `include!`
// this file, so the relative hop is `../../.rush/variant/current`.
#[cfg(feature = "ablate")]
mod ab_variant_identity {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../.rush/variant/current/src/ab_variant_identity.rs"
    ));
}
#[cfg(feature = "ablate")]
#[allow(unused_imports)]
use ab_variant_identity::{VARIANT_NAME, VARIANT_SHA256};

// PRNG: xorshift64* (same algorithm as benches/global_alloc.rs).

pub struct XorShift64(u64);

impl XorShift64 {
    pub const fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform-ish value in `0..bound` (modulo bias irrelevant: both arms of
    /// a block share the seeded sequence).
    pub fn below(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            0
        } else {
            self.next_u64() % bound
        }
    }
}

/// Block-order PRNG: `true` = ABBA, `false` = BAAB.
pub fn order_is_abba(rng: &mut XorShift64) -> bool {
    rng.next_u64() & 1 == 1
}

/// Golden-ratio multiplier for a block's second portion seed.
pub const PORTION_SEED_G: u64 = 0x9E37_79B9_7F4A_7C15;

/// Per-shim-set ring occupancy for the activation oracle: one state per arm.
pub struct RingState {
    pub churn: [bool; CHURN_LIVE],
    pub larson: [bool; LARSON_LIVE],
}

/// Exact per-portion shim-call expectation: simulates ring occupancy per
/// shim set in lockstep with the real kernels (warmup; then `seed` and
/// `seed·G|1` per block). Churn kernels share ONE ring; larson its own.
/// Dealloc fires only when the drawn slot is occupied → `ops + hits`
/// (larson draws idx, then size); others analytic. NO slack; the mutant
/// test in `tests/b4_interleave_judge_correctness.rs` proves exactness on
/// INCOMPLETE rings.
#[allow(clippy::match_same_arms)]
pub fn simulate_portion_calls(st: &mut RingState, kernel: Kernel, seed: u64) -> u64 {
    let (ops, larson) = match kernel {
        Kernel::Churn16 | Kernel::Churn64 | Kernel::Churn256 => (CHURN_OPS, false),
        Kernel::Churn1024 => (CHURN1024_OPS, false),
        Kernel::Larson => (LARSON_OPS, true),
        _ => return kernel.expected_shim_calls(),
    };
    let live = if larson {
        LARSON_LIVE as u64
    } else {
        CHURN_LIVE as u64
    };
    let mut rng = XorShift64::new(seed);
    let mut hits = 0;
    for _ in 0..ops {
        let idx = rng.below(live) as usize;
        if larson {
            hits += st.larson[idx] as u64;
            let _sz = LARSON_MIN
                + rng.below(((LARSON_MAX - LARSON_MIN) / LARSON_STEP + 1) as u64) as usize
                    * LARSON_STEP;
            st.larson[idx] = true;
        } else {
            hits += st.churn[idx] as u64;
            st.churn[idx] = true;
        }
    }
    ops + hits
}

/// Per-arm expectations for ONE block: portions run `(portion_seed,
/// portion_seed·G|1)`; each arm has its OWN ring state, both mutated in
/// place (cumulative across blocks, exactly like the real kernels).
pub fn expected_block_calls(st: &mut RingState, kernel: Kernel, portion_seed: u64) -> (u64, u64) {
    let seed2 = portion_seed.wrapping_mul(PORTION_SEED_G) | 1;
    (
        simulate_portion_calls(st, kernel, portion_seed),
        simulate_portion_calls(st, kernel, seed2),
    )
}

// Portion size constants — FIXED BEFORE PHASE 2 (dry-run chosen, 2–20 µs).

/// alloc+free pair kernels: pairs per portion.
pub const PAIR_OPS: u64 = 512;
/// bulk64: live N per pass and passes per portion.
pub const BULK64_N: usize = 64;
pub const BULK64_PASSES: u64 = 8;
/// bulk16k: live N / passes per portion (ONE full pass per portion).
pub const BULK16K_N: usize = 16384;
pub const BULK16K_PASSES: u64 = 1;
/// churn-write: live blocks and alloc+free+write steps per portion.
pub const CHURN_LIVE: usize = 256;
pub const CHURN_OPS: u64 = 128;
/// churn1024: 48 ops of 1024 B ~ 16 µs (dry-run sizing, fixed pre-phase 2).
pub const CHURN1024_OPS: u64 = 48;
/// Vec-like growth: elements at the geometric chain top; grow+release cycles.
pub const VEC_ELEMS: usize = 512;
pub const VEC_CYCLES: u64 = 24;
/// larson-like mix: live blocks and alloc+free steps per portion.
pub const LARSON_LIVE: usize = 768;
pub const LARSON_OPS: u64 = 256;
/// larson size draw `LARSON_MIN + k*LARSON_STEP`, k < (MAX-MIN)/STEP+1
/// (kernel, simulation and test replay share it).
pub const LARSON_MIN: usize = 16;
pub const LARSON_MAX: usize = 512;
pub const LARSON_STEP: usize = 16;

/// Run-time band for `pair_rt`: sizes `RT_MIN + k*RT_GRANULE` (8-granular).
pub const RT_MIN: usize = 8;
pub const RT_MAX: usize = 128;
pub const RT_GRANULE: usize = 8;

/// The kernel registry, in fixed report order.
#[derive(Clone, Copy)]
pub enum Kernel {
    Pair16,
    Pair64,
    PairRt,
    Bulk64,
    Bulk16k,
    Churn16,
    Churn64,
    Churn256,
    Churn1024,
    VecGrow,
    Larson,
}

pub const ALL_KERNELS: [Kernel; 11] = [
    Kernel::Pair16,
    Kernel::Pair64,
    Kernel::PairRt,
    Kernel::Bulk64,
    Kernel::Bulk16k,
    Kernel::Churn16,
    Kernel::Churn64,
    Kernel::Churn256,
    Kernel::Churn1024,
    Kernel::VecGrow,
    Kernel::Larson,
];

impl Kernel {
    pub fn name(self) -> &'static str {
        match self {
            Kernel::Pair16 => "pair16",
            Kernel::Pair64 => "pair64",
            Kernel::PairRt => "pair_rt",
            Kernel::Bulk64 => "bulk64",
            Kernel::Bulk16k => "bulk16k",
            Kernel::Churn16 => "churn16",
            Kernel::Churn64 => "churn64",
            Kernel::Churn256 => "churn256",
            Kernel::Churn1024 => "churn1024",
            Kernel::VecGrow => "vec512",
            Kernel::Larson => "larson",
        }
    }

    /// Parsed name -> kernel (report generator contract).
    pub fn parse(s: &str) -> Option<Kernel> {
        ALL_KERNELS.into_iter().find(|k| k.name() == s)
    }

    /// ops/portion seed-space parameter (not the portion op count itself).
    fn ops(self) -> u64 {
        match self {
            Kernel::Pair16 | Kernel::Pair64 | Kernel::PairRt => PAIR_OPS,
            Kernel::Bulk64 => BULK64_PASSES,
            Kernel::Bulk16k => BULK16K_PASSES,
            Kernel::Churn16 | Kernel::Churn64 | Kernel::Churn256 => CHURN_OPS,
            Kernel::Churn1024 => CHURN1024_OPS,
            Kernel::VecGrow => VEC_CYCLES,
            Kernel::Larson => LARSON_OPS,
        }
    }

    /// Exact shim calls (realloc = ONE call) of one steady-state portion:
    /// pair 2×`PAIR_OPS`; bulk 2×n×passes; churn/larson 2×ops; vec per
    /// chain 1 + log2(elems) + 1.
    pub fn expected_shim_calls(self) -> u64 {
        let vec_per_cycle = 1 + VEC_ELEMS.trailing_zeros() as u64 + 1; // alloc + reallocs + dealloc
        match self {
            Kernel::Pair16 | Kernel::Pair64 | Kernel::PairRt => 2 * PAIR_OPS,
            Kernel::Bulk64 => 2 * BULK64_N as u64 * BULK64_PASSES,
            Kernel::Bulk16k => 2 * BULK16K_N as u64 * BULK16K_PASSES,
            Kernel::Churn16 | Kernel::Churn64 | Kernel::Churn256 => 2 * CHURN_OPS,
            Kernel::Churn1024 => 2 * CHURN1024_OPS,
            Kernel::VecGrow => vec_per_cycle * VEC_CYCLES,
            Kernel::Larson => 2 * LARSON_OPS,
        }
    }
}

/// Non-timed body of ONE portion: call deltas + checksum (test entry point).
#[inline(never)]
pub fn portion_body(
    table: &'static ShimTable,
    kernel: Kernel,
    seed: u64,
) -> ([u64; SHIM_SLOTS], u64) {
    let mut before = [0u64; SHIM_SLOTS];
    for (i, b) in before.iter_mut().enumerate() {
        *b = shim_calls_total(i);
    }
    let t = black_box(table);
    let ops = kernel.ops();
    let acc = match kernel {
        Kernel::Pair16 => kernel_pair(t, ops, seed, 16),
        Kernel::Pair64 => kernel_pair(t, ops, seed, 64),
        Kernel::PairRt => kernel_pair(t, ops, seed, 0),
        Kernel::Bulk64 => kernel_bulk(t, ops, seed, BULK64_N),
        Kernel::Bulk16k => kernel_bulk(t, ops, seed, BULK16K_N),
        Kernel::Churn16 => kernel_churn(t, ops, seed, 16),
        Kernel::Churn64 => kernel_churn(t, ops, seed, 64),
        Kernel::Churn256 => kernel_churn(t, ops, seed, 256),
        Kernel::Churn1024 => kernel_churn(t, ops, seed, 1024),
        Kernel::VecGrow => kernel_vec(t, ops, seed),
        Kernel::Larson => kernel_larson(t, ops, seed),
    };
    black_box(acc);
    let mut deltas = [0u64; SHIM_SLOTS];
    for (i, d) in deltas.iter_mut().enumerate() {
        *d = shim_calls_total(i) - before[i];
    }
    (deltas, acc)
}

/// The FOREIGN shim slots for a portion driven by `slot` (must stay 0).
pub fn foreign_slots(slot: usize) -> [usize; SHIM_SLOTS - 1] {
    match slot {
        0 => [1, 2, 3, 4, 5],
        1 => [0, 2, 3, 4, 5],
        2 => [0, 1, 3, 4, 5],
        #[cfg(feature = "ablate")]
        4 => [0, 1, 2, 3, 5],
        #[cfg(feature = "ablate")]
        5 => [0, 1, 2, 3, 4],
        _ => [0, 1, 2, 4, 5],
    }
}

/// Sum of the foreign shim-call deltas of one portion (must be 0).
pub fn foreign_delta(deltas: &[u64; SHIM_SLOTS], slot: usize) -> u64 {
    foreign_slots(slot).iter().map(|&i| deltas[i]).sum()
}

/// One timed portion: ticks + ns + call deltas + foreign delta (0 required).
#[inline(never)]
pub fn run_portion(
    table: &'static ShimTable,
    kernel: Kernel,
    seed: u64,
) -> (u64, f64, [u64; SHIM_SLOTS], u64) {
    let q0 = qpc_ms();
    let t0 = tsc_now();
    let (deltas, _) = portion_body(table, kernel, seed);
    let t1 = tsc_now();
    let q1 = qpc_ms();
    let ticks = t1.wrapping_sub(t0);
    let foreign = foreign_delta(&deltas, table.call_slot);
    assert_eq!(
        foreign,
        0,
        "{} / {}: foreign shim calls inside the portion window ({foreign})",
        table.name,
        kernel.name()
    );
    (ticks, portion_ns(q0, q1, ticks, ticks), deltas, foreign)
}

#[cfg(windows)]
fn qpc_ms() -> i64 {
    qpc::now()
}
#[cfg(not(windows))]
fn qpc_ms() -> i64 {
    0
}

// Kernel bodies + live-slot tables: PURE MOVE to
// `b4_interleave_kernels.rs` (included here; same scope, zero logic change).
include!("b4_interleave_kernels.rs");

/// One interleaved block: 4 portions ABBA/BAAB, same portion seed for both
/// arms. Raw record: ticks A1 B1 B2 A2 + ns + harness/call/foreign deltas.
pub struct BlockRec {
    pub kernel: &'static str,
    pub order: &'static str,
    pub block: u64,
    pub seed: u64,
    pub a1: u64,
    pub b1: u64,
    pub b2: u64,
    pub a2: u64,
    pub a1_ns: f64,
    pub b1_ns: f64,
    pub b2_ns: f64,
    pub a2_ns: f64,
    pub harness_allocs: u64,
    pub a1_calls: u64,
    pub b1_calls: u64,
    pub b2_calls: u64,
    pub a2_calls: u64,
    pub a1_foreign: u64,
    pub b1_foreign: u64,
    pub b2_foreign: u64,
    pub a2_foreign: u64,
}

/// Runs one block; `abba` selects the execution order.
#[allow(clippy::too_many_lines)]
pub fn run_block(
    a: &'static ShimTable,
    b: &'static ShimTable,
    kernel: Kernel,
    block: u64,
    seed: u64,
    abba: bool,
) -> BlockRec {
    let before = HARNESS_ALLOCS.load(Ordering::Relaxed);
    // Same-arm portions are order-agnostic: sum-of-pairs stats by design.
    let (a1, a1_ns, a1c, a1f, b1, b1_ns, b1c, b1f, b2, b2_ns, b2c, b2f, a2, a2_ns, a2c, a2f);
    if abba {
        let (t, ns, c, f) = run_portion(a, kernel, seed);
        (a1, a1_ns, a1c, a1f) = (t, ns, c, f);
        let (t, ns, c, f) = run_portion(b, kernel, seed);
        (b1, b1_ns, b1c, b1f) = (t, ns, c, f);
        let (t, ns, c, f) = run_portion(b, kernel, seed.wrapping_mul(PORTION_SEED_G) | 1);
        (b2, b2_ns, b2c, b2f) = (t, ns, c, f);
        let (t, ns, c, f) = run_portion(a, kernel, seed.wrapping_mul(PORTION_SEED_G) | 1);
        (a2, a2_ns, a2c, a2f) = (t, ns, c, f);
    } else {
        let (t, ns, c, f) = run_portion(b, kernel, seed);
        (b1, b1_ns, b1c, b1f) = (t, ns, c, f);
        let (t, ns, c, f) = run_portion(a, kernel, seed);
        (a1, a1_ns, a1c, a1f) = (t, ns, c, f);
        let (t, ns, c, f) = run_portion(a, kernel, seed.wrapping_mul(PORTION_SEED_G) | 1);
        (a2, a2_ns, a2c, a2f) = (t, ns, c, f);
        let (t, ns, c, f) = run_portion(b, kernel, seed.wrapping_mul(PORTION_SEED_G) | 1);
        (b2, b2_ns, b2c, b2f) = (t, ns, c, f);
    }
    let after = HARNESS_ALLOCS.load(Ordering::Relaxed);
    BlockRec {
        kernel: kernel.name(),
        order: if abba { "ABBA" } else { "BAAB" },
        block,
        seed,
        a1,
        b1,
        b2,
        a2,
        a1_ns,
        b1_ns,
        b2_ns,
        a2_ns,
        harness_allocs: after - before,
        a1_calls: a1c[a.call_slot],
        b1_calls: b1c[b.call_slot],
        b2_calls: b2c[b.call_slot],
        a2_calls: a2c[a.call_slot],
        a1_foreign: a1f,
        b1_foreign: b1f,
        b2_foreign: b2f,
        a2_foreign: a2f,
    }
}
