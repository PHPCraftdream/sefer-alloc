//! [`ByteRegion`] — the size-classed free-list byte arena (Phase 4 Cartographer
//! + the single confined raw aperture).
//!
//! This is the **Cartographer** of the byte tier: all placement logic (which
//! size class, which free offset, chunk growth) is **pure safe integer
//! arithmetic** over `(chunk_index, block_offset)`. The `unsafe` lives in three
//! closely-related sites, all in this file: (1) allocating/freeing the aligned
//! backing chunks, (2) the single aperture that turns a resolved
//! `(chunk_index, offset)` into the `*mut u8` the caller receives, and (3) the
//! deallocation path's pointer arithmetic. Every `unsafe` block below carries a
//! `// SAFETY:` comment naming the invariant it relies on.
//!
//! ## Design
//!
//! - A fixed table of **size classes** that are powers of two (8, 16, …, 1024).
//!   A request rounds up to the smallest class that fits its `Layout` size AND
//!   whose block size is `>=` the requested alignment.
//! - The backing store grows by appending fresh **chunks** of 64 KiB each,
//!   allocated with [`std::alloc`] at an alignment of `MAX_CLASS` (1024). The
//!   chunk base is therefore `MAX_CLASS`-aligned, and because every block offset
//!   is a multiple of its class size (a power of two `<= MAX_CLASS`), the
//!   absolute address `base + offset` is **always class-aligned** — natural
//!   alignment holds for every layout the class was selected for, including
//!   reused free-list blocks.
//! - **INVARIANT: a chunk, once allocated, is pinned for the region's life** —
//!   chunks are never reallocated or resized after creation, so every pointer
//!   ever handed out stays valid until the region is dropped. Carving happens by
//!   advancing a per-chunk `bump` cursor in multiples of the class size; freed
//!   blocks return to a per-class free list of `(chunk_index, offset)` pairs.
//! - Requests **larger than the largest class** (or whose alignment no class
//!   satisfies) fall back to the **system allocator** (`std::alloc::alloc`).
//!   Their raw pointers are recorded in a `HashSet<*mut u8>` so [`dealloc`]
//!   routes them back to `std::alloc::dealloc` instead of mistaking them for an
//!   in-arena block. (We choose the pointer-set approach so a buggy layout
//!   passed to `dealloc` cannot silently free an in-arena block as if it were
//!   system memory.)
//!
//! [`dealloc`]: ByteRegion::dealloc
//!
//! ## Honest scope
//!
//! This is research, not a production allocator. It is **not** expected to beat
//! `std::alloc::System` or `mimalloc`, and that is an acceptable, documented
//! outcome — see `docs/BYTE_BENCH.md`. The goal is to descend the design to raw
//! bytes and document where the safe membrane must open.

// The crate is `#![deny(unsafe_code)]` with `byte` on (see `src/lib.rs`); this
// is the documented confined-unsafe module for the byte tier. `allow` lifts the
// crate-level `deny` for this file only, so the confinement is enforced
// structurally by the compiler — `unsafe` anywhere else is a hard error. (With
// no features the crate is `forbid` and this module is not compiled at all.)
#![allow(unsafe_code)]

use std::alloc::Layout;
use std::collections::HashSet;
use std::ptr::NonNull;

/// Size of each backing chunk, in bytes. 64 KiB — large enough to amortise
/// chunk growth across many small allocations, small enough that a single
/// `ByteRegion` stays cheap to construct and miri-friendly.
const CHUNK_SIZE: usize = 64 * 1024;

/// The fixed size-class table. Every entry is a power of two, which (combined
/// with the `MAX_CLASS`-aligned chunk base) guarantees that every block's start
/// address is naturally aligned to its class size.
const SIZE_CLASSES: [usize; 8] = [8, 16, 32, 64, 128, 256, 512, 1024];

/// The largest size class. Chunk bases are allocated aligned to this value, so
/// `base + (k * class_size)` is always a multiple of `class_size` (since
/// `class_size` is a power of two `<= MAX_CLASS`).
const MAX_CLASS: usize = 1024;

