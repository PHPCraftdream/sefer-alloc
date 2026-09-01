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
//! Since the round-11 @oh review (finding P2-1) it also pins — as a
//! documented, intentional, caller/implementor-enforced limitation — the
//! shared-head shape [`StackStorage`]'s rule 1 forbids: two implementor
//! values returning the same [`StackHead`] over different link storage
//! still compile; since the round-13 @oh review (finding P2-1) its
//! zero-initialised-links repro no longer double-issues SILENTLY — the
//! second pop trips `pop_index`'s self-loop detector and panics, so the
//! two shared-head tests below now pin the GUARD FIRING (silent corruption
//! made loud) under `#[should_panic]`.
//!
//! Since the round-12 @oh review (finding P2-1) it also pins the third
//! variant of the same hazard: the escape hatch is not a custom
//! implementor at all — [`ArrayIndexStack`]'s own `head()` (a public,
//! safe, callable trait method on the crate's recommended "safe" type)
//! hands the head reference to any caller, so a second implementor built
//! around it corrupts the OWNED stack too, not just the parasite's view.
//! Since round-13, that parasite's second pop also panics via the same
//! self-loop detector instead of silently double-issuing and corrupting
//! the owned stack's drain.
//!
//! Since the round-13 @oh review (finding P2-1) it also pins the self-loop
//! detector's LIMIT — a hand-crafted acyclic link forgery that still
//! double-issues silently — and (round-13 P2-2) the shared-LINK-STORAGE
//! variant, where two stacks with completely separate heads share one
//! link array.

#![cfg(not(loom))]

use core::sync::atomic::{AtomicU32, Ordering};

use tagged_index_stack::{ArrayIndexStack, ArrayLinks, StackHead, StackOps, StackStorage, TAIL};

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

impl StackStorage<16> for VecStorage {
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

impl StackStorage<16> for SharedHeadView<'_> {
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
/// compiler-enforced. Round-11 @oh review, finding P2-1; FLIPPED by the
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
/// and `pop_index`'s release-active rule-4 guard fires. This test now pins
/// the GUARD FIRING (the strict improvement from silent corruption to a
/// loud panic), not the corruption going undetected. It does NOT pin the
/// hazard class closed: a hand-crafted acyclic backing evades the detector
/// (`hand_crafted_acyclic_forgery_still_double_issues` below), and link
/// cells shared between two independent stacks stay acyclic too
/// (`two_stacks_sharing_link_storage_still_double_issue`, round-13 P2-2).
/// The name keeps the round-11 finding's shape description; only the
/// pinned behavior changed. If a future structural fix stops this shape
/// from compiling, this test breaks by design and the trait doc's rule 1
/// must be updated with it.
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

/// A second implementor around a head borrowed from the OWNED stack — the
/// round-12 variant of the `SharedHeadView` shape above. There is no
/// custom head-view struct here at all: the head reference comes straight
/// out of `ArrayIndexStack::head()`, a public, safe, callable trait
/// method (`StackStorage` is in scope), so the crate's own recommended
/// "safe" type hands out the exact ingredient rule 1's hazard needs.
struct Parasite<'a> {
    head: &'a StackHead<16>,
    links: ArrayLinks<64>,
}

impl StackStorage<16> for Parasite<'_> {
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

/// KNOWN, INTENTIONAL limitation — round-12 @oh review, finding P2-1;
/// FLIPPED by the round-13 @oh review, finding P2-1. Runnable
/// demonstration in the same register as the `SharedHeadView` test above:
/// it pins that [`ArrayIndexStack::head()`](ArrayIndexStack::head) is the
/// VALUE-LEVEL escape hatch of rule 1's hazard even on the fused type —
/// owning the head-and-links pair binds them to each other; it does not
/// stop a third party from calling `.head()` and building a second,
/// competing implementor around the same borrowed head.
///
/// The mechanism: pushing through `owned` writes `owned`'s link for index
/// 1, but the parasite's pops read the PARASITE's links, whose
/// zero-initialised storage answers every `load_next` with `0`. The first
/// pop legitimately returns the real head index (`1`) — and it has
/// ALREADY moved the shared head to a foreign `(0, tag)`. The second pop
/// used to hand back the never-pushed `0` and let the owned stack
/// double-issue `0` forever too; since round-13 it PANICS: popping index
/// `0` through the parasite's zero-initialised links reads
/// `next == 0 == index`, a self-loop, and `pop_index`'s release-active
/// rule-4 guard fires before the second CAS can complete. This test now
/// pins the GUARD FIRING (silent corruption made loud), not the corruption
/// going undetected — and the owned stack's further corruption becomes
/// unobservable here, because the panic ends the test before it could be
/// drained. It does NOT pin the hazard class closed: `head()` is still a
/// public escape hatch, and a parasite with hand-crafted acyclic links
/// still evades the detector (see
/// `hand_crafted_acyclic_forgery_still_double_issues` below). If a future
/// structural fix stops `head()` from handing out the head on the fused
/// type (or stops this shape from compiling), this test breaks by design,
/// and the `ArrayIndexStack` type doc and the trait doc's rule 1 must be
/// updated with it.
#[test]
#[should_panic(expected = "self-loop, corrupting the free-list into a cycle")]
fn array_index_stack_head_still_double_issue() {
    let owned = ArrayIndexStack::<16, 64>::new();
    let parasite = Parasite {
        head: owned.head(),
        links: ArrayLinks::<64>::new(),
    };

    owned.push(1);

    // First pop through `parasite` is still the real head index.
    assert_eq!(parasite.pop_index(), Some(1));
    // Second pop: the parasite's zero-initialised links answer
    // load_next(0) with 0 while the popped index IS 0 — self-loop; the
    // round-13 guard panics. (Before it: silent phantom `0`, and the
    // owned stack's own drain then double-issued `0` forever too.)
    let _ = parasite.pop_index();
}

