#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals"
))]

//! Ph3b: kind-aware entry points — every public entry must answer "what kind
//! of block is this?" from the SEGMENT HEADER's `kind` byte
//! (`BlockKind` / `SegmentHeader::kind_at`), never from the caller-supplied
//! `Layout`.
//!
//! ## What this file pins
//!
//! Ph3b replaced every entry point's layout-derived kind answer with
//! [`BlockKind`]: scalar dealloc (`dealloc_own_base`'s shared
//! `small_free_guard`, including the new plain-`production` pre-route),
//! `dealloc_batch` (`RouteToLargeFree` → `self.core.dealloc`, the SAME
//! substrate call the scalar path makes), `realloc`'s Small→Large promo gate,
//! `AllocCore::dealloc`'s `dealloc_at_base` tail, and the non-`fastbin`
//! `alloc`/`alloc_batch` pair (`alloc_with_class`). The tests below drive
//! those entries across the size/align sweep and assert the SAME
//! physical-kind answer, using only numeric observers.
//!
//! ## Oracles (no raw-pointer / header observation of our own)
//!
//! - `HeapCore::dbg_kind_at_tag(ptr)` — the decoded `SegmentKind` as a tag
//!   (`0` Primordial, `1` Small, `2` Large, `3` Unknown). This is exactly the
//!   header byte `BlockKind::of` reads, exposed as a plain safe accessor (the
//!   same pattern `tests/kind_at_strict_decode.rs` relies on).
//! - `HeapCore::dbg_active_kind_census()` — `(small, large, exact)`: the live
//!   per-kind segment counts of THIS heap's `SegmentTable`, plus whether the
//!   `ActiveKindIndex` agrees with the headers bit-for-bit.
//! - `dbg_table_count` / `dbg_owner_id_for` (is the segment still
//!   registered?), `dbg_tcache_count` / `dbg_tcache_contains` (magazine
//!   residency), `dbg_is_free_for` (free-list bit), `dbg_live_count_for`
//!   (authoritative live-block count), `dbg_class_for` /
//!   `dbg_refill_n_for_class` (the class and the magazine depth a free may
//!   park to — read back instead of re-deriving the private `FREE_PARK_CAP`
//!   formula), `dbg_drain_sidecar_ingress` (owner-side reclaim).
//!
//! All of these are `pub` `#[doc(hidden)]` test seams reachable under
//! `production internals bench-internals`; nothing here dereferences a segment
//! header through a raw pointer of its own.
//!
//! ## Why the isolated `HeapCore` face and not `#[global_allocator]`
//!
//! Same rationale as `tests/r25_4_dealloc_batch_multi_flush_oracle.rs`: with
//! `SeferAlloc` installed, the libtest harness' own `Vec`/`String`/assertion
//! traffic allocates out of the same pool between an entry call and its
//! observation, perturbing per-segment `live_count` and the census. Claiming a
//! `HeapCore` through `HeapRegistry::claim()` and driving it directly keeps
//! every observer exact. `#[global_allocator]`-shaped smoke coverage of the
//! same entries lives in `tests/global_alloc_installed.rs` and
//! `tests/r8_global_box_provenance.rs` (run alongside this file).
//!
//! ## Task-item map
//!
//! 1. the size sweep over `alloc` / `alloc_zeroed` / own `dealloc` /
//!    `realloc` / `dealloc_batch` / `alloc_batch` / `fallback` / foreign free —
//!    `alloc_alloc_zeroed_and_own_dealloc_agree_on_one_kind_per_size`,
//!    `realloc_crosses_the_boundary_in_step_with_the_header_kind`,
//!    `fallback_served_blocks_freed_by_another_thread_keep_their_kind`, and
//!    `scalar_and_batch_frees_of_one_pattern_agree_on_every_observer`
//!    (`batch-api`-gated).
//! 2. realloc Small↔Large prefix + kind census —
//!    `realloc_crosses_the_boundary_in_step_with_the_header_kind`.
//! 3. realloc OOM → null, old block intact — `realloc_oom_returns_null_and_leaves_the_old_block_intact`.
//! 4. layout agreement inside the `GlobalAlloc` contract —
//!    `matching_layout_free_keeps_the_small_route_across_a_same_class_realloc`.
//!    The Large-with-small-layout divergence is UB by the
//!    `GlobalAlloc::dealloc` contract and is pinned RED by
//!    `tests/regression_hardened_large_kind_own_free.rs` (that file's module
//!    doc carries the counterfactual) — documented there, deliberately NOT
//!    duplicated here.
//! 5. scalar vs `dealloc_batch` identity —
//!    `scalar_and_batch_frees_of_one_pattern_agree_on_every_observer`
//!    (`batch-api`-gated).
//!
//! A sixth test covers the one remaining LEGITIMATELY reachable "physically
//! Large, but the dealloc layout classifies small" shape — the R14-4
//! medium→Large promotion grown in place by OPT-G — which is exactly what
//! Ph3b's `small_free_guard` pre-route and `dealloc_batch`'s
//! `RouteToLargeFree` exist for. It needs `medium-classes`, so it is compiled
//! out of a plain `production internals bench-internals` build rather than
//! silently vacuous.

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::segment_route::RouteDirectory;
use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};
use sefer_alloc::{SeferAlloc, SegmentLayout};

/// `dbg_kind_at_tag` tags (see `tests/kind_at_strict_decode.rs`).
const TAG_PRIMORDIAL: u8 = 0;
const TAG_SMALL: u8 = 1;
const TAG_LARGE: u8 = 2;

/// A purely-Large size: a dedicated `SEGMENT`-rounded (4 MiB) segment in every
/// feature combination this file is built under.
const LARGE_SIZE: usize = 4 * 1024 * 1024;

/// Bytes of prefix verified across a realloc move leg. Small enough to keep
/// the 4 MiB arm of the sweep cheap, large enough to prove the payload moved.
const PREFIX: usize = 512;

