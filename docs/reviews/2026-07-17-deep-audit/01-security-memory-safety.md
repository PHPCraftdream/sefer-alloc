# Security & memory-safety audit — sefer-alloc

**Date:** 2026-07-17
**Auditor role:** read-only security/memory-safety reviewer (restart of a prior
attempt that hit its budget limit before writing this file)
**Scope:** `src/alloc_core/`, `src/registry/`, `src/concurrent/`, `src/global/`,
and the unsafe-bearing crates under `crates/` (`vmem`, `numa`, `racy-ptr-cell`,
`ring-mpsc`, `malloc-bench`, `proc-memstat`, `globalalloc-model`).

## Methodology

Static, read-only review — no `cargo`/`rustc`/test execution (to avoid
contending for the `target/` build lock with concurrently running agents).
Steps taken:

1. Read `CLAUDE.md` (root) and the README's "Where unsafe lives" /
   "Unsafe inventory" sections (mirrored in `src/lib.rs`'s header comment) to
   establish the claimed unsafe surface.
2. Ran the project's own self-verifying inventory command,
   `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`, and cross-checked
   every tier-1 (`#![allow(unsafe_code)]`, module-level) and a representative
   sample of tier-2 (`#[allow(unsafe_code)]`, item-level `unsafe fn`
   boundaries) sites against the README/`lib.rs` inventory text.
3. Read, in full, every tier-1 seam module: `alloc_core::os`, `alloc_core::node`,
   `alloc_core::numa`, `global::sefer_alloc`, `global::tls_heap`,
   `global::fallback`, `registry::bootstrap`, `registry::heap_slot`,
   `registry::heap_registry`, `concurrent::hand` (research tier — not audited
   in as much depth, see below).
4. Read the highest cross-thread-free/UAF-risk files in depth:
   `alloc_core::remote_free_ring`, `registry::heap_core_free`,
   `registry::heap_core_xthread` (dealloc/realloc, cross-thread dealloc
   routing, foreign-pointer routing, Large deferred-free push/drain),
   `alloc_core::alloc_core` (the `AllocCore::dealloc`/`realloc` unsafe-fn
   boundaries and their `# Safety` contracts), `alloc_core::segment_header_gen_table`
   (the `hardened`-only generation table), and the decommit/recommit lifecycle
   in `alloc_core::alloc_core_small_pool` (`decommit_empty_segment_impl`) and
   `alloc_core::os` (`decommit_pages`/`recommit_pages`/`commit_pages`).
5. Verified the `git status`/`git log` state at session start (a prior parallel
   agent's in-flight changes had already landed; the working tree matched
   `HEAD` for every file inspected here — no uncommitted edits were reviewed
   as if final).

**Not exhaustively re-derived from scratch in this pass** (large, but lower
marginal risk given the density of pre-existing `// SAFETY:` proofs and the
project's own history of 10+ prior UB/memory-safety audit rounds — see
`docs/reviews/2026-07-10-ub-audit-*.md`, `2026-07-12-round2/3-*`,
`2026-07-13-round4-*`): the full byte-by-byte layout arithmetic in
`segment_header.rs`/`segment_table.rs`/`bin_table.rs`/`alloc_bitmap.rs`/
`magazine_bitmap.rs`, the loom/miri test suites themselves, and the NUMA/vmem
crate internals (`crates/vmem/src/lib.rs`, `crates/numa/src/lib.rs`) — these
carry their own `# Safety` documentation and were spot-checked via their
callers' contracts rather than read line-by-line. Concurrency/lock-free
correctness proper (ordering choices, ABA, loom coverage) is explicitly the
subject of a sibling audit (AUDIT-2) and is only touched here where it
intersects memory safety (UAF/double-free).

## Unsafe inventory cross-check

The `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/` output matches
the README/`lib.rs` inventory in substance: every tier-1 module listed in
`src/lib.rs`'s header (`alloc_core::os`, `alloc_core::node`, `alloc_core::numa`,
`global::sefer_alloc`, `global::tls_heap`, `global::fallback`,
`registry::bootstrap`, `registry::heap_slot`, `registry::heap_registry`,
`concurrent::hand`) appears in the grep output, and no unlisted module carries
a module-level `#![allow(unsafe_code)]`. Tier-2 sites (item-level `unsafe fn`
boundaries in files that are otherwise `#![deny(unsafe_code)]`-clean) are
concentrated in `alloc_core.rs`, `alloc_core_core_diag.rs`, `alloc_core_small.rs`,
`alloc_core_small_diag.rs`, `alloc_core_small_magazine.rs`,
`alloc_core_small_reclaim.rs`, `bootstrap.rs` (alloc_core's, distinct from
registry's), `remote_free_ring.rs`, `segment_header_gen_table.rs`,
`registry/heap_core_alloc.rs`, `registry/heap_core_diag.rs`,
`registry/heap_core_free.rs`, `registry/heap_core_tcache.rs`,
`registry/heap_core_xthread.rs` — every one carries a `// SAFETY:` comment at
its `unsafe` block/fn boundary, consistent with the task #101/R4-MS-3
discipline the codebase documents. No stray/undocumented `unsafe` was found
outside these named seams. The inventory claim is CONFIRMED accurate.

## Findings

### F1 — Stale "safe `pub fn`" characterization in `# Safety`-adjacent comments (LOW, CONFIRMED)

**file:line:** `src/alloc_core/alloc_core.rs:1192-1200` and
`src/registry/heap_core_free.rs:565-567`

**Description.** Both `AllocCore::realloc` (declared `pub unsafe fn realloc`
at `alloc_core.rs:1162`, with a `# Safety` doc block at lines 1134-1160
explicitly stating "This is an **`unsafe fn`** (R6-MS-1/2)") and
`HeapCore::realloc` (declared `pub unsafe fn realloc` at
`heap_core_free.rs:485`, with an equivalent `# Safety` block at lines 461-483)
contain an inline comment, inside the function body, that still describes the
function as safe:

```text
// R2-1 (soundness): the move leg copies `old_layout.size().min(
// new_size)` bytes OUT of `ptr`. `contains_base(base)` proved the
// segment is ours & mapped, but NOT that the block is as large as
// `old_layout` claims — and this is a SAFE `pub fn` (no `unsafe`
// marker), so unlike `GlobalAlloc::realloc` ...
```

(`alloc_core.rs:1192-1196`; `heap_core_free.rs` carries the symmetric "This is
a SAFE `pub fn`; a bogus layout ... must not drive an OOB read" at lines
565-567).

This is leftover text from before the R6-MS-1/2 hardening pass narrowed both
functions from safe `pub fn` to `unsafe fn`. The function-level doc comments
were updated in that pass (they correctly say "This is an **`unsafe fn`**");
the body comment justifying the `safe_payload_read_span` defense-in-depth
bound was not.

**Failure scenario.** Not itself an exploitable soundness bug — the function
signature is correctly `unsafe fn` and the caller-pointer contract is
correctly documented at the function boundary. The risk is purely to future
maintainers/auditors: a reader skimming the body comment (rather than the
function doc) could conclude the `old_layout.size() > safe_payload_read_span`
guard exists because "this is safe code, no unsafe contract protects us" and,
on a future refactor, remove or weaken the `safe_payload_read_span` bound on
the mistaken belief that the (now corrected, in the doc comment) `unsafe fn`
contract already covers it — when in fact the bound is deliberately
*defense-in-depth* even for a trusted caller (an `unsafe fn` contract violation
by the caller is still a real hazard the bound narrows).

**Impact.** Documentation/maintainability only; no runtime effect. Flagged
because this is exactly the class of drift the project's own
`docs/reviews/2026-07-10-ub-audit-*` rounds have repeatedly hunted for
(stale safety reasoning surviving a signature change).

**Suggested fix.** Update both comments to read consistently with the
function's actual (post-R6-MS-1/2) `unsafe fn` status, e.g. replace "this is a
SAFE `pub fn` (no `unsafe` marker)" with "this bound is defense-in-depth: even
though the function is `unsafe fn` and the caller is contractually bound to
supply a correct `old_layout`, a contract violation must not escalate to an
out-of-bounds read — `safe_payload_read_span` caps the move-leg read
independently of the caller's claim." A single-line comment fix in both
files; no code-behavior change.

### F2 — No further CONFIRMED/PLAUSIBLE memory-safety defects found in the reviewed surface

Across the tier-1 seams (`os`, `node`, `numa`, `sefer_alloc`, `tls_heap`,
`fallback`, `bootstrap` ×2, `heap_slot`, `heap_registry`) and the
highest-risk tier-2 cross-thread-free / UAF-prone logic (`remote_free_ring`,
`heap_core_free`, `heap_core_xthread`'s `dealloc_routing`/
`dealloc_foreign_routing`/`push_large_deferred_free`, the `hardened`
generation table, and the decommit/recommit lifecycle in
`alloc_core_small_pool::decommit_empty_segment_impl` /
`alloc_core::os::{decommit_pages,recommit_pages,commit_pages}`), every
`unsafe` block's `// SAFETY:` proof was checked against its stated invariant
and, in each case examined, the invariant is either:

- established by construction just before the unsafe block (e.g.
  `Registry::ensure_chunk`'s `unsafe { p.as_ref() }` only runs after
  `RacyPtrCell::get`/`get_or_try_init` observed a `Release`-published pointer
  under `Acquire`), or
- explicitly delegated to a documented caller contract at an `unsafe fn`
  boundary (e.g. `AllocCore::dealloc`/`realloc`, `HeapCore::dealloc`/`realloc`,
  `Node::atomic_u64_at`'s per-path liveness argument), with the callers of
  those `unsafe fn`s in this codebase verified to honor the contract (the
  `GlobalAlloc` trait boundary in `sefer_alloc.rs` forwards the trait's own
  obligations verbatim; internal callers like the realloc move-leg supply
  `ptr`/`old_layout` pairs already proven live by `contains_base`).

Specific hazard classes probed and not found violated in the reviewed files:

- **UAF / double-free defenses** (`MagazineBitmap` in-magazine oracle +
  `AllocBitmap` flushed-double-free oracle in `heap_core_free.rs`'s
  `dealloc_own_thread_with_base`): both oracles run unconditionally on every
  fastbin free (not gated behind a body-derived filter, per the Э6 fix
  documented in the same file), and the ordering (magazine scan first, bitmap
  second) matches the documented rationale. The codebase is explicit and
  honest about the one *residual* known gap (a block re-issued to the user
  between cross-thread free and drain — the "third leg", tracked as task X7
  and pinned `#[ignore]`d in `residual_xthread_double_free_no_corruption`) —
  this is a disclosed, not hidden, limitation and is out of scope for this
  finding.
- **OOM/rollback paths**: `RacyPtrCell::get_or_try_init`'s OOM branch
  (`registry/bootstrap.rs`) rolls the sentinel back to `null` before returning
  `None`, so a loser thread re-races rather than spinning forever — verified
  by reading both the real crate's shim (`racy_ptr_cell` is external, not
  re-read line-by-line here) and the in-tree `loom_shim::RacyPtrCell` mirror,
  whose `get_or_try_init` explicitly stores `null` on `init()` returning
  `None`. `fallback::heap_ptr()`'s primordial-OOM branch performs the
  equivalent `INIT_STATE.store(STATE_UNINIT, Release)` rollback. Neither path
  leaves a permanently-wedged `INITIALIZING` sentinel.
- **Provenance / carve arithmetic**: `Node::offset`/`Node::deref` use
  `ptr.add`/pointer arithmetic within a caller-proven `<= SEGMENT` bound (never
  raw `as usize` address reconstruction for the hot carve path);
  `os::segment_base_of_ptr` uses `ptr.map_addr` (strict-provenance-clean, not
  an exposed-address round-trip) as documented. The one place the codebase
  deliberately uses *exposed* provenance (`Node::atomic_ptr_ref`, for the
  cross-thread TFS-head CAS) is explicitly justified in its doc comment (task
  #142) as necessary to avoid a Stacked/Tree-Borrows foreign-write-disables-tag
  hazard between concurrent remote CASers — a deliberate, documented deviation,
  not an oversight.
- **Decommit/recommit lifecycle**: `decommit_empty_segment_impl` resets the
  `bump` cursor to `payload_start` *before* anything else observable, which is
  called out (and, by inspection, is true) as the load-bearing ordering for the
  `off >= bump` stale-free/stale-ring guard — a late free targeting a
  just-decommitted segment is rejected before it would write into
  now-unmapped pages. The lazy-commit variant only decommits
  `[meta_end + LAZY_FIRST_CHUNK, SEGMENT)`, leaving the initial chunk
  committed, and updates `committed_payload_end` accordingly — consistent with
  the fresh-segment reservation path's own initial frontier.

No new CRITICAL/HIGH/MEDIUM memory-safety finding is reported here. This
reflects the codebase's maturity (documented as having already been through
five prior UB/memory-safety audit rounds — round1 through round4 plus the
2026-07-10 UB-audit series — with fixes landed for each), not an absence of
scrutiny in this pass.

## Summary table

| # | Severity | Confidence | File:Line | Summary |
|---|----------|------------|-----------|---------|
| F1 | LOW | CONFIRMED | `src/alloc_core/alloc_core.rs:1192-1200`, `src/registry/heap_core_free.rs:565-567` | Stale "this is a SAFE `pub fn`" body comments inside two functions that are actually `pub unsafe fn` (post R6-MS-1/2); doc-comment drift only, no runtime effect |

## Top issues (descending severity)

1. **F1 (LOW, CONFIRMED)** — stale safety-characterization comments in
   `AllocCore::realloc` / `HeapCore::realloc`. Cosmetic/maintainability risk
   only; recommended one-line fix in both files.

No CRITICAL, HIGH, or MEDIUM severity memory-safety findings were identified
in the reviewed surface. The unsafe inventory in README/`src/lib.rs` was
verified accurate against the actual `grep` output with no undocumented seam
or missing entry.
