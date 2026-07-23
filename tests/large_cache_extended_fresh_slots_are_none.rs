//! R14-1 (task #286) — postcondition test for the explicit typed
//! initialisation added to `reserve_large_cache_extension`
//! (`src/alloc_core/large_cache_extended.rs`).
//!
//! ## What this guards
//!
//! Three independent Round 13 reviews flagged that `reserve_large_cache_extension`
//! materialised `LargeCacheExtension { slots: [Option<CachedLarge>; 32] }` by
//! reserving OS-zeroed pages (`aligned_vmem::leak_zeroed_pages`) and casting
//! them directly to `*mut LargeCacheExtension`, WITHOUT ever writing a real,
//! compiler-constructed value into them. The code (and every subsequent
//! `slots.iter().position(|s| s.is_none())` scan) relied on all-zero bytes
//! being a valid `None` for `Option<CachedLarge>`. That is NOT a language
//! guarantee: `CachedLarge` is a bag of bare `*mut u8`/`usize`/`u64` fields
//! with no reserved niche the compiler is obligated to place there, so
//! whether `Option`'s all-zero encoding happens to equal `None` is an
//! unspecified, rustc-version-dependent implementation detail of
//! `repr(Rust)`, not something sound code may depend on.
//!
//! The fix makes `reserve_large_cache_extension` `ptr::write` a real,
//! compiler-constructed `[None; LARGE_CACHE_EXTENDED_SLOTS]` value into the
//! reserved pages before the pointer is ever returned/published. This test
//! is the postcondition check for that fix: immediately after the sidecar
//! first materialises (the moment the 9th distinct Large size overflows the
//! base 8 slots), every one of the 32 extension slots must read back as
//! `None` via the existing `dbg_large_cache_extended_slot_sizes` test seam.
//!
//! ## Counterfactual (documented per project convention — see CLAUDE.md
//! "Between phases" / Round 12-13 precedent for this class of gap)
//!
//! This test does NOT distinguish the fix from the old behaviour on today's
//! rustc/target: `leak_zeroed_pages` DOES currently hand back all-zero bytes,
//! and an all-zero `Option<CachedLarge>` DOES currently happen to decode as
//! `None` on this compiler's actual (unspecified) niche layout — so this
//! assertion would ALSO pass against the pre-fix code. It is a regression
//! guard against a future signature change to `reserve_large_cache_extension`
//! (e.g. someone removing the `ptr::write` believing OS-zero is "obviously"
//! enough), not a UB detector: miri/the type system cannot observe an
//! unspecified-layout coincidence breaking, only a real conformance bug that
//! actually reads `Some` where `None` was expected. See the Round 12/13
//! precedent in this project for the same honest caveat on comparable
//! zeroed-sidecar tests.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "large-cache-extended"
))]

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

fn layout(bytes: usize) -> Layout {
    Layout::from_size_align(bytes, 8).unwrap()
}

/// Same runtime-computed, density-agnostic size list as
/// `tests/large_cache_extended_materializes_on_overflow.rs` (R12-14
/// convention: never hardcode sizes that could classify differently under
/// `medium-classes-wide`/`exact-span-large`). 9 distinct non-aliasing sizes
/// is the minimum that provably overflows the base 8 slots by exactly 1,
/// forcing first materialisation of the extension sidecar.
fn large_test_sizes() -> Vec<usize> {
    let segment = SegmentLayout::SEGMENT;
    let small_max_class = AllocCore::dbg_small_class_count() - 1;
    let small_max = AllocCore::dbg_block_size(small_max_class);
    let mut n = (2 * small_max).div_ceil(segment).max(1) * segment;
    let mut sizes = Vec::with_capacity(9);
    for _ in 0..9 {
        sizes.push(n);
        n = (2 * n + 1).div_ceil(segment) * segment;
    }
    sizes
}

/// Immediately after the extension sidecar first materialises (on the 9th
/// distinct deposit), every one of the 32 extension slots must read back as
/// `None` — not just the ONE slot the 9th deposit actually writes, but ALL
/// 32, including the 31 the deposit never touches. A stray `Some` in any of
/// those 31 would mean the typed initialisation this task adds is either
/// missing or wrong (e.g. writing fewer than `LARGE_CACHE_EXTENDED_SLOTS`
/// entries).
#[test]
fn all_extension_slots_are_none_immediately_after_materialisation() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None); // isolate slot-count effect from budget

    assert!(
        !ac.dbg_large_cache_extension_materialised(),
        "extension must start unmaterialised"
    );

    // Deposit exactly 9 distinct sizes: fills the base 8, forces the 9th
    // into the extension, which materialises it for the first time here.
    let sizes = large_test_sizes();
    for &bytes in &sizes {
        let l = layout(bytes);
        let p = ac.alloc(l);
        assert!(
            !p.is_null(),
            "alloc of {bytes} bytes failed -- unexpected OOM on this host"
        );
        // SAFETY (R6-MS-1/2): pointer from the alloc immediately above,
        // live, freed exactly once here (deposits it into the large cache).
        unsafe { ac.dealloc(p, l) };
    }

    assert!(
        ac.dbg_large_cache_extension_materialised(),
        "extension must have materialised after 9 distinct non-aliasing deposits"
    );

    // Postcondition: exactly ONE extension slot holds the 9th size (the only
    // deposit that overflowed into the extension); the remaining 31 must all
    // be `None` -- the direct behavioural signature of the typed
    // initialisation this task adds.
    let ext_sizes = ac.dbg_large_cache_extended_slot_sizes();
    let occupied: Vec<usize> = ext_sizes
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.map(|_| i))
        .collect();
    assert_eq!(
        occupied.len(),
        1,
        "expected exactly 1 occupied extension slot (the 9th deposit), found {} \
         occupied at indices {occupied:?} -- a fresh sidecar must start all-`None`",
        occupied.len()
    );
    let none_count = ext_sizes.iter().filter(|s| s.is_none()).count();
    assert_eq!(
        none_count, 31,
        "expected 31 of the 32 fresh extension slots to read back `None` \
         immediately after materialisation, found {none_count}"
    );
}