// The registry is a process-global static shared by every `HeapCore` in this
// binary, so the tests serialise on it exactly like
// `tests/regression_hardened_large_kind_own_free.rs` and
// `tests/r25_4_dealloc_batch_multi_flush_oracle.rs` do.
static SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

// ── helpers ───────────────────────────────────────────────────────────────

/// Claim a registry heap and empty its magazines, so per-test observers
/// (magazine counts, `live_count` deltas) start from a known state instead of
/// whatever a previous occupant of the recycled slot left behind.
fn claim_normalised() -> *mut HeapCore {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    // SAFETY: this thread exclusively owns the claimed slot until recycled.
    let heap: &mut HeapCore = unsafe { &mut *heap };
    heap.dbg_flush_all();
    heap as *mut HeapCore
}

/// Recycle a heap this test claimed and fully drained.
fn recycle(heap: *mut HeapCore) {
    // SAFETY: `heap` was claimed by this thread and every block it issued has
    // been freed again by its owner test before the recycle.
    unsafe { HeapRegistry::recycle(heap) };
}

/// The kind tag this build's OWN classifier predicts for `(size, align)`.
/// `SegmentLayout::class_for` is the public forwarder of the same
/// `SizeClasses::class_for` every entry uses to resolve the Small class
/// payload, so this is the INDEPENDENT side of the agreement the tests assert:
/// `BlockKind` takes its authority from the header, not from this prediction.
fn predicted_tag(size: usize, align: usize) -> u8 {
    let clamped = size.max(SegmentLayout::MIN_BLOCK);
    if SegmentLayout::class_for(clamped, align).is_some() {
        TAG_SMALL
    } else {
        TAG_LARGE
    }
}

/// The two `BlockKind` variants that share the small free path.
fn is_small_tag(tag: u8) -> bool {
    tag == TAG_SMALL || tag == TAG_PRIMORDIAL
}

fn assert_kind(heap: &HeapCore, ptr: *mut u8, expected: u8, what: &str) {
    let tag = heap.dbg_kind_at_tag(ptr);
    let ok = if expected == TAG_SMALL {
        is_small_tag(tag)
    } else {
        tag == expected
    };
    assert!(
        ok,
        "{what}: the segment header decoded to kind tag {tag}, expected {expected} — \
         BlockKind and the caller's classification disagree"
    );
}

fn census(heap: &HeapCore) -> (usize, usize, bool) {
    heap.dbg_active_kind_census()
}

fn table_count(heap: &HeapCore) -> u32 {
    heap.dbg_table_count()
}

fn fill_and_verify(ptr: *mut u8, len: usize, byte: u8) {
    // SAFETY: `ptr` is a live allocation of at least `len` writable bytes.
    unsafe { std::ptr::write_bytes(ptr, byte, len) };
    verify_bytes(ptr, len, byte);
}

fn verify_bytes(ptr: *mut u8, len: usize, byte: u8) {
    let word = u64::from_ne_bytes([byte; 8]);
    let words = len / 8;
    for i in 0..words {
        // SAFETY: inside the `len` bytes just filled.
        let got = unsafe { ptr.cast::<u64>().add(i).read_volatile() };
        assert_eq!(got, word, "a {len}-byte block was clobbered at word {i}");
    }
    for i in (words * 8)..len {
        // SAFETY: inside the `len` bytes just filled.
        let got = unsafe { ptr.add(i).read_volatile() };
        assert_eq!(got, byte, "a {len}-byte block was clobbered at byte {i}");
    }
}

fn assert_zeroed(ptr: *mut u8, len: usize) {
    let words = len / 8;
    for i in 0..words {
        // SAFETY: inside the `len` zeroed bytes.
        let got = unsafe { ptr.cast::<u64>().add(i).read_volatile() };
        assert_eq!(got, 0, "alloc_zeroed left word {i} of {len} non-zero");
    }
    for i in (words * 8)..len {
        // SAFETY: inside the `len` zeroed bytes.
        let got = unsafe { ptr.add(i).read_volatile() };
        assert_eq!(got, 0, "alloc_zeroed left byte {i} of {len} non-zero");
    }
}

/// Where a freed SMALL block may legitimately rest: on its segment's free list
/// (`is_free_for`) or in this heap's magazine (`tcache_contains`). The D1
/// invariant counts a magazine-resident block as live, so BOTH are correct
/// resting places and only their union is asserted (the same disjunction
/// `tests/r3_1_fallback_remote_free.rs` uses).
fn small_resting(heap: &HeapCore, class: usize, ptr: *mut u8) -> bool {
    heap.dbg_is_free_for(ptr) || heap.dbg_tcache_contains(class, ptr)
}

// ── task item 1: the size sweep over alloc / alloc_zeroed / own dealloc ────

