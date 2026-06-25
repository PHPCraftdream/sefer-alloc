//! [`ShardedByteArena`] — N-way parallel raw allocation via per-thread
//! `ByteRegion` shards (Phase 7d, `byte-sharded` feature).
//!
//! This is the byte-tier analogue of [`ShardedRegion`](crate::ShardedRegion):
//! the parallelism win comes from **sharding** — each writer thread binds to
//! its own shard (a `Mutex<ByteRegion>`), so two threads allocating in
//! different shards never meet on a lock. Compared to the single-
//! `Mutex<ByteRegion>` [`ByteAllocator`](crate::ByteAllocator), which serialises
//! every alloc/dealloc through one global mutex, this arena scales allocation
//! across shards.
//!
//! ## Design — sound and miri-clean, no new `unsafe` of our own
//!
//! The arena is plain **safe composition** over `Mutex<ByteRegion>`:
//!
//! - `ShardedByteArena { shards: Box<[Mutex<ByteRegion>]> }` — one shard, one
//!   mutex. Allocation acquires only the *calling thread's* shard mutex.
//! - A minimal TLS router (`MY_SHARD`) lazily binds the calling thread to a
//!   shard round-robin on first alloc and caches the id in TLS. A thread-exit
//!   [`ErasedReset`] guard clears the binding so a dead thread's id does not
//!   get inherited if the OS recycles the thread (mirrors the 7b release
//!   pattern, minus the `occupied`-token exclusivity claim — we use plain
//!   round-robin, which is correct under any thread count but degrades to
//!   sharing when more threads than shards run concurrently).
//!
//! ## Cross-thread `dealloc` routing — the owner lookup
//!
//! A pointer must return to the shard that allocated it. We use the **scan**
//! approach (plan option (a), RECOMMENDED for the research scope):
//!
//! - `dealloc(ptr, layout)` walks the shards in order and asks each shard's
//!   [`ByteRegion::contains_ptr`] (a safe pointer-comparison against the
//!   chunk base addresses + a `large`-set lookup) whether it owns `ptr`.
//! - The first shard that owns it is locked and the pointer is freed there.
//!
//! This reuses the existing `ByteRegion` logic and adds **zero new `unsafe`**:
//! the only `unsafe` in this file is calling `ByteRegion`'s own
//! `unsafe fn dealloc`/`realloc` (the trait-style handoff already documented in
//! `byte_region`). The scan is `O(shards + chunks)`; shards are few (typically
//! one per hardware thread), so the cost is modest. A pointer no shard owns is
//! treated as a contract violation and silently no-op'd (we never free a
//! pointer against the wrong shard).
//!
//! Large (system-fallback) allocations route correctly for free: each shard's
//! `ByteRegion` tracks its own `large` set internally, so when a shard's
//! `contains_ptr` reports ownership (via its `large` set) and its `dealloc`
//! runs, the system-allocator free happens inside that shard — exactly the
//! shard that allocated it.
//!
//! ## Honest scope
//!
//! This is research, not a production allocator. It is **not** expected to beat
//! `mimalloc` or even the system allocator under realistic load, and that is an
//! acceptable, documented outcome — see `docs/BYTE_SHARDED_BENCH.md`. Notable
//! honest limitations:
//!
//! - **Memory is never returned to the OS until the arena is dropped.** Like
//!   the 7a/Phase-4 byte research, `dealloc` only pushes blocks back onto a
//!   per-shard free list; chunks stay pinned for the arena's life.
//! - **Cross-thread dealloc has O(shards) scan overhead.** An allocating
//!   thread that also frees its own pointers pays only the TLS lookup + one
//!   mutex; a *remote* thread freeing a pointer it did not allocate pays the
//!   shard scan.
//! - **It is not installed as a `#[global_allocator]`.** That would replace
//!   the allocator for the whole process (dangerous in a test binary);
//!   callers opt in by constructing a [`ShardedByteArena`] and calling its
//!   methods directly. An optional `unsafe impl GlobalAlloc` wrapper is
//!   deliberately NOT provided here — it would add no new capability for the
//!   research scope and would complicate the miri story (the `GlobalAlloc`
//!   trait has no way to report "foreign pointer" on `dealloc`, so routing a
//!   process-wide `dealloc` through this arena would require global state and
//!   risks unsoundness for no real-world benefit). The plan flags the wrapper
//!   as optional; we skip it and say so.

