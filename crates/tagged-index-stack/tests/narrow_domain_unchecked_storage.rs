//! Miri behavioral oracle — a legitimate UNCHECKED [`StackStorage`]
//! implementor whose declared link domain (0..8) is strictly narrower than
//! `INDEX_MASK` (0xFFFF at 16 bits), exercising trait `# Safety` clause 6
//! ("Declared link domain") together with the `unsafe fn` caller-side
//! contract on [`StackOps::push_index`]/[`ArrayIndexStack::push`].
//!
//! The executable proof: `UncheckedPool`'s
//! [`load_next`]/[`store_next`](StackStorage::store_next) use
//! `get_unchecked` INSIDE their declared domain, so under miri every single
//! link access the stack algorithm performs is validated against the real
//! 8-cell bound — the trait's clause-6 permission (unchecked access
//! OUT-of-domain) is exercised by construction, because there IS no cell
//! for any index >= 8 and any out-of-domain hook call would be an
//! immediate miri UB report (out-of-bounds access under
//! strict provenance/bounds checking).
//!
//! # Why the negative case is NOT demonstrated at runtime
//!
//! An out-of-domain push (e.g. `push_index(9)` against this 8-cell pool) is
//! a CALLER-SIDE CONTRACT violation of `push_index`'s `# Safety` clause 1
//! (link domain) — attributable to the caller, the same posture as
//! `GlobalAlloc::dealloc`'s exclusive-issuance contract — and therefore not
//! safely demonstrable at runtime: by the time the out-of-domain
//! `store_next` lands, the memory-safety question is already undefined
//! behavior by assumption, and driving it deliberately would only
//! demonstrate that UB is UB. The oracle proves the POSITIVE case: the
//! legitimate unchecked implementor stays clean under miri across seed /
//! interleave / drain cycles, so any contract-chain breakage that let SAFE
//! code drive out-of-domain accesses through this impl (a widened numeric
//! guard silently treated as a domain proof, a hook escaping its domain
//! precondition, a lost caller-side `unsafe`) would surface HERE as a miri
//! out-of-bounds report. The caller-side boundary itself — bare pushes are
//! E0133 — is pinned separately by `tests/compile_fail.rs`
//! (`push_index_requires_unsafe_block`); the in-domain/unwrapped-hook
//! negative shapes are pinned by `tests/compile_fail/hook_call_requires_unsafe/`.
//!
//! # Invocation
//!
//! Plain `cargo test -p tagged-index-stack --test
//! narrow_domain_unchecked_storage` runs this file soundly and fast
//! (default build); the REAL teeth are under the interpreter:
//! `cargo +nightly miri test -p tagged-index-stack --test
//! narrow_domain_unchecked_storage` (same shape as the repo's CI miri jobs
//! and `scripts/miri.mjs`, which sweep the focused UB targets per crate —
//! no CI plumbing added here; ci.yml is out of scope).
//!
//! # cfg-agnosticism
//!
//! This file uses only `core::sync::atomic` types of its own, so it
//! compiles and passes identically under default features,
//! `--features test-internals`, and `RUSTFLAGS="--cfg loom" --features
//! loom` builds (only the LIBRARY aliases its atomics under loom cfg; this
//! file's `AtomicU32`s stay real either way).

use core::sync::atomic::{AtomicU32, Ordering};

use tagged_index_stack::{StackHead, StackOps, StackStorage};

/// A fixed-capacity, array-backed pool with UNCHECKED cell access: cells
/// exist only for the declared domain `0..N` (N = 8 here, far below the
/// 16-bit `INDEX_MASK` of 0xFFFF). Anything the stack algorithm touches is
/// validated by miri against the REAL array bound.
struct UncheckedPool<const N: usize> {
    head: StackHead<16>,
    cells: [AtomicU32; N],
}

impl<const N: usize> UncheckedPool<N> {
    fn new() -> Self {
        Self {
            head: StackHead::new(),
            cells: [const { AtomicU32::new(0) }; N],
        }
    }
}

// SAFETY: clause-by-clause assertion of `StackStorage`'s `# Safety` list for
// THIS implementor (N = 8):
//
// 1. **One live binding per head.** `head` is a private field of this
//    struct; `head()` hands out `&self.head`, and no other binding is ever
//    built around it (the tests below drive only
//    `push_index`/`pop_index`).
// 2. **One backing, consistently.** `load_next`/`store_next` both index
//    `self.cells` by the same `index as usize` mapping, stable for the
//    value's whole life; a `load_next` observes the most recent
//    `store_next` the stack performed (Acquire/Release per the ordering
//    contract).
// 3. **Disjoint reachable populations.** These cells are touched by this
//    one head↔cells binding only; no second binding exists.
// 4. **Valid answers, dedicated cells.** Each cell is DEDICATED link
//    storage (never payload-aliased) and answers only what a push stored:
//    [`TAIL`] or an in-range index (fresh cells hold 0, and 0 is only ever
//    read after a push through this binding initialised it — the lazy-link
//    RAD-1 discipline).
// 5. **Same logical head every call.** `head()` returns `&self.head` on
//    every call.
// 6. **Declared link domain.** The declared link domain of this impl is
//    `0..N` — here `0..8`, a fixed subset of `0..INDEX_MASK`, documented
//    HERE, fixed for the impl's whole life (const-generic array, never
//    resized). The cells exist by construction (`[AtomicU32; N]`), so
//    `load_next`/`store_next` are memory-safe for every in-domain index —
//    and they deliberately use UNCHECKED access
//    (`get_unchecked`), which miri validates against the real 8-cell
//    bound on every call. Out-of-domain indices have NO cell; the hooks
//    rely on clause-6's guarantee that the stack never calls them
//    out-of-domain (discharged by `push_index`'s caller-side clause 1).
// 7. **Atomic cells.** Every cell is an `AtomicU32`, accessed only via
//    atomic `load`/`store` — a racing stale popper's `load_next` against a
//    push's `store_next` is a race on atomics, not UB.
unsafe impl<const N: usize> StackStorage<16> for UncheckedPool<N> {
    unsafe fn head(&self) -> &StackHead<16> {
        &self.head
    }

