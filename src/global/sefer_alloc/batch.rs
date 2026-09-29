// The crate is `#![deny(unsafe_code)]` with `alloc-global` on (see `src/lib.rs`).
// `allow` lifts the crate-level deny for this file only -- the ONLY `unsafe`
// here is the caller-pointer contract on `alloc_batch`/`dealloc_batch` plus
// the `// SAFETY:`-annotated pointer handoff to `HeapCore`.
#![allow(unsafe_code)]

use core::alloc::Layout;

use crate::global::fallback;
use crate::global::tls_heap::CurrentHeap;
use crate::global::tls_heap::{current_for_dealloc, CurrentHeapForDealloc};

use super::SeferAlloc;

impl SeferAlloc {
    /// R10-7 (Part 2) — **tcache-aware batch allocation** wrapper.
    ///
    /// # ⚠ EXPERIMENTAL / UNSTABLE
    ///
    /// This API has NO semver guarantees. It may change signature, behavior,
    /// or be removed entirely in any release without a major version bump,
    /// for as long as the `batch-api` feature (which requires
    /// `experimental`) remains unstable. Use at your own risk in production
    /// code.
    ///
    /// # API boundary — `batch-api` Cargo feature (R10-7 follow-up)
    ///
    /// `#[doc(hidden)]` alone is NOT a real API boundary: it hides the item
    /// from rustdoc but leaves it on the public semver/ABI surface (external
    /// code can still call it, and a signature change would still be a
    /// breaking change). This method (and `dealloc_batch` below) is
    /// additionally gated behind the **`batch-api` Cargo feature** (which
    /// itself requires `experimental` — R12-12), which is NOT part of
    /// `production` or any default-on bundle. Downstream code cannot reach
    /// this surface at all without explicitly opting in, so the signature can
    /// evolve freely without semver consequences for the vast majority of
    /// users (who build with `production` alone). Chosen over `pub(crate)` +
    /// an adapter because the existing bench/test consumers
    /// (`benches/global_alloc.rs`'s `batch_tcache` arm, `tests/batch_tcache.rs`,
    /// and the new `tests/r10_7_alloc_batch_xthread_double_free.rs`) live
    /// OUTSIDE the crate and need a `pub` path — a feature gate preserves
    /// their access pattern while adding the hard semver boundary the review
    /// asked for. `#[doc(hidden)]` was dropped (R12-12): hiding a stable-
    /// looking signature from rustdoc is not the same as marking it
    /// unstable — a user who enables `batch-api` and finds these functions
    /// via IDE autocomplete or the source deserves a visible warning, not a
    /// silently-absent one.
    ///
    /// Resolves the per-thread heap ONCE (one TLS lookup for the whole batch,
    /// vs N for N scalar `alloc` calls), then delegates to
    /// [`HeapCore::alloc_batch`], which drains the warm magazine and
    /// batch-refills only the remainder. Returns the number of slots filled
    /// (0 only on true OOM); `out[filled..]` is left uninitialised and MUST
    /// NOT be used by the caller.
    ///
    /// # Safety
    /// Same contract as [`GlobalAlloc::alloc`](core::alloc::GlobalAlloc::alloc): `layout` must be a non-zero-size
    /// valid `Layout`. Every returned non-null pointer is a live allocation owned
    /// by this allocator and must be freed exactly once via [`dealloc_batch`] (or
    /// N scalar `dealloc` calls). Null entries (on partial fill / OOM) must not
    /// be freed.
    ///
    /// [`dealloc_batch`]: Self::dealloc_batch
    #[cfg(feature = "batch-api")]
    pub unsafe fn alloc_batch(&self, layout: Layout, out: &mut [*mut u8]) -> usize {
        match self.current_heap() {
            CurrentHeap::Fallback => {
                fallback::with_heap(|h| h.alloc_batch(layout, out)).unwrap_or(0)
            }
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in a
            // registry slot owned by THIS thread (single-writer invariant) —
            // `current_heap()` just resolved it for the calling thread.
            CurrentHeap::Own(heap) => unsafe { (*heap).alloc_batch(layout, out) },
        }
    }

