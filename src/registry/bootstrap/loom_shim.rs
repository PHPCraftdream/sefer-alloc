//! Const-capable, `core::sync::atomic`-backed stand-in for
//! `once_ptr_cell::OncePtrCell`, used ONLY under `--cfg loom` so sefer's own
//! (unrelated) shadow-model loom harnesses can link the crate with its const
//! `REGISTRY` static intact — see the import-site comment above. Mirrors the
//! real cell's CAS/Release-publish/spin/rollback protocol AND its panic
//! safety, alignment guard, and `dbg_rollback_reenterable` clobber
//! protection (task #1359, first once-ptr-cell publication audit finding
//! F2 — an earlier version of this shim was missing all four and its doc
//! comment overclaimed "behaviourally faithful" anyway). Built on `core`
//! atomics so `new` stays `const` (`once_ptr_cell::OncePtrCell::new` is
//! non-`const` under the crate's OWN `--cfg loom`, since loom atomics have
//! no const constructor — this shim exists so the ROOT crate's `--cfg
//! loom` builds, which force once-ptr-cell to compile under its `loom`
//! branch too via the shared RUSTFLAGS cfg, still get a workable `static
//! REGISTRY` initializer). It is NEVER on a loom-modeled interleaving
//! (sefer's loom tests do not touch chunk cells), so it needs no loom
//! atomics for the interleavings themselves — only for this same
//! const-constructor constraint.
use core::marker::PhantomData;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicPtr, Ordering};

// The probe's RESULT type is pure data with no atomics, so it is
// loom-agnostic and the shim reuses the real crate's enum rather than
// duplicating it -- same reasoning as the `TaggedIndex` packing reused by
// the CRATE-P7 shim below.
pub(crate) use once_ptr_cell::RollbackProbe;

const SENTINEL_INITIALIZING: usize = 1;

// repr(transparent): mirrors the real crate's own layout guarantee
// (once-ptr-cell's publication readiness review, run 5, finding F1) --
// this shim is a test-only stand-in for the real type, so its layout
// should not silently diverge from what the real crate now promises.
#[repr(transparent)]
pub(crate) struct OncePtrCell<T> {
    ptr: AtomicPtr<T>,
    _marker: PhantomData<*mut T>,
}

// SAFETY: mirrors the real cell — only a raw `*mut T` ever crosses threads.
unsafe impl<T> Send for OncePtrCell<T> {}
// SAFETY: see the `Send` impl.
unsafe impl<T> Sync for OncePtrCell<T> {}

/// Rolls the sentinel back to `null` on `Drop` unless [`defuse`](Self::defuse)
/// was called first — mirrors `once_ptr_cell`'s own `RollbackGuard`
/// (task #706's fix), so a panicking `init` closure here also leaves the
/// cell `UNINIT` instead of wedged in `INITIALIZING` forever.
struct RollbackGuard<'a, T> {
    ptr: &'a AtomicPtr<T>,
    defused: bool,
}

impl<'a, T> RollbackGuard<'a, T> {
    #[inline]
    fn new(ptr: &'a AtomicPtr<T>) -> Self {
        Self {
            ptr,
            defused: false,
        }
    }

    #[inline]
    fn defuse(&mut self) {
        self.defused = true;
    }
}

impl<T> Drop for RollbackGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        if !self.defused {
            self.ptr.store(core::ptr::null_mut(), Ordering::Release);
        }
    }
}

impl<T> OncePtrCell<T> {
    /// # Panics
    ///
    /// Panics (as a const-eval failure in the `static` usage this shim
    /// exists for) if `align_of::<T>() < 2` — mirrors
    /// `once_ptr_cell::OncePtrCell::new`'s own guard: the `INITIALIZING`
    /// sentinel is the address `1`, which needs a spare low bit.
    pub(crate) const fn new() -> Self {
        assert!(
            core::mem::align_of::<T>() >= 2,
            "OncePtrCell<T> requires align_of::<T>() >= 2 so the INITIALIZING \
             sentinel (address 1) can never collide with a real published pointer"
        );
        OncePtrCell {
            ptr: AtomicPtr::new(core::ptr::null_mut()),
            _marker: PhantomData,
        }
    }

