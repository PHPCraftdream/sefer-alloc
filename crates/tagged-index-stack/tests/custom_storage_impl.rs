//! [`StackStorage`] is documented as "intentionally OPEN to external
//! implementation" (slot-resident links in caller-owned storage is the whole
//! design point), and the crate blanket-implements [`StackOps`] for every
//! `S: StackStorage<B> + ?Sized` — precisely so `&dyn StackStorage` works.
//! Every other test in this crate exercises only [`ArrayIndexStack`], so
//! neither claim was ever exercised by a real second implementor. This file
//! pins both: a small `Vec`-backed `StackStorage` impl used directly, and the
//! same storage driven through `&dyn StackStorage<16>` to pin the blanket
//! impl's `?Sized` coverage.
//!
//! It also pins the shared-storage hazard class and its current detection
//! coverage. The canonical statement of that inventory is the
//! [`StackStorage`] trait doc's "The shared-storage hazard class" section —
//! this doc points there rather than re-deriving it. In file order: the
//! shared-head shape over CUSTOM implementors (round-11 @oh finding, flipped
//! to `#[should_panic]` by round-13's self-loop detector; the former
//! round-12 `ArrayIndexStack::head()` parasite test moved out as the
//! compile-fail fixture `tests/compile_fail/array_index_stack_head/` once
//! `ArrayIndexStack` stopped implementing `StackStorage`, so the shared-head
//! shapes are covered here by
//! `two_implementor_values_sharing_one_head_still_double_issue` only), that
//! detector's
//! LIMIT (a hand-crafted acyclic forgery still double-issues silently), and
//! the shared-LINK-STORAGE variant, which no detector catches at all
//! (round-13 P2-2). Round-15 adds three pins on shapes the inventory
//! previously missed or never covered: the ONE-implementor
//! internally-disagreeing-storage shape (inventory shape 1, which had NO
//! automated coverage before round 15 — the round-11 test pins
//! shape 2; round-15 P4-2), temporal rebinding of a live head into fresh
//! links (inventory shape 4: first pop silently leaks, second pop panics;
//! round-15 P3-3), and shape 3's ONE-value form — two
//! different-width bindings over one backing inside a single implementor
//! value (round-15 P3-4).
//!
//! Since the 2026-09-01 `unsafe trait` conversion, EVERY implementor in
//! this file carries an `unsafe impl`: the five hazard tests pin the
//! runtime guard's catch/miss boundary UNDER AN ACKNOWLEDGED-BROKEN
//! CONTRACT (each `unsafe impl` names exactly which `# Safety` clause it
//! violates), while `VecStorage` is the correct-implementor reference
//! model. The compile-PASS side is pinned by
//! `vec_backed_storage_push_pop_round_trips` +
//! `push_pop_through_dyn_storage`: a correct `unsafe impl` compiles and
//! behaves correctly.

#![cfg(not(loom))]

use core::sync::atomic::{AtomicU32, Ordering};

use tagged_index_stack::{ArrayLinks, StackHead, StackOps, StackStorage, TAIL};

/// A `Vec`-backed [`StackStorage`] implementation, deliberately NOT
/// [`ArrayLinks`]: heap-allocated rather than an owned array, otherwise
/// upholding the same ordering contract (`Acquire` load, `Release` store).
struct VecStorage {
    head: StackHead<16>,
    next: Vec<AtomicU32>,
}

impl VecStorage {
    fn new(n: usize) -> Self {
        Self {
            head: StackHead::new(),
            next: (0..n).map(|_| AtomicU32::new(0)).collect(),
        }
    }
}

// SAFETY: upholds the whole contract (the reference model for a correct
// implementor): the struct owns its StackHead privately (one binding per
// head, no other route to it); load_next/store_next touch the same `next`
// Vec cell per index for its whole life (stable 1:1 mapping, coherence
// holds); no other binding exists over these cells; load_next answers only
// what a push stored (TAIL or an in-range index) from a dedicated cell;
// head() returns &self.head every call.
unsafe impl StackStorage<16> for VecStorage {
    fn head(&self) -> &StackHead<16> {
        &self.head
    }

    fn load_next(&self, index: u32) -> u32 {
        self.next[index as usize].load(Ordering::Acquire)
    }

