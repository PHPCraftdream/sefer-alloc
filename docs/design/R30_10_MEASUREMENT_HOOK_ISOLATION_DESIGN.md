# R30-10 — isolating measurement hooks as a distinct subsystem: DESIGN PROPOSAL, largely NOT implemented this round

**Task:** R30-10 (task #459), Round 30. **DESIGN-FIRST evaluation, as the
task brief explicitly requires** ("evaluate feasibility honestly before
committing to all of it... a partial adoption with a documented reason is
an acceptable outcome"). This document is the record of that evaluation:
what was measured, what was rejected on cost grounds (with the R24-6
precedent as the explicit constraint), and what — if anything — is worth
building later.

**Style precedent:** `docs/design/R30_7_TRIM_SCAVENGE_API_DESIGN.md` and
`docs/perf/R27_5_ADAPTIVE_POOL_BUDGET_DESIGN.md` — concrete signatures,
explicit invariants, an honest scope of what is NOT claimed, and a
recommendation that may be "not yet."

**Date:** 2026-07-30. **Base revision analyzed:** `main` @ `374c6d1`
(R30-9 landed) + the working tree's uncommitted R30-7 profile additions
(`src/alloc_core/profile.rs`, `src/alloc_core/large_cache_config.rs`,
`src/global/sefer_alloc.rs`, `tests/profile.rs`, `examples/r30_7_*` — these
show as uncommitted in `git status` at the start of this task but are
R30-7's, not this task's; this document does not modify them). Line
numbers and file lists below are current as of that tree, captured mechanically
(see §2 for the exact commands), not hand-counted.

---

## 0. TL;DR

**The survey (§2) puts a full relocation-of-every-hook in the same
disproportionate-cost bracket R24-6 already rejected for a SINGLE hook —
not close to it, several times past it.** `dbg_push_to_ring` alone (R24-6's
rejected case) touches 26 files; relocating the full `SAFE_MUTATORS` +
`UNSAFE_HOOKS` populations (the buckets actually worth moving, see below)
touches **102 distinct test/example/bench files**, and adding
`PURE_OBSERVERS` brings the total footprint to **139 distinct files** out
of this crate's ~227 test files. CLAUDE.md already carries R24-6's verdict
on a diff this shape: "a documentation-precision concern rather than a
regression" is not worth reproducing a 100+-file diff for. That verdict
transfers here unchanged in kind, and the number is worse in degree.

**The recurring bug class (R25-1, R29-7, R29-8, R29-17, R30-1) was never
caused by hooks being scattered across files.** Every one of those five
fixes was resolved by changing WHAT a hook's body does or WHAT its
signature guarantees (`unsafe fn` + validated pointer; delegate through a
cursor-safe internal primitive) — never by moving the hook to a different
file. R30-1's own fix, cited explicitly by this task's brief, is the
existence proof: `reserve_small_segment_impl` closed a real dangling-cursor
bug by routing through a state-safe primitive, in the SAME file
(`alloc_core_small_pool.rs`), with no relocation at all. File-level
scatter is therefore not the load-bearing defect; §3 works through this
in detail.

**What IS structurally new value, found by inspecting a live hook, not
hypothesized:** `dbg_decomp_reserve_and_keep` / `dbg_decomp_release`
(`src/alloc_core/alloc_core_small_pool.rs:1070-1115`) already hands a raw
`*mut u8` segment base out of one hook and consumes it in a paired
`unsafe fn`, with ONLY a `debug_assert!` (compiled out in `--release`)
checking the base is not the live `small_cur` cursor before releasing it.
A typed, non-forgeable, move-consumed handle would turn that release-time
runtime check into a release-time-surviving, compile-time-enforced
guarantee — independent of which file either function lives in. §5
sketches concrete signatures for this.

**Decision: path (c), design-only, with one narrow acceptance.** Full
relocation (path a) and the "one module for the SAFE_MUTATORS bucket" half
of partial relocation (path b) are both declined this round on cost
grounds documented in §2/§4. The typed-handle piece is judged genuinely
valuable but is ALSO not implemented this round: retrofitting the existing
39 `SAFE_MUTATORS` + 22 `UNSAFE_HOOKS` hooks to a new handle type is not a
zero-risk edit-in-place — most of them delegate to production code paths
by design (see the `SAFE_MUTATORS` allowlist reasons in
`tests/dbg_hook_safety_tripwire.rs`) and changing their signatures touches
the same ~100+ call sites §2 counts, for the SAME reason full relocation
does. §5 designs the handle type concretely and names the ONE pair
(`dbg_decomp_reserve_and_keep`/`dbg_decomp_release`) it would apply to
first if a future task takes this up, per §6's trigger condition.

---

## 1. The R24-6 precedent, restated as this task's binding constraint

`git log` for "R24-6" surfaces commit `6d4eec6`
("feat(cargo): move two measurement-only unsafe hooks behind a new
bench-internals feature (R24-6, task #384)"). Its own commit message is
directly on point:

> A first attempt at fixing this (via a different delegation path)
> interpreted the task too broadly — `dbg_*` hooks are this project's
> established, project-wide "test-only export pattern" per CLAUDE.md, so
> attempting to gate essentially all of them exploded into a 130+-file diff
> before hitting a context limit. That attempt was reverted, nothing
> committed from it.
>
> Re-scoped narrowly: found the actual short list of unsafe fn dbg_* hooks
> that are BOTH unsafe (tier-2 `#[allow(unsafe_code)]`) AND reachable from
> plain production alone — 4 items ... `dbg_push_to_ring` ... left as-is.
> It has ~20 existing callers across the whole alloc-xthread test suite —
> re-gating it would reproduce the same disproportionate diff explosion the
> first attempt hit, for a documentation-precision concern rather than a
> new regression.

CLAUDE.md's own "Active rules" section (the benchmark-hook rule) still
cites this today: *"The one sanctioned exception is `dbg_push_to_ring`
(R6-MS-4): ~20 test files ... re-gating it would reproduce a 130+-file
diff for a documentation-precision concern rather than a regression — its
R24-6 doc-only justification note ... is the resolution for that one, not
a precedent to extend."*

The explicit instruction this task's brief carries is to apply the SAME
discipline, not to treat "the review proposed a big architecture" as a
mandate regardless of cost. §2 is that discipline applied: measure first,
decide second.

---

## 2. Step 2 survey — the actual hook population, measured

**Enumeration source:** `tests/dbg_hook_safety_tripwire.rs`'s own
`PURE_OBSERVERS` / `SAFE_MUTATORS` / `UNSAFE_HOOKS` allowlists (R30-2, task
#451) — already the exhaustive, mechanically-verified enumeration of every
crate-public `dbg_*` hook in `src/` + `crates/`, kept in sync with reality
by that file's own tripwire test. Reused rather than re-derived.

### 2.1 Counts

```text
$ awk '/^const PURE_OBSERVERS/,/^\];/' tests/dbg_hook_safety_tripwire.rs | grep -c '::dbg_'
99
$ awk '/^const SAFE_MUTATORS/,/^\];/' tests/dbg_hook_safety_tripwire.rs | grep -c '::dbg_'
39
$ awk '/^const UNSAFE_HOOKS/,/^\];/' tests/dbg_hook_safety_tripwire.rs | grep -c '::dbg_'
22
```

| bucket | count | mutates allocator state? | migration priority (task brief's own framing) |
|---|---:|---|---|
| `PURE_OBSERVERS` | 99 | no — read-only | **lowest** — "a read-only hook cannot corrupt allocator state no matter how it's exposed" |
| `SAFE_MUTATORS` | 39 | yes, but individually justified safe (delegates to production path / bounded policy knob / CAS-guarded) | **highest** — "the genuinely hazardous stateful hooks" |
| `UNSAFE_HOOKS` | 22 | yes, behind `unsafe fn` | already behind Rust's own opt-in boundary |
| **Total** | **160** | — | — |

### 2.2 Where the hooks currently live (src/ + crates/)

```text
$ grep -rl "pub fn dbg_\|pub unsafe fn dbg_" src/ crates/ | sort
crates/racy-ptr-cell/src/lib.rs
crates/ring-mpsc/src/lib.rs
src/alloc_core/alloc_core_core_diag.rs
src/alloc_core/alloc_core_large_cache.rs
src/alloc_core/alloc_core.rs
src/alloc_core/alloc_core_small_diag.rs
src/alloc_core/alloc_core_small_pool.rs
src/alloc_core/alloc_core_small_reclaim.rs
src/alloc_core/remote_free_ring.rs
src/global/fallback.rs
src/global/sefer_alloc.rs
src/global/tls_heap.rs
src/registry/bootstrap.rs
src/registry/heap_core_diag.rs
src/registry/heap_core.rs
src/registry/heap_core_tcache.rs
src/registry/heap_overflow.rs
src/registry/heap_registry.rs
```

**18 files.** Every one of these is either already a dedicated `_diag.rs`
`#[doc(hidden)]` test-forwarder file (the established, CLAUDE.md-sanctioned
"test-only export pattern" — `alloc_core_core_diag.rs`,
`alloc_core_small_diag.rs`, `heap_core_diag.rs`) or the hook lives as a
handful of `dbg_*` methods appended to the SAME type's primary
implementation file (`alloc_core.rs`, `heap_core.rs`, `heap_registry.rs`,
`bootstrap.rs`, `heap_overflow.rs`, `tls_heap.rs`, `sefer_alloc.rs`,
`fallback.rs`, `remote_free_ring.rs`, `heap_core_tcache.rs`,
`alloc_core_large_cache.rs`, `alloc_core_small_pool.rs`,
`alloc_core_small_reclaim.rs`) — not randomly scattered, but colocated with
the type whose internals they expose. This matters for §3's reasoning.

### 2.3 Call-site counts — the actual diff-size number

For each hook in `SAFE_MUTATORS` and `UNSAFE_HOOKS` (the two buckets a
relocation would plausibly prioritize, per the task brief's own framing
that `PURE_OBSERVERS` is "lower priority/value"), the number of distinct
files under `tests/`, `examples/`, `benches/` referencing that hook's name
was counted with:

```text
$ for id in <each SAFE_MUTATORS / UNSAFE_HOOKS entry>; do
    fn="${id##*::}"
    grep -rl "\b${fn}\b" tests/ examples/ benches/
  done | sort -u | wc -l
```

**Result: 102 distinct files** reference at least one `SAFE_MUTATORS` or
`UNSAFE_HOOKS` hook. Adding `PURE_OBSERVERS` to the same count (all 160
hooks) brings the union to **139 distinct files**.

Individual hot spots, for scale:

| hook | files referencing it |
|---|---:|
| `dbg_push_to_ring` (both `HeapCore`/`AllocCore` variants combined) | 26 |
| `dbg_drain_all_rings` (both variants combined) | 19 |
| `dbg_set_large_cache_budget` | 20 |
| `dbg_drain_small_pool` / `dbg_flush_all` / `dbg_rebuild_directory` | 10 each |
| everything else in `SAFE_MUTATORS`/`UNSAFE_HOOKS` | 1–7 each |

`dbg_push_to_ring`'s own count (26) independently reproduces the "~20"
CLAUDE.md's benchmark-hook rule already cites for it — confirming the
survey methodology agrees with the number R24-6 already used to reject
re-gating that ONE hook.

### 2.4 What this means for scope

This crate has ~227 test files total (per the task brief). **A full
relocation of `SAFE_MUTATORS` + `UNSAFE_HOOKS` alone would touch ~45% of
every test file in the crate** (102/227), before even considering
`PURE_OBSERVERS`. R24-6 rejected re-gating ONE hook because its 26-file
footprint was "a documentation-precision concern rather than a
regression" not worth the diff. This survey's 102-file (or 139-file, with
observers) footprint is **4-5x that rejected case**, for the identical
kind of concern (moving working, correctly-classified code to a different
location without changing its behavior). Under R24-6's own stated
reasoning, this is not a closer call — it fails by a wider margin than the
case CLAUDE.md already used to draw the line.

---

## 3. Step 3 — is the recurring bug actually a file-organization problem?

The task brief asks this directly, and it is the question that actually
decides whether the "single module/crate relocation" piece of the
reviewed architecture pays for itself. Working through each of the five
confirmed instances of the bug class:

| round | hook | root cause | WHAT fixed it |
|---|---|---|---|
| R25-1 | `HeapCore::dbg_overflow_bitmap_clear_pass` | safe `pub fn` derived a segment base from an ARBITRARY caller pointer via bitmask, zero validation, wrote allocator metadata | made it `unsafe fn` + `bench-internals`-gated (signature/gating change, not relocation) |
| R29-7 | `dbg_restore_local_for_test` | safe `pub fn` installed a caller-supplied raw pointer as live TLS `LOCAL` with zero validation | made it `unsafe fn` + `bench-internals`-gated |
| R29-8 | `dbg_force_decommit_retain_for` | safe `pub fn` decommitted a segment through a caller pointer; base was containment-checked but `live_count == 0` was NOT | made it `unsafe fn` + `bench-internals`-gated, precondition documented |
| R29-17 | `HeapCore::dbg_directory_bit_for_ptr` | same shape — safe hook reading directory state through a caller pointer without full validation | made it `unsafe fn`/re-gated |
| R30-1 | `dbg_decomp_full_cycle` / `dbg_decomp_reserve_and_keep` / `dbg_decomp_release` | zero-argument `&mut self` hook called `reserve_small_segment()`, which unconditionally published the fresh segment as the live `small_cur` bump-carve cursor, then released that SAME segment — dangling cursor | routed through a NEW cursor-free primitive, `reserve_small_segment_impl`, in the SAME file |

**Every single fix changed what the hook's body does or what its type
signature guarantees. None of the five fixes moved a hook to a different
file, and none would have been prevented by the hook living somewhere
else.** R25-1/R29-7/R29-8/R29-17 all needed the SAME correction (raw
pointer → `unsafe fn` + explicit validation), which is exactly what
CLAUDE.md's existing per-hook rule already mandates and which
`tests/dbg_hook_safety_tripwire.rs`'s tripwire already enforces
mechanically, file-location-independent. R30-1 needed a different
correction — not a pointer-validation problem at all, but a "what
production state does this body touch and does it leave a clean
exit" problem — which the task brief's own text already identifies as the
significant new data point: *"R30-1's fix... already independently
arrived at [the target invariant] without this broader architecture."*

**Conclusion for the "single module/crate" piece specifically:** it is
**not load-bearing**. The current per-type-colocated layout (§2.2 — 18
files, each either a dedicated `_diag.rs` forwarder or a handful of
`dbg_*` methods appended to the type's own impl file) did not cause any of
the five incidents, and consolidating those 18 files into one module would
not have prevented any of them either — the defect was always in what the
function body does or what the signature promises, checkable (and in
practice, checked — by `tests/dbg_hook_safety_tripwire.rs`) regardless of
which file the function's text lives in. This matches the task brief's own
suspicion, stated explicitly in the prompt: *"the actual R30-1-class
defect [is] caused by hooks being able to touch live state via raw
pointers/direct field access regardless of which file they're in ... the
typed-handle/consume-on-destroy pieces may be the load-bearing part."*
This document's survey confirms that suspicion rather than merely
asserting it.

**What DOES independently hold up, piece by piece, from the reviewed
architecture:**

- **"Place hooks in one module/crate"** — REJECTED for cost (§2.4) AND
  low marginal value (§3, this section) — a double reason to decline, not
  just a cost trade-off.
- **"Compile only under `bench-internals`"** — ALREADY the status quo for
  every `SAFE_MUTATORS`/`UNSAFE_HOOKS` entry that needs it; the ones that
  are NOT `bench-internals`-gated (the `SAFE_MUTATORS` bucket) are
  deliberately not gated because each is individually reviewed and
  justified as safe-to-leave-ungated in
  `tests/dbg_hook_safety_tripwire.rs` — re-gating all 39 of them behind
  `bench-internals` would be a scope change with its own 177-file-sum call
  site cost (§ Step-2 raw data) for hooks the existing review already
  cleared. Not evaluated further here — that would be a DIFFERENT task
  (tightening `SAFE_MUTATORS` itself), not this one.
- **"Opaque typed handles instead of raw pointers"** — the one piece with
  a CONCRETE, currently-live counterexample proving it adds value beyond
  what exists (§4/§5 below). Judged the load-bearing piece of the reviewed
  architecture.
- **"Consume-on-destroy (`fn release(self)`)"** — inseparable from the
  handle piece; evaluated together in §5.
- **"Invariant: allocator validity preserved after every safe hook call"**
  — ALREADY the operative invariant; R30-1's own fix is exactly an
  instance of upholding it, achieved WITHOUT a typed-handle system. Stays
  true as a standing discipline; does not need new machinery to keep
  holding for hooks that (like the `SAFE_MUTATORS` bucket) delegate to a
  real production code path by construction, since the production path's
  OWN correctness already provides it.
- **"Forbid mutation of `small_cur`/live-cursor fields unless restored"**
  — already effectively true post-R30-1: `reserve_small_segment_impl` is
  the ONE remaining primitive touching segment reservation in a hook
  context, and it structurally CANNOT publish `small_cur` (it has no code
  path that writes that field — verified by reading the function, see
  `src/alloc_core/alloc_core_small.rs:1903`). The convention is already
  enforced by the function's own shape, not merely by doc comment.

---

## 4. Why the full-relocation piece fails cost/value even setting §2's raw number aside

Two independent reasons converge, either one alone sufficient:

1. **Cost** (§2): 102–139 files, 4-5x the footprint R24-6 already declined
   for one hook.
2. **Value** (§3): none of the five real incidents in this bug class were
   caused by file scatter, and none would have been prevented by
   consolidation. A large diff that does not address the actual defect
   mechanism is a worse trade than a large diff that does — this is not
   merely "expensive," it is expensive for a benefit the historical
   evidence does not support.

This is a stronger rejection than R24-6's own (R24-6's `dbg_push_to_ring`
was a real, live gating gap — the fix WOULD have closed a genuine hole,
just at disproportionate cost for a single item already covered by
documentation). Here, the primary proposed mechanism (relocation) would
not close any hole at all beyond what `tests/dbg_hook_safety_tripwire.rs`
already closes today, file-location-independent.

---

## 5. The one piece worth designing concretely: a typed, consume-on-release segment handle

### 5.1 The live counterexample this design targets

`src/alloc_core/alloc_core_small_pool.rs:1070-1115` (R29-3/R30-1 era):

```text
pub fn dbg_decomp_reserve_and_keep(&mut self) -> Option<*mut u8> {
    self.reserve_small_segment_impl()
}

pub unsafe fn dbg_decomp_release(&mut self, base: *mut u8) {
    debug_assert!(
        base != self.small_cur,
        "dbg_decomp_release: base is the live small_cur cursor — release would dangle it"
    );
    self.release_or_pool_empty_segment(base);
}
```

Three structural weaknesses, none about file location:

- **The returned `*mut u8` is forgeable.** Nothing stops a caller from
  passing ANY `*mut u8` to `dbg_decomp_release` — including a pointer that
  was never returned by `dbg_decomp_reserve_and_keep` at all. The
  `unsafe fn` boundary correctly signals "caller must uphold a contract,"
  but the contract itself ("must be a live base THIS function returned")
  is not encoded in the type.
- **The one guard that exists is compiled out in `--release`.**
  `debug_assert!` catches the `small_cur`-aliasing hazard only in debug
  builds — exactly the situation the R30-2 header doc for
  `dbg_hook_safety_tripwire.rs` already flags elsewhere (`[DEBUG_ASSERT
  ONLY]` tags on `dbg_set_cursors`/`dbg_reserve_unpublished_for_test`) as
  a reviewed-and-accepted but WEAKER-than-ideal bound.
  the class of corruption those two specific accept because it is
  ring-bookkeeping-only; this pair's own corruption class (dangling
  `small_cur`, i.e. exactly R30-1's bug) is NOT that mild — it is the same
  live hazard R30-1 fixed, still standing on a debug-only backstop for
  this specific accessor pair even after the fix.
- **Nothing stops double-release.** Calling `dbg_decomp_release(base)`
  twice with the same value compiles fine; only `release_or_pool_empty_segment`'s
  own internal state (not the hook's signature) determines what happens
  the second time.

### 5.2 Proposed handle type — concrete sketch

```text
/// Opaque handle to a small segment reserved via a measurement-only
/// primitive, standing in place of a bare `*mut u8` for exactly the
/// `dbg_decomp_reserve_and_keep` / `dbg_decomp_release` pair. Cannot be
/// constructed from an arbitrary address — the ONLY constructor is
/// `pub(crate)`, called exclusively from inside `AllocCore`'s own
/// reservation path, so a handle in existence is, by construction, backed
/// by a genuinely live, table-registered, `Small`-kind segment this
/// allocator itself just reserved.
pub struct ReservedSmallSegment {
    base: *mut u8,
    // Private field: no external code can read `base` out of this type
    // either, foreclosing the "extract the pointer, forge a second handle
    // around it" bypass a merely-private-constructor (but public-field)
    // struct would still allow.
}

impl ReservedSmallSegment {
    /// Only `AllocCore` itself may mint one of these, and only by actually
    /// reserving a segment first — no other module, no test, no bench can
    /// construct a `ReservedSmallSegment` around an address it merely
    /// computed or received from elsewhere.
    pub(crate) fn new_from_reservation(base: *mut u8) -> Self {
        Self { base }
    }
}

impl AllocCore {
    /// Reserve a small segment for measurement, returning a handle instead
    /// of a bare pointer. Same underlying primitive as today
    /// (`reserve_small_segment_impl`) — this changes the RETURN TYPE only,
    /// not the reservation mechanism, so it inherits R30-1's fix (never
    /// touches `small_cur`) unchanged.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_reserve_and_keep(&mut self) -> Option<ReservedSmallSegment> {
        self.reserve_small_segment_impl()
            .map(ReservedSmallSegment::new_from_reservation)
    }

    /// Release a reserved segment. Takes the handle BY VALUE — the only
    /// way to obtain a `ReservedSmallSegment` is `dbg_decomp_reserve_and_keep`,
    /// and the only way to consume one is this method (or letting it drop,
    /// see §5.3), so:
    ///   - a forged handle cannot exist (private field + pub(crate) ctor),
    ///   - a double-release is a COMPILE ERROR (the first call moves
    ///     `handle`; a second call has no value left to move — E0382 "use
    ///     of moved value"), not a runtime hazard silently accepted or
    ///     caught only by a debug_assert.
    /// No `unsafe` needed on THIS signature — the safety-relevant
    /// precondition ("base is a live, owned, Small-kind segment") is now
    /// upheld by the type itself, not by caller discipline. This is the
    /// actual soundness improvement: what was an `unsafe fn` contract
    /// enforced by convention becomes a safe fn enforced by the type
    /// system.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_release(&mut self, handle: ReservedSmallSegment) {
        self.release_or_pool_empty_segment(handle.base);
        core::mem::forget(handle); // base already consumed; skip Drop's own release
    }
}
```

### 5.3 Consume-on-drop as defence-in-depth

A `Drop` impl that panics (debug) or leaks-with-a-counter (release) if a
`ReservedSmallSegment` is dropped WITHOUT going through
`dbg_decomp_release` closes the remaining gap (a caller who reserves and
simply never releases, currently possible and currently silent):

```text
impl Drop for ReservedSmallSegment {
    fn drop(&mut self) {
        // Reaching here means the handle was never passed to
        // dbg_decomp_release — a caller-side bug (forgot to release), not
        // a soundness hazard (leaking a reservation this way still leaves
        // the segment correctly registered in the table; it is a
        // measurement-harness leak, not allocator corruption). Debug-only
        // loud signal; not a hard abort in release (a diagnostic tool
        // panicking a benchmark process on its own bug would be a worse
        // failure mode than a logged leak).
        debug_assert!(false, "ReservedSmallSegment dropped without release — measurement-harness bug");
    }
}
```

### 5.4 Why this is scoped to ONE pair, not applied crate-wide this round

Every other `SAFE_MUTATORS` entry already avoids the raw-pointer-handle
shape by a DIFFERENT means: most delegate directly to a real production
function operating on a table-owned base (`dbg_find_segment_with_free`,
`dbg_drain_all_rings`, `dbg_rebuild_directory`, ...) rather than accepting
an externally-supplied pointer at all; the handful that DO take a pointer
(`dbg_force_coarse_dirty_bit_for`) are already containment-checked against
the live segment table before use, matching the "classification A" pattern
`tests/dbg_hook_safety_tripwire.rs`'s own header doc already describes.
`dbg_decomp_reserve_and_keep`/`dbg_decomp_release` is the ONE pair in the
current inventory that both (a) manufactures a NEW raw pointer via a
`dbg_*` call (not merely accepting a pointer that already existed) and (b)
requires the CALLER to hold and later hand back that exact value — the
mint-then-redeem shape a typed handle exists to make safe. Retrofitting a
handle type onto the other 37 `SAFE_MUTATORS` entries would mean changing
signatures that currently take `&self`/`&mut self` with no pointer
parameter at all into signatures wrapping SOME internal state in a handle
— a much larger, much less obviously load-bearing change, not justified by
this survey (their existing safety argument already holds without a
handle, precisely because they don't mint-and-redeem a pointer in the
first place).

---

## 6. Recommendation and disposition

**Path (c) — design-only this round**, with the following disposition per
piece of the reviewed architecture:

| piece | disposition | reason |
|---|---|---|
| single module/crate relocation of ALL `dbg_*` hooks | **declined** | §2 (102–139 files, 4-5x R24-6's rejected case) + §3 (zero of five real incidents were file-scatter-caused) |
| compile only under `bench-internals` | **status quo unchanged** | already true for `UNSAFE_HOOKS`; `SAFE_MUTATORS` deliberately stays ungated per its own per-entry review, out of this task's scope |
| opaque typed handles + consume-on-destroy | **designed (§5), not implemented** | genuinely valuable (concrete counterexample found), but retrofitting even the narrow target pair touches call sites in `examples/r29_3_decomposition_gate.rs` and is not a zero-risk drive-by edit within THIS task's design-first mandate — see below for why it is deferred rather than done partially |
| "allocator validity preserved after every safe hook" invariant | **already the operative standard** | R30-1 is the existence proof it is achievable without new machinery; `tests/dbg_hook_safety_tripwire.rs` is the mechanical enforcement |
| forbid `small_cur` mutation without restore | **already effectively enforced** | `reserve_small_segment_impl` structurally cannot write `small_cur` (verified by reading it) |

**Why the typed-handle piece is deferred rather than partially
implemented this round despite being judged valuable:** the task
brief's constraint set requires, for ANY touched call site, running the
relevant test suite and confirming nothing broke, and requires zero-trust
review of the diff. Implementing §5's sketch for real would change
`dbg_decomp_reserve_and_keep`'s return type from `Option<*mut u8>` to
`Option<ReservedSmallSegment>` and `dbg_decomp_release`'s parameter from
`unsafe fn(&mut self, base: *mut u8)` to `fn(&mut self, handle:
ReservedSmallSegment)` — a breaking signature change for BOTH the
`AllocCore` and `HeapCore` delegation layers (`heap_core_diag.rs:854-857`
forwards `dbg_decomp_release` verbatim) and their real call sites
(confirmed via grep: `examples/r29_3_decomposition_gate.rs` and
`tests/r30_1_decomp_full_cycle_cursor_safety.rs` — the latter is R30-1's
OWN counterfactual regression test for this exact hook pair, so touching
it here would mean re-verifying the very test that proves R30-1's fix,
inside a task whose deliverable is a design document, not a code change).
That is a small, tractable diff on its own (roughly 5 files: the two
definition sites, the one delegation forward, the example call site, the
R30-1 regression test) — genuinely NOT R24-6-scale — but
combining "design this round" with "also land it in the same task" was
judged to conflate two different review bars: the design in §5 deserves
scrutiny as a NEW pattern (first typed handle of this shape in the crate)
before code lands, not a same-task rubber stamp. Recorded as a named,
narrowly-scoped follow-up (§6.1) rather than expanded scope creep on this
design task.

### 6.1 Follow-up item and its trigger — filed to `docs/CORRECTNESS_OPEN_ITEMS.md`

Filed as item 7 in `docs/CORRECTNESS_OPEN_ITEMS.md`'s "Tracked, not yet
actioned" section (see that file for the exact entry). Summary: implement
§5's `ReservedSmallSegment` handle for the
`dbg_decomp_reserve_and_keep`/`dbg_decomp_release` pair specifically (the
~4-file diff named above), triggered by EITHER (a) a 6th instance of this
bug class being found (making the handle pattern's value proven twice
over, not just once), OR (b) any future task adding a SECOND
mint-then-redeem raw-pointer `dbg_*` pair to the inventory (at which point
building the handle once and applying it to both pairs amortizes the
one-time review cost this document deferred).

---

## 7. What this document does NOT claim

- **No `src/`, `Cargo.toml`, `tests/`, or `benches/` file is changed by
  this document's own scope.** §5's code blocks are illustrative sketches
  ("SKETCH" register, matching R30-7/R27-5's convention), not applied
  code. The only files this task's commit actually adds/changes are this
  design document and the `docs/CORRECTNESS_OPEN_ITEMS.md` /
  `CHANGELOG.md` entries recording the decision.
- **No claim that file-level scatter is harmless in general** — only that
  it was not the causal mechanism in the five specific incidents this bug
  class comprises, checked by reading each fix rather than assumed.
- **No claim that `SAFE_MUTATORS`'s existing per-entry safety
  justifications are wrong or need revisiting** — this document takes
  `tests/dbg_hook_safety_tripwire.rs`'s existing classification as given
  and correct; it is the enumeration source, not a target of re-review.
- **No claim that a full relocation would NEVER be worth it** — only that,
  measured against this round's actual data (102-139 files, zero of five
  incidents file-scatter-caused), it is not worth it NOW. A future round
  with different facts (e.g. a demonstrated maintenance cost from scatter
  itself, not merely a recurring soundness bug class already explained by
  §3) could reach a different conclusion; this document does not attempt
  to foreclose that.

---

## 8. Files changed by this task

| file | change |
|---|---|
| `docs/design/R30_10_MEASUREMENT_HOOK_ISOLATION_DESIGN.md` | this design document (new) |
| `docs/CORRECTNESS_OPEN_ITEMS.md` | item 7 added — the narrowly-scoped typed-handle follow-up, with its trigger condition |
| `CHANGELOG.md` | Round 30 section — R30-10 entry, stated as design-only, not a shipped-code round |

No `src/`, `Cargo.toml`, `tests/`, or `benches/` file is touched by this
task.
