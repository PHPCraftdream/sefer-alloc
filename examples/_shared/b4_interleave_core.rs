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

/// Shim-call counters per shim SET: 0=sefer_a, 1=sefer_b, 2=mi_a, 3=mi_b
/// (A/A modes use two sets against the SAME static).
pub static SHIM_CALLS: [AtomicU64; 4] = {
    [
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
    ]
};

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
            SHIM_CALLS[$slot].fetch_add(1, Ordering::Relaxed);
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
            SHIM_CALLS[$slot].fetch_add(1, Ordering::Relaxed);
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
            SHIM_CALLS[$slot].fetch_add(1, Ordering::Relaxed);
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

/// Judge mode: which pair of shim tables the block runner drives.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Ab,      // sefer vs mimalloc (the actual judge)
    AaSefer, // sefer×sefer A/A calibration (control)
    AaMi,    // mimalloc×mimalloc A/A calibration (control)
}
impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Ab => "ab",
            Mode::AaSefer => "aa_sefer",
            Mode::AaMi => "aa_mi",
        }
    }

    pub fn tables(self) -> (&'static ShimTable, &'static ShimTable) {
        match self {
            Mode::Ab => (&SEFER_A_TABLE, &MI_A_TABLE),
            Mode::AaSefer => (&SEFER_A_TABLE, &SEFER_B_TABLE),
            Mode::AaMi => (&MI_A_TABLE, &MI_B_TABLE),
        }
    }

    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "ab" => Some(Mode::Ab),
            "aa_sefer" => Some(Mode::AaSefer),
            "aa_mi" => Some(Mode::AaMi),
            _ => None,
        }
    }
}

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

// Live-slot storage: `thread_local!` + `Cell<*mut u8>`, const-init arrays
// (no TLS allocation inside the window); PER ARM — a shared ring would free
// a live pointer CROSS-ALLOCATOR (violates the caller's GlobalAlloc contract).
const ARMS: usize = 4;

thread_local! {
    static CHURN_SLOTS: [[Cell<*mut u8>; CHURN_LIVE]; ARMS] =
        const { [const { [const { Cell::new(core::ptr::null_mut()) }; CHURN_LIVE] }; ARMS] };
    /// Exact size of the allocation in each churn slot; every churn dealloc
    /// uses THIS size (no foreign Layout).
    static CHURN_SIZES: [[Cell<usize>; CHURN_LIVE]; ARMS] =
        const { [const { [const { Cell::new(0) }; CHURN_LIVE] }; ARMS] };
    static LARSON_SLOTS: [[Cell<*mut u8>; LARSON_LIVE]; ARMS] =
        const { [const { [const { Cell::new(core::ptr::null_mut()) }; LARSON_LIVE] }; ARMS] };
    static BULK_SLOTS: [[Cell<*mut u8>; BULK16K_N]; ARMS] =
        const { [const { [const { Cell::new(core::ptr::null_mut()) }; BULK16K_N] }; ARMS] };
    static VEC_SLOT: [Cell<*mut u8>; ARMS] =
        const { [const { Cell::new(core::ptr::null_mut()) }; ARMS] };
    /// Current capacity (bytes/8) of the vec slot: teardown dealloc and
    /// every realloc use the EXACT old layout.
    static VEC_CAPS: [Cell<usize>; ARMS] = const { [const { Cell::new(0) }; ARMS] };
    /// Exact size of the allocation in each larson slot (the size is
    /// random); every larson dealloc uses THIS size.
    static LARSON_SIZES: [[Cell<usize>; LARSON_LIVE]; ARMS] =
        const { [const { [const { Cell::new(0) }; LARSON_LIVE] }; ARMS] };
}

pub const VEC_ALIGN: usize = 8; // u64 elements
pub const LARSON_ALIGN: usize = 16;
/// Align parameters of the other shim-facing kernels (raw header fields).
pub const PAIR_ALIGN: usize = 8;
pub const CHURN_ALIGN: usize = 8;

// The kernels — `#[inline(never)]`, generic over the black-boxed shim
// table; seeded so both arms run the IDENTICAL sequence; checksum return.