    /// R10-7 (Part 2); batched by R11-4 — **batch deallocation** wrapper.
    ///
    /// # ⚠ EXPERIMENTAL / UNSTABLE
    ///
    /// This API has NO semver guarantees. It may change signature, behavior,
    /// or be removed entirely in any release without a major version bump,
    /// for as long as the `batch-api` feature (which requires
    /// `experimental`) remains unstable. Use at your own risk in production
    /// code.
    ///
    /// Same `batch-api` feature boundary as [`alloc_batch`] (see that
    /// method's API-boundary doc section). Resolves the per-thread heap
    /// ONCE, then delegates to [`HeapCore::dealloc_batch`], which partitions
    /// `blocks` into a this-heap-owned Small-classified fast subset (batched
    /// magazine-fill + `flush_class` overflow — see that method's doc
    /// comment for the full mechanism and the stated magazine-warmth
    /// trade-off) and a scalar fallback for everything else (foreign,
    /// cross-thread-owned, Large-classified, null). Null entries are always
    /// skipped (matching the per-block contract).
    ///
    /// oxx R2-03: resolved via the same DEALLOC-ONLY `current_for_dealloc`
    /// the scalar `GlobalAlloc::dealloc` (`global_alloc.rs`) uses — NOT
    /// `self.current_heap()` — so a dealloc-only thread (never allocated, or
    /// already `TORN`) does not pay to claim/materialise a registry slot or
    /// take the fallback spinlock just to free a batch of foreign pointers.
    /// See `tls_heap::current_for_dealloc`'s doc comment for the full
    /// rationale and `SeferAlloc::dealloc`'s `ForeignNoBind` arm for the
    /// identical trade-off note (TORN + fallback-owned).
    ///
    /// # Safety
    /// Same contract as [`GlobalAlloc::dealloc`](core::alloc::GlobalAlloc::dealloc): every non-null `blocks[i]`
    /// must be the exact start pointer of a currently-live allocation made by
    /// this allocator, with `layout` matching its allocation, and freed at most
    /// once. Null entries are always safe (skipped).
    ///
    /// [`alloc_batch`]: Self::alloc_batch
    #[cfg(feature = "batch-api")]
    pub unsafe fn dealloc_batch(&self, layout: Layout, blocks: &[*mut u8]) {
        match current_for_dealloc() {
            CurrentHeapForDealloc::Own(heap) => {
                // SAFETY: `heap` is non-null and points to a live
                // `HeapCore` owned by THIS thread (single-writer
                // invariant); `HeapCore::dealloc_batch` upholds the same
                // per-block contract as scalar `dealloc` for every entry
                // it does not route through its batched fast path.
                unsafe { (*heap).dealloc_batch(layout, blocks) };
            }
            CurrentHeapForDealloc::ForeignNoBind => {
                // Mirrors `SeferAlloc::dealloc`'s `ForeignNoBind` arm
                // exactly, looped per non-null block: this thread never
                // bound a heap (or its slot was already recycled/torn),
                // so every valid pointer reaching here is foreign BY
                // CONSTRUCTION — route it through the heap-instance-
                // independent routing tail, WITHOUT claiming a registry
                // slot and WITHOUT constructing or dereferencing any
                // `*mut HeapCore`.
                //
                // SAFETY: `blocks`/`layout` are the caller-bound
                // dealloc-batch contract pair (this whole fn is `unsafe
                // fn`); `dealloc_foreign_routing` applies the same
                // null-base and magic-mismatch guards the scalar foreign
                // path uses before touching any segment memory.
                for &ptr in blocks {
                    if ptr.is_null() {
                        continue;
                    }
                    let base = crate::alloc_core::os::segment_base_of_ptr(ptr);
                    crate::registry::HeapCore::dealloc_foreign_routing(ptr, base, layout, None);
                }
            }
        }
    }
}
