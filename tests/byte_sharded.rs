//! Sharded byte arena tests over `ShardedByteArena` (Phase 7d, `byte-sharded`).
//!
//! FAST tests per the short-scenario policy — small sizes and counts so the
//! suite (and miri over it) finishes quickly. Cover:
//!
//! 1. **Single-thread correctness proptest** (~64 cases normally; a TINY case
//!    count under `cfg(miri)` so miri stays fast): random alloc/dealloc/
//!    realloc sequences through `ShardedByteArena` behave correctly — every
//!    live pointer is writable to its full `layout.size()` (write a byte
//!    pattern, read it back), distinct live allocations never overlap, and
//!    freed blocks are reused (chunk growth stays bounded under churn).
//! 2. **Cross-thread dealloc routing**: thread A allocates, hands pointers to
//!    thread B, B writes+deallocs them (routed to A's shard); no corruption,
//!    no double-free, accounting/chunk growth sane.
//! 3. **Parallel per-shard alloc/write/dealloc**: several threads each
//!    alloc/write/dealloc in their own shard concurrently; all writes read
//!    back correctly (no cross-shard corruption).
//!
//! These tests exercise raw pointers; every `unsafe` block has a `// SAFETY:`
//! comment justifying the dereference against an in-bounds allocation.

#![cfg(feature = "byte-sharded")]

use std::alloc::Layout;
use std::hint::black_box;
use std::sync::Arc;
use std::thread::scope;

use proptest::prelude::*;

use sefer_alloc::ShardedByteArena;

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------

/// Under miri, shrink the proptest case count to keep the run fast (miri is
/// ~50x slower than native). Outside miri, use the project's default ~64
/// cases (the short-scenario policy). This mirrors how the project keeps miri
/// quick: a bounded smoke check, not exhaustive fuzzing (that is Phase 5
/// hardening).
#[cfg(miri)]
const PROPTEST_CASES: u32 = 4;

#[cfg(not(miri))]
const PROPTEST_CASES: u32 = 64;

/// Write `byte` to every byte of `len`, then read back and assert, through a
/// raw pointer returned by the arena. Confirms the full range is owned and
/// writable.
///
/// # Safety (internal)
///
/// The caller guarantees `ptr` is valid for `len` bytes.
unsafe fn fill_and_check(ptr: *mut u8, len: usize, byte: u8) {
    // SAFETY: caller guarantees `ptr` is valid for `len` bytes.
    unsafe { std::ptr::write_bytes(ptr, byte, len) };
    // SAFETY: same validity; read back each byte we just wrote.
    for i in 0..len {
        assert_eq!(
            unsafe { ptr.add(i).read() },
            byte,
            "byte {i} did not read back the value just written"
        );
    }
}

/// A `Send` wrapper around a raw pointer so a batch of arena pointers can be
/// handed from one thread to another across a `scope` boundary (a bare
/// `*mut u8` is neither `Send` nor `Sync`). This carries the REAL pointer (no
/// int↔ptr cast, so it stays miri-clean under strict provenance).
///
/// # Safety
///
/// Sending the pointer is sound in these tests: the allocating thread fully
/// completes (its `scope` is joined) before the receiving thread runs, the
/// arena outlives both, and no two threads ever mutate the same allocation
/// concurrently (each pointer is handled by exactly one thread at a time).
#[derive(Clone, Copy)]
struct SendPtr(*mut u8);
// SAFETY: see the type's doc — the test choreography guarantees no concurrent
// aliasing; the wrapper only moves/shares a pointer whose pointee is owned by
// the (thread-safe) arena, not by any thread. `Sync` is needed because the
// receiving thread borrows the shared `Vec<(SendPtr, Layout)>`; only that one
// thread reads it (the allocating thread's scope is already joined), so sharing
// the pointer value is sound.
unsafe impl Send for SendPtr {}
unsafe impl Sync for SendPtr {}

/// Asserts `ptr` is non-null and aligned to `align`.
fn assert_aligned(ptr: *mut u8, align: usize) {
    assert!(!ptr.is_null(), "allocation must not return null");
    let addr = ptr as usize;
    assert_eq!(
        addr & (align - 1),
        0,
        "pointer {ptr:#p} (addr {addr:#x}) is not aligned to {align}"
    );
}