/// The round-13 self-loop detector's LIMIT — a hand-crafted ACYCLIC
/// forgery still double-issues silently. KNOWN, INTENTIONAL limitation,
/// same register as the two `#[should_panic]` tests above: this pins that
/// `pop_index`'s rule-4 guard is a SHAPE detector (the
/// zero-initialised-foreign-backing shape, whose `next == index` self-loop
/// is unreachable for a contract-abiding chain), NOT a structural fix for
/// the shared-storage hazard class.
///
/// The parasite here forges its OWN links before popping —
/// `links[1] = 0`, `links[0] = TAIL` — instead of accepting the
/// zero-initialised default. The chain it hands `pop_index` is perfectly
/// acyclic and numerically valid (`1 -> 0 -> TAIL`): the first pop
/// legitimately returns the head index `1`, and the second pop returns
/// the never-pushed `0` — a phantom index, in a parent allocator a second
/// owner for a live slot — with NO panic, because no link ever points
/// back to its own index. If a future structural fix detects
/// forged-but-acyclic chains too, this test breaks by design and
/// `pop_index`'s `# Panics` must be updated with it.
#[test]
fn hand_crafted_acyclic_forgery_still_double_issues() {
    let owned = ArrayIndexStack::<16, 64>::new();
    let parasite = Parasite {
        head: owned.head(),
        links: ArrayLinks::<64>::new(),
    };

    owned.push(1);

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
/// runtime detector exists for this shape.
///
/// The hazard: TWO stacks built over the SAME link cells — each with a
/// completely separate, freshly constructed, individually rule-coherent
/// [`StackHead`] — overwrite each other's links on every push. Concretely:
/// `a` pushes 1 then 2 (`links[1] = TAIL`, `links[2] = 1`); `b` pushes 3
/// then 1 (`links[3] = TAIL`, then `links[1] = 3`, CLOBBERING the `TAIL`
/// `a` stored there). `a`'s chain has silently become `2 -> 1 -> 3 ->
/// TAIL` and `b`'s is `1 -> 3 -> TAIL`. Draining `a` yields `2, 1, 3`;
/// draining `b` yields `1, 3` — indices 1 AND 3 are each handed out
/// TWICE, across two stacks that appear individually correct by every
/// existing rule. In a parent allocator that is two live slots with two
/// owners each.
///
/// Why nothing catches it: every link value is numerically valid and the
/// shared chain stays perfectly ACYCLIC, so `pop_index`'s rule-4 guard —
/// including round-13's self-loop detector — cannot fire; rule 1 is about
/// heads and is satisfied (the heads are separate); rules 2, 4 and 5 are
/// per-implementor and each value satisfies them on its own. Only rule
/// 3's VALUE-level clause (link-cell exclusivity, round-13) NAMES this
/// obligation — implementor/caller discipline, not something the type
/// system, the blanket impl, or a runtime guard enforces. Discharge it by
/// construction: one link-cell population per stack. If a future revision
/// adds a detector for cross-stack cell sharing (or stops this shape from
/// compiling), this test breaks by design and the trait doc's rule 3 must
/// be updated with it.
#[test]
fn two_stacks_sharing_link_storage_still_double_issue() {
    struct SharedLinksView<'a> {
        head: StackHead<16>,
        links: &'a ArrayLinks<64>,
    }

    impl StackStorage<16> for SharedLinksView<'_> {
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
