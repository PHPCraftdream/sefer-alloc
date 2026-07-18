//! Empirical check of invariants **M1 (validity)** and **M3 (no-overlap)**
//! from `docs/INVARIANTS.md`, using ONLY the 100%-safe public surface
//! (`Box`/`Vec`/`Arc` + ordinary `Drop`) of an installed `SeferAlloc`.
//!
//! ## Why this test exists
//!
//! Task #202's investigation traced a probabilistic SIGSEGV to a *deliberate,
//! contract-level* double-free through the crate's own `unsafe fn dealloc` —
//! i.e. caller UB reachable only through `unsafe`, exactly like std's own
//! `GlobalAlloc::dealloc`. The narrower, *soundness-relevant* question is
//! different: does `SeferAlloc::alloc` ever hand out a pointer whose
//! `[ptr, ptr+size)` range intersects the range of any other currently-live
//! allocation? If it did, two entirely-safe, correctly-single-dropped
//! `Box`/`Vec`/`Arc` values could end up pointing at overlapping memory;
//! dropping each (each individually correct) would still corrupt / double-free
//! the shared bytes — reachable from 100% safe code with no `unsafe` anywhere
//! on the caller side.
//!
//! Two prior audits of this property were *static reads* of the source — they
//! found no such path, but neither RAN anything. This file is the first
//! **empirical** complement: it installs `SeferAlloc` as the binary's
//! `#[global_allocator]`, then runs a multithreaded churn in which every
//! allocation is:
//!
//!   - tracked by `(start_address, end_address, unique_sentinel)` in a shared
//!     mutex-protected live table,
//!   - checked **at registration** for range overlap against the immediate
//!     predecessor and successor in the address-sorted live set (M3 — provably
//!     sufficient, see `LiveTable`),
//!   - sentinel-stamped (a unique 64-bit value written into BOTH the first 8
//!     bytes and the last 8 bytes via safe slice indexing), and re-verified
//!     **mid-life** (periodic full-table walk) AND **at drop** — a mismatch
//!     means a foreign write clobbered memory this allocation still owns, the
//!     strongest possible aliasing signal (M1 — the bytes we own are ours for
//!     the duration of the lease).
//!
//! ## What this does NOT exercise
//!
//! Through purely-safe stable Rust, `Box`/`Vec`/`Arc` reach only the
//! `GlobalAlloc::alloc` + `dealloc` pair; `alloc_zeroed` is not reachable from
//! safe constructors on stable Rust (`Vec::with_capacity_zeroed` /
//! `Box::new_zeroed_slice` are nightly-only). `alloc_zeroed` routes through the
//! SAME allocation core as `alloc` (it is `alloc` + a memset), so an aliasing
//! bug there would also manifest via `alloc`. This test therefore empirically
//! covers the aliasing surface reachable from safe code; the residual
//! `alloc_zeroed`-vs-`alloc` difference (zeroing) is orthogonal to M1/M3.
//!
//! ## Zero `unsafe` in this file
//!
//! This file contains NO `unsafe` blocks and calls NO `unsafe fn`. Address
//! observation is via `Vec::as_ptr() as usize` — a safe cast-to-int, NOT a
//! pointer dereference. Every byte read/write goes through safe slice indexing.
//! Verify with:
//! `grep -nE 'unsafe' tests/stress_safe_surface_no_aliasing.rs`
//! (expected: zero matches).

#![cfg(feature = "alloc-global")]

use std::collections::BTreeMap;
use std::ops::Bound;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use sefer_alloc::SeferAlloc;

// Install `SeferAlloc` as THIS test binary's process-wide global allocator.
// Every allocation in this binary — `Vec`, `Arc`, the `BTreeMap` backing the
// shared live table, even libtest's own harness allocations — routes through
// `SeferAlloc` and the per-thread registry-backed heap.
//
// `#[global_allocator]` is per-binary; integration tests each compile to a
// SEPARATE binary, so this static does NOT collide with the declarations in
// `tests/global_alloc_mt.rs`, `tests/global_alloc_installed.rs`, etc. No
// `SerialGuard` is needed: this file has a single test function, so there is
// no intra-binary parallelism to coordinate, and cross-binary coordination is
// unnecessary (each binary has its own process and its own registry).
#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

