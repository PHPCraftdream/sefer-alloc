//! Task #223: a Small route registration must no longer zero-fill the
//! ~40 KiB `SmallSidecar` through the allocator. The default backing is an
//! OS-zeroed anonymous VM span (`SmallSidecarBacking::Vmem`); the observed
//! initial state is the same documented all-zero pattern, and the
//! `sidecar_backing_counts_for_test` oracle proves the new path is taken
//! while the full-struct zero-fill `System` allocation (the historical
//! calloc-shaped path) does not happen. The `System` fallback is exercised
//! explicitly via `fail_vmem_reserve_for_test`, and producer
//! publish/issue + owner scan/drain traffic runs end to end on many
//! vmem-backed routes.
//!
//! ## Counterfactual sensitivity (the explicit-fill oracle)
//!
//! The byte-content assertions alone CANNOT go red if someone reintroduces
//! a full userspace zero-fill on the VM path (an unconditional
//! `write_bytes` leaves the bytes zero and the backing/heap counters
//! untouched). `SmallSidecar::explicit_sidecar_fill_words_for_test` — a
//! relaxed library counter bumped at EVERY userspace zero-fill site of the
//! sidecar payload (the `#[cfg(miri)]` full-span `write_bytes` in
//! `allocate`, and `initialize_at`'s per-word pending construction) — is
//! the oracle that does: on the VM path (non-miri) its delta across a
//! registration cycle must be EXACTLY 0, and on the forced heap-fallback
//! path (and on miri, on the VM path) it must be > 0.
#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals"
))]

use core::mem::size_of;
use std::sync::Mutex;

use sefer_alloc::registry::segment_route::{
    RouteDirectory, RouteKind, RouteRegistration, SmallSidecar,
};

const SEGMENT: usize = 4 * 1024 * 1024;
const GRANULES: usize = 262_144;
const MIN_BLOCK: usize = 16;
const WORDS: usize = 4096; // SidecarBitmap::WORDS: one u64 per 64 granules
const LEAVES: usize = 1024; // ClassLeaves leaf slots (4 KiB leaf / 16 B granule)
const PRESENT_WORDS: usize = 16; // ClassLeaves::PRESENT_WORDS (LEAVES / 64)
const CHUNKS: usize = 64; // ClassLeaves slab chunks (LEAVES / 16 chunk leaves)

