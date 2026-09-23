//! The `free_slots` tagged Treiber stack: `Registry`'s `StackStorage` impls
//! (the real `unsafe impl` and its `--cfg loom` mirror) plus the
//! `pop_free_slot` / `push_free_slot` / `bump_count` primitives built on
//! them.
#![allow(unsafe_code)]

use core::sync::atomic::Ordering;

#[cfg(loom)]
use crate::registry::bootstrap::loom_shim::StackOps as _;
#[cfg(not(loom))]
use tagged_index_stack::StackOps as _;

use crate::registry::bootstrap::{Registry, MAX_HEAPS};
use crate::registry::heap_slot::NEXT_FREE_TAIL;

// Treiber-stack primitives on the `free_slots` stack.
//
// The stack itself — the tagged head word, the ABA guard, the H-2
// empty-transition tag preservation, and the RAD-1 lazy-link discipline — now
// lives in the `tagged-index-stack` crate (CRATE-P7); `Registry::free_slots` is
// a `StackHead<16>`. `Registry` implements the crate's `StackStorage<16>` trait
// below — binding the head to the SLOT-RESIDENT links ONCE, per impl (the P1-1
// structural fix: one implementor owns the head↔links binding, instead of the
// old per-call `RegistryLinks` adapter re-asserting it on every `push`/`pop`) —
// so the crate's blanket `StackOps` `push_index`/`pop_index` write each slot's
// next link into the slot's own `next_free: AtomicU32` field (never
// a second array) exactly as the hand-rolled Treiber loop used to — preserving
// the lazy-links saving (the crate only ever writes a link inside
// `push_index`).

/// Binds `Registry`'s head (`free_slots`) to its slot-resident `next_free`
/// links via the crate's [`tagged_index_stack::StackStorage`] trait (one impl
/// establishes the binding, once). `load_next`/`store_next` funnel through
/// `reg.slot(idx)` (the single chunk-materialising slot accessor), so the
/// crate's stack reaches the exact per-slot `AtomicU32` the previous inline
/// Treiber loop used — with the same `Acquire`/`Release` orderings the crate's
/// contract requires.
///
/// The crate's `TAIL` sentinel (`u32::MAX`) is numerically identical to
/// [`NEXT_FREE_TAIL`], so a slot link written by `push_index` reads back as the
/// familiar tail sentinel; the compile-time `const` assert below pins that
/// equivalence so a future divergence fails loudly at compile time (in every
/// build profile) rather than silently corrupting a chain.
///
/// The two cfg-gated impls below have IDENTICAL bodies, deliberately: under
/// `--cfg loom`, `Registry::free_slots` is the const-capable loom shim
/// (`bootstrap::loom_shim::StackHead`), whose `StackStorage` mirrors the real
/// trait's shape — so the bodies must stay in lockstep with each other.
/// The cfg-gating is no longer symmetric in KIND: the real (non-loom) impl
/// below is an `unsafe impl` of the real trait (which is `unsafe` as of the
/// 2026-09-01 conversion — see the crate's "Where unsafe lives" docs),
/// carrying the `// SAFETY:` justification above the impl; the `--cfg loom`
/// mirror impl stays a PLAIN impl of the loom shim's own `pub(crate)`
/// mirror trait, deliberately NOT converted (see the note at its declaration
/// in `bootstrap/loom_shim.rs`). The bodies remain in lockstep.
const _: () = assert!(
    NEXT_FREE_TAIL == tagged_index_stack::TAIL,
    "the registry's tail sentinel must equal the crate's TAIL so slot links \
     round-trip through the tagged stack unchanged"
);