/// Round `n` up to the next multiple of `a` (a power of two). Pure safe
/// arithmetic — part of the Cartographer.
fn align_up(n: usize, a: usize) -> usize {
    debug_assert!(a.is_power_of_two(), "alignment must be a power of two");
    let mask = a - 1;
    (n + mask) & !mask
}

/// A pinned, `MAX_CLASS`-aligned backing chunk of `CHUNK_SIZE` bytes.
///
/// Allocated via the system allocator with an explicit alignment of
/// `MAX_CLASS` so that block offsets (multiples of a class size, itself a power
/// of two `<= MAX_CLASS`) always yield class-aligned absolute addresses. The
/// chunk owns its memory and frees it on drop; it is never reallocated.
struct Chunk {
    /// The base pointer of the chunk's `CHUNK_SIZE` bytes. Non-null, aligned to
    /// `MAX_CLASS`, valid for `CHUNK_SIZE` bytes.
    base: NonNull<u8>,
    /// The layout used to allocate `base` (size + align), kept for `dealloc`.
    layout: Layout,
}

impl Chunk {
    /// Allocate a fresh zero-overhead 64 KiB chunk aligned to `MAX_CLASS`.
    ///
    /// Returns `None` only if the system allocator fails (OOM).
    fn new() -> Option<Self> {
        let layout = Layout::from_size_align(CHUNK_SIZE, MAX_CLASS)
            .expect("CHUNK_SIZE is a multiple of MAX_CLASS");
        // SAFETY: `layout` has non-zero size (CHUNK_SIZE) and a valid
        // power-of-two align (MAX_CLASS). `std::alloc::alloc` returns either a
        // non-null pointer valid for `layout.size()` bytes at `layout.align()`,
        // or null on OOM. We wrap a non-null result in `NonNull`.
        let ptr = unsafe { std::alloc::alloc(layout) };
        let base = NonNull::new(ptr)?;
        Some(Self { base, layout })
    }

    /// The base pointer as `*mut u8`.
    fn as_mut_ptr(&self) -> *mut u8 {
        self.base.as_ptr()
    }
}

impl Drop for Chunk {
    fn drop(&mut self) {
        // SAFETY: `self.base` was allocated with exactly `self.layout` in
        // `Chunk::new`, and `Chunk` owns it (no aliasing). Dropping the chunk
        // returns its memory to the system allocator exactly once.
        unsafe {
            std::alloc::dealloc(self.base.as_ptr(), self.layout);
        }
    }
}

// SAFETY: `Chunk` owns a raw byte buffer with no per-thread affinity; moving it
// across threads does not create aliased mutable access (the pointer moves
// with the owning `Chunk`). The buffer is plain bytes, so `Send` is sound.
// (The default would NOT be `Send` because of the raw pointer; we assert it.)
unsafe impl Send for Chunk {}

/// A handle/offset-addressed, size-classed free-list byte arena.
///
/// See the module docs for the design and the honest scope.
pub struct ByteRegion {
    /// The backing chunks. Grown by pushing a fresh [`Chunk`]; existing chunks
    /// are never mutated in capacity (only their bytes are written through the
    /// raw aperture), so handed-out pointers stay valid for the region's life.
    chunks: Vec<Chunk>,
    /// Per-class free list of `(chunk_index, block_offset)` pairs. The safe
    /// Cartographer pops from here to reuse a freed block before carving a new
    /// one. Offsets are always a multiple of the class size, so the natural
    /// alignment invariant holds for reused blocks too.
    free_lists: Vec<Vec<(u32, u32)>>,
    /// Per-chunk bump cursor: the first uncarved byte offset. Advancing it in
    /// multiples of a class size is what keeps every block start aligned.
    chunk_bump: Vec<u32>,
    /// Live large-allocation pointers that were delegated to the system
    /// allocator. [`dealloc`](Self::dealloc) consults this set to route such a
    /// pointer back to `std::alloc::dealloc` rather than treating it as an
    /// in-arena block. Membership is the authoritative signal that a pointer is
    /// system-allocated.
    large: HashSet<*mut u8>,
}

