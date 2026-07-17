//! `globalalloc-model` — differential-test any allocator against a reference model.
//!
//! Apply a random stream of `alloc` / `dealloc` / `realloc` / `alloc_zeroed`
//! operations to the allocator under test AND to a trivial reference model
//! (a `Vec` of live blocks), asserting the **M1–M4 oracles** on every step:
//!
//! - **M1 (validity):** every returned pointer is non-null, aligned to the
//!   requested align, and writable for the requested size (write a distinctive
//!   fill byte, read it back).
//! - **M2 (no double-free / UAF):** the model only frees live pointers; a
//!   second `dealloc` of the same pointer is a no-op that must not corrupt the
//!   allocator.
//! - **M3 (no overlap):** two simultaneously-live allocations never share a
//!   byte — checked against every live block, and re-checked at run end via a
//!   per-block fill that would detect cross-contamination.
//! - **M4 (alignment & size fidelity):** the returned pointer always satisfies
//!   the requested size and align.
//! - **`alloc_zeroed` contract:** every byte of a zeroed allocation reads as 0.
//! - **`realloc` prefix preservation:** the `min(old, new)` prefix is preserved.
//!
//! One model, two front-ends. The same [`drive`] loop powers both:
//! - a proptest [`Strategy`](op_strategy) over `Vec<Op>` (`proptest` feature) —
//!   for `cargo test` and the bounded miri run, and
//! - an [`impl Arbitrary for OpStream`](OpStream) (`arbitrary` feature) — for
//!   `cargo fuzz` / libFuzzer.
//!
//! An oracle improvement thus reaches proptest, miri, AND libFuzzer at once.
//!
//! # The allocator seam
//!
//! The driver is generic over a minimal [`RawAllocator`] trait — exactly the
//! `alloc` / `dealloc` / `realloc` / `alloc_zeroed` surface of
//! [`core::alloc::GlobalAlloc`], for which a blanket impl is provided. A plain
//! owned allocator with the same four methods (e.g. sefer's `AllocCore`) can
//! implement the trait directly.
//!
//! # Example (text — not a doctest)
//!
//! ```text
//! use globalalloc_model::{drive, Config};
//! use std::alloc::System;
//!
//! // proptest:
//! proptest! {
//!     #[test]
//!     fn matches_model(ops in globalalloc_model::op_strategy(Config::default(), 0..200)) {
//!         drive(&System, Config::default(), &ops);
//!     }
//! }
//!
//! // libFuzzer:
//! fuzz_target!(|stream: globalalloc_model::OpStream| {
//!     drive(&System, Config::default(), &stream.ops);
//! });
//! ```

// This crate's one job includes calling the allocator-under-test's raw-pointer
// API and dereferencing the pointers it hands back — inherently `unsafe`. That
// is the single reason this seam holds `unsafe`: the `RawAllocator` trait is
// `unsafe` (its impls must return valid pointers for the requested layout), and
// the oracle loop writes/reads through a pointer the allocator just returned for
// the size it was asked for. Every such site carries a `// SAFETY:` note.
#![allow(unsafe_code)]

use core::alloc::{GlobalAlloc, Layout};

/// A minimal raw-allocator surface: the four `GlobalAlloc` methods over
/// [`Layout`]. The differential [`drive`] loop is generic over this trait.
///
/// A blanket impl covers every [`GlobalAlloc`]; a plain owned allocator with the
/// same four methods can implement it directly.
///
/// # Safety
///
/// This trait is `unsafe` to implement. An implementor must behave as a correct
/// allocator so the oracles are testing the allocator, not papering over a
/// broken trait impl:
///
/// - [`alloc`](RawAllocator::alloc) / [`alloc_zeroed`](RawAllocator::alloc_zeroed)
///   return either null (allocation failed) or a pointer valid for reads and
///   writes over `layout.size()` bytes and aligned to `layout.align()`. A
///   non-null pointer from `alloc_zeroed` points at `layout.size()` zero bytes.
/// - [`dealloc`](RawAllocator::dealloc) is called only with a pointer previously
///   returned by a matching `alloc`/`alloc_zeroed`/`realloc` on `self` with the
///   same `layout`. When [`Config::double_free`] is set the harness deliberately
///   frees the SAME pointer a second time (the M2 no-op oracle) — enable it only
///   for an allocator whose contract makes that a safe no-op.
/// - [`realloc`](RawAllocator::realloc) either returns null (leaving the old
///   block live and valid) or a pointer valid for `new_size` bytes whose first
///   `min(old_size, new_size)` bytes equal the old block's, consuming the old
///   pointer on a non-null return.
pub unsafe trait RawAllocator {
    /// Allocate `layout.size()` bytes at `layout.align()`; null on failure.
    ///
    /// # Safety
    /// See the [trait-level contract](RawAllocator#safety).
    unsafe fn alloc(&self, layout: Layout) -> *mut u8;