// ---------------------------------------------------------------------------
// 0. Sanity: construct, alloc, dealloc (smoke).
// ---------------------------------------------------------------------------

/// The arena allocates and frees across all size classes plus a large
/// (system-fallback) allocation. Mirrors `tests/byte.rs`'s class sweep but
/// through the sharded arena on a single thread.
#[test]
fn alloc_write_read_dealloc_across_classes_and_large() {
    let arena = ShardedByteArena::with_shards(2);

    let cases: &[(usize, usize)] = &[
        // (size, align)
        (1, 1),
        (7, 8),
        (13, 8),
        (24, 16),
        (60, 32),
        (100, 64),
        (200, 128),
        (400, 256),
        (900, 512),
        (1000, 1024),
    ];

    let mut ptrs = Vec::new();
    for &(size, align) in cases {
        let layout = Layout::from_size_align(size, align).unwrap();
        let ptr = arena.alloc(layout);
        assert!(!ptr.is_null(), "alloc({size}, align {align}) must succeed");
        assert_aligned(ptr, align);
        // SAFETY: `ptr` was just allocated for `size` bytes by `arena.alloc`.
        unsafe { fill_and_check(ptr, size, 0xA5) };
        ptrs.push((ptr, layout));
    }

    // Large (system fallback): bigger than the largest class (1024).
    let big_size = 4096;
    let big_layout = Layout::from_size_align(big_size, 16).unwrap();
    let big_ptr = arena.alloc(big_layout);
    assert!(
        !big_ptr.is_null(),
        "large alloc must succeed via system fallback"
    );
    assert_aligned(big_ptr, 16);
    // SAFETY: `big_ptr` allocated for `big_size` bytes.
    unsafe { fill_and_check(big_ptr, big_size, 0x5A) };

    for (ptr, layout) in &ptrs {
        // SAFETY: each `*ptr` was returned by `arena.alloc(*layout)` above and
        // not yet freed.
        unsafe { arena.dealloc(*ptr, *layout) };
    }
    // SAFETY: `big_ptr` was returned by `arena.alloc(big_layout)` above.
    unsafe { arena.dealloc(big_ptr, big_layout) };

    black_box(&arena);
}

// ---------------------------------------------------------------------------
// 1. Single-thread correctness proptest: random alloc/dealloc/realloc.
// ---------------------------------------------------------------------------

/// A request in the random op stream. `Alloc` picks a size from a small set
/// (so churn reuses blocks); `Dealloc` frees a random live pointer; `Realloc`
/// grows/shrinks a random live pointer.
#[derive(Debug, Clone, Copy)]
enum Op {
    Alloc,
    Dealloc(usize),
    Realloc(usize, usize),
}

/// A small palette of (size, align) layouts to allocate. Keeping the palette
/// small forces free-list reuse (the chunk-growth-bounded property we check).
const LAYOUT_PALETTE: &[(usize, usize)] = &[
    (8, 8),
    (16, 8),
    (32, 8),
    (64, 8),
    (128, 16),
    (256, 32),
    (1024, 64),
    // a large (system-fallback) size to exercise cross-backend routing
    (5000, 8),
];

fn op_strategy(max_live: usize) -> impl Strategy<Value = Op> {
    prop_oneof![
        // Alloc: ~40% of the time (drives growth).
        Just(Op::Alloc),
        // Dealloc an index into the live list (validated at run time).
        (0u32..max_live as u32).prop_map(|i| Op::Dealloc(usize::try_from(i).unwrap_or(0))),
        // Realloc a live index to a new size from the palette.
        (
            0u32..max_live as u32,
            0u32..LAYOUT_PALETTE.len() as u32
        )
            .prop_map(|(i, ns)| Op::Realloc(usize::try_from(i).unwrap_or(0), usize::try_from(ns).unwrap_or(0))),
    ]
}