/// Serialization mechanism for ALL counter-snapshot assertions in this
/// file: the counters (`sidecar_backing_counts_for_test`,
/// `explicit_sidecar_fill_words_for_test`, `system_totals_for_test`) are
/// PROCESS-lifetime relaxed statics shared with the rest of the test
/// binary, and libtest runs `#[test]` fns on parallel worker threads —
/// without a lock, another test's registration interleaves between a
/// snapshot and its matching delta read and contaminates the delta. The
/// `TEST_LOCK` mutex is the shared guard: every test in this file that
/// takes a counter snapshot holds it for the WHOLE body, so the deltas
/// observe only this test's own sidecar events.
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// EXHAUSTIVE all-zero observations over a sidecar whose storage was
/// recycled dirty memory (the `poison` trick from
/// `r6_b4_sidecar_registration_zero_state`): EVERY pending word (all 4096,
/// all bits clear), EVERY class code (all 262144 granules: None /
/// never-issued), every leaf-presence word (all zero), every mixed-leaf
/// slot (all null) and every chunk slot (all null). Under miri a reduced
/// sample keeps the same shape at a checked budget.
fn assert_zero_state(directory: &RouteDirectory, root: *mut u8, sidecar: &SmallSidecar) {
    assert_eq!(sidecar.mixed_leaves_for_test(), 0);
    // The pin's `pending_for_test` probe is used only by the miri sample.
    #[allow(unused_variables)]
    let pin = directory.lookup(root).unwrap();
    #[cfg(not(miri))]
    {
        // Every pending word: all bits clear.
        for word in 0..WORDS {
            assert_eq!(
                sidecar.pending_word_for_test(word),
                0,
                "pending word {word} not all-zero"
            );
        }
        // Every leaf-presence word: all bits clear.
        for word in 0..PRESENT_WORDS {
            assert_eq!(
                sidecar.leaf_presence_word_for_test(word),
                0,
                "presence word {word} not all-zero"
            );
        }
        // Every mixed-leaf slot: null pointer.
        for leaf in 0..LEAVES {
            assert_eq!(
                sidecar.mixed_leaf_slot_addr_for_test(leaf),
                0,
                "mixed slot {leaf} not null"
            );
        }
        // Every chunk slot: null pointer.
        for chunk in 0..CHUNKS {
            assert_eq!(
                sidecar.chunk_slot_addr_for_test(chunk),
                0,
                "chunk slot {chunk} not null"
            );
        }
        // Every class code: never issued (None).
        for granule in 0..GRANULES {
            let offset = (granule * MIN_BLOCK) as u32;
            assert_eq!(sidecar.class_at_for_test(offset), None, "class {granule}");
        }
    }
    #[cfg(miri)]
    {
        // Reduced sample under miri (same shape, checked budget): a
        // handful of pending words, presence words, slots and class codes.
        for word in [0usize, 1, WORDS / 2, WORDS - 1] {
            assert_eq!(
                sidecar.pending_word_for_test(word),
                0,
                "pending word {word}"
            );
        }
        for word in [0usize, PRESENT_WORDS / 2, PRESENT_WORDS - 1] {
            assert_eq!(
                sidecar.leaf_presence_word_for_test(word),
                0,
                "presence word {word}"
            );
        }
        for leaf in [0usize, 1, LEAVES / 2, LEAVES - 1] {
            assert_eq!(
                sidecar.mixed_leaf_slot_addr_for_test(leaf),
                0,
                "mixed {leaf}"
            );
        }
        for chunk in [0usize, CHUNKS / 2, CHUNKS - 1] {
            assert_eq!(sidecar.chunk_slot_addr_for_test(chunk), 0, "chunk {chunk}");
        }
        for granule in [0usize, 1, GRANULES / 2, GRANULES - 1] {
            let offset = (granule * MIN_BLOCK) as u32;
            let addr = root.with_addr(root.addr() + offset as usize);
            assert!(!pin.pending_for_test(addr), "pending bit {granule}");
            assert_eq!(sidecar.class_at_for_test(offset), None, "class {granule}");
        }
    }
}

fn register_small<'a>(directory: &'a RouteDirectory, root: *mut u8) -> RouteRegistration<'a> {
    // SAFETY: root is a live, SEGMENT-aligned reservation and the route
    // pointer is inside it, matching `register`'s contract.
    unsafe { directory.register(root, SEGMENT, root, 9, RouteKind::Small) }.unwrap()
}

fn seg_reservation() -> aligned_vmem::Reservation {
    aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap()
}

/// Path-activation oracle + EXHAUSTIVE all-zero observations on the default
/// (OS-zeroed span) path. Counterfactuals: (1) on the historical path the
/// registration made ONE System allocation of exactly
/// `size_of::<SmallSidecar>()` bytes (and zero-filled them), so the
/// unchanged System totals and the unchanged HEAP counter pair would be
/// red; (2) if the VM branch were changed to ALSO run a userspace zero-fill
/// (an unconditional full `write_bytes`), the byte contents and all other
/// counters would still pass — but `explicit_sidecar_fill_words_for_test`
/// would move off exactly 0 and this test would go red (under miri the
/// oracle instead demands a nonzero delta: the System-backed VM path there
/// fills explicitly through the single `explicit_zero_fill_words`
/// operation).
#[test]
fn vmem_backed_registration_skips_full_zero_fill() {
    let _guard = TEST_LOCK.lock().unwrap();
    let system_before = SmallSidecar::system_totals_for_test();
    let counts_before = SmallSidecar::sidecar_backing_counts_for_test();
    let fill_before = SmallSidecar::explicit_sidecar_fill_words_for_test();
    let directory = RouteDirectory::new();
    let reservation = seg_reservation();
    let root = reservation.as_ptr();
    // Recycled-dirty-memory pressure: free a poisoned same-size System
    // block immediately before registration, so a skipped initialization
    // would surface as observable garbage, not page-luck zeros.
    let poison = vec![0xA5_u8; size_of::<SmallSidecar>()];
    drop(poison);
    let route = register_small(&directory, root);
    let counts_after = SmallSidecar::sidecar_backing_counts_for_test();
    assert_eq!(counts_after.0 - counts_before.0, 1, "OS-zeroed path taken");
    assert_eq!(counts_after.2, counts_before.2, "heap fallback NOT taken");
    assert_eq!(counts_after.3, counts_before.3);
    let fill_registered = SmallSidecar::explicit_sidecar_fill_words_for_test();
    // ONE oracle, TWO correct cfg arms: under a real OS backend the VM span
    // path runs NO userspace fill (delta exactly 0); under miri the
    // System-backed VM path runs `explicit_zero_fill_words` and MUST move
    // the counter — a nonzero native delta, or a zero miri delta, is the
    // regression this oracle exists for.
    #[cfg(not(miri))]
    assert_eq!(
        fill_registered, fill_before,
        "explicit zero-fill counter MUST stay untouched on the OS-zeroed \
         path (nonzero here = a userspace fill was (re)introduced)"
    );
    #[cfg(miri)]
    assert!(
        fill_registered > fill_before,
        "miri's System-backed VM path must fill explicitly and account for \
         it (zero delta = the fill oracle lost its fill site)"
    );
    let system_after = SmallSidecar::system_totals_for_test();
    assert_eq!(
        system_after, system_before,
        "no System request at all for the sidecar (old path: +1 alloc of the full size)"
    );
    assert_zero_state(&directory, root, route.small_sidecar().unwrap());
    drop(route);
    let counts_released = SmallSidecar::sidecar_backing_counts_for_test();
    assert_eq!(
        counts_released.1 - counts_after.1,
        1,
        "span released exactly once"
    );
    assert_eq!(counts_released.3, counts_after.3);
    assert_eq!(SmallSidecar::system_totals_for_test(), system_before);
    assert_eq!(
        SmallSidecar::explicit_sidecar_fill_words_for_test(),
        fill_registered,
        "release must not fill either"
    );
}

