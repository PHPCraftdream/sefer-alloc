//! Free-side hot path for [`HeapCore`] (mechanical split of `heap_core.rs`,
//! task R6-CQ-7b; further split of the former flat `heap_core_free.rs`).
//!
//! This file holds the `impl HeapCore { .. }` block for `dealloc` and its
//! non-fastbin own-thread free body `dealloc_own_thread` (two of the most
//! safety-critical methods in the former monolith, each carrying a full
//! `# Safety` doc from R6-MS-1/2), plus the file-head items shared across
//! the `free/` split: the `medium_promotion_reachable!` cfg-predicate macro
//! (R19-8, task #344), the `MEDIUM_REALLOC_PROMOTION_THRESHOLD` const
//! (R14-4, task #289), and the `HARDENED_LARGE_NOOP_COUNT` diagnostic
//! counter (R22-12, task #363). The rest of the former file lives in the
//! sibling modules: `dealloc_own_base` (`dealloc_own_thread_with_base`) and
//! `realloc` (`realloc` + `try_promote_to_large`). Otherwise a pure
//! code-movement sibling of `heap_core.rs`; no other behavior changed — the
//! `# Safety` docs and `#[allow(unsafe_code)]` attributes moved byte-for-byte.

use core::alloc::Layout;

use crate::registry::heap_core::HeapCore;

/// R19-8 (task #344): single source of truth for the promotion-reachable
/// `#[cfg]` predicate, otherwise hand-duplicated across 4 sites in the
/// `free/` split
/// (the `MEDIUM_REALLOC_PROMOTION_THRESHOLD` const, branch (A)'s gated
/// block, `realloc`'s promotion call site, and `try_promote_to_large`'s own
/// definition). Wraps either an item (`const`/`fn`) or a statement (a bare
/// block or `if`) in the identical `#[cfg(...)]` gate so those 4 sites can
/// never textually diverge from each other.
///
/// Three arms, matched on a literal leading token rather than on the `item`/
/// `stmt` fragment specifiers directly, for a real reason discovered while
/// building this macro (not a style preference): a naive two-arm design —
/// `($item:item) => {...}; ($stmt:stmt) => {...};` — breaks no matter which
/// arm is listed first.
///
///   - `item` first: `macro_rules!` tries arms in order, and an
///     `item`-fragment parse that fails (e.g. against a leading `{` or `if`,
///     as at the two statement call sites) is a HARD parse error ("expected
///     an item keyword"), not a graceful fall-through to the next arm — it
///     aborts the whole match instead of trying the `stmt` arm that would
///     have succeeded.
///   - `stmt` first (or a single `stmt`-only arm): the `stmt` fragment
///     grammar DOES accept a bare item (a `const`/`fn` declaration is a
///     valid "item statement"), so it matches the two item call sites too —
///     but then splicing `#[cfg(...)] $stmt` back in at ITEM position (a
///     `const` at module scope, an `fn` inside `impl HeapCore`) fails
///     ("expected item after attributes"): an interpolated `stmt` fragment
///     is opaque, and an outer attribute directly preceding it cannot be
///     re-attached to the item hiding inside it.
///
/// The fix: capture the `const`/`fn` sites via `$(#[$m:meta])* $vis:vis
/// const/fn $($rest:tt)*` (the `$vis:vis` binder added in the `free/` split
/// so the split's widened `pub(super)` `MEDIUM_REALLOC_PROMOTION_THRESHOLD`
/// site still matches this arm — `$vis:vis` matches an absent visibility
/// too, so private sites are unchanged) — plain token-tree matching, which
/// is transparent (no
/// fragment-opacity restriction) and never commits to full item/stmt grammar
/// before falling through, so a literal-keyword mismatch (e.g. `const`
/// against a `{`-led block) is a graceful "try the next arm", not a hard
/// error. The generic `$stmt:stmt` arm last catches the two remaining
/// (block / bare `if`) call sites, where the fragment IS spliced back into
/// genuine statement position, so no opacity issue arises there.
///
/// Does NOT cover the `SegmentKind` import cfg or branch (B)'s cfg — both
/// COMBINE this predicate with `hardened` (union / negation respectively),
/// and Rust's `#[cfg(...)]` attribute cannot accept a macro invocation as its
/// argument, so those two remain hand-written (see their own comments for
/// the cross-reference back to this macro).
macro_rules! medium_promotion_reachable {
    ($(#[$m:meta])* $vis:vis const $($rest:tt)*) => {
        $(#[$m])*
        #[cfg(all(
            feature = "medium-classes",
            any(
                not(feature = "exact-span-large"),
                all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
            )
        ))]
        $vis const $($rest)*
    };
    ($(#[$m:meta])* fn $($rest:tt)*) => {
        $(#[$m])*
        #[cfg(all(
            feature = "medium-classes",
            any(
                not(feature = "exact-span-large"),
                all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
            )
        ))]
        fn $($rest)*
    };
    ($stmt:stmt) => {
        #[cfg(all(
            feature = "medium-classes",
            any(
                not(feature = "exact-span-large"),
                all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
            )
        ))]
        $stmt
    };
}