    fn store_next(&self, index: u32, next: u32) {
        self.next[index as usize].store(next, Ordering::Release);
    }
}

/// An external, non-`ArrayLinks` [`StackStorage`] implementor works exactly
/// like the owned-array type does: push/pop round-trips and LIFO order hold.
#[test]
fn vec_backed_storage_push_pop_round_trips() {
    let storage = VecStorage::new(8);

    for i in 0..4u32 {
        storage.push_index(i);
    }
    let mut got = Vec::new();
    while let Some(i) = storage.pop_index() {
        got.push(i);
    }
    assert_eq!(
        got,
        vec![3, 2, 1, 0],
        "LIFO order over a non-ArrayLinks storage"
    );
    assert_eq!(storage.pop_index(), None);
}

/// The [`StackOps`] blanket impl covers `S: StackStorage<B> + ?Sized`, so
/// `&dyn StackStorage<16>` must compile and behave identically — this pins
/// that coverage of unsized implementors, part of the frozen 0.1.0 surface.
#[test]
fn push_pop_through_dyn_storage() {
    let vec_storage = VecStorage::new(4);
    let storage_dyn: &dyn StackStorage<16> = &vec_storage;

    storage_dyn.push_index(2);
    assert_eq!(storage_dyn.pop_index(), Some(2));
    assert_eq!(storage_dyn.pop_index(), None);
}

/// A borrowed-head [`StackStorage`] implementor: the head reference and the
/// link array are supplied independently at construction, so TWO values can
/// share one [`StackHead`] while each carries its OWN links — the exact
/// shape the [`StackStorage`] trait doc's rule 1 forbids and nothing (the
/// type system, the blanket impl, or a runtime guard) prevents.
struct SharedHeadView<'a> {
    head: &'a StackHead<16>,
    links: &'a ArrayLinks<64>,
}

// SAFETY: DELIBERATE contract violation — clause 1 (one live binding per
// head): the borrowed head is handed to TWO live implementor values with
// different links.
unsafe impl StackStorage<16> for SharedHeadView<'_> {
    fn head(&self) -> &StackHead<16> {
        self.head
    }

    fn load_next(&self, index: u32) -> u32 {
        self.links.load_next(index)
    }

    fn store_next(&self, index: u32, next: u32) {
        self.links.store_next(index, next)
    }
}

/// KNOWN, INTENTIONAL limitation — caller/implementor-enforced, NOT
/// compiler-enforced (the [`StackStorage`] trait doc's rule 1). Round-11
/// @oh review, finding P2-1; FLIPPED by the
/// round-13 @oh review, finding P2-1.
///
/// The hazard: two separately constructed, individually rule-coherent
/// `StackStorage` values whose `head()` methods return the SAME
/// [`StackHead`] while their links differ compile without a single
/// warning. Popping through the second value reads links from the WRONG
/// backing: the first pop hands back the real head index, and every pop
/// after that used to hand back the wrong backing's zero-initialised
/// answer — `0`, which was never pushed through ANY implementor —
/// silently, forever.
///
/// Since round-13 the second pop PANICS instead: `via_b`'s
/// zero-initialised links answer `load_next(0)` with `0` while the popped
/// index IS `0` — a self-loop a contract-abiding chain can never contain —
/// and `pop_index`'s release-active rule-4 guard fires. This test pins the
/// GUARD FIRING (the strict improvement from silent corruption to a loud
/// panic), not the hazard class closing: what such a shape still evades is
/// inventoried in the trait doc's "The shared-storage hazard class"
/// section, and pinned by the two tests below. The name keeps the round-11
/// finding's shape description; only the pinned behavior changed. If a
/// future structural fix stops this shape from compiling, this test breaks
/// by design and the trait doc's rule 1 must be updated with it.
#[test]
#[should_panic(expected = "self-loop, corrupting the free-list into a cycle")]
fn two_implementor_values_sharing_one_head_still_double_issue() {
    let head = StackHead::<16>::new();
    let links_a = ArrayLinks::<64>::new();
    let links_b = ArrayLinks::<64>::new();

    let via_a = SharedHeadView {
        head: &head,
        links: &links_a,
    };
    let via_b = SharedHeadView {
        head: &head,
        links: &links_b,
    };

    via_a.push_index(1);

    // First pop through `via_b` is still legitimate: it IS the head. The
    // guard only fires on the SECOND pop, when the wrong backing's
    // zero-initialised `load_next(0)` answer coincides with the popped
    // index itself.
    assert_eq!(via_b.pop_index(), Some(1));
    // Second pop: self-loop (next == index == 0) — panics under the
    // round-13 guard; before it, this silently returned the phantom `0`
    // forever.
    let _ = via_b.pop_index();
}