// SAFETY: `Registry` upholds every clause of the real
// `tagged_index_stack::StackStorage` `# Safety` contract:
// 1. One live binding per head, for the head's whole life — `Registry` is a
//    process singleton (`static REGISTRY: Registry = Registry::new()` in
//    `src/registry/bootstrap/registry.rs`; `Registry::new` is a private `const fn`;
//    sole in-crate access via `bootstrap::ensure() -> &'static Registry`), and
//    `free_slots` is `pub(crate)` whose ONLY reference ever handed out is
//    this impl's `head()` (the cfg(loom) mirror below under loom), consumed
//    only by the crate's push/pop CAS loops.
//    ACKNOWLEDGED EXPOSURE (run-5 audit P3-1): under
//    `alloc-global + internals`, `sefer_alloc::registry` is a
//    `#[doc(hidden)] pub` module, so `ensure()` hands `&'static Registry` to
//    EXTERNAL safe code, and the crate's `StackOps` blanket impl drives
//    `pop_index` — a safe `fn` taking no witness — through this binding from
//    outside the crate. `push_index`, by contrast, is now an `unsafe fn`
//    carrying the crate's three-clause caller contract (link domain +
//    liveness + exclusive ownership epoch), so external SAFE code cannot
//    drive it (E0133): any external push must be an `unsafe` call whose
//    author takes on that contract. The remaining
//    safe-code surface (`pop_index`, and the impl's load/store pairs via the
//    push/pop loops with adversarial indices) bounds the exposure to an
//    AVAILABILITY hazard — an unauthorized pop can only leak/livelock, never
//    double-issue a slot (the slot-state CAS remains the defence; see clause
//    3's bound) — not a soundness one.
// 2. One backing, consistently — `load_next`/`store_next` both resolve the
//    index through `Registry::slot` (the single index→slot path) onto the
//    SAME slot-resident `next_free: AtomicU32` cell; slots are `&'static`,
//    chunks materialise exactly once, so the index↔cell mapping is stable
//    for the process life.
// 3. Disjoint reachable-index populations — the only non-doc references to
//    `next_free` in the crate are the two cfg-gated impls' load/store pairs
//    plus the field declaration in `heap_slot.rs`, so no second binding over
//    the cells exists. (The clause-1 `internals` exposure adds no second
//    binding either — external code reaches the cells only through THIS
//    impl's load/store pairs, inside the crate's own push/pop loops — but it
//    can drive those loops with adversarial indices. The slot-state CAS is
//    the defence that bounds the consequence: every index the stack hands to
//    a heap owner must additionally win that slot's `FREE → LIVE` CAS in
//    `claim`/`claim_with_config`, and every push-back is preceded by the
//    winner's `LIVE → FREE` CAS — so an adversarial external caller can
//    steal a FREE slot off the list (a leak), livelock `claim`, or trip the
//    pop-side rule-4 panic: availability failures, never double ownership
//    of a `HeapCore`.)
// 4. Valid answers, dedicated cells — `next_free` is a dedicated per-slot
//    link field, never payload-aliased; every pushed index is `< MAX_HEAPS =
//    4096` (minted by `bump_count`, which returns `None` at `>= MAX_HEAPS`,
//    or a previously minted/recycled one re-pushed only through
//    `push_free_slot`, whose `debug_assert!` enforces the bound) and
//    `MAX_HEAPS (4096) < INDEX_MASK (0xFFFF at INDEX_BITS = 16)`, the empty
//    sentinel reserved above the cap.
// 5. Same logical head every call — `head()` returns `&self.free_slots`, a
//    fixed field.
// 6. Declared link domain — `Registry`'s domain is `0..MAX_HEAPS` (4096),
//    documented here and fixed for the process life: the chunked slot
//    array's allocation policy guarantees every index in that domain has a
//    materialised backing cell. `load_next`/`store_next` resolve indices
//    through the CHECKED `slot()` accessor, so they are memory-safe for
//    every in-domain index (indeed for any index — no unchecked access is
//    used anywhere in the impl).
// 7. Atomic cells — the link cells are `AtomicU32` fields (`next_free`),
//    accessed only through atomic `load`/`store` with `Acquire`/`Release`.
#[cfg(not(loom))]
unsafe impl tagged_index_stack::StackStorage<16> for Registry {
    #[inline]
    unsafe fn head(&self) -> &tagged_index_stack::StackHead<16> {
        &self.free_slots
    }

    #[inline]
    unsafe fn load_next(&self, index: u32) -> u32 {
        // R6-OPT-P0-2: `index < MAX_HEAPS` by construction (the stack only ever
        // holds indices that `push_free_slot` put there, and those are valid
        // slot indices); `slot()` resolves it through the chunked slot array.
        // NOTE (CRATE-P7): the pre-swap inline `pop_free_slot` carried a
        // defensive `idx >= MAX_HEAPS -> None` early return; the crate delegation
        // intentionally drops it in favour of FAIL-LOUD — a head word so
        // corrupted that it names an out-of-range index now panics (OOB in
        // `slot()`) rather than silently returning an empty stack, which is the
        // preferable failure mode for an unreachable-by-construction invariant.
        self.slot(index as usize).next_free.load(Ordering::Acquire)
    }

    #[inline]
    unsafe fn store_next(&self, index: u32, next: u32) {
        self.slot(index as usize)
            .next_free
            .store(next, Ordering::Release);
    }
}

#[cfg(loom)]
impl crate::registry::bootstrap::loom_shim::StackStorage<16> for Registry {
    #[inline]
    fn head(&self) -> &crate::registry::bootstrap::loom_shim::StackHead<16> {
        &self.free_slots
    }

