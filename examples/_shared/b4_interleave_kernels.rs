// B4 J1′ interleave judge — KERNEL BODIES + live-slot tables.
// PURE MOVE out of `b4_interleave_core.rs` (≤1000-line cap); included
// verbatim from that file (`include!`), same single scope.

// Live-slot storage: `thread_local!` + `Cell<*mut u8>`, const-init arrays
// (no TLS allocation inside the window); PER ARM — a shared ring would free
// a live pointer CROSS-ALLOCATOR (violates the caller's GlobalAlloc contract).
/// Live-arm count of the slot tables = the number of shim sets
/// ([`SHIM_SLOTS`]).
const ARMS: usize = SHIM_SLOTS;

thread_local! {
    static CHURN_SLOTS: [[Cell<*mut u8>; CHURN_LIVE]; ARMS] =
        const { [const { [const { Cell::new(core::ptr::null_mut()) }; CHURN_LIVE] }; ARMS] };
    /// Exact size of the allocation in each churn slot; every churn dealloc
    /// uses THIS size (no foreign Layout).
    static CHURN_SIZES: [[Cell<usize>; CHURN_LIVE]; ARMS] =
        const { [const { [const { Cell::new(0) }; CHURN_LIVE] }; ARMS] };
    static LARSON_SLOTS: [[Cell<*mut u8>; LARSON_LIVE]; ARMS] =
        const { [const { [const { Cell::new(core::ptr::null_mut()) }; LARSON_LIVE] }; ARMS] };
    static BULK_SLOTS: [[Cell<*mut u8>; BULK64K_N]; ARMS] =
        const { [const { [const { Cell::new(core::ptr::null_mut()) }; BULK64K_N] }; ARMS] };
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
fn kernel_bulk(t: &ShimTable, passes: u64, seed: u64, n: usize, size: usize) -> u64 {
    let mut rng = XorShift64::new(seed);
    let mut acc: u64 = 0;
    let mut pass: u64 = 0;
    while pass < passes {
        let arm = t.call_slot;
        BULK_SLOTS.with(|slots| {
            let mut i = 0;
            while i < n {
                let p = unsafe {
                    // SAFETY: __rust_alloc face, `size` >= 32, align 8.
                    (t.alloc)(size, 8)
                };
                if p.is_null() {
                    panic!("{}: bulk alloc({size}) returned null", t.name);
                }
                slots[arm][i].set(p);
                unsafe {
                    // SAFETY: fresh `size`-byte allocation, size >= 8.
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
                    // is exactly (size, 8) of the same shim set.
                    (t.dealloc)(p, size, 8)
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
    for (i, b) in before.iter_mut().enumerate() {
        *b = shim_calls_total(i);
    }
    let t = black_box(table);
    let ops = kernel.ops();
    let acc = match kernel {
        Kernel::Pair16 => kernel_pair(t, ops, seed, 16),
        Kernel::Pair64 => kernel_pair(t, ops, seed, 64),
        Kernel::PairRt => kernel_pair(t, ops, seed, 0),
        Kernel::Bulk64 => kernel_bulk(t, ops, seed, BULK64_N, 64),
        Kernel::Bulk16k => kernel_bulk(t, ops, seed, BULK16K_N, 64),
        Kernel::Bulk64k => kernel_bulk(t, ops, seed, BULK64K_N, 64),
        Kernel::Bulk16k32 => kernel_bulk(t, ops, seed, BULK16K32_N, 32),
        Kernel::Churn16 => kernel_churn(t, ops, seed, 16),
        Kernel::Churn64 => kernel_churn(t, ops, seed, 64),
        Kernel::Churn256 => kernel_churn(t, ops, seed, 256),
        Kernel::Churn1024 => kernel_churn(t, ops, seed, 1024),
        Kernel::VecGrow => kernel_vec(t, ops, seed),
        Kernel::Larson => kernel_larson(t, ops, seed),
    };
    black_box(acc);
    let mut deltas = [0u64; 4];
    for (i, d) in deltas.iter_mut().enumerate() {
        *d = shim_calls_total(i) - before[i];
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