/// A self-sufficient implementor that OWNS its head and its links — the
/// same shape a hand-rolled third-party `StackStorage` impl takes. The
/// former round-12 variant borrowed its head out of
/// `ArrayIndexStack::head()`; that extraction route is CLOSED
/// (`ArrayIndexStack` no longer implements `StackStorage` — see the
/// compile-fail fixture `tests/compile_fail/array_index_stack_head/`), so
/// this struct now constructs its own `StackHead` like any other custom
/// implementor.
struct Parasite {
    head: StackHead<16>,
    links: ArrayLinks<64>,
}

// SAFETY: DELIBERATE contract violation — clause 2 (load_next must observe
// the most recent store_next the stack performed): the test overwrites the
// backing behind the algorithm's back, so load_next answers values the
// crate never stored.
unsafe impl StackStorage<16> for Parasite {
    fn head(&self) -> &StackHead<16> {
        &self.head
    }

    fn load_next(&self, index: u32) -> u32 {
        self.links.load_next(index)
    }

    fn store_next(&self, index: u32, next: u32) {
        self.links.store_next(index, next)
    }
}

/// The round-13 self-loop detector's LIMIT — a hand-crafted ACYCLIC
/// forgery still double-issues silently. KNOWN, INTENTIONAL limitation,
/// same register as the `#[should_panic]` tests around it: this pins that
/// `pop_index`'s rule-4 guard is a SHAPE detector (the
/// zero-initialised-foreign-backing shape, whose `next == index` self-loop
/// is unreachable for a contract-abiding chain), NOT a structural fix for
/// the shared-storage hazard class — see the [`StackStorage`] trait doc's
/// "The shared-storage hazard class" section for the full catch/miss
/// boundary.
///
/// The head no longer comes from `ArrayIndexStack::head()` — that
/// extraction route is CLOSED (see the compile-fail fixture
/// `tests/compile_fail/array_index_stack_head/`). What this test now
/// demonstrates is the pure hand-forged-acyclic-backing shape: the
/// implementor's backing is overwritten BEHIND the algorithm's back, so
/// its `load_next` answers with values the crate never stored (a violation
/// of rule 3's coherence clause) — and the acyclic forgery evades the
/// round-13 self-loop detector. The detector's limit, unchanged.
///
/// Mechanism: the parasite pushes index `1` for real (`links[1] = TAIL`,
/// head `(1, tag)`), then forges its own links before popping —
/// `links[1] = 0`, `links[0] = TAIL`. The chain it hands `pop_index` is
/// perfectly acyclic and numerically valid (`1 -> 0 -> TAIL`): the first
/// pop legitimately returns the head index `1`, and the second pop returns
/// the never-pushed `0` — a phantom index, in a parent allocator a second
/// owner for a live slot — with NO panic, because no link ever points
/// back to its own index. If a future structural fix detects
/// forged-but-acyclic chains too, this test breaks by design and
/// `pop_index`'s `# Panics` must be updated with it.
#[test]
fn hand_crafted_acyclic_forgery_still_double_issues() {
    let parasite = Parasite {
        head: StackHead::<16>::new(),
        links: ArrayLinks::<64>::new(),
    };

    // A REAL push through the implementor: links[1] = TAIL, head = (1, tag).
    parasite.push_index(1);

    // Forge an acyclic chain in the parasite's own links BEFORE popping:
    // index 1 (the head) chains to 0, 0 chains to TAIL.
    parasite.links.store_next(1, 0);
    parasite.links.store_next(0, TAIL);

    assert_eq!(parasite.pop_index(), Some(1));
    // The never-pushed 0, handed out silently — no self-loop anywhere in
    // 1 -> 0 -> TAIL, so the round-13 detector stays quiet.
    assert_eq!(parasite.pop_index(), Some(0));
}