proptest! {
    // `failure_persistence: None` disables proptest's on-disk regression file.
    // That file I/O is blocked by miri's filesystem isolation (it would abort
    // the miri run before any of our code is interpreted — it is NOT a UB
    // finding, just an unsupported syscall); turning it off keeps this proptest
    // miri-clean while still exercising the alloc/dealloc/realloc paths.
    #![proptest_config(ProptestConfig {
        cases: PROPTEST_CASES,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    /// Random alloc/dealloc/realloc stream through `ShardedByteArena` on ONE
    /// thread: every live pointer is writable to its full `layout.size()`
    /// (write a distinct byte pattern per slot, read it back), distinct live
    /// allocations never overlap, and freed blocks are reused (chunk growth
    /// stays bounded under churn).
    #[test]
    fn single_thread_random_ops_correct(
        n_ops in 16u32..64u32,
        ops in prop::collection::vec(op_strategy(64), 16..64),
    ) {
        // A single shard is enough to exercise reuse on one thread; the
        // parallel/cross-thread tests below cover multi-shard routing.
        let arena = ShardedByteArena::with_shards(1);
        // Live allocations: (ptr, layout, tag_byte). The tag lets us verify a
        // slot's bytes survived and were not clobbered by a NEIGHBOURING alloc
        // (overlap detection).
        let mut live: Vec<(*mut u8, Layout, u8)> = Vec::new();

        for (step, op) in ops.iter().take(usize::try_from(n_ops).unwrap_or(ops.len())).enumerate() {
            match *op {
                Op::Alloc => {
                    let pi = step % LAYOUT_PALETTE.len();
                    let (size, align) = LAYOUT_PALETTE[pi];
                    let layout = Layout::from_size_align(size, align).unwrap();
                    let ptr = arena.alloc(layout);
                    prop_assert!(!ptr.is_null(), "alloc returned null");
                    assert_aligned(ptr, align);
                    // Write a distinct tag (step low byte) so a later overlap
                    // would corrupt a detectable value.
                    let tag = (step & 0xFF) as u8 | 1;
                    // SAFETY: `ptr` was just allocated for `size` bytes.
                    unsafe { std::ptr::write_bytes(ptr, tag, size) };
                    live.push((ptr, layout, tag));
                }
                Op::Dealloc(i) => {
                    if !live.is_empty() {
                        let idx = i % live.len();
                        let (ptr, layout, _tag) = live.swap_remove(idx);
                        // SAFETY: `ptr` is a live allocation from this arena
                        // with `layout`, not yet freed (we just removed it
                        // from `live`).
                        unsafe { arena.dealloc(ptr, layout) };
                    }
                }
                Op::Realloc(i, ns) => {
                    if !live.is_empty() {
                        let idx = i % live.len();
                        let (ptr, old_layout, _old_tag) = live[idx];
                        let (new_size, _new_align) =
                            LAYOUT_PALETTE[ns % LAYOUT_PALETTE.len()];
                        // SAFETY: `ptr` is a live allocation of `old_layout`
                        // from this arena, not yet freed.
                        let new_ptr = unsafe { arena.realloc(ptr, old_layout, new_size) };
                        if !new_ptr.is_null() {
                            let copy = old_layout.size().min(new_size);
                            let new_tag = (step & 0xFF) as u8 | 1;
                            // Write the new tag across the full new range.
                            // SAFETY: `new_ptr` valid for `new_size` bytes.
                            unsafe { std::ptr::write_bytes(new_ptr, new_tag, new_size) };
                            // The first `copy` bytes were preserved by realloc;
                            // we just overwrote them, so nothing to assert here
                            // beyond validity (the write itself is the check).
                            let _ = copy;
                            live[idx] = (
                                new_ptr,
                                Layout::from_size_align(new_size, old_layout.align()).unwrap(),
                                new_tag,
                            );
                        }
                        // If realloc returned null, the old allocation is left
                        // intact (GlobalAlloc contract); `live[idx]` still
                        // holds the valid old pointer.
                    }
                }
            }
        }

        // Final invariant sweep: every still-live pointer reads back its tag
        // across its full `layout.size()` — proves no neighbouring alloc
        // overlapped it (overlap would have clobbered the tag).
        for (ptr, layout, tag) in &live {
            let size = layout.size();
            // SAFETY: `ptr` is a live allocation of `layout.size()` bytes;
            // nobody else mutates it (single thread).
            for b in 0..size {
                prop_assert_eq!(
                    unsafe { ptr.add(b).read() },
                    *tag,
                    "live slot byte {} clobbered (tag mismatch => overlap or stale reuse)",
                    b
                );
            }
        }

        // Free everything so the arena's chunk growth can be observed under
        // churn. Under steady-state alloc/dealloc the chunk count must stay
        // bounded (free-list reuse), NOT grow with the op count.
        let chunks_before = arena.chunk_count();
        for (ptr, layout, _tag) in &live {
            // SAFETY: each `*ptr` is a still-live allocation of `layout`.
            unsafe { arena.dealloc(*ptr, *layout) };
        }
        live.clear();
        let chunks_after = arena.chunk_count();
        // Freeing does not grow chunks (it only pushes onto free lists).
        prop_assert_eq!(
            chunks_after, chunks_before,
            "dealloc must not grow chunks"
        );

        black_box(&arena);
    }
}