    unsafe fn load_next(&self, index: u32) -> u32 {
        // SAFETY: `index` is in this impl's declared domain 0..N (the
        // stack algorithm only calls the hooks in-domain, per trait `#
        // Safety` clause 6 and `push_index`'s caller-side clause 1), and
        // the domain cells exist by construction — the unchecked bound is
        // the clause-6 permission, miri-checked against the real array.
        unsafe { self.cells.get_unchecked(index as usize) }.load(Ordering::Acquire)
    }

    unsafe fn store_next(&self, index: u32, next: u32) {
        // SAFETY: same domain argument as `load_next` above — `index` is
        // in the declared domain 0..N whose cells exist by construction.
        unsafe { self.cells.get_unchecked(index as usize) }.store(next, Ordering::Release);
    }
}

/// Seed every in-domain index exactly once, interleave pops and re-pushes
/// (only re-pushing an index `pop()` just RETURNED — the liveness clause),
/// then drain to empty and assert full conservation and LIFO order where
/// deterministic. Every link access the algorithm performs lands in one of
/// the 8 real cells, so miri validates the whole cycle against the
/// declared domain.
#[test]
fn narrow_domain_unchecked_storage_seed_interleave_drain_conserves() {
    let pool: UncheckedPool<8> = UncheckedPool::new();

    // Seed 0..8, each exactly once: LIFO drain order is fully
    // deterministic (7,6,...,0).
    for i in 0..8u32 {
        // SAFETY: `i` is in the pool's declared domain 0..8 and has never
        // been pushed (fresh pool) — both caller-side clauses discharged.
        unsafe { pool.push_index(i) };
    }

    // Interleave: pop three, re-push each exactly once (returned ⇒ not
    // live ⇒ liveness clause holds), then pop one more that stays out.
    for _ in 0..3 {
        let idx = pool.pop_index().expect("stack holds all eight seeds");
        // SAFETY: `idx` was just RETURNED by `pop_index`, so it is not
        // live and it is in-domain — both caller-side clauses discharged.
        unsafe { pool.push_index(idx) };
    }
    let stayed_out = pool.pop_index().expect("stack non-empty");
    assert_eq!(
        stayed_out, 7,
        "each re-pushed top re-lands on top, so 7 surfaces a fourth time"
    );

    // Drain to empty; assert conservation against the pushed multiset.
    let mut drained = Vec::new();
    while let Some(i) = pool.pop_index() {
        drained.push(i);
    }
    let mut remaining = drained.clone();
    remaining.sort_unstable();
    let expected: Vec<u32> = (0..7u32).collect();
    assert_eq!(
        remaining, expected,
        "drain returns exactly the pushed-and-not-yet-returned multiset — \
         no index lost or duplicated (7 left as `stayed_out`)"
    );
    assert_eq!(
        drained.len(),
        7,
        "every still-pushed index is popped exactly once (full conservation)"
    );
    assert_eq!(pool.pop_index(), None, "drain reaches empty");

    // Deterministic LIFO tail: the stack held [6, 5, 4, 3, 2, 1, 0 (top)]
    // beneath the re-popped 7.
    assert_eq!(&drained[..4], &[6, 5, 4, 3], "LIFO where deterministic");
}

/// End-to-end round trip through a fresh pool (`vec_backed`-style): seed a
/// strict subset of the domain, drain, assert LIFO and emptiness.
#[test]
fn narrow_domain_unchecked_storage_round_trips_end_to_end() {
    let pool: UncheckedPool<8> = UncheckedPool::new();

    for i in 0..5u32 {
        // SAFETY: `i` is in-domain (0..8) and never yet pushed.
        unsafe { pool.push_index(i) };
    }
    let mut got = Vec::new();
    while let Some(i) = pool.pop_index() {
        got.push(i);
    }
    assert_eq!(
        got,
        vec![4, 3, 2, 1, 0],
        "LIFO round trip over the 8-cell pool"
    );
    assert_eq!(pool.pop_index(), None, "drained to empty");
}