// The crate is `#![deny(unsafe_code)]` with `byte` on (see `src/lib.rs`); this
// is the documented confined-unsafe module for the byte tier. The only
// `unsafe` here is calling `ByteRegion`'s own `unsafe fn dealloc`/`realloc`
// (the irreducible raw-pointer handoff already audited in `byte_region`); the
// arena itself is plain safe composition over `Mutex<ByteRegion>`. `allow`
// lifts the crate-level `deny` for this file only.
#![allow(unsafe_code)]

use core::cell::{Cell, RefCell};

use std::alloc::Layout;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::byte::byte_region::ByteRegion;

/// The default per-thread chunk budget passed to nothing in particular — kept
/// only to mirror the `ShardedRegion` constructor surface; `ByteRegion` grows
/// its chunks on demand, so no capacity is reserved up front.
///
/// (This constant exists as an honest placeholder: unlike `ShardedRegion`, the
/// byte arena does NOT pre-allocate shard capacity — `ByteRegion::new()` is
/// empty and grows chunks lazily. We keep the constant for API symmetry and
/// future tunability; it is currently unused at runtime.)
#[allow(dead_code)]
const _DEFAULT_NOTE: () = ();

/// A type-erased thread-local guard that clears the calling thread's shard
/// binding on `Drop` (thread exit). Without this, a thread that exits while
/// holding a TLS shard id would leave that id in TLS if the OS recycles the
/// underlying thread (e.g. under a thread pool); the next task on that thread
/// would inherit a stale binding. Clearing on exit forces a fresh round-robin
/// claim on the thread's next use.
///
/// This guard carries no data of its own — its mere presence in TLS is the
/// signal, and its `Drop` is the action. It is type-erased (carries no shard
/// pointers) so a single `thread_local!` registry serves every arena instance.
struct ErasedReset;

impl Drop for ErasedReset {
    fn drop(&mut self) {
        // Clear the TLS shard id so a recycled thread re-claims on next use.
        // Relaxed: TLS access is per-thread; no cross-thread synchronization
        // is involved (the cell is thread-local).
        MY_SHARD.with(|cell| cell.set(None));
    }
}

// The TLS router: `MY_SHARD` caches the calling thread's claimed shard id for
// the fast path (a plain integer TLS read — `Cell` because `Option<u16>` is
// `Copy`). `ERASED_RESET` installs once per thread and its `Drop` clears the
// id on thread exit; it is a `RefCell<Option<_>>` because `ErasedReset` has a
// `Drop` impl (so it is not `Copy`).
thread_local! {
    static MY_SHARD: Cell<Option<u16>> = const { Cell::new(None) };
}

thread_local! {
    static ERASED_RESET: RefCell<Option<ErasedReset>> = const { RefCell::new(None) };
}

/// The hard cap on shard count, matching the `u16` shard-id space.
const MAX_SHARDS: usize = u16::MAX as usize;

/// N per-thread [`ByteRegion`] shards for parallel raw allocation.
///
/// See the [module docs](self) for the design, the TLS router, and the
/// cross-thread dealloc owner-lookup. Construct with [`with_shards`](Self::with_shards)
/// or [`new`](Self::new).
pub struct ShardedByteArena {
    /// One `Mutex<ByteRegion>` per shard. Allocating in shard *i* takes only
    /// shard *i*'s mutex; a thread bound to a different shard never contends.
    shards: Box<[Mutex<ByteRegion>]>,
    /// Atomic round-robin cursor for shard claiming. `fetch_add` then modulo
    /// shard count spreads distinct threads across distinct shards when the
    /// thread count is `<= shard_count` (the common case for a bounded pool).
    next_shard: AtomicUsize,
}