/// Ph3b task item 1 — `alloc`, `alloc_zeroed` and the own-thread `dealloc`
/// must all resolve ONE physical kind per `(size, align)`, across the whole
/// swept range: 1 / 7 / 16 / 17 B, 256 B, 1024 B, `align > MIN_BLOCK`, the
/// real `SMALL_MAX` boundary on BOTH sides, and a pure-Large 4 MiB.
///
/// Oracle: the decoded header kind (`dbg_kind_at_tag`) plus the per-kind
/// segment census (`dbg_active_kind_census`) and the segment-table observer.
/// The sweep boundaries come from the PUBLIC `SegmentLayout::SMALL_MAX` rather
/// than being hardcoded, so the test follows the crate's real Small/Large
/// boundary (~253 KiB under `production`, 1 MiB under `medium-classes`).
#[test]
fn alloc_alloc_zeroed_and_own_dealloc_agree_on_one_kind_per_size() {
    let _g = SerialGuard::acquire();
    let raw = claim_normalised();
    // SAFETY: owned exclusively by this thread.
    let heap: &mut HeapCore = unsafe { &mut *raw };

    let sweep: [(usize, usize); 10] = [
        (1, 8),
        (7, 8),
        (16, 16),
        (17, 16),
        (256, 8),
        (1024, 8),
        (64, 128),                          // align > MIN_BLOCK
        (SegmentLayout::SMALL_MAX, 16),     // last Small class
        (SegmentLayout::SMALL_MAX + 1, 16), // first Large size
        (LARGE_SIZE, 8),                    // pure Large
    ];

    for (size, align) in sweep {
        let layout = Layout::from_size_align(size, align).unwrap();
        let expected = predicted_tag(size, align);
        let before = census(heap);

        // ── alloc ────────────────────────────────────────────────────────
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "alloc({size}, align {align}) returned null");
        assert_kind(heap, p, expected, "alloc");
        let opened = census(heap);
        assert_eq!(
            opened.1,
            before.1 + usize::from(expected == TAG_LARGE),
            "alloc({size}, align {align}): the Large-segment census must move by \
             exactly {} — a Small request must never open a Large segment and a \
             Large one must open exactly one",
            usize::from(expected == TAG_LARGE),
        );
        if expected == TAG_LARGE {
            assert_eq!(
                opened.0, before.0,
                "alloc({size}, align {align}): a Large request must not open a Small segment"
            );
        } else {
            assert!(
                opened.0 >= before.0,
                "alloc({size}, align {align}): a Small request must not close a Small segment"
            );
        }
        assert!(
            opened.2,
            "the active-kind index must stay in exact agreement"
        );

        fill_and_verify(p, size, 0xA7);

        // ── alloc_zeroed ─────────────────────────────────────────────────
        let z = heap.alloc_zeroed(layout);
        assert!(
            !z.is_null(),
            "alloc_zeroed({size}, align {align}) returned null"
        );
        assert_kind(heap, z, expected, "alloc_zeroed");
        let after_zeroed = census(heap);
        assert_eq!(
            after_zeroed.1,
            opened.1 + usize::from(expected == TAG_LARGE),
            "alloc_zeroed({size}, align {align}) must open exactly the number of \
             Large segments one more scalar alloc would — a second live block of \
             a Large-classified layout owns its own dedicated segment"
        );
        if expected == TAG_LARGE {
            assert_eq!(
                after_zeroed.0, opened.0,
                "alloc_zeroed({size}, align {align}): a Large request must not \
                 open a Small segment"
            );
        }
        assert!(after_zeroed.2, "the active-kind index must stay exact");
        assert_zeroed(z, size);

        // ── own-thread dealloc ───────────────────────────────────────────
        // The second (zeroed) block is still live here, so the baseline for
        // the free's census delta is taken right before it, not before the
        // pair of allocations.
        let census_before_free = census(heap);
        // SAFETY: `p` is live with the layout it was allocated with and is
        // freed exactly once here.
        unsafe { heap.dealloc(p, layout) };
        if expected == TAG_LARGE {
            assert!(
                heap.dbg_owner_id_for(p).is_none(),
                "own dealloc({size}, align {align}): the Large segment must be \
                 unregistered — BlockKind must route the free to the substrate"
            );
            assert_eq!(
                census(heap).1,
                census_before_free.1 - 1,
                "own dealloc({size}, align {align}): the kind-routed Large free \
                 must retire exactly one Large segment"
            );
        } else {
            let class = heap
                .dbg_class_for(layout)
                .expect("a small-classified layout must resolve a class");
            assert!(
                small_resting(heap, class, p),
                "own dealloc({size}, align {align}): a Small-kind block must land \
                 in the magazine or on its free list"
            );
        }
        // SAFETY: `z` is live with the same layout, freed exactly once here.
        unsafe { heap.dealloc(z, layout) };

        let end = census(heap);
        if expected == TAG_LARGE {
            assert_eq!(
                end, before,
                "the alloc/zeroed/free round at {size} B must restore the kind census exactly"
            );
        } else {
            assert_eq!(
                end.1, before.1,
                "the Small round at {size} B must not touch the Large census"
            );
            assert!(
                end.0 >= before.0 && end.2,
                "the Small round at {size} B must stay consistent"
            );
        }
    }

    recycle(raw);
}

// ── task items 1 & 2: realloc across the Small/Large boundary ─────────────