/// A borrowed-LINKS [`StackStorage`] implementor with its OWN head value:
/// the fourth variant of the double-issue family, and the first that does
/// NOT involve a shared head at all. KNOWN, INTENTIONAL limitation —
/// round-13 @oh review, finding P2-2 — documented, not detected: no cheap
/// runtime detector exists for this shape (the [`StackStorage`] trait
/// doc's rule 3 binding-level clause; the trait doc's hazard inventory lists
/// it as the one shape with no detection at all).
///
/// The hazard: TWO stacks built over the SAME link cells — each with a
/// completely separate, freshly constructed, individually rule-coherent
/// [`StackHead`] — where one index is REACHABLE from both (cell sharing
/// per se is harmless with disjoint index populations — each
/// `store_next`/`load_next` touches only its own cell — see rule 3's
/// binding-level clause): the second stack's push of an index live in
/// the first overwrites a link the first still chains through. Concretely:
/// `a` pushes 1 then 2 (`links[1] = TAIL`, `links[2] = 1`); `b` pushes 3
/// then 1 (`links[3] = TAIL`, then `links[1] = 3`, CLOBBERING the `TAIL`
/// `a` stored there). `a`'s chain has silently become `2 -> 1 -> 3 ->
/// TAIL` and `b`'s is `1 -> 3 -> TAIL`. Draining `a` yields `2, 1, 3`;
/// draining `b` yields `1, 3` — indices 1 AND 3 are each handed out
/// TWICE, across two stacks that appear individually correct by every
/// existing rule. In a parent allocator that is two live slots with two
/// owners each. Every link value stays numerically valid and the shared
/// chain stays perfectly ACYCLIC, so `pop_index`'s rule-4 guard —
/// including round-13's self-loop detector — cannot fire; only rule 3's
/// binding-level clause NAMES the obligation — implementor/caller
/// discipline, not something the type system, the blanket impl, or a
/// runtime guard enforces. Discharge it by construction: disjoint index
/// populations per binding over any shared cell population. If a future
/// revision adds a detector for
/// cross-stack cell sharing (or stops this shape from compiling), this
/// test breaks by design and the trait doc's rule 3 must be updated with
/// it.
#[test]
fn two_stacks_sharing_link_storage_still_double_issue() {
    struct SharedLinksView<'a> {
        head: StackHead<16>,
        links: &'a ArrayLinks<64>,
    }

    // SAFETY: DELIBERATE contract violation — clause 3 (no index reachable
    // from two live bindings over shared cells): two live bindings, separate
    // heads, same cells, overlapping reachability.
    unsafe impl StackStorage<16> for SharedLinksView<'_> {
        fn head(&self) -> &StackHead<16> {
            &self.head
        }

        fn load_next(&self, index: u32) -> u32 {
            self.links.load_next(index)
        }

        fn store_next(&self, index: u32, next: u32) {
            self.links.store_next(index, next)
        }
    }

    let links = ArrayLinks::<64>::new();
    let a = SharedLinksView {
        head: StackHead::new(),
        links: &links,
    };
    let b = SharedLinksView {
        head: StackHead::new(),
        links: &links,
    };

    a.push_index(1);
    a.push_index(2);
    // `b`'s push of 1 clobbers `links[1]` — the TAIL `a` stored there —
    // with 3, splicing `a`'s chain onto `b`'s tail.
    b.push_index(3);
    b.push_index(1);

    let mut from_a = Vec::new();
    while let Some(i) = a.pop_index() {
        from_a.push(i);
    }
    let mut from_b = Vec::new();
    while let Some(i) = b.pop_index() {
        from_b.push(i);
    }

    assert_eq!(
        from_a,
        vec![2, 1, 3],
        "a's chain was silently spliced to 2 -> 1 -> 3 -> TAIL by b's push \
         of 1 overwriting links[1]"
    );
    assert_eq!(
        from_b,
        vec![1, 3],
        "indices 1 and 3 were each handed out TWICE across the two stacks; \
         no panic fired because the shared chain stays acyclic"
    );
}

