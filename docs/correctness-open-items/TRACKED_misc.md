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

**Card count:** 2.

**Why split by theme, not by item-number range (task #1222, 2026-08-20):**
task #1221 (same day) split the former single `TRACKED.md` into four
number-range files, balanced by line count. The owner rejected that split
and asked for a thematic split instead -- grouping cards by what they are
actually ABOUT, derived from reading all 70 cards rather than assumed.
Every one of the 42+ code/CI/script citations of this index across the
repo cites an item by NUMBER, never by topic or file, so
`docs/CORRECTNESS_OPEN_ITEMS.md` (the thin index) now carries a complete
item-N to file lookup table covering all 70 numbers (including the
`59a`/`59b` sub-items) -- that table, not this file's name, is what keeps
the by-number citation convention a one-hop lookup under a thematic split.
(Split 2026-08-20, task #1222, superseding task #1221's number-range
split the same day.)

---
45. **`numa-shim`'s `CURRENT_NODE_SLOT: RefCell<u32>` where a `Cell<u32>`
    would do, and its accessor still uses a panicking `borrow_mut()`.**
    (Filed 2026-08-09, task #778/F4, round-closing review — audit §A2,
    INFO, left untouched by task #726's visibility narrowing.)

    - **Status:** OPEN — cosmetic/defensive, not a live bug.
    - **Current-number-or-verdict:** `crates/numa-shim/src/lib.rs`'s
      `CURRENT_NODE_SLOT` thread-local is `RefCell<u32>`; `Cell<u32>` would
      be strictly sufficient (only ever `get`/`set` a `Copy` value) and
      would structurally rule out the §B17 reentrant-borrow hazard this
      same module documents and defends against for its sibling `CALLS`
      cell (`record()`'s `try_borrow_mut`) — `set_current_node` still calls
      a PANICKING `RefCell::borrow_mut()` (`crates/numa-shim/src/lib.rs:149` as
      of task #726), not the `try_borrow_mut` pattern its sibling was
      deliberately given.
    - **Evidence:**
      `docs/reviews/2026-08-07-numa-shim-rust-intel-audit.md` §A2.
    - **Next trigger:** low priority — fold into any future edit that
      touches the `mock` module's thread-locals (e.g. a future `CALLS_CAP`
      follow-up, item 46's public-surface work, or a `mock` API revision
      before first publish, task #657).

49. **CLOSED** by task #997 (P3-8 pass 2). See 'Recently resolved' in RESOLVED.md for the full closure narrative.