/// Ph3b task items 1 and 2 — the `realloc` entry in both directions.
///
/// Small→Large: the block moves to a dedicated Large segment (header kind
/// `Large`, exactly one more Large segment in the census) and the payload
/// prefix survives the move leg.
/// Large→Small: the Large segment is released (census back to baseline) and
/// the surviving prefix is the one the caller wrote.
///
/// Both directions are also the `realloc` arm of task item 1's sweep: the kind
/// answer must follow the NEW physical location, not the old layout.
#[test]
fn realloc_crosses_the_boundary_in_step_with_the_header_kind() {
    let _g = SerialGuard::acquire();
    let raw = claim_normalised();
    // SAFETY: owned exclusively by this thread.
    let heap: &mut HeapCore = unsafe { &mut *raw };

    let small = Layout::from_size_align(256, 8).unwrap();
    let large = Layout::from_size_align(LARGE_SIZE, 8).unwrap();
    let tiny = Layout::from_size_align(64, 8).unwrap();
    let baseline = census(heap);

    // ── Small -> Large ───────────────────────────────────────────────────
    let p = heap.alloc(small);
    assert!(!p.is_null(), "setup: 256 B alloc failed");
    fill_and_verify(p, 256, 0x5A);

    // SAFETY: `p` is live with `small`; ownership transfers to the result on a
    // non-null return.
    let grown = unsafe { heap.realloc(p, small, LARGE_SIZE) };
    assert!(!grown.is_null(), "Small->Large realloc must succeed");
    assert_kind(heap, grown, TAG_LARGE, "realloc(Small->Large)");
    let crossed = census(heap);
    assert_eq!(
        crossed.1,
        baseline.1 + 1,
        "Small->Large: exactly one Large segment must now be live"
    );
    assert!(
        crossed.2,
        "the active-kind index must stay exact across a boundary crossing"
    );
    // Only the SOURCE size is preserved across the move leg (`copy =
    // min(old_size, new_size)`); the grown tail is uninitialised by contract,
    // exactly like `GlobalAlloc::realloc`.
    verify_bytes(grown, 256, 0x5A);

    // SAFETY: `grown` is live with `large`, freed exactly once here.
    unsafe { heap.dealloc(grown, large) };
    assert_eq!(
        census(heap),
        baseline,
        "Small->Large round: freeing the Large segment must restore the census"
    );

    // ── Large -> Small ───────────────────────────────────────────────────
    let q = heap.alloc(large);
    assert!(!q.is_null(), "setup: 4 MiB alloc failed");
    assert_kind(heap, q, TAG_LARGE, "alloc(4 MiB)");
    fill_and_verify(q, PREFIX, 0x3C);
    // SAFETY: `q` is live with `large`; ownership transfers to the result.
    let shrunk = unsafe { heap.realloc(q, large, 64) };
    assert!(!shrunk.is_null(), "Large->Small realloc must succeed");
    assert_kind(heap, shrunk, TAG_SMALL, "realloc(Large->Small)");
    let back = census(heap);
    assert_eq!(
        back.1, baseline.1,
        "Large->Small: the dedicated Large segment must have been released"
    );
    assert!(
        back.2,
        "the active-kind index must stay exact across a shrink"
    );
    verify_bytes(shrunk, 64, 0x3C);

    // SAFETY: `shrunk` is live with `tiny`, freed exactly once here.
    unsafe { heap.dealloc(shrunk, tiny) };
    assert_eq!(
        census(heap).1,
        baseline.1,
        "Large->Small round: the Large census must end where it started"
    );

    recycle(raw);
}

// ── task item 3: realloc OOM ──────────────────────────────────────────────

/// Ph3b task item 3 — an unsatisfiable `realloc` must return null and leave
/// the old object completely usable: prefix intact, still freeable, heap still
/// serving new allocations, kind census and segment table unmoved.
///
/// Two OOM shapes are driven:
///  - an unsatisfiable SIZE whose `Layout` is itself invalid (`usize::MAX`,
///    `usize::MAX / 2`) — `realloc`'s move leg rejects the layout before
///    touching the source (the same oracle `tests/r8_large_alignment.rs` uses
///    for `usize::MAX`);
///  - a genuine ALLOCATION failure through the in-crate
///    registration-failure injector (`RouteDirectory::fail_next_registrations_for_test`),
///    which makes the move leg's destination `self.alloc(new_layout)` return
///    null. The control realloc immediately after it proves the null was the
///    injected refusal and not a permanently wedged heap.
#[test]
fn realloc_oom_returns_null_and_leaves_the_old_block_intact() {
    let _g = SerialGuard::acquire();
    let raw = claim_normalised();
    // SAFETY: owned exclusively by this thread.
    let heap: &mut HeapCore = unsafe { &mut *raw };

    let small = Layout::from_size_align(64, 8).unwrap();
    let big = Layout::from_size_align(LARGE_SIZE, 8).unwrap();
    let p = heap.alloc(small);
    assert!(!p.is_null(), "setup: 64 B alloc failed");
    fill_and_verify(p, 64, 0x5A);
    let baseline = census(heap);
    let baseline_table = table_count(heap);

    for new_size in [usize::MAX, usize::MAX / 2] {
        // SAFETY: `p` is live with `small`; a null return must leave it intact.
        let got = unsafe { heap.realloc(p, small, new_size) };
        assert!(
            got.is_null(),
            "realloc(64 -> {new_size}) must fail: the request is unsatisfiable"
        );
        verify_bytes(p, 64, 0x5A);
        assert_eq!(
            census(heap),
            baseline,
            "a failed realloc must not move the kind census (new_size = {new_size})"
        );
        assert_eq!(
            table_count(heap),
            baseline_table,
            "a failed realloc must not register a segment (new_size = {new_size})"
        );
    }

    // A genuine allocation failure: refuse the next route registrations so the
    // destination alloc of the move leg returns null. Under `medium-classes`
    // TWO legs register (the promotion leg, then the move leg's own alloc);
    // under plain `production` the promotion is compiled out and only the
    // move leg registers — hence the runtime count.
    RouteDirectory::fail_next_registrations_for_test(if HeapCore::dbg_promotion_compiled() {
        2
    } else {
        1
    });
    // SAFETY: `p` is live with `small`; a null return must leave it intact.
    let got = unsafe { heap.realloc(p, small, LARGE_SIZE) };
    assert!(
        got.is_null(),
        "realloc with a refused destination registration must fail cleanly"
    );
    verify_bytes(p, 64, 0x5A);
    assert_eq!(
        census(heap),
        baseline,
        "the refused realloc must leave the Large-segment census untouched"
    );
    assert_eq!(
        table_count(heap),
        baseline_table,
        "the refused realloc must not leave a half-registered segment behind"
    );

    // Control: with the injector exhausted the very same realloc must succeed,
    // proving the null above was the injected refusal and the heap recovered.
    // Only the SOURCE size (64 B) is preserved across the move leg.
    // SAFETY: `p` is live with `small`; ownership transfers on success.
    let moved = unsafe { heap.realloc(p, small, LARGE_SIZE) };
    assert!(
        !moved.is_null(),
        "control realloc must succeed after the OOM"
    );
    assert_kind(heap, moved, TAG_LARGE, "control realloc(64 -> 4 MiB)");
    verify_bytes(moved, 64, 0x5A);
    // SAFETY: `moved` is live with `big`, freed exactly once here.
    unsafe { heap.dealloc(moved, big) };

    // The heap still serves ordinary allocations afterwards.
    let fresh = heap.alloc(small);
    assert!(
        !fresh.is_null(),
        "the heap must still serve 64 B after the OOM"
    );
    // SAFETY: `fresh` is live with `small`, freed exactly once here.
    unsafe { heap.dealloc(fresh, small) };
    assert_eq!(census(heap), baseline, "the heap must end where it started");

    recycle(raw);
}

