//! Task S1 (#165) — an AGGRESSIVE, FAST, multi-threaded boundary-stress
//! harness that tries to PROVOKE allocator bugs from strictly SAFE usage.
//!
//! ## The contract (iron)
//!
//! This harness stays STRICTLY inside the legal `GlobalAlloc` envelope: every
//! allocation is freed EXACTLY ONCE with the SAME layout it was allocated with.
//! There is NO double-free, NO foreign pointer, NO use-after-free, NO
//! mismatched-layout free — not even in the single-thread sections. It calls
//! `unsafe { alloc/dealloc }` only because `GlobalAlloc` is an unsafe API; it
//! obeys the contract exactly as legitimate std code (`Vec`/`Box`/`HashMap`)
//! would. Contract *violations* are the caller's UB and are out of scope here —
//! the M2 no-op guards for illegal double-free are covered by
//! `regression_magazine_oracles` / `regression_magazine_bump_guard` /
//! `regression_xthread_double_free_residual`; this test does NOT duplicate or
//! trigger those.
//!
//! ## The goal
//!
//! Break the allocator's OWN invariants under legal concurrent pressure on
//! boundaries — aliasing / two live allocations sharing memory / wrong
//! alignment / too-small usable size / accounting drift. The detectors are:
//!
//! 1. **CANARY** — immediately after each successful alloc the ENTIRE requested
//!    `size` bytes are filled with a per-allocation pattern derived from
//!    (thread id, ptr addr, per-op counter). Just before freeing, the pattern
//!    is read back and asserted intact. A mismatch means another live
//!    allocation aliased this memory or a neighbour corrupted it — a real bug.
//! 2. **ALIGNMENT** — `(ptr as usize) % layout.align() == 0` on every alloc.
//! 3. **NON-NULL-DISTINCT** (per thread) — two live allocations in the same
//!    thread must never share a pointer (a per-thread `HashSet` check). The
//!    canary already catches cross-thread aliasing (an aliased block's canary
//!    would be clobbered by the other owner). A global distinctness check is
//!    behind the heavy flag only (it needs a mutex, too slow for the default).
//! 4. Post-run sanity — the allocator still serves a fresh alloc.
//!
//! ## Fast by default
//!
//! Default total runtime is UNDER ~2 s (it runs in the normal suite). Per
//! thread the op budget is a small fixed number; thread count is
//! `min(available_parallelism, 8)`. Heavy mode is opt-in via
//! `SEFER_STRESS_HEAVY` (a multiplier on the op budget, default 1). The run is
//! bounded by op count, never by wall-clock sleeping, so the default length is
//! deterministic.
//!
//! ## Determinism / reproducibility
//!
//! A seeded xorshift64* PRNG per thread; each thread's seed is `base_seed`
//! XOR-mixed with its thread index. The base seed defaults to a fixed constant
//! (`0x5EFE_2A11_0C57_0165`) so the default run is fully deterministic; it can
//! be overridden with `SEFER_STRESS_SEED`. On ANY assertion failure the panic
//! message prints the base seed, the thread index, and the op index so the
//! failure is replayable.
//!
//! ## How it drives the allocator
//!
//! One `static GLOBAL: SeferAlloc` shared across threads (it is `Sync`). It is
//! NOT installed as `#[global_allocator]` — every thread calls `GLOBAL.alloc` /
//! `GLOBAL.dealloc` through the `GlobalAlloc` impl directly (exactly like
//! `benches/global_alloc.rs`). Each thread binds its own per-thread `HeapCore`
//! via TLS on first alloc, exercising the real production per-thread-heap +
//! magazine + cross-thread machinery.

#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "alloc-xthread"
))]

use std::alloc::{GlobalAlloc, Layout};
use std::collections::HashSet;
use std::sync::mpsc;
use std::thread::available_parallelism;

use sefer_alloc::SeferAlloc;