/// alloc+free pairs of a fixed `size` (or runtime size when `size == 0`).
#[inline(never)]
fn kernel_pair(t: &ShimTable, ops: u64, seed: u64, size: usize) -> u64 {
    let mut rng = XorShift64::new(seed);
    let mut acc: u64 = 0;
    let mut i: u64 = 0;
    while i < ops {
        let sz = if size == 0 {
            // runtime size in [RT_MIN, RT_MAX], 8-byte granular
            RT_MIN
                + ((rng.below(((RT_MAX - RT_MIN) / RT_GRANULE + 1) as u64) as usize) * RT_GRANULE)
        } else {
            size
        };
        let p = unsafe {
            // SAFETY: shim = the installed allocator's __rust_alloc face;
            // sz in [8,128], align 8.
            (t.alloc)(sz, 8)
        };
        if p.is_null() {
            panic!("{}: pair alloc({sz}) returned null", t.name);
        }
        unsafe {
            // SAFETY: p is valid for writes of `sz` bytes (just alloc'd).
            core::ptr::write_bytes(p, (i & 0xff) as u8, 8.min(sz));
        }
        acc = acc.wrapping_add(p as u64);
        unsafe {
            // SAFETY: same allocation, matching layout.
            (t.dealloc)(p, sz, 8)
        };
        i += 1;
    }
    acc
}

/// bulk: N live blocks per pass × `passes`, freeing each cohort per pass.
#[inline(never)]
fn kernel_bulk(t: &ShimTable, passes: u64, seed: u64, n: usize) -> u64 {
    let mut rng = XorShift64::new(seed);
    let mut acc: u64 = 0;
    let mut pass: u64 = 0;
    while pass < passes {
        let arm = t.call_slot;
        BULK_SLOTS.with(|slots| {
            let mut i = 0;
            while i < n {
                let p = unsafe {
                    // SAFETY: __rust_alloc face, size 64 align 8.
                    (t.alloc)(64, 8)
                };
                if p.is_null() {
                    panic!("{}: bulk alloc(64) returned null", t.name);
                }
                slots[arm][i].set(p);
                unsafe {
                    // SAFETY: fresh 64-byte allocation.
                    core::ptr::write_bytes(p, 0xA5, 8);
                }
                acc = acc.wrapping_add(p as u64);
                i += 1;
            }
            let mut j = 0;
            while j < n {
                let p = slots[arm][j].get();
                unsafe {
                    // SAFETY: p from the alloc above (null panics): layout
                    // is exactly (64, 8) of the same shim set.
                    (t.dealloc)(p, 64, 8)
                };
                slots[arm][j].set(core::ptr::null_mut());
                j += 1;
            }
            acc = acc.wrapping_add(rng.next_u64() & 1);
        });
        pass += 1;
    }
    acc
}

/// churn-write: `CHURN_LIVE` ring; per op free one occupied slot, alloc a
/// `size` replacement, write the first 16 bytes.
#[inline(never)]
fn kernel_churn(t: &ShimTable, ops: u64, seed: u64, size: usize) -> u64 {
    let mut rng = XorShift64::new(seed);
    let mut acc: u64 = 0;
    let mut i: u64 = 0;
    let arm = t.call_slot;
    CHURN_SLOTS.with(|slots| {
        CHURN_SIZES.with(|sizes| {
            while i < ops {
                let idx = rng.below(CHURN_LIVE as u64) as usize;
                let old = slots[arm][idx].get();
                let old_sz = sizes[arm][idx].get();
                if !old.is_null() {
                    sizes[arm][idx].set(0);
                    unsafe {
                        // SAFETY: old from the churn alloc of this shim set,
                        // exactly (old_sz, 8), stored in CHURN_SIZES.
                        (t.dealloc)(old, old_sz, 8)
                    };
                }
                let p = unsafe {
                    // SAFETY: __rust_alloc face, fixed size, align 8.
                    (t.alloc)(size, 8)
                };
                if p.is_null() {
                    panic!("{}: churn alloc({size}) returned null", t.name);
                }
                slots[arm][idx].set(p);
                sizes[arm][idx].set(size);
                unsafe {
                    // SAFETY: fresh `size`-byte allocation.
                    core::ptr::write_bytes(p, (i & 0xff) as u8, 16.min(size));
                }
                acc = acc.wrapping_add(p as u64);
                i += 1;
            }
        })
    });
    acc
}