// ── task item 4: layout agreement inside the GlobalAlloc contract ─────────

/// Ph3b task item 4 — a dealloc whose layout differs from the allocation's,
/// but only INSIDE the `GlobalAlloc` contract, must not change the kind the
/// free takes.
///
/// The Large-with-small-layout divergence is UB by the `GlobalAlloc::dealloc`
/// contract (the layout no longer matches the allocation), so this test
/// deliberately does NOT construct it at the allocator level — it is already
/// pinned RED by `tests/regression_hardened_large_kind_own_free.rs`
/// (`large_ptr_small_layout_free_is_noop` and its branch-(A) siblings; that
/// file's module doc carries the counterfactual), and re-creating it here
/// would only duplicate UB.
///
/// What IS proved here, safely, are the two in-contract divergences:
///  (a) a sub-`MIN_BLOCK` request (`17` B) is clamped up to the 32 B class on
///      BOTH the allocation and the free, so the class derived from the layout
///      is exactly the class the block belongs to — the free parks in that
///      class's magazine instead of routing on the raw 17-byte size;
///  (b) an OPT-F same-class in-place realloc (`17` → `32`) changes the
///      caller's layout while leaving the block in the SAME class and the SAME
///      segment, so the post-realloc `dealloc` (whose layout is, by contract,
///      the new one) still takes the Small route for that very class.
#[test]
fn matching_layout_free_keeps_the_small_route_across_a_same_class_realloc() {
    let _g = SerialGuard::acquire();
    let raw = claim_normalised();
    // SAFETY: owned exclusively by this thread.
    let heap: &mut HeapCore = unsafe { &mut *raw };

    let a = Layout::from_size_align(17, 16).unwrap();
    let b = Layout::from_size_align(32, 16).unwrap();

    let class_a = heap
        .dbg_class_for(a)
        .expect("17 B @ align 16 must classify Small");
    let class_b = heap
        .dbg_class_for(b)
        .expect("32 B @ align 16 must classify Small");
    assert_eq!(
        class_a, class_b,
        "OPT-F precondition: 17 B and 32 B must share one size class"
    );

    // (a) the clamp: 17 B allocates AND frees through the 32 B class.
    let p = heap.alloc(a);
    assert!(!p.is_null(), "setup: alloc(17, 16) failed");
    assert_kind(heap, p, TAG_SMALL, "alloc(17, 16)");
    // SAFETY: `p` is live with `a`, freed exactly once here.
    unsafe { heap.dealloc(p, a) };
    assert!(
        heap.dbg_tcache_contains(class_a, p),
        "a clamped small free must park in class {class_a}'s magazine, not route \
         on the raw 17-byte layout size"
    );

    // (b) the same-class in-place realloc keeps the block exactly where it is.
    let q = heap.alloc(a);
    assert!(!q.is_null(), "setup: second alloc(17, 16) failed");
    // SAFETY: `q` is live with `a`; ownership transfers on a non-null return.
    let kept = unsafe { heap.realloc(q, a, 32) };
    assert!(!kept.is_null(), "same-class realloc must succeed");
    assert_eq!(
        kept, q,
        "OPT-F must keep a same-class resize in place — the block's kind and \
         class cannot change"
    );
    assert_kind(heap, kept, TAG_SMALL, "realloc(17 -> 32)");
    // SAFETY: `kept` is live with the POST-realloc layout `b` (the contract
    // layout for the resized allocation), freed exactly once here.
    unsafe { heap.dealloc(kept, b) };
    assert!(
        heap.dbg_tcache_contains(class_b, kept),
        "the post-realloc small free must park in the same class {class_b} — a \
         layout change that stays inside the contract must not change the kind route"
    );

    // Positive control for the same entry point: a genuinely Large block freed
    // with its exact Large layout takes the OTHER route — its segment is
    // unregistered instead of being parked in a magazine.
    let large = Layout::from_size_align(LARGE_SIZE, 8).unwrap();
    let big = heap.alloc(large);
    assert!(!big.is_null(), "setup: 4 MiB alloc failed");
    assert_kind(heap, big, TAG_LARGE, "alloc(4 MiB)");
    // SAFETY: `big` is live with `large`, freed exactly once here.
    unsafe { heap.dealloc(big, large) };
    assert!(
        heap.dbg_owner_id_for(big).is_none(),
        "a Large-kind block freed with its exact Large layout must route to the \
         substrate and unregister its segment"
    );

    recycle(raw);
}

// ── task item 1: the fallback and foreign-free entries ────────────────────