    fn is_ready(p: *mut T) -> bool {
        let a = p.addr();
        a != 0 && a != SENTINEL_INITIALIZING
    }

    pub(crate) fn get(&self) -> Option<NonNull<T>> {
        let p = self.ptr.load(Ordering::Acquire);
        if Self::is_ready(p) {
            // SAFETY: `is_ready(p)` proved `p` is neither null nor the
            // sentinel, so it is a real published pointer.
            Some(unsafe { NonNull::new_unchecked(p) })
        } else {
            None
        }
    }

    pub(crate) fn get_or_try_init<F>(&self, init: F) -> Option<NonNull<T>>
    where
        F: FnOnce() -> Option<NonNull<T>>,
    {
        let sentinel = core::ptr::without_provenance_mut::<T>(SENTINEL_INITIALIZING);
        loop {
            let p = self.ptr.load(Ordering::Acquire);
            if Self::is_ready(p) {
                // SAFETY: `is_ready(p)` proved `p` is neither null nor the
                // sentinel, so it is a real published pointer.
                return Some(unsafe { NonNull::new_unchecked(p) });
            }
            match self.ptr.compare_exchange(
                core::ptr::null_mut(),
                sentinel,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // Hold a rollback guard across `init()` so a panicking
                    // `init` also rolls the sentinel back (task #706-class
                    // fix, mirroring the real crate).
                    let mut guard = RollbackGuard::new(&self.ptr);
                    match init() {
                        Some(ptr) => {
                            let raw = ptr.as_ptr();
                            // Release-active check (task #707-class fix,
                            // mirroring the real crate): a safe `init`
                            // closure can hand back the sentinel address
                            // itself, which would otherwise get published
                            // as if `READY` and wedge every reader.
                            assert!(
                                Self::is_ready(raw),
                                "OncePtrCell (loom_shim): init returned the null/sentinel address"
                            );
                            self.ptr.store(raw, Ordering::Release);
                            guard.defuse();
                            return Some(ptr);
                        }
                        None => {
                            guard.defuse();
                            self.ptr.store(core::ptr::null_mut(), Ordering::Release);
                            return None;
                        }
                    }
                }
                Err(_) => loop {
                    let p = self.ptr.load(Ordering::Acquire);
                    let a = p.addr();
                    if a == SENTINEL_INITIALIZING {
                        core::hint::spin_loop();
                        continue;
                    }
                    if a != 0 {
                        // SAFETY: `a != 0` rules out null, and the
                        // `== SENTINEL_INITIALIZING` arm above already
                        // returned — so `p` is a real published pointer.
                        return Some(unsafe { NonNull::new_unchecked(p) });
                    }
                    break;
                },
            }
        }
    }

    pub(crate) fn dbg_is_ready(&self) -> bool {
        Self::is_ready(self.ptr.load(Ordering::Acquire))
    }

    /// Mirrors `once_ptr_cell::OncePtrCell::dbg_rollback_reenterable`'s
    /// own conditional-restore contract (task #1359-class fix): the
    /// final restore-to-null only fires if this probe's own
    /// postcondition CAS actually re-won the cell. If a concurrent
    /// `get_or_try_init` raced in during the probe's rollback-then-reCAS
    /// window and won instead, storing `null` unconditionally here would
    /// clobber that other owner's sentinel or published pointer — the
    /// exact clobber this probe must not cause.
    pub(crate) fn dbg_rollback_reenterable(&self) -> RollbackProbe {
        let sentinel = core::ptr::without_provenance_mut::<T>(SENTINEL_INITIALIZING);
        if self
            .ptr
            .compare_exchange(
                core::ptr::null_mut(),
                sentinel,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_err()
        {
            return RollbackProbe::NotApplicable;
        }
        self.ptr.store(core::ptr::null_mut(), Ordering::Release);
        let postcondition_holds = self
            .ptr
            .compare_exchange(
                core::ptr::null_mut(),
                sentinel,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok();
        if !postcondition_holds {
            return RollbackProbe::NotApplicable;
        }
        self.ptr.store(core::ptr::null_mut(), Ordering::Release);
        RollbackProbe::Proven
    }
}

// CRATE-P7: const-capable, `core`-atomic stand-in for
// the tagged free-list head (`tagged_index_stack::StackHead<16>`) +
// StackStorage/StackOps binding.
// -----------------------------------------------------------------------

use core::sync::atomic::AtomicU64;
// The PACKING (`TaggedIndex`) is pure `const fn` bit arithmetic with no
// atomics, and the `TAIL` sentinel carries no atomics either — both are
// loom-agnostic, so the shim reuses the REAL crate types for them and
// only re-implements the `AtomicU64` head on `core` atomics. The shim
// replicates the crate's `StackOps` push_index/pop_index HEAD PROTOCOL (same
// H-2 running-tag empty transition, the same CAS orderings with ONE
// deliberate divergence (note 6 below), same RAD-1 lazy links —
// `store_next` only ever fires inside `push_index`)
// but is NOT a byte-for-byte replica of the shipped type. Deliberate
// divergences, each irrelevant to what the shim is FOR — model-checking
// free_slots' head protocol in the root crate's loom CI jobs (deliberately
// no hardcoded count — a "THREE" here already went stale once):
//   1. no CAS-retry backoff — the shipped `push_index`/`pop_index` spin
//      `1 << spins.min(BACKOFF_SPIN_CAP)` times between retries; the shim
//      retries immediately, forever. The backoff is a pure LATENCY device:
//      `core::hint::spin_loop()` touches no atomic and adds no interleaving
//      loom could explore, so copying it in would add zero protocol content.
//   2. no `push_index` release-active `index < INDEX_MASK` guard — the guard
//      catches a caller-contract violation the registry's own construction
//      excludes here: only slot indices `< MAX_HEAPS (4096)` are ever
//      pushed, well under `INDEX_MASK (65535)`. Dead code in this shim.
//   3. no `pop_index` release-active rule-4 guard on `load_next`'s result —
//      same reasoning: `HeapSlot::next_free` is written ONLY by the shipped
//      `push_index` (with `TAIL` or a previously-admitted index), so the
//      guard's condition is unsatisfiable for this consumer.
//   4. no CAS-retry counting — the shipped `push_index`/`pop_index` each
//      increment a process-global telemetry counter
//      (`PUSH_RETRY_COUNT` / `POP_RETRY_COUNT`: plain
//      `core::sync::atomic::AtomicUsize` statics, a `Relaxed`
//      `fetch_add(1)` on the CAS-retry `Err(actual)` arm, read by nothing
//      in the algorithm) so tests can assert the retry branch was actually
//      reached; the shim has no counterpart. Retry bookkeeping is pure
//      activation telemetry with no bearing on the head-word CAS protocol
//      the shim exists to model-check, and a bare
//      `core::sync::atomic::AtomicU64` head carries no such metadata to
//      replicate.
//   5. the shipped `push_index`/`pop_index` now pack through the
//      crate-PRIVATE truncating fast path (`pack_truncating`); this
//      shim cannot name that private item, so it packs through the
//      checked public `pack` (which returns `Option`) instead. Since
//      the P1-1 seal fix (below) this needs no special-casing: the real
//      `push_index_impl` refuses instead of wrapping once the tag
//      reaches `TaggedIndex::TAG_MAX`, so the tag this shim ever hands
//      to `pack` is always `<= TAG_MAX` — always in range — and the
//      `.expect`s below make the shim's in-range caller guarantees
//      explicit; an `expect` firing would be a model bug, and it adds
//      no atomic ops for loom to interleave. (Pre-P1-1-fix this note
//      described an explicit tag-wrap mask at the push call site,
//      because the real code's `wrapping_add(1)` could legitimately
//      produce `2^TAG_BITS` at the old wrap boundary, which the checked
//      `pack` rejects; that boundary no longer exists in the real code
//      or this shim.)
//   6. the shim's `push_index` loads the head with `Acquire` where the
//      shipped `push_index_impl` loads it `Relaxed` (that load's own
//      comment in the crate proves `Relaxed` sufficient: push uses the
//      observed word ONLY as `(index, tag)` values, never to follow a
//      link). Stronger, not weaker, so the shim cannot exhibit a
//      weaker-memory-order behaviour the shipped code lacks; the REAL
//      verification of the shipped ordering is the crate's own loom
//      suite (`crates/tagged-index-stack/tests/loom_aba.rs`), and this
//      shim is never on a modeled interleaving anyway (see the
//      `loom_shim` note at its `use` site below). All other orderings
//      (push's Release/Relaxed CAS, pop's Acquire/Acquire CAS and
//      Acquire initial head load) are replicated exactly.
//   7. the shim's `push_index` is a SAFE `fn`; the real crate's
//      `StackOps::push_index` is now an `unsafe fn` carrying a caller-side
//      three-clause contract (the pushed index must be in the implementor's
//      declared link domain, must not currently be reachable through the
//      head, and must be backed by a unique, not-yet-consumed
//      publish/recycle authority epoch — freshly minted or obtained from
//      one successful pop — consumed by the push's own CAS). This is an
//      intentional, documented divergence of the test
//      shim, not lockstep drift: the loom model checks the head protocol,
//      not the `unsafe` boundary, and the shim's callers are the same
//      registry paths whose SAFETY proofs (see `heap_registry/stack.rs`'s
//      `push_free_slot`) discharge the real contract. The P1-1 seal
//      (`-> Result<(), TagExhausted>`, refusing once the tag reaches
//      `TaggedIndex::TAG_MAX`) is now part of the real protocol this
//      shim mirrors, so it is NOT listed as a divergence — see the shim's
//      `push_index` body below, which matches `push_index_impl`'s
//      seal-check placement (before any side effect) exactly.
// Keeping the shim minimal is deliberate: every shipped feature copied into
// it doubles the surface that can silently drift from the real type.
use tagged_index_stack::{TagExhausted, TaggedIndex, TAIL};

/// Const-capable stand-in for the tagged free-list head
/// (`tagged_index_stack::StackHead<16>`) + StackStorage/StackOps binding,
/// used ONLY under `--cfg loom`, so `static REGISTRY: Registry =
/// Registry::new()` still const-evaluates (loom's `AtomicU64::new` is
/// non-`const`). Never on a loom-modeled interleaving — the real-type
/// verification is the crate's own `loom_aba` suite. Fixed at
/// `INDEX_BITS = 16` (the only width the registry uses), so it needs no
/// const generic beyond mirroring the real type's parameter.
pub(crate) struct StackHead<const INDEX_BITS: u32> {
    head: AtomicU64,
}

impl<const INDEX_BITS: u32> StackHead<INDEX_BITS> {
    pub(crate) const fn new() -> Self {
        StackHead {
            // Bootstrap emptiness is index-only; tag 0 is the initial
            // generation. Keep this shim on the stable public packing
            // primitives instead of depending on a crate-private helper.
            head: AtomicU64::new(
                match TaggedIndex::<INDEX_BITS>::pack(TaggedIndex::<INDEX_BITS>::empty_index(), 0) {
                    Some(word) => word,
                    None => panic!("bootstrap empty halves must be in range"),
                },
            ),
        }
    }

    /// Thin wrapper over the head atomic's load (mirrors the real
    /// `StackHead::load`'s shape — for the `StackOps` blanket impl).
    pub(crate) fn load(&self, ordering: Ordering) -> u64 {
        self.head.load(ordering)
    }

    /// Thin wrapper over the head atomic's compare_exchange (mirrors the
    /// real `StackHead::compare_exchange`'s shape — for the `StackOps`
    /// blanket impl).
    pub(crate) fn compare_exchange(
        &self,
        current: u64,
        new: u64,
        success: Ordering,
        failure: Ordering,
    ) -> Result<u64, u64> {
        self.head.compare_exchange(current, new, success, failure)
    }
}

// Deliberately NOT converted to an `unsafe trait`: this mirror is
// `pub(crate)`, used only under `--cfg loom`, and never a public
// extension point — the 2026-09-01 unsafe-trait decision on the real
// `tagged_index_stack::StackStorage` does not reach it. It stays a
// plain, safe trait, while the REAL impl in `heap_registry/stack.rs` is an
// `unsafe impl` of the real trait.
/// Mirror of the real `tagged_index_stack::StackStorage` trait: one
/// implementor owns the head↔links binding.
pub(crate) trait StackStorage<const INDEX_BITS: u32> {
    fn head(&self) -> &StackHead<INDEX_BITS>;
    fn load_next(&self, index: u32) -> u32;
    fn store_next(&self, index: u32, next: u32);
}

/// Mirror of the real `tagged_index_stack::StackOps` trait, blanket-implemented
/// for every `StackStorage` implementor (same shape as the crate).
pub(crate) trait StackOps<const INDEX_BITS: u32>: StackStorage<INDEX_BITS> {
    /// Head-protocol replica of `StackOps::push_index` (Release CAS, tag
    /// bump, RAD-1 lazy link write inside push only, P1-1 seal at the
    /// tag ceiling; no backoff and no link-domain/liveness/
    /// exclusive-ownership caller-contract guard — see the module
    /// comment above).
    /// Divergence note 7: this is a safe `fn`, while the real crate's
    /// `StackOps::push_index` is now an `unsafe fn` with a caller-side
    /// link-domain + liveness + exclusive-ownership-epoch contract — an
    /// intentional, documented shim divergence, not lockstep drift. The
    /// `Result<(), TagExhausted>`
    /// return type and the seal check below are NOT a divergence: they
    /// mirror the real protocol exactly.
    fn push_index(&self, index: u32) -> Result<(), TagExhausted> {
        let head_ref = self.head();
        // Divergence note 6: deliberately STRONGER than the shipped
        // `push_index_impl`'s `Relaxed` initial head load.
        let mut head = head_ref.load(Ordering::Acquire);
        loop {
            let (cur_idx, tag) = TaggedIndex::<INDEX_BITS>::unpack(head);
            // P1-1 seal: mirrors `push_index_impl`'s check exactly —
            // same placement (after unpack, before any side effect), so
            // a first-attempt refusal here has no observable effect
            // either.
            if tag == TaggedIndex::<INDEX_BITS>::TAG_MAX {
                return Err(TagExhausted);
            }
            let next_link = if TaggedIndex::<INDEX_BITS>::is_empty(head) {
                TAIL
            } else {
                cur_idx
            };
            self.store_next(index, next_link);
            // In range by construction: `tag < TAG_MAX` (checked above),
            // so `tag + 1 <= TAG_MAX` — no wrap, no mask needed (see
            // divergence note 5).
            let new_tag = tag + 1;
            let new_head = TaggedIndex::<INDEX_BITS>::pack(index, new_tag).expect(
                "shim halves in range: slot index < INDEX_MASK (divergence note 2), tag <= TAG_MAX (checked above)",
            );
            match head_ref.compare_exchange(head, new_head, Ordering::Release, Ordering::Relaxed) {
                Ok(_) => return Ok(()),
                Err(actual) => head = actual,
            }
        }
    }

    /// Head-protocol replica of `StackOps::pop_index` (Acquire CAS, same
    /// tag; H-2 running-tag preservation on the empty transition; no
    /// backoff and no rule-4 guard — see the module comment above).
    fn pop_index(&self) -> Option<u32> {
        let head_ref = self.head();
        let mut head = head_ref.load(Ordering::Acquire);
        loop {
            if TaggedIndex::<INDEX_BITS>::is_empty(head) {
                return None;
            }
            let (index, tag) = TaggedIndex::<INDEX_BITS>::unpack(head);
            let next = self.load_next(index);
            let new_head = if next == TAIL {
                // H-2: preserve the RUNNING tag across the empty transition.
                TaggedIndex::<INDEX_BITS>::pack(TaggedIndex::<INDEX_BITS>::empty_index(), tag)
                    .expect("empty_index and the unpacked tag are both in range")
            } else {
                TaggedIndex::<INDEX_BITS>::pack(next, tag).expect(
                    "links hold only TAIL or admitted indices < INDEX_MASK (divergence note 3)",
                )
            };
            match head_ref.compare_exchange(head, new_head, Ordering::Acquire, Ordering::Acquire) {
                Ok(_) => return Some(index),
                Err(actual) => head = actual,
            }
        }
    }
}

impl<const B: u32, S: StackStorage<B> + ?Sized> StackOps<B> for S {}