// Re-exported (sibling-module visibility only) so the other `free/` modules
// (`dealloc_own_base`, `realloc`) can invoke it at their own call/definition
// sites; `dealloc` itself uses it textually below.
pub(super) use medium_promotion_reachable;

medium_promotion_reachable! {
    /// R14-4 (task #289): the requested-size threshold above which a GROWING
    /// realloc of a Small/medium-classified block (`medium-classes` only) is
    /// diverted directly to a Large allocation instead of walking the medium
    /// size-class ladder one class at a time. 256 KiB — the smallest
    /// `medium-classes` class — per the design doc's recommendation
    /// (`R11_3_REALLOC_SMALL_TO_LARGE_PROMOTION_DESIGN.md` §0): by this size the
    /// object has already paid for at least one medium-class carve, so promoting
    /// it is not premature for a buffer that turns out to be a one-shot small
    /// object, while still capturing roughly half the growth ladder's copy cost
    /// (the doc's own threshold sweep: 128 KiB promotes too eagerly, paying RSS
    /// for objects that may never grow again; 384 KiB defers too long, leaving
    /// most of the ladder-walk cost on the table).
    ///
    /// R15-3 (task #305, review finding P2-3): gated by the SAME extended
    /// predicate as the promotion call site and `try_promote_to_large` below, not
    /// bare `medium-classes` — see the call site's doc comment for the full root
    /// cause. This constant is used ONLY inside that narrower-gated call site, so
    /// its own `#[cfg]` must match exactly or it becomes an "unused constant"
    /// warning (promoted to a hard error under `-D warnings`, per this crate's
    /// `npm run check`) in any build where `medium-classes` is on but the
    /// zero-headroom exclusion has switched promotion off.
    ///
    /// R19-8 (task #344): the `#[cfg]` gating this const is now generated by
    /// the `medium_promotion_reachable!` macro above, not hand-written.
    pub(super) const MEDIUM_REALLOC_PROMOTION_THRESHOLD: usize = 256 * 1024;
}

/// DIAGNOSTIC (R22-12, task #363): process-wide count of `hardened` defensive
/// no-ops fired on a Large-kind own-thread `dealloc` — i.e. a `GlobalAlloc`
/// contract violation (a fabricated/mismatched small layout freeing a pointer
/// that actually lives in a Large segment) that `hardened` detected and
/// rejected instead of silently corrupting the magazine.
///
/// **One shared counter for BOTH of this file's defensive-no-op return
/// points**, not two independent ones:
///
///   - **branch (A)'s mismatch case** (`dealloc_own_thread_with_base`,
///     promotion compiled in): `large_layout_consistent` rejects the
///     caller's layout against the segment's current `large_size`/
///     `large_align` (R19-1/task #337, extended by R22-5/task #356 to also
///     check align) — the free degrades to a no-op instead of really
///     freeing the segment.
///   - **branch (B)** (`dealloc_own_thread_with_base`, promotion NOT
///     compiled in): `kind_at(base) == Large` on a free keyed by a small
///     layout is, in a promotion-off build, structurally ALWAYS a contract
///     violation (task #25's original defensive no-op).
///
/// From an integrator's point of view these are the SAME observable event —
/// "hardened detected and rejected a contract-violating Large free" — just
/// reached via two different `#[cfg]`-mutually-exclusive code shapes (branch
/// (A) requires the promotion predicate; branch (B) requires its negation;
/// see this file's own comments at each branch for why they can never both
/// compile in the same build). Splitting them into two counters would ask an
/// integrator to know which internal branch fired for a distinction that
/// carries no actionable difference on their side — both mean "fix your
/// caller, it is passing a layout that does not match the pointer".
///
/// **This counter has ZERO effect on allocator behavior** — the decision to
/// reject was already made by the surrounding `if`/`#[cfg]`; this only counts
/// how often that pre-existing decision fires. Read via
/// [`HeapCore::dbg_hardened_large_noop_count`]. Reads 0 unless `alloc-stats`
/// is on — the per-event increment is gated behind `alloc-stats`, matching
/// [`OPT_H_ATTEMPTS`](crate::alloc_core::alloc_core::OPT_H_ATTEMPTS)'s
/// convention; the static itself is always compiled (gated on `alloc-core`,
/// which both `hardened` and plain `medium-classes` promotion depend on
/// transitively — see `Cargo.toml`) so the accessor has a stable definition
/// regardless of the rest of the feature set. Relaxed ordering — a
/// diagnostic count, not a synchronization primitive.
#[cfg(feature = "alloc-core")]
pub(crate) static HARDENED_LARGE_NOOP_COUNT: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