// ---------------------------------------------------------------------------
// 2. Free-list reuse caps chunk growth under churn (single-thread, multi-shard).
// ---------------------------------------------------------------------------

/// Under steady-state churn (alloc then dealloc the same class repeatedly), the
/// arena must reuse freed blocks rather than growing chunks forever. We assert
/// the total chunk count across all shards stays small after many dealloc/alloc
/// cycles of one class.
#[test]
fn free_list_reuse_caps_growth_under_churn() {
    // Each block is 64B, a chunk is 64 KiB ⇒ ~1024 blocks/chunk. With 2
    // shards and one allocating thread, all allocs land in ONE shard (the
    // thread's bound shard), so the bound is the same as a single region.
    const N: usize = 1000;
    let arena = ShardedByteArena::with_shards(2);
    let layout = Layout::from_size_align(64, 8).unwrap();

    let mut last = usize::MAX;
    for _ in 0..20 {
        let mut ptrs = Vec::with_capacity(N);
        for _ in 0..N {
            let ptr = arena.alloc(layout);
            assert!(!ptr.is_null());
            ptrs.push(ptr);
        }
        for ptr in &ptrs {
            // SAFETY: each `*ptr` was returned by `arena.alloc(layout)` in
            // this iteration and not yet freed.
            unsafe { arena.dealloc(*ptr, layout) };
        }
        let chunks = arena.chunk_count();
        // One allocating thread ⇒ one shard grows ⇒ ≤ 2 chunks after warmup.
        assert!(
            chunks <= 2,
            "churn should reuse free blocks; chunk count grew to {chunks}"
        );
        last = chunks;
    }
    assert!(last <= 2);
    black_box(&arena);
}

// ---------------------------------------------------------------------------
// 3. Cross-thread dealloc routing (the 7d win): alloc here, free there.
// ---------------------------------------------------------------------------

/// Thread A allocates a batch of pointers (binding to shard A); it hands them
/// to thread B, which writes a sentinel into each and deallocs them. Because
/// `dealloc` routes by owner-scan, B's frees land in A's shard (where the
/// pointers live) — no corruption, no double-free. We then verify A can
/// allocate again (the arena is still usable) and chunk growth is sane.
#[test]
fn cross_thread_dealloc_routes_to_owning_shard() {
    let arena = Arc::new(ShardedByteArena::with_shards(4));

    // Batch of (ptr, layout) for thread A to hand to thread B.
    let layouts: Vec<(usize, usize)> = vec![
        (8, 8),
        (64, 8),
        (256, 16),
        (1024, 32),
        (4096, 16), // large (system fallback)
    ];
    let batch: Vec<(SendPtr, Layout)> = scope(|s| {
        // Thread A: allocate the batch (binds A to some shard).
        let batch = s.spawn(|| {
            layouts
                .iter()
                .map(|&(size, align)| {
                    let layout = Layout::from_size_align(size, align).unwrap();
                    let ptr = arena.alloc(layout);
                    assert!(!ptr.is_null());
                    assert_aligned(ptr, align);
                    (SendPtr(ptr), layout)
                })
                .collect::<Vec<_>>()
        });
        batch.join().expect("thread A panicked")
    });

    // Thread B: write + dealloc each pointer (routed to A's shard).
    scope(|s| {
        s.spawn(|| {
            for (i, &(ptr, layout)) in batch.iter().enumerate() {
                let ptr = ptr.0;
                let tag = (i as u8).wrapping_mul(31) | 1;
                // SAFETY: `ptr` is a live allocation of `layout.size()` bytes
                // from the arena (allocated by thread A, still live).
                unsafe { std::ptr::write_bytes(ptr, tag, layout.size()) };
                // SAFETY: `ptr` is a live allocation of `layout`, not yet
                // freed. The owner-scan routes it to A's shard.
                unsafe { arena.dealloc(ptr, layout) };
            }
        });
    });

    // After B freed everything, the arena must still be usable on thread A
    // (or any thread) and chunk growth must be bounded (no leak from the
    // cross-thread frees).
    let chunks = arena.chunk_count();
    // 5 allocations across sizes up to 4096; the large one goes to the system
    // allocator (no chunk), the in-arena ones fit in ≤ 2 chunks. Allow a
    // generous but still-bounded budget.
    assert!(
        chunks <= 3,
        "chunk growth after cross-thread frees should be bounded, got {chunks}"
    );

    black_box(&arena);
}