/// The `System` fallback (forced via `fail_vmem_reserve_for_test`) keeps the
/// historical contract exactly: full in-place zero construction over
/// possibly-recycled dirty storage, System totals +one allocation of the
/// full sidecar size, release balances it. Counterfactual: the
/// explicit-fill counter MUST move on this path (delta exactly
/// `SidecarBitmap::WORDS` pending words) — a fallback that silently skipped
/// its zero construction would leave recycled garbage and a zero delta.
#[test]
fn heap_fallback_constructs_full_zero_state() {
    let _guard = TEST_LOCK.lock().unwrap();
    let system_before = SmallSidecar::system_totals_for_test();
    let counts_before = SmallSidecar::sidecar_backing_counts_for_test();
    let fill_before = SmallSidecar::explicit_sidecar_fill_words_for_test();
    SmallSidecar::fail_vmem_reserve_for_test(1);
    let directory = RouteDirectory::new();
    let reservation = seg_reservation();
    let root = reservation.as_ptr();
    let poison = vec![0xA5_u8; size_of::<SmallSidecar>()];
    drop(poison);
    let route = register_small(&directory, root);
    SmallSidecar::fail_vmem_reserve_for_test(0);
    let counts_after = SmallSidecar::sidecar_backing_counts_for_test();
    assert_eq!(counts_after.2 - counts_before.2, 1, "heap fallback taken");
    assert_eq!(counts_after.0, counts_before.0, "vmem path skipped");
    let fill_after = SmallSidecar::explicit_sidecar_fill_words_for_test();
    assert!(
        fill_after > fill_before,
        "explicit-fill counter must move on the heap fallback \
         (zero delta = the fallback skipped its zero construction)"
    );
    let system_after = SmallSidecar::system_totals_for_test();
    assert_eq!(system_after.0 - system_before.0, size_of::<SmallSidecar>());
    assert_eq!(system_after.1 - system_before.1, 0);
    assert_eq!(system_after.3 - system_before.3, 1);
    assert_zero_state(&directory, root, route.small_sidecar().unwrap());
    drop(route);
    let counts_released = SmallSidecar::sidecar_backing_counts_for_test();
    assert_eq!(
        counts_released.3 - counts_after.3,
        1,
        "heap free exactly once"
    );
    assert_eq!(counts_released.1, counts_after.1);
    let system_released = SmallSidecar::system_totals_for_test();
    assert_eq!(
        system_released.2 - system_after.2,
        size_of::<SmallSidecar>()
    );
    assert_eq!(system_released.4 - system_after.4, 1);
}