/// Ph3b task item 1 — the `fallback` and `dealloc foreign` entries.
///
/// The primordial fallback heap serves blocks before any thread binds a
/// registry slot; freeing one of them from ANOTHER thread is the foreign entry
/// (`SeferAlloc::dealloc` resolving to `ForeignNoBind` →
/// `HeapCore::publish_foreign`), which the owner then reclaims.
///
/// Oracle: the same numeric pair as everywhere else — the decoded header kind
/// (`dbg_kind_at_tag`) plus the kind census (`dbg_active_kind_census`) on the
/// fallback heap, and the owner's authoritative `dbg_is_free_for` /
/// `dbg_tcache_contains` / `dbg_drain_sidecar_ingress`.
#[test]
fn fallback_served_blocks_freed_by_another_thread_keep_their_kind() {
    let _g = SerialGuard::acquire();
    let layout = Layout::from_size_align(256, 8).unwrap();
    let large_layout = Layout::from_size_align(LARGE_SIZE, 8).unwrap();

    let (addresses, anchor, baseline) = HeapCore::dbg_with_fallback_for_test(|heap| {
        let baseline = census(heap);
        let anchor = heap.alloc(layout);
        assert!(!anchor.is_null(), "fallback alloc(256) failed");
        assert_kind(heap, anchor, TAG_SMALL, "fallback alloc(256)");

        let mut addresses = Vec::new();
        for _ in 0..8 {
            let p = heap.alloc(layout);
            assert!(!p.is_null(), "fallback alloc(256) failed");
            assert_kind(heap, p, TAG_SMALL, "fallback alloc(256)");
            addresses.push(p.expose_provenance());
        }

        // A Large block on the same fallback heap: a dedicated segment, and
        // its own-thread free is the Large route already covered above.
        let big = heap.alloc(large_layout);
        assert!(!big.is_null(), "fallback alloc(4 MiB) failed");
        assert_kind(heap, big, TAG_LARGE, "fallback alloc(4 MiB)");
        let opened = census(heap);
        assert_eq!(
            opened.1,
            baseline.1 + 1,
            "a 4 MiB fallback request must open exactly one Large segment"
        );
        assert!(
            opened.2,
            "the fallback heap's active-kind index must stay exact"
        );
        // SAFETY: `big` is live with `large_layout`, freed exactly once here.
        unsafe { heap.dealloc(big, large_layout) };
        let closed = census(heap);
        assert_eq!(
            closed, baseline,
            "the Large free must restore the fallback heap's kind census"
        );

        (addresses, anchor.expose_provenance(), baseline)
    })
    .expect("the production fallback heap must initialise");

    // The foreign entry: another thread frees blocks it never allocated.
    let remote = addresses.clone();
    std::thread::spawn(move || {
        for address in remote {
            // SAFETY: each block was uniquely transferred to this thread and is
            // freed exactly once, with its exact original Layout.
            unsafe {
                SeferAlloc::new().dealloc(std::ptr::with_exposed_provenance_mut(address), layout)
            };
        }
    })
    .join()
    .expect("the foreign-free thread must not panic");

    HeapCore::dbg_with_fallback_for_test(|heap| {
        let anchor = std::ptr::with_exposed_provenance_mut(anchor);
        let class = heap
            .dbg_class_for(layout)
            .expect("256 B @ align 8 must classify Small");
        // `publish_foreign` takes one of two paths for a fallback-owned block:
        // an IDLE fallback reclaims the free synchronously, a BUSY one (lock
        // held) publishes to the sidecar and the owner drains it later (the
        // shape `tests/r3_1_fallback_remote_free.rs` forces with
        // `dbg_dealloc_while_fallback_lock_held`). A drain here consumes the
        // second path; the first has nothing left to drain. Either way every
        // freed block must end at a real resting place.
        let drained = heap.dbg_drain_sidecar_ingress();
        for address in &addresses {
            let p = std::ptr::with_exposed_provenance_mut(*address);
            assert!(
                heap.dbg_is_free_for(p) || heap.dbg_tcache_contains(class, p),
                "a foreign-freed SMALL block must reach the same resting places \
                 an own-thread free would give it (drained {drained})"
            );
        }
        let after = census(heap);
        assert_eq!(
            after.1, baseline.1,
            "the foreign frees must not change the fallback heap's Large census"
        );
        assert!(
            after.2,
            "the fallback heap's active-kind index must stay exact"
        );

        // The anchor survives the whole exchange and is still freeable.
        fill_and_verify(anchor, 256, 0x6B);
        // SAFETY: `anchor` is the one block never transferred to the other
        // thread, freed exactly once here.
        unsafe { heap.dealloc(anchor, layout) };
    })
    .expect("the production fallback heap must stay initialised");
}

// ── task items 1 & 5: the batch entries (batch-api gated) ─────────────────