/// Inventory shape 1 — ONE implementor whose
/// [`load_next`](StackStorage::load_next)/
/// [`store_next`](StackStorage::store_next) read and write DIFFERENT
/// backings behind one head. KNOWN, INTENTIONAL limitation —
/// implementor-enforced (the [`StackStorage`] trait doc's rules 3 and 4),
/// not structurally impossible, auditable only inside the one impl block.
/// This is shape 1's first automated coverage: the two round-11/12
/// `#[should_panic]` tests above both pin shape 2, so before round 15
/// shape 1 had NO pinning test at all (round-15 @oh review, P4-2).
///
/// Mechanism (the zero-initialised sub-shape): pushing 1 stores
/// `write_links[1] = TAIL`, but every pop READS `read_links`, which
/// answers `0` for every index. The first pop legitimately returns the
/// head index `1` — and it has ALREADY moved the head to a phantom
/// `(0, tag)`. The second pop reads `read_links[0] == 0 == index`: a
/// self-loop a contract-abiding chain can never contain, so the round-13
/// rule-4 guard panics — one pop later than the corruption it names. See
/// the trait doc's "Detection coverage" for the catch/miss boundary.
#[test]
#[should_panic(expected = "self-loop, corrupting the free-list into a cycle")]
fn internally_disagreeing_storage_still_double_issue() {
    struct DisagreeingStorage {
        head: StackHead<16>,
        read_links: ArrayLinks<64>,
        write_links: ArrayLinks<64>,
    }

    // SAFETY: DELIBERATE contract violation — clause 2 (one backing,
    // consistently): load_next and store_next read and write DIFFERENT
    // backings.
    unsafe impl StackStorage<16> for DisagreeingStorage {
        fn head(&self) -> &StackHead<16> {
            &self.head
        }

        fn load_next(&self, index: u32) -> u32 {
            self.read_links.load_next(index)
        }

        fn store_next(&self, index: u32, next: u32) {
            self.write_links.store_next(index, next)
        }
    }

    let storage = DisagreeingStorage {
        head: StackHead::new(),
        read_links: ArrayLinks::<64>::new(),
        write_links: ArrayLinks::<64>::new(),
    };

    storage.push_index(1);

    // First pop: reads the FOREIGN `read_links` (0 — in range and != 1),
    // so the guard stays silent while the head moves to a phantom
    // `(0, tag)`.
    assert_eq!(storage.pop_index(), Some(1));
    // Second pop: read_links[0] answers 0 == the popped index — a
    // self-loop; the guard fires one pop too late.
    let _ = storage.pop_index();
}

/// Inventory shape 4 — temporal rebinding: a LIVE [`StackHead`] VALUE moved
/// into FRESH links, one head↔links binding replaced across time by another
/// over the same head (round-15 @oh review, P3-3). Not covered by rule 1's
/// elaboration (no `&StackHead` reference is ever shared — the head moves
/// by value, and `old` is consumed, so there is never more than ONE live
/// implementor value) nor by the inventory's old two-live-values scoping;
/// only rule 1's HEADLINE ("one backing ... for the whole life of a
/// non-empty stack") covers it, in spirit. KNOWN, INTENTIONAL limitation:
/// the FIRST pop is a silent leak, and only the second pop's self-loop
/// makes the rebinding loud — one index too late.
#[test]
#[should_panic(expected = "self-loop, corrupting the free-list into a cycle")]
fn head_moved_into_fresh_links_leaks_and_then_panics() {
    struct Pool {
        head: StackHead<16>,
        links: ArrayLinks<64>,
    }

    // SAFETY: DELIBERATE contract violation — clause 1's temporal half (a
    // live head rebound to different links across time).
    unsafe impl StackStorage<16> for Pool {
        fn head(&self) -> &StackHead<16> {
            &self.head
        }

        fn load_next(&self, index: u32) -> u32 {
            self.links.load_next(index)
        }

        fn store_next(&self, index: u32, next: u32) {
            self.links.store_next(index, next)
        }
    }

    let old = Pool {
        head: StackHead::new(),
        links: ArrayLinks::<64>::new(),
    };
    old.push_index(1);
    old.push_index(2);

    // Rebind: move the LIVE head into a fresh, zero-initialised backing.
    // `old.head` moves by value — no reference is shared, `old` is
    // consumed field-by-field, and there is never a second live value.
    let grown = Pool {
        head: old.head,
        links: ArrayLinks::<64>::new(),
    };

    // First pop: grown's fresh links answer load_next(2) with 0 — in range
    // and != 2, so the guard stays silent; the stale head index 2 is
    // returned from a backing that no longer describes the chain, and
    // index 1 (whose link lived in the old backing) is silently LEAKED.
    assert_eq!(grown.pop_index(), Some(2));
    // Second pop: load_next(0) answers 0 == the popped index — a
    // self-loop; the guard fires one index too late.
    let _ = grown.pop_index();
}