// One shared allocator, driven directly through its `GlobalAlloc` impl (NOT
// installed as the test binary's `#[global_allocator]`). `SeferAlloc` is `Sync`;
// every thread binds its own per-thread heap via TLS on first alloc.
static GLOBAL: SeferAlloc = SeferAlloc::new();

/// Default base seed. Fixed so the default run is fully deterministic — no
/// Date/random. Overridable via `SEFER_STRESS_SEED`.
const DEFAULT_SEED: u64 = 0x5EFE_2A11_0C57_0165;

/// Per-thread op budget in the DEFAULT (non-heavy) run. Multiplied by the
/// `SEFER_STRESS_HEAVY` factor. Chosen so the whole default suite finishes well
/// under ~2 s on a typical dev box.
const BASE_OPS_PER_THREAD: usize = 6_000;

/// Maximum number of live slots a worker thread keeps simultaneously. Bounded
/// so the live set stays small and the canary re-verify is cheap.
const LIVE_SLOTS: usize = 384;

/// Hard cap on thread count regardless of `available_parallelism`.
const MAX_THREADS: usize = 8;

/// Deterministic, dependency-free PRNG (xorshift64*), copied from
/// `benches/global_alloc.rs`. No external `rand` crate.
struct XorShift64(u64);

impl XorShift64 {
    fn new(seed: u64) -> Self {
        // Avoid the all-zero fixed point.
        Self(seed | 1)
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

    #[inline]
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % (n as u64)) as usize
    }
}

/// Boundary-concentrated size seams: the exact 256 size class, its immediate
/// neighbours (240 / 304), the 16 B floor, page-aligned steps, near-`SMALL_MAX`
/// values, and a few Large (> `SMALL_MAX`) sizes. `SMALL_MAX` is ~253 KiB.
const SIZE_SEAMS: &[usize] = &[
    16, 17, 24, 31, 32, 48, 64, 128, 129, 240, 255, 256, 257, 304, 512, 513, 1024, 1025, 2048,
    4096, 6144, 8192, 12288, 16384, 65536, 131072,  // near/at SMALL_MAX boundary and beyond:
    258048,  // ~ SMALL_MAX region
    262144,  // 256 KiB — Large (> SMALL_MAX)
    524288,  // 512 KiB — Large
    1048576, // 1 MiB — Large
];

/// Alignment seams: powers of two. `Layout::from_size_align` is fallible for
/// (size, align) pairs where `size` rounded up to `align` overflows `isize`, so
/// every draw is validated before use.
const ALIGN_SEAMS: &[usize] = &[8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096];

/// Draw a boundary-concentrated `Layout`. Returns `None` if the drawn
/// (size, align) pair is not a valid `Layout` (in which case the caller simply
/// skips this op — a legal outcome, not a bug).
fn draw_layout(rng: &mut XorShift64) -> Option<Layout> {
    let size = SIZE_SEAMS[rng.below(SIZE_SEAMS.len())];
    // Bias toward small aligns (the common case) but reach the large seams.
    let align = if rng.below(4) == 0 {
        ALIGN_SEAMS[rng.below(ALIGN_SEAMS.len())]
    } else {
        // 8 or 16 — the overwhelmingly common alignments.
        if rng.below(2) == 0 {
            8
        } else {
            16
        }
    };
    Layout::from_size_align(size, align).ok()
}

/// Per-allocation canary BASE word, derived once from (thread id, ptr addr, op
/// counter). Each word at byte-offset `off` is `base ^ (off as u64)`, so the
/// pattern is position-dependent (an aliasing neighbour's fill at a different
/// base clobbers ours in a way the read-back detects) yet cheap to compute
/// per-word — no per-byte hashing, which keeps the (unoptimised) test fast even
/// when it fills the ENTIRE requested size of a large block.
#[inline]
fn canary_base(tid: usize, addr: usize, op: usize) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64; // FNV-ish offset basis
    h ^= tid as u64;
    h = h.wrapping_mul(0x100_0000_01b3);
    h ^= addr as u64;
    h = h.wrapping_mul(0x100_0000_01b3);
    h ^= op as u64;
    h = h.wrapping_mul(0x100_0000_01b3);
    // Fold so tiny (tid, op) differences spread across all 8 bytes.
    h ^= h >> 29;
    h.wrapping_mul(0xff51_afd7_ed55_8ccd)
}