    #[inline]
    fn load_next(&self, index: u32) -> u32 {
        // R6-OPT-P0-2: `index < MAX_HEAPS` by construction (the stack only ever
        // holds indices that `push_free_slot` put there, and those are valid
        // slot indices); `slot()` resolves it through the chunked slot array.
        // NOTE (CRATE-P7): the pre-swap inline `pop_free_slot` carried a
        // defensive `idx >= MAX_HEAPS -> None` early return; the crate delegation
        // intentionally drops it in favour of FAIL-LOUD — a head word so
        // corrupted that it names an out-of-range index now panics (OOB in
        // `slot()`) rather than silently returning an empty stack, which is the
        // preferable failure mode for an unreachable-by-construction invariant.
        self.slot(index as usize).next_free.load(Ordering::Acquire)
    }

    #[inline]
    fn store_next(&self, index: u32, next: u32) {
        self.slot(index as usize)
            .next_free
            .store(next, Ordering::Release);
    }
}

/// Pop a free slot index off the `free_slots` stack, or `None` if empty.
///
/// Delegates to [`tagged_index_stack::StackOps::pop_index`] through
/// `Registry`'s own `StackStorage` impl (the slot-resident links): the crate
/// performs the tagged Treiber pop (the ABA guard and the H-2
/// empty-transition tag preservation are inside it), reading the popped slot's
/// `next_free` link via the impl's `load_next`.
pub(super) fn pop_free_slot(reg: &Registry) -> Option<usize> {
    reg.pop_index().map(|idx| idx as usize)
}