/// Ph3b task items 1 and 5 — `alloc_batch` / `dealloc_batch` must classify and
/// route by the SAME physical kind the scalar entries use, and the batched
/// free of one pattern must leave every observer identical to the scalar free
/// of the same pattern.
///
/// Three arms of ONE pattern (64 blocks of 256 B) are compared:
/// all-scalar, the task's split shape (half scalar, half
/// `dealloc_batch`), and all-`dealloc_batch`.
///
/// Gated on `batch-api`: that experimental feature is NOT part of `production`
/// (see its `Cargo.toml` doc), so under a plain
/// `production internals bench-internals` build this surface does not exist
/// and the test is compiled out rather than silently vacuous.
#[cfg(feature = "batch-api")]
#[test]
fn scalar_and_batch_frees_of_one_pattern_agree_on_every_observer() {
    let _g = SerialGuard::acquire();
    let raw = claim_normalised();
    // SAFETY: owned exclusively by this thread.
    let heap: &mut HeapCore = unsafe { &mut *raw };

    // The task's pattern: 64 blocks of 256 B. `256` B is a small class whose
    // `FREE_PARK_CAP` is the full magazine depth, so both the scalar and the
    // batched free park the same number of blocks and flush the rest.
    let layout = Layout::from_size_align(256, 8).unwrap();
    let n = 64usize;
    let class = heap
        .dbg_class_for(layout)
        .expect("256 B @ align 8 must classify Small");

    /// Every aggregate observer a kind-aware routing decision can move.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Observers {
        census: (usize, usize, bool),
        table: u32,
        magazine: u16,
        live: u32,
    }

    fn observe(heap: &HeapCore, blocks: &[*mut u8], class: usize) -> Observers {
        Observers {
            census: census(heap),
            table: table_count(heap),
            magazine: heap.dbg_tcache_count(class),
            live: heap
                .dbg_live_count_for(blocks[0])
                .expect("the pattern's segment must be small and registered"),
        }
    }

    fn alloc_pattern(heap: &mut HeapCore, layout: Layout, n: usize) -> Vec<*mut u8> {
        let mut blocks = Vec::with_capacity(n);
        for _ in 0..n {
            let p = heap.alloc(layout);
            assert!(!p.is_null(), "pattern alloc returned null");
            blocks.push(p);
        }
        blocks
    }

    let mut arms: Vec<(&str, Vec<*mut u8>, Observers, Observers)> = Vec::new();

    // Arm 1 — every free through the SCALAR entry.
    {
        let blocks = alloc_pattern(heap, layout, n);
        let before = observe(heap, &blocks, class);
        // SAFETY: every entry came from this heap with `layout`, freed once.
        for &p in &blocks {
            unsafe { heap.dealloc(p, layout) };
        }
        let after = observe(heap, &blocks, class);
        arms.push(("scalar", blocks, before, after));
        heap.dbg_flush_all();
    }

    // Arm 2 — the task's SPLIT shape: half the pattern freed scalar-wise, half
    // through `dealloc_batch`.
    {
        let blocks = alloc_pattern(heap, layout, n);
        let half = n / 2;
        let before = observe(heap, &blocks, class);
        // SAFETY: the first `half` entries are live blocks of this heap.
        for &p in &blocks[..half] {
            unsafe { heap.dealloc(p, layout) };
        }
        // SAFETY: the remaining entries are live blocks of this heap, each
        // freed exactly once, in one well-formed batched call.
        unsafe { heap.dealloc_batch(layout, &blocks[half..]) };
        let after = observe(heap, &blocks, class);
        arms.push(("half/half", blocks, before, after));
        heap.dbg_flush_all();
    }

    // Arm 3 — every free through the BATCH entry.
    {
        let blocks = alloc_pattern(heap, layout, n);
        let before = observe(heap, &blocks, class);
        // SAFETY: every entry is a live block of this heap, freed once.
        unsafe { heap.dealloc_batch(layout, &blocks) };
        let after = observe(heap, &blocks, class);
        arms.push(("batch", blocks, before, after));
        heap.dbg_flush_all();
    }

    let scalar_after = arms[0].3.clone();
    for (name, blocks, before, after) in &arms {
        // Non-vacuity: the free really did park and really did flush.
        assert!(
            after.magazine > 0,
            "arm {name}: the pattern free must leave blocks magazine-resident"
        );
        // D1 identity: `magazine` blocks stayed live, the rest were flushed
        // (one `dec_live` each). A misrouted batch entry would leave one of
        // them issued or double-count it, and this sum would break.
        let flushed = before.live - after.live;
        assert_eq!(
            after.magazine as usize + flushed as usize,
            n,
            "arm {name}: {parked} magazine-resident + {flushed} flushed must \
             account for all {n} freed blocks",
            parked = after.magazine
        );
        // Every freed block must be at a real resting place — never still
        // issued, never lost.
        for &p in blocks {
            assert!(
                small_resting(heap, class, p),
                "arm {name}: a freed block must reach a magazine or free-list \
                 resting place"
            );
        }
        // Identity with the fully-scalar run on every aggregate observer.
        assert_eq!(
            after.census, scalar_after.census,
            "arm {name}: the kind census must match the fully-scalar run"
        );
        assert_eq!(
            after.table, scalar_after.table,
            "arm {name}: the segment-table count must match the fully-scalar run"
        );
        assert_eq!(
            after.magazine, scalar_after.magazine,
            "arm {name}: the magazine depth must match the fully-scalar run"
        );
        assert_eq!(
            after.live, scalar_after.live,
            "arm {name}: the authoritative live count must match the fully-scalar run"
        );
    }

    // The `alloc_batch` entry must classify by the same header kind as the
    // scalar `alloc`, on both sides of the Small/Large boundary, and its
    // batched free must round-trip the census.
    for (size, align) in [(256usize, 8usize), (LARGE_SIZE, 8)] {
        let layout = Layout::from_size_align(size, align).unwrap();
        let expected = predicted_tag(size, align);
        let before = census(heap);
        heap.dbg_flush_all();
        let mut out = [std::ptr::null_mut(); 4];
        let filled = heap.alloc_batch(layout, &mut out);
        assert_eq!(filled, 4, "alloc_batch({size}) under-filled");
        for p in out {
            assert!(!p.is_null(), "alloc_batch({size}) returned a null slot");
            assert_kind(heap, p, expected, "alloc_batch");
        }
        let opened = census(heap);
        // `alloc_batch_large` loops the substrate `alloc`, so a Large batch
        // opens one dedicated segment PER SLOT — exactly what the same number
        // of scalar `alloc` calls would do.
        let expected_large_delta = if expected == TAG_LARGE { out.len() } else { 0 };
        assert_eq!(
            opened.1,
            before.1 + expected_large_delta,
            "alloc_batch({size}) must open exactly the Large segments the \
             scalar entry would ({expected_large_delta})"
        );
        assert!(opened.2, "the active-kind index must stay exact");
        // SAFETY: every slot came from `alloc_batch` with `layout`, freed once.
        unsafe { heap.dealloc_batch(layout, &out) };
        assert_eq!(
            census(heap),
            before,
            "alloc_batch + dealloc_batch must round-trip the kind census"
        );
    }

    recycle(raw);
}

// ── Ph3b's own headline case: physically Large, small-classifying layout ───