/// Inventory shape 3's ONE-value form (round-15 @oh review, P3-4): TWO
/// head↔links bindings over ONE shared backing, both inside a SINGLE
/// implementor value via two `StackStorage` impls at different widths —
/// falsifying "two implementor values" as the inventory's counting unit
/// (the trait doc's inventory now counts BINDINGS). KNOWN, INTENTIONAL
/// limitation, same register as
/// `two_stacks_sharing_link_storage_still_double_issue`: every link value
/// stays numerically valid and the shared chain stays perfectly ACYCLIC,
/// so no detector fires.
///
/// Mechanism: the wide (16-bit) binding pushes 1 then 2 (`links[1] =
/// TAIL`, `links[2] = 1`); the narrow (12-bit) binding pushes 3, then
/// re-pushes 1 (`links[3] = TAIL`, then `links[1] = 3`, CLOBBERING the
/// TAIL the wide binding stored there). The wide chain has silently become
/// `2 -> 1 -> 3 -> TAIL` and the narrow one `1 -> 3 -> TAIL`: the wide
/// drain yields `2, 1, 3` and the narrow drain `1, 3` — indices 1 AND 3
/// are each handed out TWICE, with ONE implementor value existing the
/// entire time. In a parent allocator that is two live slots with two
/// owners each.
#[test]
fn one_value_two_bindings_shared_backing_still_double_issue() {
    struct DualWidth {
        wide_head: StackHead<16>,
        narrow_head: StackHead<12>,
        links: ArrayLinks<64>,
    }

    // SAFETY: DELIBERATE contract violation for BOTH impls below — clause 3
    // (disjoint reachable-index populations across shared cells): two
    // bindings over ONE backing inside one value.
    unsafe impl StackStorage<16> for DualWidth {
        fn head(&self) -> &StackHead<16> {
            &self.wide_head
        }

        fn load_next(&self, index: u32) -> u32 {
            self.links.load_next(index)
        }

        fn store_next(&self, index: u32, next: u32) {
            self.links.store_next(index, next)
        }
    }

    unsafe impl StackStorage<12> for DualWidth {
        fn head(&self) -> &StackHead<12> {
            &self.narrow_head
        }

        fn load_next(&self, index: u32) -> u32 {
            self.links.load_next(index)
        }

        fn store_next(&self, index: u32, next: u32) {
            self.links.store_next(index, next)
        }
    }

    let dual = DualWidth {
        wide_head: StackHead::new(),
        narrow_head: StackHead::new(),
        links: ArrayLinks::<64>::new(),
    };
    let wide: &dyn StackStorage<16> = &dual;
    let narrow: &dyn StackStorage<12> = &dual;

    wide.push_index(1);
    wide.push_index(2);
    narrow.push_index(3);
    // The narrow re-push of 1 clobbers links[1] — the TAIL the wide
    // binding stored there — with 3, splicing the wide chain onto the
    // narrow tail.
    narrow.push_index(1);

    let mut from_wide = Vec::new();
    while let Some(i) = wide.pop_index() {
        from_wide.push(i);
    }
    let mut from_narrow = Vec::new();
    while let Some(i) = narrow.pop_index() {
        from_narrow.push(i);
    }

    assert_eq!(
        from_wide,
        vec![2, 1, 3],
        "wide chain was silently spliced to 2 -> 1 -> 3 -> TAIL by the narrow \
         binding's re-push of 1 overwriting links[1]"
    );
    assert_eq!(
        from_narrow,
        vec![1, 3],
        "indices 1 and 3 were each handed out TWICE across the two bindings — \
         ONE implementor value the whole time; no panic fired because the \
         shared chain stays acyclic"
    );
}