/// Vec-like growth via realloc: geometric doubling, then release; `cycles`×.
#[inline(never)]
fn kernel_vec(t: &ShimTable, cycles: u64, _seed: u64) -> u64 {
    let mut acc: u64 = 0;
    let mut c: u64 = 0;
    let arm = t.call_slot;
    VEC_SLOT.with(|slots| {
        VEC_CAPS.with(|caps| {
            while c < cycles {
                slots[arm].set(core::ptr::null_mut());
                caps[arm].set(0);
                let mut elems: usize = 1;
                let mut cap: usize = 1;
                while elems <= VEC_ELEMS {
                    let p = if cap < elems {
                        let old = slots[arm].get();
                        let np = unsafe {
                            // SAFETY: (cap*8, VEC_ALIGN) is the EXACT layout
                            // of the previous alloc/realloc of this shim set
                            // (caps[arm] mirrors it); __rust_realloc shape.
                            (t.realloc)(old, cap * 8, VEC_ALIGN, elems * 8)
                        };
                        slots[arm].set(np);
                        np
                    } else {
                        let np = unsafe {
                            // SAFETY: __rust_alloc face, elems*8 bytes, align 8.
                            (t.alloc)(elems * 8, VEC_ALIGN)
                        };
                        slots[arm].set(np);
                        np
                    };
                    if p.is_null() {
                        panic!(
                            "{}: vec alloc/realloc({} bytes) returned null",
                            t.name,
                            elems * 8
                        );
                    }
                    cap = elems;
                    caps[arm].set(cap);
                    unsafe {
                        // SAFETY: byte `cap*8 - 1` is in the current buffer.
                        core::ptr::write_bytes(p.add(cap * 8 - 1), c as u8, 1);
                    }
                    acc = acc.wrapping_add(p as u64);
                    elems = elems.saturating_mul(2);
                }
                let p = slots[arm].get();
                let capb = caps[arm].get();
                unsafe {
                    // SAFETY: (capb*8, VEC_ALIGN) is the exact layout of the
                    // last buffer of this shim set (mirrored in caps[arm]).
                    (t.dealloc)(p, capb * 8, VEC_ALIGN)
                };
                slots[arm].set(core::ptr::null_mut());
                caps[arm].set(0);
                c += 1;
            }
        })
    });
    acc
}

/// larson-like mix: sizes `LARSON_MIN..=LARSON_MAX` (STEP-granular)
/// alloc+write, freed from a deep ring; draws idx, then size.
#[inline(never)]
fn kernel_larson(t: &ShimTable, ops: u64, seed: u64) -> u64 {
    let mut rng = XorShift64::new(seed);
    let mut acc: u64 = 0;
    let mut i: u64 = 0;
    let arm = t.call_slot;
    LARSON_SLOTS.with(|slots| {
        LARSON_SIZES.with(|sizes| {
            while i < ops {
                let idx = rng.below(LARSON_LIVE as u64) as usize;
                let sz = LARSON_MIN
                    + (rng.below(((LARSON_MAX - LARSON_MIN) / LARSON_STEP + 1) as u64) as usize)
                        * LARSON_STEP;
                let old = slots[arm][idx].get();
                let old_sz = sizes[arm][idx].get();
                if !old.is_null() {
                    sizes[arm][idx].set(0);
                    unsafe {
                        // SAFETY: old from the larson alloc of this shim set,
                        // exactly (old_sz, LARSON_ALIGN), stored in
                        // LARSON_SIZES; __rust_dealloc shape.
                        (t.dealloc)(old, old_sz, LARSON_ALIGN)
                    };
                }
                let p = unsafe {
                    // SAFETY: __rust_alloc face, sz in [16,512], align 16.
                    (t.alloc)(sz, LARSON_ALIGN)
                };
                if p.is_null() {
                    panic!("{}: larson alloc({sz}) returned null", t.name);
                }
                slots[arm][idx].set(p);
                sizes[arm][idx].set(sz);
                unsafe {
                    // SAFETY: fresh `sz`-byte allocation.
                    core::ptr::write_bytes(p, (sz & 0xff) as u8, 16.min(sz));
                }
                acc = acc.wrapping_add(p as u64 ^ sz as u64);
                i += 1;
            }
        })
    });
    acc
}