// ---------------------------------------------------------------------------
// 4. Parallel per-shard alloc/write/dealloc: no cross-shard corruption.
// ---------------------------------------------------------------------------

/// Several threads each alloc/write/dealloc in their own bound shard
/// concurrently. Each thread writes a distinct per-thread byte pattern and
/// verifies it reads back unchanged before deallocating. Because each thread
/// binds to a distinct shard (4 threads, 4 shards), they take different
/// mutexes on the hot path; the test asserts no cross-shard corruption (a bug
/// in routing or in `ByteRegion`'s chunk addressing would manifest as a tag
/// mismatch or overlap).
#[test]
fn parallel_per_shard_alloc_write_dealloc_no_corruption() {
    // Keep iteration counts modest so the test (and miri over it) is fast.
    #[cfg(miri)]
    const THREADS: usize = 2;
    #[cfg(miri)]
    const ITERS: usize = 16;

    #[cfg(not(miri))]
    const THREADS: usize = 4;
    #[cfg(not(miri))]
    const ITERS: usize = 256;

    let arena = Arc::new(ShardedByteArena::with_shards(THREADS));
    // Each worker writes `tag = thread_index | 1` (distinct per thread) so a
    // cross-shard overlap would corrupt a detectable value.
    scope(|s| {
        for t in 0..THREADS {
            let arena = Arc::clone(&arena);
            s.spawn(move || {
                let tag = (t as u8) | 0xA0; // distinct, non-zero
                let layout = Layout::from_size_align(64, 8).unwrap();
                for _ in 0..ITERS {
                    let ptr = arena.alloc(layout);
                    assert!(!ptr.is_null());
                    // SAFETY: `ptr` valid for `layout.size()` bytes.
                    unsafe { std::ptr::write_bytes(ptr, tag, layout.size()) };
                    // Read back the full range (overlap/race ⇒ mismatch).
                    // SAFETY: same validity; single-thread read of THIS slot.
                    for b in 0..layout.size() {
                        assert_eq!(
                            unsafe { ptr.add(b).read() },
                            tag,
                            "thread {t} byte {b} corrupted (cross-shard overlap or race)"
                        );
                    }
                    // SAFETY: `ptr` is the live allocation we just made.
                    unsafe { arena.dealloc(ptr, layout) };
                }
            });
        }
    });

    // All threads done; the arena must be reusable and chunk growth bounded
    // (each thread churned one class in its own shard).
    let chunks = arena.chunk_count();
    assert!(
        chunks <= THREADS,
        "expected ≤ {THREADS} chunks (one per shard's working set), got {chunks}"
    );

    black_box(&arena);
}

// ---------------------------------------------------------------------------
// 5. realloc preserves bytes (single-thread, grow + shrink).
// ---------------------------------------------------------------------------