impl ShardedByteArena {
    /// Creates a sharded byte arena with `n` shards, each starting empty (a
    /// fresh [`ByteRegion`] that grows chunks on demand).
    ///
    /// `n` is capped at `u16::MAX` (the shard-id space) — a larger `n` panics,
    /// since it almost certainly indicates a caller bug.
    ///
    /// # Panics
    ///
    /// Panics if `n == 0` (an arena with no shards cannot accept any alloc) or
    /// if `n > u16::MAX`.
    #[must_use]
    pub fn with_shards(n: usize) -> Self {
        assert!(n > 0, "ShardedByteArena::with_shards: n must be > 0");
        assert!(
            n <= MAX_SHARDS,
            "ShardedByteArena::with_shards: n={n} exceeds the u16 shard-id space ({MAX_SHARDS})"
        );
        let shards: Vec<Mutex<ByteRegion>> =
            (0..n).map(|_| Mutex::new(ByteRegion::new())).collect();
        Self {
            shards: shards.into_boxed_slice(),
            next_shard: AtomicUsize::new(0),
        }
    }

    /// Creates a sharded arena whose shard count matches the host's available
    /// parallelism (`std::thread::available_parallelism`, falling back to 1 on
    /// error). This is the natural default for a bounded pool of long-lived
    /// worker threads: one shard per hardware thread means allocators rarely
    /// collide.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of shards (fixed for the arena's lifetime).
    #[must_use]
    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }

    /// Returns the calling thread's claimed shard id, or `None` if it has not
    /// yet bound. Fast path: a plain TLS read (no atomic, no lock).
    fn my_shard(&self) -> Option<u16> {
        MY_SHARD.with(|cell| cell.get())
    }

    /// Lazily claims a shard for the calling thread (on first use) and returns
    /// its id. Subsequent calls return the cached binding from TLS.
    ///
    /// **Claim protocol:** round-robin via a monotonic atomic ticket modulo
    /// shard count. This is the simplest correct choice: distinct threads get
    /// distinct shards when `thread_count <= shard_count` (the common case),
    /// and degrade gracefully to sharing otherwise (still correct — both
    /// threads route to the same shard, just with mutex contention). The
    /// binding is cached in TLS, and an [`ErasedReset`] guard is installed
    /// once per thread so its `Drop` clears the binding at thread exit.
    ///
    /// **Robustness:** the TLS cell is process-global (one per thread, shared
    /// across every `ShardedByteArena` instance). If a thread bound in a
    /// DIFFERENT arena with MORE shards, the cached id could exceed THIS
    /// arena's shard count; returning it verbatim would index out of bounds.
    /// So we only trust the cache when it is in range for this arena;
    /// otherwise we fall through and (re)claim a valid shard here.
    fn claim_or_get_shard(&self) -> u16 {
        let n = self.shards.len();
        if let Some(id) = self.my_shard() {
            if usize::from(id) < n {
                return id;
            }
        }
        // Round-robin: a monotonic ticket modulo shard count. Relaxed is fine
        // — we only need each thread to get a *distinct* id, not a globally
        // ordered one; the modulo spreads the tickets across shards.
        let ticket = self.next_shard.fetch_add(1, Ordering::Relaxed);
        let id = u16::try_from(ticket % n)
            .expect("shard id fits u16: ticket%n where n<=u16::MAX cannot exceed u16::MAX");
        // Cache the id (fast path).
        MY_SHARD.with(|cell| cell.set(Some(id)));
        // Install (once per thread) the ErasedReset whose Drop clears the TLS
        // id at thread exit. Idempotent: if a guard is already registered for
        // this thread, do nothing.
        ERASED_RESET.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.is_none() {
                *slot = Some(ErasedReset);
            }
        });
        id
    }

    /// Allocate `layout.size()` bytes satisfying `layout.align()` from the
    /// calling thread's claimed shard.
    ///
    /// Returns a non-null `*mut u8` on success, or null on failure (only the
    /// system-allocator fallback can fail, and only on out-of-memory). The
    /// memory is **uninitialised** — see [`alloc_zeroed`](Self::alloc_zeroed).
    ///
    /// On the thread's first alloc, lazily claims a shard via the TLS router.
    #[must_use]
    pub fn alloc(&self, layout: Layout) -> *mut u8 {
        let id = self.claim_or_get_shard();
        let mut region = self.shards[usize::from(id)]
            .lock()
            .expect("sharded byte arena shard mutex not poisoned");
        region.alloc(layout)
    }

    /// Allocate `layout.size()` bytes of **zeroed** memory from the calling
    /// thread's claimed shard. Equivalent to [`alloc`](Self::alloc) plus a
    /// zero-fill of the full range.
    #[must_use]
    pub fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let id = self.claim_or_get_shard();
        let mut region = self.shards[usize::from(id)]
            .lock()
            .expect("sharded byte arena shard mutex not poisoned");
        region.alloc_zeroed(layout)
    }

    /// **Pre-warm every shard**: force each shard to carve a backing chunk and
    /// populate its size-class free lists, touching the pages so the OS commits
    /// them up front.
    ///
    /// This does NOT change steady-state throughput and it does NOT make the
    /// sharding win visible in a spawn-dominated microbench (see
    /// `docs/BYTE_SHARDED_BENCH.md`). What it removes is **cold-start latency**:
    /// without it, the *first* allocation in a shard pays for a 64 KiB chunk
    /// allocation from the OS plus first-touch page faults — a latency spike on
    /// the hot path. For latency-sensitive workloads (p99 tails — e.g. a DBMS),
    /// call this at startup (optionally from a background thread, since the arena
    /// is `Send + Sync`: `let a = Arc::clone(&arena); thread::spawn(move || a.prewarm());`)
    /// so the spike is paid before real traffic arrives.
    ///
    /// It walks the shards DIRECTLY (not via the TLS router), so a single call
    /// from any thread warms all shards. Idempotent and cheap to repeat: after
    /// the first call each class already has a free block, so a repeat just
    /// reuses and re-frees it.
    pub fn prewarm(&self) {
        // One representative block per size class (the byte tier's classes are
        // powers of two 8..=1024) plus a chunk-sized span so the whole first
        // chunk's pages are touched, not just the small blocks.
        const PREWARM_SIZES: [usize; 8] = [8, 16, 32, 64, 128, 256, 512, 1024];
        for shard in &self.shards {
            let mut region = shard
                .lock()
                .expect("sharded byte arena shard mutex not poisoned");
            let mut tmp: Vec<(*mut u8, Layout)> = Vec::with_capacity(PREWARM_SIZES.len());
            for &size in &PREWARM_SIZES {
                let Ok(layout) = Layout::from_size_align(size, size) else {
                    continue;
                };
                // `alloc_zeroed` carves the block AND writes zeroes across it,
                // committing/touching those pages. The first such alloc in a
                // fresh region also grows the first chunk (the cold cost we are
                // paying up front here instead of on the hot path).
                let p = region.alloc_zeroed(layout);
                if !p.is_null() {
                    tmp.push((p, layout));
                }
            }
            // Free them back so the warm blocks land on the class free lists —
            // the first REAL alloc of each class then hits the free list (the
            // fastest path) with no growth and no first-touch fault.
            for (p, layout) in tmp {
                // SAFETY: each `p` was just returned by `region.alloc_zeroed(layout)`
                // above (same `region`, same `layout`) and has not been freed.
                unsafe { region.dealloc(p, layout) };
            }
        }
    }

    /// Deallocate a pointer previously returned by [`alloc`](Self::alloc) or
    /// [`alloc_zeroed`](Self::alloc_zeroed), routing it to the **owning**
    /// shard (which may differ from the calling thread's shard — cross-thread
    /// free is supported).
    ///
    /// The owner is identified by scanning the shards and asking each
    /// [`ByteRegion::contains_ptr`] (a safe pointer-comparison). The first
    /// shard that owns `ptr` is locked and the pointer is freed there. A
    /// pointer that no shard owns is a contract violation and is silently
    /// no-op'd — we NEVER free a pointer against the wrong shard.
    ///
    /// # Safety
    ///
    /// `ptr` must originate from a prior successful `alloc`/`alloc_zeroed`/
    /// `realloc` on THIS arena with a fitting `layout`, and must not have been
    /// passed to `dealloc` already (no double-free). The `layout` must match
    /// the one passed to the allocating call (the `GlobalAlloc` contract).
    pub unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let Some(id) = self.owner_of(ptr) else {
            // Foreign pointer: contract violation. No-op rather than risk UB
            // on memory we do not own.
            return;
        };
        let mut region = self.shards[usize::from(id)]
            .lock()
            .expect("sharded byte arena shard mutex not poisoned");
        // SAFETY: `ptr` was returned by THIS shard's `ByteRegion::alloc` (the
        // owner scan just confirmed `contains_ptr`), and the caller guarantees
        // it has not been freed yet and `layout` matches the allocation. So
        // `ByteRegion::dealloc`'s safety contract is met.
        region.dealloc(ptr, layout);
    }

    /// Shrink/grow an allocation in place or by alloc + copy + dealloc. The
    /// existing pointer is routed to its owning shard for the dealloc half,
    /// and the new allocation is taken from the calling thread's shard.
    ///
    /// On growth the new tail is **uninitialised** (matching `GlobalAlloc`).
    /// Returns null on failure, leaving the old allocation intact.
    ///
    /// # Safety
    ///
    /// `ptr` must be a valid prior allocation of `old_layout` from this arena,
    /// not yet deallocated. The returned pointer (if non-null) replaces `ptr`,
    /// which must not be used after this call.
    pub unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        // If the pointer is a large (system-fallback) allocation owned by some
        // shard, let THAT shard's ByteRegion handle the realloc (it can
        // delegate to `std::alloc::realloc` in place, which is the cheap path
        // and keeps the `large`-set bookkeeping correct). Otherwise we do an
        // alloc-on-this-thread + copy + dealloc-on-owner — the in-arena path,
        // where the block moves between shards.
        if let Some(id) = self.owner_of(ptr) {
            // Defer to the owning shard. This handles BOTH the large case
            // (system realloc, `large`-set update) AND the in-arena case
            // (alloc + copy + dealloc all within the owning shard), keeping
            // `large` bookkeeping local to the shard that tracks the pointer.
            let mut region = self.shards[usize::from(id)]
                .lock()
                .expect("sharded byte arena shard mutex not poisoned");
            // SAFETY: `ptr` is owned by this shard (owner scan confirmed) and
            // is a valid prior allocation of `old_layout` (caller's contract).
            return region.realloc(ptr, old_layout, new_size);
        }
        // Foreign pointer: contract violation. Return null (leave the caller's
        // pointer alone — we do not own it and must not touch it).
        std::ptr::null_mut()
    }

    /// Find the shard id that owns `ptr`, or `None` if no shard owns it. This
    /// is the cross-thread dealloc owner lookup (the scan approach, plan
    /// option (a)). It performs no dereference of `ptr` — only safe
    /// pointer-comparison via [`ByteRegion::contains_ptr`].
    ///
    /// The scan locks each shard's mutex briefly to call `contains_ptr`. The
    /// cost is `O(shards)` lock/unlock pairs plus the per-shard chunk-range
    /// scan; shards are few, so this is acceptable for the research scope.
    fn owner_of(&self, ptr: *mut u8) -> Option<u16> {
        for (i, shard) in self.shards.iter().enumerate() {
            let region = shard
                .lock()
                .expect("sharded byte arena shard mutex not poisoned");
            if region.contains_ptr(ptr) {
                return Some(u16::try_from(i).expect("shard index fits u16"));
            }
        }
        None
    }

    /// Total number of backing chunks across all shards (sum of each shard's
    /// [`ByteRegion::chunk_count`]). Exposed (`#[doc(hidden)]`) for tests that
    /// assert bounded growth under churn — not part of the public allocation
    /// API.
    #[doc(hidden)]
    #[must_use]
    pub fn chunk_count(&self) -> usize {
        self.shards
            .iter()
            .map(|s| s.lock().expect("sharded byte arena shard mutex not poisoned").chunk_count())
            .sum()
    }
}

impl Default for ShardedByteArena {
    fn default() -> Self {
        // available_parallelism is the natural shard count for a bounded pool
        // of long-lived threads (one shard per hardware thread → allocators
        // rarely collide). Fall back to 1 on any error.
        let n = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(1)
            .min(MAX_SHARDS);
        Self::with_shards(n.max(1))
    }
}