/// Ph3b's own headline shape — a block that PHYSICALLY lives in a Large
/// segment while its dealloc layout still classifies Small.
///
/// That shape is legitimately reachable (no contract violation) only through
/// the R14-4 medium→Large promotion (task #289) followed by OPT-G's in-place
/// grow: the promotion diverts the block to a dedicated Large segment at the
/// 256 KiB threshold, OPT-G then grows it in place up to `SMALL_MAX`, and the
/// `GlobalAlloc`-correct dealloc layout for the GROWN size classifies small
/// under `medium-classes`. Ph3b's `small_free_guard` pre-route and
/// `dealloc_batch`'s `RouteToLargeFree` are exactly what keep that free on the
/// kind-keyed Large path instead of the magazine.
///
/// Gated on `medium-classes`; the runtime `dbg_promotion_compiled()` check
/// additionally skips the R15-3 zero-headroom exclusion (`exact-span-large`
/// without the `large-reserved-capacity`/`numa-aware` exception), the same
/// non-vacuous-but-passing idiom
/// `tests/r17_4_inplace_grown_large_dealloc_routes_by_kind.rs` uses.
#[cfg(feature = "medium-classes")]
#[test]
fn promoted_then_grown_large_block_routes_its_free_by_kind_not_by_layout() {
    if !HeapCore::dbg_promotion_compiled() {
        eprintln!(
            "promotion is compiled out in this build (R15-3 zero-headroom \
             exclusion) — the Large-with-small-layout shape is unreachable"
        );
        return;
    }

    let _g = SerialGuard::acquire();
    let raw = claim_normalised();
    // SAFETY: owned exclusively by this thread.
    let heap: &mut HeapCore = unsafe { &mut *raw };

    let start = Layout::from_size_align(64 * 1024, 8).unwrap();
    let promote_to = 324 * 1024; // past the 256 KiB promotion threshold
    let grown_to = 512 * 1024; // still under medium-classes' 1 MiB SMALL_MAX
    let grown = Layout::from_size_align(grown_to, 8).unwrap();

    let baseline = census(heap);
    let p = heap.alloc(start);
    assert!(!p.is_null(), "setup: 64 KiB alloc failed");
    fill_and_verify(p, 64 * 1024, 0x11);

    // SAFETY: `p` is live with `start`; ownership transfers on a non-null return.
    let promoted = unsafe { heap.realloc(p, start, promote_to) };
    assert!(!promoted.is_null(), "the promoting realloc must succeed");
    assert_ne!(
        promoted, p,
        "promotion must move the block to a Large segment"
    );
    assert_kind(heap, promoted, TAG_LARGE, "realloc(64 KiB -> 324 KiB)");
    let promoted_census = census(heap);
    assert_eq!(
        promoted_census.1,
        baseline.1 + 1,
        "the promotion must have opened exactly one Large segment"
    );

    let promote_layout = Layout::from_size_align(promote_to, 8).unwrap();
    // SAFETY: `promoted` is live with `promote_layout`.
    let grown_ptr = unsafe { heap.realloc(promoted, promote_layout, grown_to) };
    assert!(!grown_ptr.is_null(), "the in-place grow must succeed");
    assert_eq!(
        grown_ptr, promoted,
        "OPT-G must grow the promoted block IN PLACE inside its Large segment"
    );
    assert_kind(
        heap,
        grown_ptr,
        TAG_LARGE,
        "realloc(324 KiB -> 512 KiB) on a promoted block",
    );
    verify_bytes(grown_ptr, PREFIX, 0x11);

    // THE Ph3b assertion: a 512 KiB layout classifies SMALL
    // (`class_for(512 KiB, 8) == Some`), yet the block lives in a Large
    // segment — the free must route by the header's kind and actually release
    // the segment, not park the Large payload address in a small magazine.
    assert!(
        heap.dbg_class_for(grown).is_some(),
        "precondition: the post-grow layout must classify SMALL — that is what \
         makes this the kind-vs-layout disagreement Ph3b resolves"
    );
    // SAFETY: `grown_ptr` is live with `grown`, freed exactly once here.
    unsafe { heap.dealloc(grown_ptr, grown) };
    assert!(
        heap.dbg_owner_id_for(grown_ptr).is_none(),
        "PH3B PRE-ROUTE BROKEN: the promoted-and-grown Large block freed with a \
         small-classifying layout must reach the substrate Large free and \
         unregister its segment (a magazine push here would read and write the \
         Large payload as bitmap state and leak the segment)"
    );

    // The batch twin of the same shape (Ph3b fix B1: `RouteToLargeFree` must go
    // to `self.core.dealloc`, the SAME substrate call the scalar path makes).
    #[cfg(feature = "batch-api")]
    {
        let q = heap.alloc(start);
        assert!(!q.is_null(), "setup: second 64 KiB alloc failed");
        fill_and_verify(q, 64 * 1024, 0x22);
        // SAFETY: `q` is live with `start`; ownership transfers on success.
        let promoted2 = unsafe { heap.realloc(q, start, promote_to) };
        assert!(
            !promoted2.is_null(),
            "the second promoting realloc must succeed"
        );
        assert_kind(heap, promoted2, TAG_LARGE, "second promotion");
        // SAFETY: `promoted2` is live with `promote_layout`.
        let grown2 = unsafe { heap.realloc(promoted2, promote_layout, grown_to) };
        assert!(!grown2.is_null(), "the second in-place grow must succeed");
        assert_eq!(grown2, promoted2, "OPT-G must grow in place again");
        verify_bytes(grown2, PREFIX, 0x22);
        // SAFETY: `grown2` is the single live block of this heap issued with
        // `grown`, freed exactly once in one well-formed batched call.
        unsafe { heap.dealloc_batch(grown, &[grown2]) };
        assert!(
            heap.dbg_owner_id_for(grown2).is_none(),
            "PH3B FIX B1 BROKEN: `dealloc_batch` must route a physically-Large \
             block to the substrate Large free even when its layout classifies \
             small — the scalar and batched entries must not drift apart"
        );
    }

    assert_eq!(
        census(heap).1,
        baseline.1,
        "the kind-routed frees must restore the Large-segment census"
    );

    recycle(raw);
}
