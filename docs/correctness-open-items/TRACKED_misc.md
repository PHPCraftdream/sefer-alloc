# Correctness / CI-debt open items -- [T] Tracked tier -- residual -- does not fit any other category

**Part of the split index.** This file holds the full text of every **[T]**
(tracked, not yet actioned) card whose subject matches this file's own
criterion (below). Start at `docs/CORRECTNESS_OPEN_ITEMS.md` for the
purpose/scope/convention header and the round-start reading order, and for
the complete item-number to file lookup table; come here for these specific
card bodies. See `docs/correctness-open-items/ACTIVE.md` for the **[A]**
tier, `docs/correctness-open-items/RESOLVED.md` for the closure trail, and
the sibling `[T]`-tier files (`TRACKED_hook_safety.md`, `TRACKED_verification_coverage.md`, `TRACKED_platform_contracts.md`, `TRACKED_ci_gate_coverage.md`, `TRACKED_test_flakiness.md`, `TRACKED_correctness_residuals.md`, `TRACKED_publish_readiness.md`, `TRACKED_process_record.md`) for the rest of
the tier.

**Criterion for this file:** A card lands here only if it does not share the defining criterion of any category above. Each card here is a genuine one-off: item 45 is a numa-shim RefCell-vs-Cell defensive-coding/panic-safety nit (not an OS-contract question, not a hook, not flakiness); item 49 is an aligned-vmem edition-2021-vs-2024 explicit-unsafe{}-block hygiene item (about FFI call-site annotation style, not about a dbg_* hook, a platform contract, or CI wiring).

**Card count:** 4.

**Why split by theme, not by item-number range (task #1222, 2026-08-20):**
task #1221 (same day) split the former single `TRACKED.md` into four
number-range files, balanced by line count. The owner rejected that split
and asked for a thematic split instead -- grouping cards by what they are
actually ABOUT, derived from reading all 70 cards rather than assumed.
Every citation of this index that points at ONE SPECIFIC ITEM carries
that item's number, in the form `` `docs/CORRECTNESS_OPEN_ITEMS.md`
item N `` -- task #1227 repaired the seven in `aligned-vmem` that did
not, and two outside it were still open as of that task (both are
recorded in the thin index's Structure section). Citations that point
at the FILE as a whole, at a named SECTION, or at a CLASS of items
rather than one item carry no item number and never needed one (task
#1227's finding; until #1236 these headers overclaimed it as a
universal, asserting that no citation ever pointed at anything but
an item number). Only the numbered citations depend on where item
numbers live, and `docs/CORRECTNESS_OPEN_ITEMS.md` (the thin index)
carries the complete, mechanically generated item-N -> file lookup
table covering EVERY `[T]`-tier number (including the `59a`/`59b`
sub-items) that keeps them resolving -- that table, not this file's
name, is what makes the thematic split safe: the lookup is two-hop
(index table, then this file), but mechanical and always correct. No citing-file
count is typed in this header on purpose: the "42+" typed here at
the split was already 43 (census against the split commit) -- #1230
removed it from one of these nine headers, #1236 from the other
eight; compare against this command's output, never a hardcoded
count:

```text
git grep -l "docs/CORRECTNESS_OPEN_ITEMS\.md" -- ':!docs/' | wc -l
```

(Split 2026-08-20, task #1222, superseding task #1221's number-range
split the same day.)

---
45. **CLOSED** by task #1342 (twentieth review F3, `docs/reviews/2026-08-25-021741-numa-shim-publication-audit-run-17-Sol-codex.md`; fired the item's own "fold into any future edit that touches the `mock` module's thread-locals" trigger). See 'Recently resolved' in RESOLVED.md for the full closure narrative.

49. **CLOSED** by task #997 (P3-8 pass 2). See 'Recently resolved' in RESOLVED.md for the full closure narrative.

154. **[T] Task-history prose in `src/` doc comments outweighs the code.** (Filed 2026-09-28, src review round 1 finding R1-11, residual part.)

    - **Status:** OPEN — deferred. The mechanical parts of R1-11 are done: `heap_overflow` split under the cap and the shared `HeapRegistry::claim_impl` (commit `2630b090`), plus the `tests/src_file_size_cap.rs` tripwire that enforces the 1000-line cap.
    - **Current-number-or-verdict:** the review measured about 20,000 `///`/`//!` lines against about 15,600 code lines in `src/`. Much of the prose is task history (`R6-OPT-P0-4`, `task #136`, ...) duplicated across files; the earlier R2-23 and R3-4 findings were stale prose of exactly this kind.
    - **Next trigger:** a dedicated docs round that moves history into `docs/` (ADR-style) and leaves invariants and SAFETY reasoning in the code, one module per commit. Pure doc diffs, but several tests `include_str!` source files and pin phrases, so rerun the doc tripwires after each module.
    - **Evidence:** review §R1-11.

157. **[T] `AllocStats` has no public counter for third-tier (intrusive spill) cross-thread free pressure.** (Filed 2026-09-28, oxx R2-04 follow-up.)

    - **Status:** OPEN — deferred; would add public API.
    - **Current-number-or-verdict:** after R2-09, `cross_thread_frees_lost` is legacy and always `0`, and `ring_overflows` counts only first-tier misses. It cannot tell a free rescued by `HeapOverflow`/retry from one that went to the spill. Only the `internals`/`bench-internals`-gated `HeapCore::dbg_spill_ledger_for_test` sees spill traffic. The `ring_overflows` doc now says so (commit `c4c86584`).
    - **Next trigger:** an operator or benchmark that needs to see spill pressure in production builds. `AllocStats` is `#[non_exhaustive]`, so a new field is additive; deprecating `cross_thread_frees_lost` belongs in the same semver window.
    - **Evidence:** `docs/reviews/2026-09-28-154558-src-review-oxx-round-2.md` §R2-04; `src/global/alloc_stats.rs`.