/// Monotonic counter supplying a unique 64-bit sentinel per allocation.
/// Starting at 1 so sentinel 0 is never handed out (a zero sentinel would be
/// indistinguishable from "uninitialised / zeroed memory" on some paths).
static SENTINEL_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Total allocations exercised across all workers (surfaced in the final
/// report and used to confirm the run was non-trivial).
static TOTAL_ALLOCS: AtomicU64 = AtomicU64::new(0);

/// One entry in the shared live-allocation table.
struct LiveEntry {
    /// Exclusive end address = start + size. Tracked separately so the overlap
    /// check on insertion is a single integer compare (`end > new_start`).
    end: usize,
    /// Requested size in bytes. Used to locate the far-end sentinel probe
    /// (`data[size-8..size]`) during mid-life verification.
    size: usize,
    /// The unique sentinel written into the allocation's first AND last 8 bytes
    /// at construction. A mismatch on re-read is the M1 aliasing/clobber signal.
    sentinel: u64,
    /// Shared handle to the live allocation's bytes, so the mid-life verifier
    /// can re-read the sentinel through safe indexing without any `unsafe`.
    /// The buffer is freed exactly once: the table's Arc clone is dropped under
    /// the table Mutex during `Tracked::drop`, then the `Tracked`'s own Arc
    /// drops at the end of that same `drop` call — both on the owning worker
    /// thread, so no cross-thread free of the tracked allocation occurs.
    data: Arc<Vec<u8>>,
}

/// Shared live-allocation table, keyed by start address. The BTreeMap ordering
/// makes the M3 overlap check O(log n): a new range `[a, a+s)` overlaps an
/// existing live range iff it overlaps its immediate BTreeMap predecessor
/// (largest live start `<= a`) or its immediate successor (smallest live start
/// `> a`). Proof sketch: any earlier range `Q` (with `Q.start < pred.start`)
/// that could overlap the new one would have `Q.end > a`; but `Q` and `pred`
/// are both live (hence disjoint by induction), so `Q.end <= pred.start <= a`,
/// contradiction. The successor side is symmetric. Hence checking only the two
/// immediate neighbours is necessary AND sufficient.
type LiveTable = BTreeMap<usize, LiveEntry>;

/// A tracked live allocation. Owns its buffer through an `Arc` (cloned into
/// the shared live table at construction; removed on drop). `new` and `Drop`
/// are 100% safe code.
struct Tracked {
    addr: usize,
    size: usize,
    sentinel: u64,
    data: Arc<Vec<u8>>,
    table: Arc<Mutex<LiveTable>>,
}

impl Tracked {
    /// Allocate a fresh `size`-byte buffer through the global allocator, stamp
    /// a unique sentinel into the first AND last 8 bytes, and register it in
    /// the shared live table — panicking loudly if the new allocation's
    /// `[addr, addr+size)` range overlaps any currently-live entry (M3
/// violation) or if the address is already registered under an exact-base
    /// alias.
    fn new(size: usize, table: Arc<Mutex<LiveTable>>) -> Tracked {
        assert!(size >= 16, "size classes must hold sentinels at both ends");
        // `vec![0u8; size]` calls `alloc` (through the installed global
        // allocator) for exactly `size` bytes; `len == capacity == size`, so
        // `[ptr, ptr+size)` is the precise live footprint of this allocation.
        let mut data: Vec<u8> = vec![0u8; size];
        let addr = data.as_ptr() as usize; // safe cast-to-int, NOT a deref
        let new_end = addr.checked_add(size).expect("addr + size overflow");
        let sentinel = SENTINEL_COUNTER.fetch_add(1, Ordering::Relaxed);
        let sent_bytes = sentinel.to_le_bytes();
        // Safe sentinel writes — slice indexing, no raw dereference. Probes
        // BOTH ends of the allocation so an M1 "valid for fewer than size
        // bytes" short-slab bug is caught too (the far-end write would land
        // outside the actually-valid region and clobber a neighbour, which
        // that neighbour's sentinel check then surfaces).
        data[0..8].copy_from_slice(&sent_bytes);
        data[size - 8..size].copy_from_slice(&sent_bytes);
        let data = Arc::new(data);

        // Register + overlap check under the table Mutex.
        {
            let mut live = table.lock().expect("live table poisoned on insert");

            // Predecessor: largest live start <= addr. Overlaps iff its end >
            // addr. (This subsumes the exact-base-alias case: if pred.start ==
            // addr, then pred.end == addr + pred.size > addr since pred.size >
            // 0, so the check fires.)
            if let Some((&p_start, p)) = live.range(..=addr).next_back() {
                if p.end > addr {
                    panic!(
                        "M3 VIOLATION — new alloc [{:#x}, {:#x}) (sentinel {:#x}) \
                         overlaps LIVE predecessor [{:#x}, {:#x}) (sentinel {:#x}). \
                         SeferAlloc::alloc returned an address range intersecting a \
                         still-live allocation — reachable from 100% safe code.",
                        addr, new_end, sentinel, p_start, p.end, p.sentinel,
                    );
                }
            }
            // Successor: smallest live start > addr. Overlaps iff start < new_end.
            if let Some((&s_start, s)) =
                live.range((Bound::Excluded(addr), Bound::Unbounded)).next()
            {
                if s_start < new_end {
                    panic!(
                        "M3 VIOLATION — new alloc [{:#x}, {:#x}) (sentinel {:#x}) \
                         overlaps LIVE successor [{:#x}, {:#x}) (sentinel {:#x}). \
                         SeferAlloc::alloc returned an address range intersecting a \
                         still-live allocation — reachable from 100% safe code.",
                        addr, new_end, sentinel, s_start, s.end, s.sentinel,
                    );
                }
            }

            live.insert(addr, LiveEntry { end: new_end, size, sentinel, data: Arc::clone(&data) });
        }

        TOTAL_ALLOCS.fetch_add(1, Ordering::Relaxed);
        Tracked { addr, size, sentinel, data, table }
    }
}

impl Drop for Tracked {
    fn drop(&mut self) {
        // (1) Sentinel re-verify: are THIS allocation's first AND last 8 bytes
        // still the sentinel we wrote at construction? If some other (aliasing)
        // live allocation clobbered them, this fires — the strongest M1 signal.
        let near = u64::from_le_bytes(self.data[0..8].try_into().unwrap());
        assert_eq!(
            near, self.sentinel,
            "M1 VIOLATION — near-end sentinel clobbered in still-live alloc at \
             [{:#x}, {:#x}): wrote {:#x} at construction, read back {:#x}. Another \
             allocation overwrote memory we still own (aliasing).",
            self.addr, self.addr + self.size, self.sentinel, near,
        );
        let far = u64::from_le_bytes(
            self.data[self.size - 8..self.size].try_into().unwrap(),
        );
        assert_eq!(
            far, self.sentinel,
            "M1 VIOLATION — far-end sentinel clobbered in still-live alloc at \
             [{:#x}, {:#x}): wrote {:#x} at construction, read back {:#x}. The \
             allocation was not valid for the full requested size, or another \
             allocation overwrote its tail (aliasing).",
            self.addr, self.addr + self.size, self.sentinel, far,
        );

        // (2) Deregister from the shared live table. The table's Arc clone is
        // dropped here (under the Mutex); the Tracked's own Arc drops at the
        // end of this function, freeing the buffer on this same worker thread
        // — no cross-thread free of the tracked allocation itself.
        let mut live = self.table.lock().expect("live table poisoned on drop");
        let removed = live.remove(&self.addr);
        assert!(
            removed.is_some(),
            "M1 VIOLATION — drop of [{:#x}, {:#x}) found no live entry; the \
             address was never registered (or was deregistered twice).",
            self.addr,
            self.addr + self.size,
        );
    }
}

/// Walk every currently-live entry and re-verify BOTH its sentinels. Catches
/// an M1 aliasing/clobber on a still-live allocation BEFORE its `Drop` runs
/// (the `Drop` check only fires at end-of-life; this fires mid-life). Holds
/// the table Mutex for the whole walk, serialising against concurrent
/// `Tracked::drop` (so no entry can be removed — and its buffer freed — while
/// we are reading it). Returns the number of entries checked.
fn verify_all_sentinels(table: &Mutex<LiveTable>) -> usize {
    let live = table.lock().expect("live table poisoned on verify");
    let mut checked = 0usize;
    for (&addr, entry) in live.iter() {
        let near = u64::from_le_bytes(entry.data[0..8].try_into().unwrap());
        assert_eq!(
            near, entry.sentinel,
            "M1 VIOLATION — near-end sentinel clobbered in still-live alloc at \
             {:#x} (mid-life verify): wrote {:#x}, read back {:#x}. Another \
             allocation overwrote memory we still own (aliasing).",
            addr, entry.sentinel, near,
        );
        let far = u64::from_le_bytes(
            entry.data[entry.size - 8..entry.size].try_into().unwrap(),
        );
        assert_eq!(
            far, entry.sentinel,
            "M1 VIOLATION — far-end sentinel clobbered in still-live alloc at \
             {:#x} (mid-life verify): wrote {:#x}, read back {:#x}. The allocation \
             was not valid for the full requested size, or another allocation \
             overwrote its tail (aliasing).",
            addr, entry.sentinel, far,
        );
        checked += 1;
    }
    checked
}

// Size classes chosen to span small-bin classes (16/64/256 — magazine/TFS
// path), medium classes (1024/4096), and the Large-allocator path (65536 —
// dedicated segment).
const SIZE_CLASSES: [usize; 6] = [16, 64, 256, 1024, 4096, 65536];

const N_WORKERS: usize = 6;
const ITERS_PER_WORKER: u32 = 1500;
/// Pseudo-varying cadence for the mid-life full-table sentinel verification.
/// A small prime so different workers' verify beats drift relative to each
/// other and to the loop's drop cadence.
const VERIFY_EVERY: u32 = 37;
/// Soft cap on concurrently-held allocations per worker. Bounds the global
/// live set (≤ N_WORKERS × this) while still exercising many
/// simultaneously-live allocations across size classes and threads.
const TARGET_LIVE_PER_WORKER: usize = 32;

/// Worker loop: allocate `Tracked` buffers across the size classes, hold a
/// bounded live set, drop older ones to exercise free-list reuse under churn,
/// and periodically invoke the mid-life full-table sentinel verification.
/// Returns the number of full-table verify passes performed.
fn worker(tid: usize, table: Arc<Mutex<LiveTable>>) -> u64 {
    let mut live: Vec<Tracked> = Vec::with_capacity(TARGET_LIVE_PER_WORKER + 4);
    let mut verify_passes: u64 = 0;
    for i in 0..ITERS_PER_WORKER {
        // Rotate size class by (tid + i) so different workers hit different
        // classes on the same iteration — cross-thread size-class contention.
        let size = SIZE_CLASSES[(tid.wrapping_add(i as usize)) % SIZE_CLASSES.len()];
        live.push(Tracked::new(size, Arc::clone(&table)));

        // Bound the per-worker live set: drop one when over target. The drop
        // index varies with `i` so we don't always free the same position —
        // exercises LIFO, FIFO, and mid-vector free paths through the allocator.
        if live.len() > TARGET_LIVE_PER_WORKER {
            let idx = (i as usize) % live.len();
            live.swap_remove(idx); // drops the removed Tracked (sentinel re-checked in Drop)
        }

        // Periodic mid-life full-table verify.
        if i % VERIFY_EVERY == 0 {
            verify_passes += 1;
            let n = verify_all_sentinels(&table);
            // Soft upper-bound sanity check: catches a runaway (e.g. if Drop
            // ever stopped deregistering, the live set would grow unboundedly).
            debug_assert!(
                n <= N_WORKERS * (TARGET_LIVE_PER_WORKER + 8),
                "live table unexpectedly large: {n} entries"
            );
        }
    }
    // Drop everything still held. Each Tracked's Drop re-verifies its sentinel
    // and deregisters from the shared table.
    live.clear();
    verify_passes
}

/// M1/M3 empirical gate: spawn N workers churning safe allocations of varied
/// sizes through the installed `SeferAlloc`, with a shared live-address table
/// that detects any overlap at registration time and sentinel re-verification
/// (mid-life AND at drop) that detects any byte clobbering. PASS = the test
/// reached here without an overlap/clobber panic AND the live table is empty
/// (every allocation was deregistered by its single, correct Drop).
#[test]
fn safe_surface_never_aliases_live_allocations() {
    let table: Arc<Mutex<LiveTable>> = Arc::new(Mutex::new(BTreeMap::new()));

    let handles: Vec<_> = (0..N_WORKERS)
        .map(|tid| {
            let table = Arc::clone(&table);
            std::thread::spawn(move || worker(tid, table))
        })
        .collect();

    let mut total_verify_passes: u64 = 0;
    for h in handles {
        match h.join() {
            Ok(vp) => total_verify_passes += vp,
            // A worker panicked — almost certainly an M1/M3 assertion firing.
            // Resume the panic on the main thread so the ORIGINAL assertion
            // message becomes the test failure (rather than "worker panicked").
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    // After every worker joined, every Tracked has been dropped → table empty.
    // A non-empty table means a `Tracked::drop` failed to deregister (an M1
    // bookkeeping violation) or a Drop was skipped (which safe code cannot do).
    let live = table.lock().expect("final lock poisoned");
    assert!(
        live.is_empty(),
        "M1/M3 leak check: {} live entries remain after all workers joined — a \
         Tracked::drop failed to deregister from the live table",
        live.len(),
    );
    drop(live);

    let total_allocs = TOTAL_ALLOCS.load(Ordering::Acquire);
    eprintln!(
        "[stress_safe_surface_no_aliasing] {} allocations across {} workers x {} \
         iters; {} full-table sentinel verify passes; size classes {:?}. \
         No M1/M3 violation detected (no aliasing, no sentinel clobber).",
        total_allocs, N_WORKERS, ITERS_PER_WORKER, total_verify_passes, SIZE_CLASSES,
    );
}