/// Non-timed body of ONE portion: call deltas + checksum (test entry point).
#[inline(never)]
pub fn portion_body(table: &'static ShimTable, kernel: Kernel, seed: u64) -> ([u64; 4], u64) {
    let mut before = [0u64; 4];
    for (i, c) in SHIM_CALLS.iter().enumerate() {
        before[i] = c.load(Ordering::Relaxed);
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
    let mut deltas = [0u64; 4];
    for (i, c) in SHIM_CALLS.iter().enumerate() {
        deltas[i] = c.load(Ordering::Relaxed) - before[i];
    }
    (deltas, acc)
}

/// The FOREIGN shim slots for a portion driven by `slot` (must stay 0).
pub fn foreign_slots(slot: usize) -> [usize; 3] {
    match slot {
        0 => [1, 2, 3],
        1 => [0, 2, 3],
        2 => [0, 1, 3],
        _ => [0, 1, 2],
    }
}

/// Sum of the foreign shim-call deltas of one portion (must be 0).
pub fn foreign_delta(deltas: &[u64; 4], slot: usize) -> u64 {
    foreign_slots(slot).iter().map(|&i| deltas[i]).sum()
}

/// One timed portion: ticks + ns + call deltas + foreign delta (0 required).
#[inline(never)]
pub fn run_portion(
    table: &'static ShimTable,
    kernel: Kernel,
    seed: u64,
) -> (u64, f64, [u64; 4], u64) {
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

// Read accessors for the live-slot tables (safe Cell reads for the
// test-side in-window payload/occupancy checks).

/// (`ptr`, `size`) of a live churn slot, or `None` when free.
pub fn churn_slot(arm: usize, idx: usize) -> Option<(*mut u8, usize)> {
    CHURN_SLOTS.with(|s| {
        CHURN_SIZES.with(|z| {
            let p = s[arm][idx].get();
            if p.is_null() {
                None
            } else {
                Some((p, z[arm][idx].get()))
            }
        })
    })
}

/// (`ptr`, `size`) of a live larson slot, or `None` when free.
pub fn larson_slot(arm: usize, idx: usize) -> Option<(*mut u8, usize)> {
    LARSON_SLOTS.with(|s| {
        LARSON_SIZES.with(|z| {
            let p = s[arm][idx].get();
            if p.is_null() {
                None
            } else {
                Some((p, z[arm][idx].get()))
            }
        })
    })
}

/// Explicit teardown: frees EVERY live block of every arm through its OWN
/// shim table with the block's OWN recorded layout. Judge calls before exit.
pub fn teardown_arms(tables: [&'static ShimTable; 4]) {
    for (arm, t) in tables.iter().enumerate() {
        CHURN_SLOTS.with(|s| {
            CHURN_SIZES.with(|z| {
                for i in 0..CHURN_LIVE {
                    let p = s[arm][i].get();
                    if !p.is_null() {
                        let sz = z[arm][i].get();
                        s[arm][i].set(core::ptr::null_mut());
                        z[arm][i].set(0);
                        unsafe {
                            // SAFETY: p from this arm's shim set, exactly
                            // (sz, 8), recorded in CHURN_SIZES.
                            (t.dealloc)(p, sz, 8)
                        };
                    }
                }
            })
        });
        LARSON_SLOTS.with(|s| {
            LARSON_SIZES.with(|z| {
                for i in 0..LARSON_LIVE {
                    let p = s[arm][i].get();
                    if !p.is_null() {
                        let sz = z[arm][i].get();
                        s[arm][i].set(core::ptr::null_mut());
                        z[arm][i].set(0);
                        unsafe {
                            // SAFETY: p from this arm's shim set, exactly
                            // (sz, LARSON_ALIGN) (LARSON_SIZES).
                            (t.dealloc)(p, sz, LARSON_ALIGN)
                        };
                    }
                }
            })
        });
        VEC_SLOT.with(|s| {
            VEC_CAPS.with(|caps| {
                let p = s[arm].get();
                if !p.is_null() {
                    let capb = caps[arm].get();
                    s[arm].set(core::ptr::null_mut());
                    caps[arm].set(0);
                    unsafe {
                        // SAFETY: p is this arm's live vec buffer, exact
                        // layout (capb*8, VEC_ALIGN) (VEC_CAPS).
                        (t.dealloc)(p, capb * 8, VEC_ALIGN)
                    };
                }
            })
        });
    }
}

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