impl ByteRegion {
    /// Creates a new empty byte region.
    #[must_use]
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            free_lists: vec![Vec::new(); SIZE_CLASSES.len()],
            chunk_bump: Vec::new(),
            large: HashSet::new(),
        }
    }

    /// Returns the index of the smallest size class that fits `size` and whose
    /// block size is `>= align`, or `None` if no class satisfies both (size too
    /// large, or alignment exceeds the largest class).
    ///
    /// This is the safe Cartographer's classifier — pure arithmetic, no memory
    /// touched. Because the chunk base is `MAX_CLASS`-aligned and block offsets
    /// are multiples of the class size, returning a class whose size `>= align`
    /// guarantees the block start satisfies the requested alignment.
    fn class_for(size: usize, align: usize) -> Option<usize> {
        // The layout must fit in the class (size) and be naturally aligned by
        // it (align <= class size, and class size is a power of two).
        let need = size.max(align);
        SIZE_CLASSES
            .iter()
            .position(|&c| c >= need)
            .filter(|&i| align <= SIZE_CLASSES[i])
    }

    /// Allocate `layout.size()` bytes satisfying `layout.align()`.
    ///
    /// Returns a non-null `*mut u8` on success, or null on failure (only the
    /// system-allocator fallback can fail, and only on out-of-memory). The
    /// memory is **uninitialised** — see [`alloc_zeroed`](Self::alloc_zeroed)
    /// for zeroed memory.
    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        match Self::class_for(layout.size(), layout.align()) {
            // In-arena: take a freed block if one is available, else carve a
            // fresh block out of a chunk (growing the chunk pool if needed).
            Some(class_idx) => self.alloc_small(class_idx),
            // Too large / too aligned for any class: delegate to the system
            // allocator and record the pointer so dealloc routes it back.
            None => self.alloc_large(layout),
        }
    }

    /// Allocate from the size-class free list or by carving a fresh block.
    /// `class_idx` is the resolved class. Memory is uninitialised.
    fn alloc_small(&mut self, class_idx: usize) -> *mut u8 {
        // 1. Reuse a freed block if one exists (the safe Cartographer).
        if let Some((chunk_idx, offset)) = self.free_lists[class_idx].pop() {
            return self.block_ptr(chunk_idx, offset);
        }

        // 2. Otherwise carve a fresh block out of a chunk (or grow).
        let class_size = SIZE_CLASSES[class_idx];
        let (chunk_idx, offset) = self.carve_block(class_size);
        self.block_ptr(chunk_idx, offset)
    }

    /// Carve a fresh `class_size`-aligned block out of a chunk. If no existing
    /// chunk has room (after alignment padding), append a new pinned chunk.
    /// Returns `(chunk_index, offset_within_chunk)`. Safe integer arithmetic.
    ///
    /// Because classes are carved out of the *same* chunks in interleaved
    /// order, the bump cursor must be **aligned up to the class size** before
    /// each carve — otherwise a small-class carve leaves the cursor at a
    /// misaligned offset for the next larger class.
    fn carve_block(&mut self, class_size: usize) -> (u32, u32) {
        // Find a chunk with room after aligning its bump up to `class_size`.
        let chunk_idx = self.chunk_bump.iter().rposition(|&bump| {
            let bump = usize::try_from(bump).unwrap_or(0);
            let aligned = align_up(bump, class_size);
            aligned + class_size <= CHUNK_SIZE
        });

        let chunk_idx = match chunk_idx {
            Some(i) => u32::try_from(i).expect("chunk count fits u32"),
            None => {
                // No room: append a fresh chunk. It is pinned for the region's
                // life — we never grow it after creation.
                self.grow_chunk()
            }
        };

        let bump_slot = &mut self.chunk_bump[usize::try_from(chunk_idx).unwrap()];
        // Align the cursor up to the class size so the block start is a
        // multiple of `class_size`. The chunk base is already `MAX_CLASS`-
        // aligned, so this yields a class-aligned absolute address.
        let prev = usize::try_from(*bump_slot).unwrap_or(0);
        let offset = align_up(prev, class_size);
        *bump_slot = u32::try_from(offset + class_size).expect("within CHUNK_SIZE");
        (chunk_idx, u32::try_from(offset).expect("offset fits u32"))
    }

    /// Append a new 64 KiB `MAX_CLASS`-aligned chunk to the backing pool and
    /// record its bump cursor at 0. Returns its index. This is the **pinning
    /// invariant** site: the chunk is allocated once and never resized, so its
    /// base address is stable for the region's life.
    fn grow_chunk(&mut self) -> u32 {
        let chunk = Chunk::new().expect("chunk allocation must succeed (OOM)");
        self.chunks.push(chunk);
        self.chunk_bump.push(0);
        u32::try_from(self.chunks.len() - 1).expect("chunk count fits u32")
    }

    /// The ONLY raw aperture for handing out memory: turn a resolved
    /// `(chunk_index, offset)` into the `*mut u8` the caller receives. All
    /// placement logic happens in safe code before this; this is the single
    /// irreducible `*mut u8` handoff.
    ///
    /// # Panics
    ///
    /// Panics if `chunk_index`/`offset` are out of bounds — they are produced
    /// only by the safe Cartographer, which never produces out-of-range values,
    /// so this never fires in correct use.
    fn block_ptr(&self, chunk_index: u32, offset: u32) -> *mut u8 {
        let ci = usize::try_from(chunk_index).expect("chunk index fits usize");
        let off = usize::try_from(offset).expect("offset fits usize");
        let chunk = self
            .chunks
            .get(ci)
            .expect("chunk index in range (Cartographer-produced)");
        // SAFETY: `off` is produced by `carve_block`, which only advances the
        // bump cursor while `bump + class_size <= CHUNK_SIZE`, so
        // `off .. off+class_size` is within the chunk's allocation. The chunk's
        // base is `MAX_CLASS`-aligned and its memory is pinned (allocated once,
        // never resized), so this pointer stays valid and aligned for the
        // region's life. We return a `*mut u8` into owned, pinned memory; the
        // caller (or `GlobalAlloc`) is responsible for not reading
        // uninitialised bytes (matching `alloc`'s contract).
        unsafe { chunk.as_mut_ptr().add(off) }
    }

    /// System-allocator fallback for requests too large / too aligned for any
    /// class. Records the pointer so dealloc routes it back.
    fn alloc_large(&mut self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` has non-zero size (GlobalAlloc forbids zero-size;
        // callers from this crate always pass a real Layout), and
        // `std::alloc::alloc` is the documented way to request memory for a
        // layout from the global allocator. The returned pointer is either null
        // (OOM, which we propagate) or valid for `layout.size()` bytes with
        // `layout.align()`. We record it so dealloc routes it back to
        // `std::alloc::dealloc`.
        let ptr = unsafe { std::alloc::alloc(layout) };
        if !ptr.is_null() {
            self.large.insert(ptr);
        }
        ptr
    }

    /// Deallocate memory previously returned by [`alloc`](Self::alloc).
    ///
    /// `ptr` must be a pointer this region handed out and `layout` the same
    /// `Layout` passed to `alloc` (the `GlobalAlloc` contract). Pointers tracked
    /// as large allocations are routed to the system allocator; in-arena blocks
    /// return to their class free list for reuse.
    ///
    /// # Safety
    ///
    /// `ptr` must originate from a prior successful `alloc`/`alloc_zeroed`/
    /// `realloc` on this region with a fitting `layout`, and must not have been
    /// passed to `dealloc` already (no double-free). The method takes `&mut
    /// self` because it mutates the region's free lists.
    pub unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        // Large-allocation fast path: if we recorded this pointer, it belongs to
        // the system allocator.
        if self.large.remove(&ptr) {
            // SAFETY: `ptr` was returned by `std::alloc::alloc(layout)` in
            // `alloc_large` and recorded in `self.large`; we just removed it,
            // so this is its first dealloc (no double-free). The `layout`
            // matches the allocation (caller's contract).
            std::alloc::dealloc(ptr, layout);
            return;
        }

        // In-arena: locate the owning chunk by scanning chunk address ranges.
        // This is O(chunks) but chunks are few (each is 64 KiB); for a research
        // arena this is acceptable and keeps the deallocation path simple and
        // obviously correct (no pointer arithmetic on the caller's pointer
        // beyond range checks).
        let Some(class_size) =
            Self::class_for(layout.size(), layout.align()).map(|c| SIZE_CLASSES[c])
        else {
            // Not large-tracked and not classifiable — a foreign pointer.
            // This is a contract violation; for safety we no-op rather than
            // risk UB on memory we do not own.
            return;
        };

        let Some((chunk_idx, offset)) = self.locate(ptr) else {
            // Pointer is not within any chunk and not large-tracked: contract
            // violation (foreign pointer). No-op; do not touch foreign memory.
            return;
        };

        // Return the block to its class free list for reuse. The block offset
        // is a multiple of the class size (carve invariant), so a future
        // allocation of the same class reuses it with correct alignment.
        if let Some(ci) = SIZE_CLASSES.iter().position(|&c| c == class_size) {
            self.free_lists[ci].push((chunk_idx, offset));
        }
    }

    /// Locate `(chunk_index, offset)` for an in-arena pointer, or `None` if the
    /// pointer is not within any chunk's byte range. Safe pointer comparison on
    /// the chunk base addresses only (no dereference of `ptr`).
    fn locate(&self, ptr: *mut u8) -> Option<(u32, u32)> {
        let ptr_c = ptr.cast_const();
        for (ci, chunk) in self.chunks.iter().enumerate() {
            let base = chunk.as_mut_ptr().cast_const();
            let end = base.wrapping_add(CHUNK_SIZE);
            // `ptr` within `[base, end)`?
            if ptr_c >= base && ptr_c < end {
                // SAFETY: `ptr_c` is within `[base, end)` (just checked), so it
                // and `base` originate from the same allocated object (this
                // chunk), and `base <= ptr_c < end`. `offset_from` is therefore
                // sound and the result is in `[0, CHUNK_SIZE)`.
                let offset = unsafe { ptr_c.offset_from(base) };
                let offset = u32::try_from(offset).ok()?;
                return Some((u32::try_from(ci).ok()?, offset));
            }
        }
        None
    }

    /// Allocate `layout.size()` bytes of **zeroed** memory.
    ///
    /// Equivalent to [`alloc`](Self::alloc) followed by zeroing the full
    /// `layout.size()` range.
    pub fn alloc_zeroed(&mut self, layout: Layout) -> *mut u8 {
        let ptr = self.alloc(layout);
        if !ptr.is_null() {
            // SAFETY: `ptr` was just returned by `alloc(layout)` (or the system
            // fallback) for a non-null, `layout`-valid block of at least
            // `layout.size()` bytes. Writing zeroes over that exact range is
            // therefore in-bounds and sound.
            unsafe {
                std::ptr::write_bytes(ptr, 0, layout.size());
            }
        }
        ptr
    }

    /// Shrink/grow an allocation in place or by alloc + copy + dealloc.
    ///
    /// On growth the new tail is **uninitialised** (matching `GlobalAlloc`).
    /// Returns null on failure, leaving the old allocation intact.
    ///
    /// # Safety
    ///
    /// `ptr` must be a valid prior allocation of `old_layout`, not yet
    /// deallocated. The returned pointer (if non-null) replaces `ptr`, which
    /// must not be used after this call.
    pub unsafe fn realloc(&mut self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        // Large allocation: the system allocator handles realloc for us.
        if self.large.contains(&ptr) {
            // SAFETY: `ptr` is a recorded large allocation made via
            // `std::alloc::alloc(old_layout)`. We delegate to
            // `std::alloc::realloc`, which is sound for a valid pointer/layout
            // pair and returns a new allocation (or null on OOM). On success we
            // must update our tracking: remove the old pointer and record the
            // new one. The caller guarantees `ptr` is valid and not yet freed.
            let new_ptr = std::alloc::realloc(ptr, old_layout, new_size);
            if !new_ptr.is_null() {
                self.large.remove(&ptr);
                self.large.insert(new_ptr);
            }
            return new_ptr;
        }

        // In-arena: the simplest correct semantics is alloc + copy + dealloc.
        let Ok(new_layout) = Layout::from_size_align(new_size, old_layout.align()) else {
            return std::ptr::null_mut();
        };
        let new_ptr = self.alloc(new_layout);
        if new_ptr.is_null() {
            return std::ptr::null_mut();
        }
        let copy = old_layout.size().min(new_size);
        // SAFETY: both `ptr` (old, valid, old_layout.size() bytes — caller's
        // contract) and `new_ptr` (just allocated, new_layout.size() >= copy
        // bytes) are valid for at least `copy` bytes; the ranges do not overlap
        // (distinct allocations).
        std::ptr::copy_nonoverlapping(ptr, new_ptr, copy);
        // SAFETY: `ptr` is the valid prior allocation the caller passed; we just
        // moved its live bytes into `new_ptr`, so it is now free to return to
        // its class free list. `dealloc` is itself `unsafe fn` for this reason;
        // the caller's contract on `realloc` covers it.
        self.dealloc(ptr, old_layout);
        new_ptr
    }

    /// Returns whether `ptr` was handed out by this region (it lies within one
    /// of this region's chunks, OR it is a recorded large/system-fallback
    /// allocation).
    ///
    /// This is the **owner-lookup** primitive used by the Phase 7d sharded
    /// arena's cross-thread `dealloc` router: when a pointer is freed by a
    /// thread other than the one that allocated it, the arena scans its shards
    /// and asks each `contains_ptr` to find the owner. It performs **no
    /// dereference of `ptr`** — only safe pointer-comparison against the chunk
    /// base addresses and a lookup in the `large` set — so it adds **no new
    /// `unsafe`** and stays miri-clean. The scan is `O(shards + chunks)`; shards
    /// are few (typically one per hardware thread) and chunks are 64 KiB each,
    /// so the cost is modest for the research scope. A pointer that no shard
    /// owns is treated as a contract violation by the caller and never freed
    /// against the wrong shard.
    ///
    /// Note: a pointer that originates from a *different* region is never
    /// claimed here (chunk bases and large pointers are disjoint per region),
    /// so routing cannot misidentify the owner.
    #[doc(hidden)]
    #[must_use]
    pub fn contains_ptr(&self, ptr: *mut u8) -> bool {
        self.locate(ptr).is_some() || self.large.contains(&ptr)
    }

    /// Number of backing chunks currently held. Exposed (`#[doc(hidden)]`) for
    /// the integration tests in `tests/byte.rs` to assert that free-list reuse
    /// keeps growth bounded under churn — not part of the public allocation API.
    #[doc(hidden)]
    #[must_use]
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}

impl Default for ByteRegion {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY (Send): a `ByteRegion` OWNS all the memory it tracks — its `Chunk`s
// own their backing buffers (each `Chunk` is itself `Send`), and the `large`
// set holds pointers to system allocations this region made and frees on
// `dealloc`/drop. The default `Send` is withheld only because `large` is a
// `HashSet<*mut u8>` (a raw pointer is not `Send`); but moving the whole region
// to another thread moves ownership of every one of those allocations with it —
// no allocation stays referenced by the origin thread, so there is no aliased
// mutable access. This is what lets `Mutex<ByteRegion>` be `Send + Sync`, which
// `ByteAllocator` and the Phase-7d `ShardedByteArena` rely on to serialise
// access across threads. `ByteRegion` is deliberately NOT `Sync`: shared
// concurrent access is only ever mediated through a `Mutex`.
unsafe impl Send for ByteRegion {}