/// Producers publish / the OWNER drains, end to end, on many vmem-backed
/// routes — through the REAL protocol, not owner-local `AllocCore::dealloc`:
/// each segment registers a Small route on the new path, the producer
/// issues blocks via the route pin's `publish_small` (terminal publication),
/// and the OWNER scans/drains the sidecar bitmap and gets the published
/// frees back exactly once.
#[test]
fn vmem_backed_routes_publish_and_drain_on_many_segments() {
    let _guard = TEST_LOCK.lock().unwrap();
    let counts_before = SmallSidecar::sidecar_backing_counts_for_test();
    let fill_before = SmallSidecar::explicit_sidecar_fill_words_for_test();
    let iters = if cfg!(miri) { 2 } else { 16 };
    // Blocks each segment's producer issues + publishes, in issue order.
    const OFFSETS: [u32; 4] = [0, 16, 4096, 2 * 4096 + 16];
    for i in 0..iters {
        let directory = RouteDirectory::new();
        let reservation = seg_reservation();
        let root = reservation.as_ptr();
        let route = register_small(&directory, root);
        assert_eq!(
            SmallSidecar::sidecar_backing_counts_for_test().0 - counts_before.0,
            i + 1,
            "segment {i}: registration took the OS-zeroed path"
        );
        let sidecar = route.small_sidecar().unwrap();
        // Producer issue path (RouteRegistration → owner class leaves +
        // bitmap).
        for (j, offset) in OFFSETS.iter().enumerate() {
            let class = j as u8 % 3;
            assert!(
                route.prepare_small(*offset, class),
                "segment {i} prepare {offset}"
            );
            assert!(
                route.issue_small(*offset, class),
                "segment {i} issue {offset}"
            );
        }
        // Producer terminal publication through the ROUTE PIN (the real
        // foreign-free entry: directory lookup by block address, then the
        // pin's `publish_small`).
        for offset in OFFSETS {
            let pin = directory
                .lookup(root.with_addr(root.addr() + offset as usize))
                .expect("issued block must route to its segment");
            // SAFETY: each offset was issued once above and is transferred
            // exactly once here.
            assert!(
                unsafe { pin.publish_small(offset) },
                "segment {i} publish {offset}"
            );
        }
        // OWNER drain path: scan the sidecar bitmap and collect the
        // published frees exactly once, in bitmap order.
        let mut scan = sidecar
            .scan((OFFSETS[3] as usize + MIN_BLOCK).next_power_of_two())
            .unwrap();
        let mut records = Vec::new();
        while let Some(mut cut) = scan.next_cut() {
            while let Some(record) = cut.pop() {
                records.push((record.offset, record.class));
            }
        }
        let mut expected: Vec<(u32, u8)> = OFFSETS
            .iter()
            .enumerate()
            .map(|(j, o)| (*o, j as u8 % 3))
            .collect();
        expected.sort();
        records.sort();
        assert_eq!(records, expected, "segment {i}: drained frees");
        drop(scan);
        // Dropping the registration (last owner reference) unlinks and
        // releases the OS-zeroed span exactly once.
        drop(route);
        assert!(
            directory.lookup(root).is_none(),
            "segment {i}: route removed after last owner reference"
        );
    }
    let counts_after = SmallSidecar::sidecar_backing_counts_for_test();
    assert_eq!(
        counts_after.0 - counts_before.0,
        iters,
        "every segment registration took the OS-zeroed path"
    );
    assert_eq!(counts_after.2, counts_before.2, "no heap fallback");
    assert_eq!(
        counts_after.1 - counts_before.1,
        iters,
        "each dropped route released its span exactly once"
    );
    assert_eq!(counts_after.3, counts_before.3);
    // Same two-cfg oracle as `vmem_backed_registration_skips_full_zero_fill`:
    // native — no fill anywhere in the cycle; miri — exactly the per-
    // registration explicit fill (iters segments, each one fill operation).
    #[cfg(not(miri))]
    assert_eq!(
        SmallSidecar::explicit_sidecar_fill_words_for_test(),
        fill_before,
        "no userspace zero-fill across the whole many-segment cycle"
    );
    #[cfg(miri)]
    assert!(
        SmallSidecar::explicit_sidecar_fill_words_for_test() > fill_before,
        "miri: each of the {iters} registrations must fill its span explicitly"
    );
}