/// The canary word expected at byte offset `off` (a multiple of 8) for a block
/// whose base word is `base`.
#[inline]
fn canary_word(base: u64, off: usize) -> u64 {
    base ^ (off as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

/// The canary byte expected at byte offset `off` (for the ragged tail past the
/// last whole word). Consistent with `canary_word`'s little-endian byte layout.
#[inline]
fn canary_tail_byte(base: u64, off: usize) -> u8 {
    let word_off = off & !7usize;
    let w = canary_word(base, word_off);
    (w >> (8 * (off - word_off))) as u8
}

/// A live allocation: its pointer, the exact layout it was allocated with, and
/// the op counter that seeds its canary.
#[derive(Clone, Copy)]
struct Live {
    ptr: *mut u8,
    layout: Layout,
    op: usize,
}

// `Live` holds a raw pointer; it is only ever touched by its owning thread (or
// handed over a channel with a single owner at a time). Sending it is sound
// under this harness's exactly-one-owner discipline.
unsafe impl Send for Live {}

/// Fill exactly `layout.size()` bytes with the canary pattern. That is what the
/// caller owns (usable >= size), so this never writes out of bounds. Fills
/// word-wise (the min alignment is 8, so `ptr` is 8-aligned) with a ragged byte
/// tail — the ENTIRE requested size is covered.
///
/// # Safety
/// `ptr` must be a live allocation of at least `layout.size()` bytes.
unsafe fn write_canary(l: &Live, tid: usize) {
    let base = canary_base(tid, l.ptr as usize, l.op);
    let size = l.layout.size();
    let whole = size & !7usize;
    let mut off = 0;
    while off < whole {
        // SAFETY: `ptr` is 8-aligned (align >= 8) and off+8 <= size.
        l.ptr.add(off).cast::<u64>().write(canary_word(base, off));
        off += 8;
    }
    while off < size {
        // SAFETY: off < size <= usable bytes owned by the caller.
        l.ptr.add(off).write(canary_tail_byte(base, off));
        off += 1;
    }
}

/// Read the canary back and panic (with a replayable message) on any mismatch.
///
/// # Safety
/// `ptr` must be the same live allocation `write_canary` was called on.
unsafe fn verify_canary(l: &Live, tid: usize, base_seed: u64, op_idx: usize) {
    let addr = l.ptr as usize;
    let base = canary_base(tid, addr, l.op);
    let size = l.layout.size();
    let whole = size & !7usize;
    let mut off = 0;
    while off < whole {
        // SAFETY: `ptr` is 8-aligned (align >= 8) and off+8 <= size.
        let got = l.ptr.add(off).cast::<u64>().read();
        let want = canary_word(base, off);
        assert!(
            got == want,
            "CANARY CORRUPTION at word off {off}/{size} (ptr={addr:#x}, \
             align={align}): got {got:#018x}, want {want:#018x}. Another live \
             allocation aliased this block or a neighbour overwrote it. \
             REPRO: SEFER_STRESS_SEED={base_seed:#x} thread={tid} op={op_idx} \
             alloc_op={alloc_op}",
            align = l.layout.align(),
            alloc_op = l.op,
        );
        off += 8;
    }
    while off < size {
        // SAFETY: off < size <= usable bytes owned by the caller.
        let got = l.ptr.add(off).read();
        let want = canary_tail_byte(base, off);
        assert!(
            got == want,
            "CANARY CORRUPTION at tail byte {off}/{size} (ptr={addr:#x}): got \
             {got:#04x}, want {want:#04x}. REPRO: SEFER_STRESS_SEED={base_seed:#x} \
             thread={tid} op={op_idx} alloc_op={alloc_op}",
            alloc_op = l.op,
        );
        off += 1;
    }
}

/// Run the randomized boundary op-mix on one worker thread. `consumer_tx`, if
/// `Some`, is the channel to a consumer thread that will verify+free a fraction
/// of this thread's allocations (the cross-thread ownership-transfer lane).
fn worker(
    tid: usize,
    base_seed: u64,
    ops: usize,
    consumer_tx: Option<mpsc::Sender<(Live, usize)>>,
) {
    // Derive this thread's seed from the base seed + thread index.
    let mut rng = XorShift64::new(base_seed ^ (tid as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));

    // The bounded live set. `None` = empty slot.
    let mut live: Vec<Option<Live>> = vec![None; LIVE_SLOTS];
    // Per-thread pointer distinctness: no two live blocks share a pointer.
    let mut live_ptrs: HashSet<usize> = HashSet::with_capacity(LIVE_SLOTS);

    for op_idx in 0..ops {
        // Op mix. Weight so alloc and free stay roughly balanced and realloc /
        // transfer are occasional.
        let roll = rng.below(100);

        // Choose a slot to act on.
        let slot = rng.below(LIVE_SLOTS);

        if roll < 6 {
            // ── realloc across a class/page boundary (canary must survive) ──
            if let Some(old) = live[slot] {
                // SAFETY: `old` is a live block we own with `old.layout`.
                unsafe { verify_canary(&old, tid, base_seed, op_idx) };
                let new_size = SIZE_SEAMS[rng.below(SIZE_SEAMS.len())];
                let new_layout = match Layout::from_size_align(new_size, old.layout.align()) {
                    Ok(l) => l,
                    Err(_) => continue,
                };
                // SAFETY: `old.ptr` came from `GLOBAL` with `old.layout`.
                let np = unsafe { GLOBAL.realloc(old.ptr, old.layout, new_size) };
                if np.is_null() {
                    // realloc failed: the OLD block is still live and valid.
                    // Its canary is intact (realloc leaves it untouched on
                    // failure); re-verify then move on.
                    // SAFETY: still the same live block.
                    unsafe { verify_canary(&old, tid, base_seed, op_idx) };
                    continue;
                }
                // The first min(old, new) bytes must survive the realloc — the
                // canary was keyed on the OLD (addr, op). Verify word-wise then
                // a ragged byte tail.
                let survived = old.layout.size().min(new_size);
                let old_addr = old.ptr as usize;
                let old_base = canary_base(tid, old_addr, old.op);
                let whole = survived & !7usize;
                let mut off = 0;
                while off < whole {
                    // SAFETY: np is 8-aligned (align >= 8) and off+8 <= survived.
                    let got = unsafe { np.add(off).cast::<u64>().read() };
                    let want = canary_word(old_base, off);
                    assert!(
                        got == want,
                        "REALLOC LOST DATA at word off {off}/{survived} \
                         ({old_bytes}->{new_size}): got {got:#018x}, want \
                         {want:#018x}. REPRO: SEFER_STRESS_SEED={base_seed:#x} \
                         thread={tid} op={op_idx}",
                        old_bytes = old.layout.size(),
                    );
                    off += 8;
                }
                while off < survived {
                    // SAFETY: off < survived <= new usable size.
                    let got = unsafe { np.add(off).read() };
                    let want = canary_tail_byte(old_base, off);
                    assert!(
                        got == want,
                        "REALLOC LOST DATA at tail byte {off}/{survived} \
                         ({old_bytes}->{new_size}): got {got:#04x}, want \
                         {want:#04x}. REPRO: SEFER_STRESS_SEED={base_seed:#x} \
                         thread={tid} op={op_idx}",
                        old_bytes = old.layout.size(),
                    );
                    off += 1;
                }
                // Now own `np` under `new_layout`; re-canary the whole block so
                // subsequent verifies use the new addr/op basis.
                live_ptrs.remove(&old_addr);
                let np_addr = np as usize;
                assert!(
                    np_addr.is_multiple_of(new_layout.align()),
                    "REALLOC MISALIGNED: ptr={np_addr:#x} align={}. REPRO: \
                     SEFER_STRESS_SEED={base_seed:#x} thread={tid} op={op_idx}",
                    new_layout.align(),
                );
                // Distinctness: the realloc'd pointer must not collide with a
                // DIFFERENT live block (it may equal old_addr, already removed).
                assert!(
                    live_ptrs.insert(np_addr),
                    "REALLOC ALIASED a live block: ptr={np_addr:#x}. REPRO: \
                     SEFER_STRESS_SEED={base_seed:#x} thread={tid} op={op_idx}",
                );
                let nl = Live {
                    ptr: np,
                    layout: new_layout,
                    op: op_idx,
                };
                // SAFETY: `np` is live for `new_layout.size()` bytes.
                unsafe { write_canary(&nl, tid) };
                live[slot] = Some(nl);
            }
            continue;
        }

        if let Some(l) = live[slot] {
            // ── the slot is occupied: free it (or hand it to the consumer) ──
            // SAFETY: `l` is a live block we own; verify its canary first.
            unsafe { verify_canary(&l, tid, base_seed, op_idx) };
            live[slot] = None;
            live_ptrs.remove(&(l.ptr as usize));

            if roll < 12 {
                if let Some(tx) = consumer_tx.as_ref() {
                    // Cross-thread transfer lane: the consumer becomes the sole
                    // owner and will verify + free EXACTLY ONCE. If the send
                    // fails (consumer gone) we own it and free it ourselves.
                    if let Err(e) = tx.send((l, tid)) {
                        let (l, _) = e.0;
                        // SAFETY: exactly-once free with the same layout.
                        unsafe { GLOBAL.dealloc(l.ptr, l.layout) };
                    }
                    continue;
                }
            }
            // Own-thread free — exactly once, same layout.
            // SAFETY: `l.ptr` came from `GLOBAL` with `l.layout`, freed once.
            unsafe { GLOBAL.dealloc(l.ptr, l.layout) };
        } else {
            // ── the slot is empty: allocate into it ──
            let layout = match draw_layout(&mut rng) {
                Some(l) => l,
                None => continue,
            };
            // SAFETY: `layout` has non-zero size and a valid alignment.
            let ptr = unsafe { GLOBAL.alloc(layout) };
            if ptr.is_null() {
                // A legitimate null (e.g. a huge Large request under memory
                // pressure). Skip — do NOT deref.
                continue;
            }
            let addr = ptr as usize;
            assert!(
                addr.is_multiple_of(layout.align()),
                "ALLOC MISALIGNED: ptr={addr:#x} align={} size={}. REPRO: \
                 SEFER_STRESS_SEED={base_seed:#x} thread={tid} op={op_idx}",
                layout.align(),
                layout.size(),
            );
            assert!(
                live_ptrs.insert(addr),
                "ALLOC ALIASED a live block in the same thread: ptr={addr:#x} \
                 size={}. Two live allocations share memory — a real bug. \
                 REPRO: SEFER_STRESS_SEED={base_seed:#x} thread={tid} \
                 op={op_idx}",
                layout.size(),
            );
            let l = Live {
                ptr,
                layout,
                op: op_idx,
            };
            // SAFETY: `ptr` is live for `layout.size()` bytes.
            unsafe { write_canary(&l, tid) };
            live[slot] = Some(l);
        }
    }

    // Drain the live set — free everything exactly once with its own layout.
    for slot in live.iter_mut() {
        if let Some(l) = slot.take() {
            // SAFETY: `l` is a live block we own; verify then free once.
            unsafe {
                verify_canary(&l, tid, base_seed, usize::MAX);
                GLOBAL.dealloc(l.ptr, l.layout);
            }
        }
    }
}

/// The consumer thread of the cross-thread ownership-transfer lane: it receives
/// `(Live, producer_tid)` blocks, verifies the producer's canary (the block was
/// filled by the producer), and frees each EXACTLY ONCE with its original
/// layout. This exercises the `RemoteFreeRing` under contention.
fn consumer(rx: mpsc::Receiver<(Live, usize)>, base_seed: u64) {
    for (l, producer_tid) in rx {
        // SAFETY: we are now the SOLE owner of `l`; verify the producer's
        // canary (keyed on the producer's tid) then free exactly once.
        unsafe {
            verify_canary(&l, producer_tid, base_seed, usize::MAX - 1);
            GLOBAL.dealloc(l.ptr, l.layout);
        }
    }
}

/// Read the base seed: `SEFER_STRESS_SEED` (decimal or `0x`-hex) or the fixed
/// default so the standard run is deterministic.
fn base_seed() -> u64 {
    match std::env::var("SEFER_STRESS_SEED") {
        Ok(s) => {
            let t = s.trim();
            let parsed = if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                u64::from_str_radix(hex, 16)
            } else {
                t.parse::<u64>()
            };
            parsed.unwrap_or(DEFAULT_SEED)
        }
        Err(_) => DEFAULT_SEED,
    }
}

/// Read the heavy-mode op-budget multiplier: `SEFER_STRESS_HEAVY` (default 1).
fn heavy_multiplier() -> usize {
    std::env::var("SEFER_STRESS_HEAVY")
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .filter(|&m| m >= 1)
        .unwrap_or(1)
}

fn thread_count() -> usize {
    available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(2, MAX_THREADS)
}

/// THE gate: many threads run the boundary op-mix concurrently against one
/// shared `SeferAlloc`, with a cross-thread ownership-transfer lane feeding a
/// consumer thread. Canary + alignment + per-thread distinctness detectors fire
/// on any allocator invariant break. Bounded by op count (fast by default;
/// heavy opt-in via `SEFER_STRESS_HEAVY`).
#[test]
fn concurrent_boundary_stress() {
    let base_seed = base_seed();
    let mult = heavy_multiplier();
    let ops = BASE_OPS_PER_THREAD.saturating_mul(mult);
    let n_threads = thread_count();

    if mult > 1 {
        eprintln!(
            "stress_concurrent_boundaries: HEAVY x{mult} — {n_threads} threads \
             x {ops} ops, base_seed={base_seed:#x}"
        );
    }

    // The cross-thread transfer lane: producers send blocks to one consumer.
    let (tx, rx) = mpsc::channel::<(Live, usize)>();
    let consumer_handle = {
        std::thread::Builder::new()
            .name("stress-consumer".into())
            .spawn(move || consumer(rx, base_seed))
            .expect("spawn consumer")
    };

    let workers: Vec<_> = (0..n_threads)
        .map(|tid| {
            // Every worker feeds the SAME consumer (contention on the ring).
            let tx = tx.clone();
            std::thread::Builder::new()
                .name(format!("stress-worker-{tid}"))
                .spawn(move || worker(tid, base_seed, ops, Some(tx)))
                .expect("spawn worker")
        })
        .collect();

    // Drop our own sender so the consumer's `rx` iterator terminates once every
    // worker has finished (each worker holds a clone until it returns).
    drop(tx);

    for (tid, h) in workers.into_iter().enumerate() {
        h.join().unwrap_or_else(|_| {
            panic!(
                "worker {tid} panicked — a real allocator bug was provoked. \
                 REPRO: SEFER_STRESS_SEED={base_seed:#x} (re-run with \
                 --nocapture to see the failing thread/op)."
            )
        });
    }
    consumer_handle
        .join()
        .expect("consumer thread panicked — cross-thread free provoked a bug");

    // Post-run sanity: the allocator still serves a fresh alloc.
    let layout = Layout::from_size_align(64, 8).unwrap();
    // SAFETY: valid layout; freed exactly once below.
    let p = unsafe { GLOBAL.alloc(layout) };
    assert!(!p.is_null(), "allocator dead after the stress run");
    // SAFETY: `p` came from `GLOBAL` with `layout`, freed exactly once.
    unsafe { GLOBAL.dealloc(p, layout) };
}