    /// Allocate zeroed `layout.size()` bytes at `layout.align()`; null on failure.
    ///
    /// # Safety
    /// See the [trait-level contract](RawAllocator#safety).
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8;

    /// Free the block `ptr` that was allocated with `layout`.
    ///
    /// # Safety
    /// See the [trait-level contract](RawAllocator#safety).
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout);

    /// Resize the block `ptr` (allocated with `old_layout`) to `new_size` bytes.
    ///
    /// # Safety
    /// See the [trait-level contract](RawAllocator#safety).
    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8;
}

// SAFETY: `GlobalAlloc` has exactly this contract by definition, so forwarding
// each method one-to-one preserves it; a correct `GlobalAlloc` is a correct
// `RawAllocator`.
unsafe impl<A: GlobalAlloc> RawAllocator for A {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarding under the identical `GlobalAlloc::alloc` contract.
        unsafe { GlobalAlloc::alloc(self, layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarding under the identical `GlobalAlloc::alloc_zeroed` contract.
        unsafe { GlobalAlloc::alloc_zeroed(self, layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarding under the identical `GlobalAlloc::dealloc` contract.
        unsafe { GlobalAlloc::dealloc(self, ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarding under the identical `GlobalAlloc::realloc` contract.
        unsafe { GlobalAlloc::realloc(self, ptr, old_layout, new_size) }
    }
}

/// One operation against the allocator and the reference model.
///
/// `Dealloc` / `Realloc` index fields are reduced modulo the live count when
/// applied, so they are always in range regardless of the value generated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// Allocate `size` bytes at `align` (a power of two).
    Alloc { size: usize, align: usize },
    /// Allocate zeroed `size` bytes at `align` (exercises the zero path).
    AllocZeroed { size: usize, align: usize },
    /// Free the `i`-th live allocation (index reduced modulo the live count).
    Dealloc(usize),
    /// Realloc the `i`-th live allocation to `new_size` (index reduced modulo
    /// the live count).
    Realloc { i: usize, new_size: usize },
}

/// A live allocation in the reference model: its pointer, size, align, and the
/// fill byte written across the whole block (so M3 contamination is detectable).
#[derive(Clone, Copy)]
pub struct Live {
    /// The pointer returned by the allocator.
    pub ptr: *mut u8,
    /// The allocation's size in bytes.
    pub size: usize,
    /// The allocation's alignment.
    pub align: usize,
    /// The fill byte written over the whole block.
    pub fill: u8,
}

// SAFETY: `Live` only carries a raw pointer plus plain integers. It is never
// dereferenced across threads by this crate; `Send` lets a proptest harness
// hold a `Vec<Live>` across its (single-threaded) closure boundary, mirroring
// the `unsafe impl Send for Live` in the original in-tree copies.
unsafe impl Send for Live {}

/// Size-distribution knobs for the op-stream generators.
///
/// The defaults span a small hot-path range plus a rare, capped large arm — the
/// shape every in-tree copy used. Tune per allocator-under-test (e.g. set
/// `large_max` above the allocator's small-class ceiling to exercise its
/// dedicated-large path).
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Upper bound (inclusive) of the small size arm.
    pub small_max: usize,
    /// Upper bound (inclusive) of the large size arm.
    pub large_max: usize,
    /// Relative weight of the small arm in the proptest size strategy.
    pub small_weight: u32,
    /// Relative weight of the large arm in the proptest size strategy.
    pub large_weight: u32,
    /// Maximum alignment (a power of two) offered by the align strategy.
    pub max_align: usize,
    /// Whether to exercise the **M2 double-free-is-no-op oracle**: after each
    /// `Dealloc`, free the SAME pointer a second time.
    ///
    /// This is a *stronger-than-`GlobalAlloc`* guarantee — a real system malloc
    /// treats a double-free as undefined behaviour (heap corruption), so this is
    /// **off by default**. Turn it ON only for an allocator whose contract is
    /// that a redundant `dealloc` of an already-freed pointer is a safe no-op
    /// (e.g. sefer's `AllocCore`). The oracle cannot itself observe "no-op"
    /// directly; its guard is that the allocator must not corrupt — a later
    /// op/teardown that would then touch corrupted state is what catches it.
    pub double_free: bool,
}

impl Default for Config {
    fn default() -> Self {
        // Matches the historical `heap_differential` shape: 9:1 small:large,
        // small <= 4 KiB, large capped at 128 KiB, aligns up to 4096. The M2
        // double-free is OFF by default (unsafe against a real malloc); consumers
        // whose allocator tolerates it opt in via `double_free`.
        Config {
            small_max: 4096,
            large_max: 128 * 1024,
            small_weight: 9,
            large_weight: 1,
            max_align: 4096,
            double_free: false,
        }
    }
}

/// Whether `[a, a+asize)` and `[b, b+bsize)` overlap. Touching endpoints are
/// adjacent, NOT overlapping.
#[must_use]
pub fn ranges_overlap(a: usize, asize: usize, b: usize, bsize: usize) -> bool {
    !(a + asize <= b || b + bsize <= a)
}

/// Run the op stream against `alloc` and the reference model, asserting the
/// M1–M4 oracles on every step. Panics (the natural oracle-failure signal for
/// both proptest and libFuzzer) the moment any oracle is violated.
///
/// All survivors are freed and the model dropped before returning (no UAF in a
/// teardown walk). `config.double_free` selects whether the M2
/// double-free-is-no-op oracle is exercised (off by default — a real malloc
/// would corrupt); the other `config` fields shape the generators, not `drive`.
///
/// # Panics
///
/// On any oracle violation: a null return where memory was not exhausted, a
/// misaligned pointer, a live-block overlap, a byte that does not read back, a
/// non-zero byte from `alloc_zeroed`, or a lost realloc prefix byte.
pub fn drive<A: RawAllocator>(alloc: &A, config: Config, ops: &[Op]) {
    let mut live: Vec<Live> = Vec::new();
    let mut next_fill: u8 = 1;

    for op in ops {
        match *op {
            Op::Alloc { size, align } => {
                let layout = Layout::from_size_align(size, align).expect("valid layout");
                // SAFETY: `layout` is valid; the returned pointer is checked for
                // null and used only for `size` bytes, as the contract permits.
                let ptr = unsafe { alloc.alloc(layout) };
                assert!(!ptr.is_null(), "M1: alloc returned null");
                assert_eq!((ptr as usize) % align, 0, "M1/M4: pointer not aligned");
                for other in &live {
                    assert!(
                        !ranges_overlap(ptr as usize, size, other.ptr as usize, other.size),
                        "M3: new alloc overlaps a live block"
                    );
                }
                let fill = next_fill;
                next_fill = next_fill.wrapping_add(1).max(1);
                // SAFETY: `ptr` is non-null and valid for `size` bytes (just
                // allocated for `layout`); we write then read those bytes only.
                unsafe {
                    for b in 0..size {
                        ptr.add(b).write(fill);
                    }
                    for b in 0..size {
                        assert_eq!(ptr.add(b).read(), fill, "M1: byte did not read back");
                    }
                }
                live.push(Live {
                    ptr,
                    size,
                    align,
                    fill,
                });
            }
            Op::AllocZeroed { size, align } => {
                let layout = Layout::from_size_align(size, align).expect("valid layout");
                // SAFETY: `layout` valid; pointer checked for null, used only for
                // `size` bytes.
                let ptr = unsafe { alloc.alloc_zeroed(layout) };
                assert!(!ptr.is_null(), "M1: alloc_zeroed returned null");
                assert_eq!((ptr as usize) % align, 0, "M1/M4: not aligned");
                // SAFETY: `ptr` valid for `size` freshly-zeroed bytes.
                unsafe {
                    for b in 0..size {
                        assert_eq!(ptr.add(b).read(), 0, "alloc_zeroed: byte not zeroed");
                    }
                }
                let fill = next_fill;
                next_fill = next_fill.wrapping_add(1).max(1);
                // SAFETY: `ptr` valid for `size` bytes; re-fill so later M3
                // contamination checks have a marker.
                unsafe {
                    for b in 0..size {
                        ptr.add(b).write(fill);
                    }
                }
                live.push(Live {
                    ptr,
                    size,
                    align,
                    fill,
                });
            }
            Op::Dealloc(i) => {
                if !live.is_empty() {
                    let i = i % live.len();
                    let l = live.swap_remove(i);
                    let layout = Layout::from_size_align(l.size, l.align).expect("valid layout");
                    // SAFETY: `l.ptr` is a live block allocated with `layout`,
                    // freed exactly once here (the swap_remove drops it from the
                    // model), honoring the `dealloc` contract.
                    unsafe { alloc.dealloc(l.ptr, layout) };
                    if config.double_free {
                        // M2: a second dealloc of the same pointer must be a
                        // no-op that does not corrupt the allocator. Opt-in
                        // (`Config::double_free`) — a stronger-than-`GlobalAlloc`
                        // guarantee; a real malloc would corrupt here.
                        // SAFETY: intentional M2 exercise — for an allocator whose
                        // documented contract is that this is a no-op.
                        unsafe { alloc.dealloc(l.ptr, layout) };
                    }
                }
            }
            Op::Realloc { i, new_size } => {
                if !live.is_empty() {
                    let i = i % live.len();
                    let l = live[i];
                    let old_layout =
                        Layout::from_size_align(l.size, l.align).expect("valid layout");
                    // SAFETY: `l.ptr` is a live block allocated with `old_layout`;
                    // it is consumed on a non-null return per the realloc contract.
                    let new_ptr = unsafe { alloc.realloc(l.ptr, old_layout, new_size) };
                    if new_ptr.is_null() {
                        // Realloc failed: the old block is still live & valid.
                        continue;
                    }
                    assert_eq!((new_ptr as usize) % l.align, 0, "M1: realloc not aligned");
                    let keep = l.size.min(new_size);
                    // The preserved prefix must still hold the old fill byte.
                    // SAFETY: `new_ptr` is valid for `new_size >= keep` bytes.
                    unsafe {
                        for b in 0..keep {
                            assert_eq!(new_ptr.add(b).read(), l.fill, "realloc lost a prefix byte");
                        }
                    }
                    // Re-establish a fresh fill across the whole new extent so
                    // M3 contamination checks and later reallocs stay coherent
                    // (the grown tail is legitimately uninitialised otherwise).
                    let fill = next_fill;
                    next_fill = next_fill.wrapping_add(1).max(1);
                    // SAFETY: `new_ptr` is valid for `new_size` bytes.
                    unsafe {
                        for b in 0..new_size {
                            new_ptr.add(b).write(fill);
                        }
                    }
                    live[i] = Live {
                        ptr: new_ptr,
                        size: new_size,
                        align: l.align,
                        fill,
                    };
                }
            }
        }
    }

    // M3 at run end: every survivor still holds its own fill (no block was
    // silently clobbered by another live allocation).
    for l in &live {
        // SAFETY: `l.ptr` is live and valid for `l.size` bytes.
        unsafe {
            for b in 0..l.size {
                assert_eq!(l.ptr.add(b).read(), l.fill, "M3: live block clobbered");
            }
        }
    }

    // Free all survivors, then drop the model (M2: no double-free, no UAF in a
    // teardown walk).
    for l in &live {
        let layout = Layout::from_size_align(l.size, l.align).expect("valid layout");
        // SAFETY: `l.ptr` is a live block allocated with `layout`, freed exactly
        // once here.
        unsafe { alloc.dealloc(l.ptr, layout) };
    }
    drop(live);
}

#[cfg(feature = "proptest")]
mod strategy;
#[cfg(feature = "proptest")]
pub use strategy::op_strategy;

#[cfg(feature = "arbitrary")]
mod arbitrary_stream;
#[cfg(feature = "arbitrary")]
pub use arbitrary_stream::OpStream;