/// Push a slot index onto the `free_slots` stack.
///
/// Delegates to [`tagged_index_stack::StackOps::push_index`] (now an
/// `unsafe fn` returning `Result<(), TagExhausted>` — the P1-1 seal fix)
/// through `Registry`'s own `StackStorage` impl (the slot-resident links):
/// the crate writes this slot's `next_free` link (via the impl's
/// `store_next`) and bumps the ABA tag on the CAS. `idx < MAX_HEAPS` (the
/// caller derived it from a valid heap pointer), so it fits the 16-bit index
/// half with room below the empty sentinel.
///
/// # `Err(TagExhausted)` handling — an intentional, documented one-slot leak
///
/// `free_slots`' tag is strictly monotonic (never wraps — see this module's
/// "ABA defence" section) and SEALS once it reaches
/// `tagged_index_stack::TaggedIndex::TAG_MAX`: every push through this ONE
/// head thereafter returns `Err(TagExhausted)`, permanently (pops are
/// unaffected). Reaching that point needs `>= 2^48 - 1` successful pushes
/// onto `free_slots` over this process's whole lifetime — this module's ABA
/// defence section's own budget analysis puts that at a MINIMUM of ~3.3 days
/// of continuously saturated pushes at the crate's documented hardware
/// ceiling, realistically far longer for an actual workload — astronomically
/// rare, but no longer merely "improbable" the way a wrap-based ABA
/// collision used to be: this crate's contract now makes it a real, though
/// vanishingly unlikely, terminal event, not a probabilistic risk. On that
/// `Err`, the caller has ALREADY performed the `LIVE → FREE` slot-state CAS
/// (in `recycle` / `push_back_after_oom`), so `idx`'s slot is genuinely
/// `FREE` — but the index itself never rejoins `free_slots`: it is dropped
/// here, leaking exactly ONE slot out of `MAX_HEAPS` (never reclaimed,
/// silently unavailable to future `claim`s). This is the deliberate
/// trade-off, not an oversight: panicking in an allocator's free path (an
/// abort-equivalent failure mode) is worse than losing one slot once, this
/// deep into the tag's lifetime, and propagating the error up through
/// `recycle`'s `pub unsafe fn` (a `GlobalAlloc`-adjacent deallocation path
/// with no natural place to surface a `Result`) would force every caller —
/// realistically forever unreachable — to handle a case with no actionable
/// recovery. Once `free_slots` is sealed, every SUBSEQUENT recycle leaks one
/// more slot the same way — a slow-motion, terminal exhaustion of the free
/// list, not a single one-off loss — matching `StackHead`'s own documented
/// "Sealing is permanent — no reset" posture (`tagged-index-stack`'s
/// crate docs): no reset/rotation API exists or is planned, by design.
pub(super) fn push_free_slot(reg: &Registry, idx: u32) {
    debug_assert!(
        (idx as usize) < MAX_HEAPS,
        "push_free_slot given an out-of-range slot index"
    );
    // NOTE: the `debug_assert!` above is NOT part of this proof — it compiles
    // out under `--release` and cannot be relied on for a safety argument.
    // SAFETY: `StackOps::push_index`'s three-clause caller contract (link
    // domain + liveness + exclusive ownership epoch), upheld for
    // both callers of `push_free_slot` (`recycle` and `push_back_after_oom`;
    // R2-08 removed the third, `ConflictRollback::drop`):
    // - LINK DOMAIN: every index reaching here is `< MAX_HEAPS` release-actively
    //   — recycled indices via `recycle`'s `if idx >= MAX_HEAPS { return; }`
    //   early return (~line 356), freshly-minted indices via `bump_count`'s
    //   `None`-at-`>= MAX_HEAPS` cap (~line 751) — and
    //   `MAX_HEAPS (4096) < INDEX_MASK (0xFFFF)`, so `idx` is inside
    //   `Registry`'s declared link domain `0..MAX_HEAPS` (slot-resident
    //   `next_free` cells; the chunked slot array guarantees the cell exists
    //   for every in-domain index — `unsafe impl` SAFETY clause 6 above).
    // - LIVENESS: `idx` is not currently reachable through the head. A
    //   fresh-minted index has never been pushed. A recycled index reaches the
    //   push by one of two paths: (a) `recycle` — the push is gated on THIS
    //   caller's release-active `LIVE → FREE` CAS win (~line 368); an
    //   already-FREE slot loses that CAS and early-returns without pushing, so
    //   a slot still on the free list can never reach the push; (b)
    //   `push_back_after_oom` — here the
    //   liveness leg is the documented sole-writer invariant, NOT a
    //   release-active gate: the caller won the slot's `FREE → LIVE` CAS in
    //   `claim`/`claim_with_config` and is its sole writer until the push, so
    //   the slot is `LIVE` — by construction not on the free list (a slot is
    //   only ever pushed after leaving `LIVE`, and only `claim`'s `FREE →
    //   LIVE` winner can put it back there); the `LIVE → FREE` CAS in this
    //   path is defensive shape whose success is debug-asserted only (run-5
    //   P4-3). In both paths the slot-state machine guarantees at most one
    //   owner, so the index is never simultaneously reachable and re-pushed —
    //   and (P1-1 fix) this liveness argument is now UNCONDITIONALLY sound,
    //   not conditionally-safe-modulo-wrap: previously a lost-CAS observer
    //   parked across a full tag wrap could reinstall a stale expectation
    //   regardless of the slot-state machine; since the tag can no longer
    //   wrap (it seals instead — see `push_index`'s `# Errors`), no amount of
    //   further churn can ever reinstate a stale `(index, tag)` pair, so this
    //   argument holds for the head's ENTIRE lifetime, not just "until an
    //   astronomically distant wrap".
    // - EXCLUSIVE OWNERSHIP EPOCH: every index reaching this push arrives
    //   with a unique, not-yet-consumed publish/recycle authority. A
    //   fresh-minted index (`bump_count`) has never been pushed: its
    //   authority is freshly minted. A recycled index's authority was
    //   obtained by the ONE successful `pop_index` (via `pop_free_slot` in
    //   `pick_slot`, called by `claim`/`claim_with_config`) whose CAS
    //   removed it from the free list — that pop transferred authority to
    //   the `FREE → LIVE` winner,
    //   and the slot-state machine keeps that winner the authority's ONLY
    //   holder until it pushes back: while the slot is `LIVE` it is not on
    //   the free list, so no second `pop_index` can mint a competing epoch,
    //   and the slot becomes pushable again only through its owner's own
    //   `LIVE → FREE` CAS (`recycle`'s is release-active and a loser
    //   early-returns without pushing, so an already-FREE — still-listed —
    //   slot can never reach this push). This push consumes that authority
    //   at its own successful head CAS (push_index clause 3): two pushes
    //   acting on the SAME epoch cannot occur, because between any two
    //   pushes of one index there is always the intervening successful
    //   `pop_index` that re-listed it.
    // Under loom the mirror `StackOps::push_index` (bootstrap/loom_shim.rs)
    // is a SAFE fn (divergence note 7), so the unsafe block is cfg-gated.
    #[cfg(not(loom))]
    let result = unsafe { reg.push_index(idx) };
    #[cfg(loom)]
    let result = reg.push_index(idx);

    // See this function's own doc ("`Err(TagExhausted)` handling") for why
    // this is an intentional no-op leak, not a panic or a propagated error.
    let _ = result;
}

/// Mint a fresh slot by bumping `count`. Returns the new slot's index, or
/// `None` if `count` has reached `MAX_HEAPS`. The new slot is already in its
/// bootstrap state (`FREE`, generation 0, heap uninitialised) thanks to the
/// `const` initialiser; no extra init is needed.
pub(super) fn bump_count(reg: &Registry) -> Option<usize> {
    // fetch_add is RMW: AcqRel so we see any prior slot writes (none needed
    // here, but conservative) and later claimers see our bump.
    let idx = reg.count.fetch_add(1, Ordering::AcqRel);
    if idx as usize >= MAX_HEAPS {
        // Roll back the bump (best-effort) and report exhaustion. Under
        // concurrency a rollback race is benign (the cap is a soft bound; an
        // over-bump just wastes an index slot).
        reg.count.fetch_sub(1, Ordering::AcqRel);
        return None;
    }
    Some(idx as usize)
}