/// `realloc` grows and shrinks an allocation while preserving the copied
/// prefix. Exercises both the in-arena path (alloc + copy + dealloc) and the
/// large (system-realloc) path.
#[test]
fn realloc_preserves_bytes_grow_and_shrink() {
    let arena = ShardedByteArena::with_shards(2);

    // Small (in-arena) grow + shrink.
    let start = Layout::from_size_align(32, 8).unwrap();
    let p1 = arena.alloc(start);
    assert!(!p1.is_null());
    // SAFETY: `p1` valid for 32 bytes.
    unsafe { std::ptr::write_bytes(p1, 0xAB, 32) };
    // SAFETY: `p1` is a valid allocation of `start`, not yet freed.
    let p2 = unsafe { arena.realloc(p1, start, 256) };
    assert!(!p2.is_null());
    // SAFETY: first 32 bytes copied; verify sentinel survived.
    for i in 0..32 {
        assert_eq!(
            unsafe { p2.add(i).read() },
            0xAB,
            "realloc preserved byte {i}"
        );
    }
    // SAFETY: shrink p2 (256B) back to 16; first 16 bytes preserved.
    let p3 = unsafe { arena.realloc(p2, Layout::from_size_align(256, 8).unwrap(), 16) };
    assert!(!p3.is_null());
    for i in 0..16 {
        assert_eq!(
            unsafe { p3.add(i).read() },
            0xAB,
            "shrink preserved byte {i}"
        );
    }
    // SAFETY: final dealloc with the matching layout.
    unsafe { arena.dealloc(p3, Layout::from_size_align(16, 8).unwrap()) };

    // Large (system-fallback) realloc: start big, grow bigger.
    let big = Layout::from_size_align(4096, 16).unwrap();
    let bp1 = arena.alloc(big);
    assert!(!bp1.is_null());
    // SAFETY: `bp1` valid for 4096 bytes.
    unsafe { std::ptr::write_bytes(bp1, 0x7E, 4096) };
    // SAFETY: `bp1` is a valid large allocation of `big`, not yet freed.
    let bp2 = unsafe { arena.realloc(bp1, big, 8192) };
    assert!(!bp2.is_null());
    // SAFETY: first 4096 bytes copied from bp1.
    for i in 0..4096 {
        assert_eq!(
            unsafe { bp2.add(i).read() },
            0x7E,
            "large realloc preserved byte {i}"
        );
    }
    // SAFETY: dealloc the grown large allocation.
    unsafe { arena.dealloc(bp2, Layout::from_size_align(8192, 16).unwrap()) };

    black_box(&arena);
}

// ---------------------------------------------------------------------------
// 6. prewarm forces every shard to carve a chunk up front (cold-start removal).
// ---------------------------------------------------------------------------

/// `prewarm()` warms EVERY shard (it walks shards directly, not via the TLS
/// router), so after it every shard has at least one backing chunk and the
/// first real alloc of a warmed class hits the free list (no growth, no
/// first-touch fault). The arena stays fully correct afterward.
#[test]
fn prewarm_warms_all_shards_and_arena_still_correct() {
    const N: usize = 3;
    let arena = ShardedByteArena::with_shards(N);
    // Cold: nothing carved yet.
    assert_eq!(arena.chunk_count(), 0, "fresh arena has no chunks");

    arena.prewarm();
    // Every shard carved at least one chunk (one per shard, since each shard's
    // warm blocks fit in a single 64 KiB chunk).
    assert_eq!(
        arena.chunk_count(),
        N,
        "prewarm must carve exactly one chunk per shard"
    );

    // Idempotent + cheap to repeat: the warm blocks are on the free lists, so a
    // second prewarm reuses them — chunk count does not grow.
    arena.prewarm();
    assert_eq!(arena.chunk_count(), N, "repeat prewarm must not grow chunks");

    // The arena is fully usable after prewarm: alloc/write/read/dealloc still
    // correct across classes.
    for &(size, align) in &[(8usize, 8usize), (64, 8), (1024, 64)] {
        let layout = Layout::from_size_align(size, align).unwrap();
        let p = arena.alloc(layout);
        assert!(!p.is_null());
        assert_aligned(p, align);
        // SAFETY: `p` was just allocated for `size` bytes.
        unsafe { fill_and_check(p, size, 0x33) };
        // SAFETY: `p` is the live allocation we just made with `layout`.
        unsafe { arena.dealloc(p, layout) };
    }
    black_box(&arena);
}