impl HeapCore {
    /// Deallocate `ptr` (previously returned by [`alloc`](Self::alloc)).
    ///
    /// Own-thread path: routes to the owning segment's `BinTable` via
    /// [`AllocCore::dealloc`](crate::alloc_core::AllocCore::dealloc) (which applies the M2 double-free guard).
    /// Under `alloc-xthread`: if the segment is stamped with another heap's
    /// head, route cross-thread via the TFS (the §2.2 protocol re-based on
    /// the registry). Foreign pointers (not a sefer segment) are a safe
    /// no-op.
    ///
    /// This is an **`unsafe fn`** (R6-MS-1/2): it forwards the
    /// [`AllocCore::dealloc`](crate::alloc_core::AllocCore::dealloc) caller-pointer contract. The crate's former
    /// posture was a safe `pub fn`; reversed after the round5 review — see
    /// [`AllocCore::dealloc`](crate::alloc_core::AllocCore::dealloc)'s `# Safety` and `CHANGELOG.md` (R6-MS-1/2).
    ///
    /// # Safety
    ///
    /// The caller must uphold the [`GlobalAlloc::dealloc`] contract for `ptr`
    /// and `layout`:
    ///
    /// - `ptr` is **null** OR the exact **start** pointer of a currently-LIVE
    ///   allocation made by *this* `HeapCore` (own segment, or — under
    ///   `alloc-xthread` — a live segment owned by another heap in the same
    ///   process that this call will route cross-thread). It MUST NOT be an
    ///   interior pointer.
    /// - `layout` exactly matches the allocation's layout.
    /// - The allocation is freed **at most once**; `ptr` is not re-issued
    ///   after this call.
    /// - `ptr` is not a foreign / already-released-unmapped base.
    ///
    /// Null `ptr` is always safe (early return). The M2 defensive paths remain
    /// as defence-in-depth, not a substitute for the contract.
    #[inline(always)]
    #[allow(unsafe_code)] // R6-MS-1/2: `unsafe fn` boundary (caller-pointer contract).
    pub unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        #[cfg(feature = "alloc-xthread")]
        {
            self.dealloc_routing(ptr, layout);
        }
        #[cfg(not(feature = "alloc-xthread"))]
        {
            self.dealloc_own_thread(ptr, layout);
        }
    }

    /// Own-thread dealloc: small frees go to the magazine (under fastbin),
    /// everything else to `core.dealloc`. Called from the `!alloc-xthread`
    /// path (no routing needed) and from `dealloc_routing` after confirming
    /// the block is ours.
    ///
    /// Э9 (P7.1, task #160): under fastbin this delegates to
    /// [`dealloc_own_thread_with_base`](Self::dealloc_own_thread_with_base),
    /// computing `base = os::segment_base_of_ptr(ptr)` itself. The
    /// Э9 (P7.1): under fastbin the magazine body lives in
    /// [`dealloc_own_thread_with_base`](Self::dealloc_own_thread_with_base)
    /// (which takes the pre-computed `base`), and BOTH callers of the
    /// own-thread path under fastbin already hold `base` (the cross-thread
    /// `dealloc_routing` from its `contains_base` check; there is no
    /// `!alloc-xthread` caller under fastbin since `fastbin ⟹ alloc-xthread`).
    /// So this own-arg wrapper is compiled ONLY when fastbin is OFF — where the
    /// own-thread path has no magazine and simply delegates to `core.dealloc`.
    /// Callers: the `!alloc-xthread` branch of [`dealloc`](Self::dealloc) and
    /// the non-fastbin arm of `dealloc_routing`.
    #[cfg(not(all(feature = "alloc-global", feature = "fastbin")))]
    #[inline(always)]
    pub(crate) fn dealloc_own_thread(&mut self, ptr: *mut u8, layout: Layout) {
        // Non-fastbin own-thread free: no magazine — delegate to core.
        // SAFETY: this own-thread body is reached only from `HeapCore::dealloc`,
        // an `unsafe fn` whose caller bound `ptr`/`layout` to the
        // `GlobalAlloc::dealloc` contract (valid live start pointer, matching
        // layout, freed once); we forward the same pair unchanged.
        #[allow(unsafe_code)] // R6-MS-1/2: unsafe call into `AllocCore::dealloc`.
        unsafe {
            self.core.dealloc(ptr, layout)
        };
    }
}
