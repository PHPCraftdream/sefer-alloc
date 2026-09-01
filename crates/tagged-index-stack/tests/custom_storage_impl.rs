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
//! still compile and still double-issue.
//!
//! Since the round-12 @oh review (finding P2-1) it also pins the third
//! variant of the same hazard: the escape hatch is not a custom
//! implementor at all — [`ArrayIndexStack`]'s own `head()` (a public,
//! safe, callable trait method on the crate's recommended "safe" type)
//! hands the head reference to any caller, so a second implementor built
//! around it corrupts the OWNED stack too, not just the parasite's view.

#![cfg(not(loom))]

use core::sync::atomic::{AtomicU32, Ordering};

use tagged_index_stack::{ArrayIndexStack, ArrayLinks, StackHead, StackOps, StackStorage};

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
/// compiler-enforced. This test is a runnable demonstration, not a
/// correctness check: it pins the HAZARDOUS behavior of the shared-head
/// shape so it cannot drift silently in either direction.
///
/// The hazard (round-11 @oh review, finding P2-1): two separately
/// constructed, individually rule-coherent `StackStorage` values whose
/// `head()` methods return the SAME `StackHead` while their links differ
/// compile without a single warning. Popping through the second value reads
/// links from the WRONG backing: the first pop hands back the real head
/// index, and every pop after that hands back whatever the wrong backing's
/// zero-initialised storage answers with — here `0`, which was never pushed
/// through ANY implementor. In a parent allocator that is the original
/// release-blocking failure mode: an index nobody owns is handed out, and a
/// live slot gets a second owner. Nothing fires: rule 3 is worded per
/// implementor and each value is individually coherent, and `pop_index`'s
/// release-active guard only rejects link values that are neither `TAIL`
/// nor a valid index — `0` is valid.
///
/// This is the same category of caller discipline as `push_index`'s
/// no-double-push liveness rule (see that method's `# Caller contract`):
/// real, documented, and unenforceable at acceptable cost. Discharge it by
/// construction — one implementor value per head, keeping the head
/// reference private to that one value. (Round-12 correction: the owned
/// `ArrayIndexStack` shape fuses head and links but is NOT by itself the
/// discharge — its `head()` remains a public escape hatch; see the test
/// below.) If a future revision makes this shape stop compiling or stop
/// double-issuing (a structural fix), this test breaks by design and the
/// trait doc's rule 1 must be updated with it.
#[test]
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

    // Pop through `via_b` — a DIFFERENT implementor value over the SAME
    // head. The first pop legitimately returns 1 (it IS the head); every
    // pop after it double-issues 0, which was never pushed at all.
    let mut popped = Vec::new();
    for _ in 0..5 {
        if let Some(i) = via_b.pop_index() {
            popped.push(i);
        }
    }
    assert_eq!(
        popped,
        vec![1, 0, 0, 0, 0],
        "index 0 was never pushed through any implementor; via_b's own \
         zero-initialised links answer every load_next with 0"
    );
    // It does not stop, either: the head is now (0, tag), and the
    // (0, tag) -> (0, tag) compare_exchange succeeds trivially.
    assert_eq!(via_b.pop_index(), Some(0));
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

/// KNOWN, INTENTIONAL limitation — round-12 @oh review, finding P2-1.
/// Runnable demonstration, not a correctness check, same register as the
/// `SharedHeadView` test above: it pins that `ArrayIndexStack::head()` is
/// the VALUE-LEVEL escape hatch of rule 1's hazard even on the fused
/// type — owning the head-and-links pair binds them to each other; it
/// does not stop a third party from calling `.head()` and building a
/// second, competing implementor around the same borrowed head.
///
/// The mechanism, and why it is WORSE than the `SharedHeadView` shape:
/// pushing through `owned` writes `owned`'s link for index 1, but the
/// parasite's pops read the PARASITE's links, whose zero-initialised
/// storage answers every `load_next` with `0`. The first pop legitimately
/// returns the real head index (`1`); every pop after it hands back `0`,
/// which was never pushed through ANY implementor. Worse still, the
/// corruption propagates BACK into the owned stack: the parasite's pops
/// leave the shared head at `(0, tag)`, and `owned`'s own `links[0]` is
/// still `0`, so `owned`'s `(0, tag) -> (0, tag)` compare_exchange
/// succeeds trivially and the owned stack double-issues `0` forever too.
///
/// Nothing fires to catch it: the trait rules are worded per implementor
/// and each value is individually coherent, and `pop_index`'s
/// release-active guard only rejects link values that are neither `TAIL`
/// nor a valid index — `0` is valid. If a future structural fix stops
/// `head()` from handing out the head on the fused type (or stops this
/// shape from compiling or double-issuing), this test breaks by design,
/// and the `ArrayIndexStack` type doc and the trait doc's rule 1 must be
/// updated with it.
#[test]
fn array_index_stack_head_still_double_issue() {
    let owned = ArrayIndexStack::<16, 64>::new();
    let parasite = Parasite {
        head: owned.head(),
        links: ArrayLinks::<64>::new(),
    };

    owned.push(1);

    // Pop through `parasite` — a DIFFERENT implementor value over the
    // owned stack's head. First pop: the real head index; every pop
    // after it: the never-pushed `0`.
    let mut popped = Vec::new();
    for _ in 0..5 {
        if let Some(i) = parasite.pop_index() {
            popped.push(i);
        }
    }
    assert_eq!(
        popped,
        vec![1, 0, 0, 0, 0],
        "index 0 was never pushed through any implementor; the parasite's \
         own zero-initialised links answer every load_next with 0"
    );

    // The owned stack is corrupted too: the shared head now reads
    // (0, tag), and `owned`'s own links[0] is still 0, so its CAS
    // succeeds trivially.
    assert!(
        !owned.is_empty(),
        "the parasite's pops left the shared head at a non-empty (0, tag)"
    );
    assert_eq!(owned.pop(), Some(0));
    assert_eq!(owned.pop(), Some(0));
}
