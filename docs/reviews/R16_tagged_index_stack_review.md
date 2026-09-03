# R16 — tagged-index-stack code review (delta from R1)

Round 16. Read-only. Reviewed delta: commits between R1 review (`3df5a2b`)
and current `HEAD`, scoped to `crates/tagged-index-stack/`.

---

## 1. Stale "two-clause" reference in `push_index_impl`'s `# Safety` doc (doc bug)

**File:** `src/imp.rs:1434-1435`
**Severity:** medium — the public contract now has three clauses, but this doc
still says two.

```rust
/// # Safety
///
/// Same caller-side contract as [`StackOps::push_index`]'s `# Safety` —
/// the normative location, which this crate cross-references here. This
/// function is the shared body behind both [`StackOps::push_index`] and
/// [`ArrayIndexStack::push`]; its caller must discharge the link-domain
/// and liveness clauses.
```

The phrase "link-domain and liveness clauses" describes the OLD two-clause
contract. The current contract (as documented in `StackOps::push_index`'s `#
Safety`, the normative location this doc cross-references) has THREE clauses:
link-domain, liveness, AND exclusive temporal ownership (clause 3, added in
`f65ee31`). The module-level `// Single documented reason to hold `unsafe``
comment on line 1440 correctly names all three ("link domain + liveness +
exclusive ownership"), but the `# Safety` doc comment itself is stale.

A reader of `push_index_impl`'s `# Safety` doc would conclude the contract is
the old two-clause shape, missing clause 3 entirely. Since `push_index_impl`
is `pub(crate)` and the doc is the only inline contract statement for this
function, the stale text is the most likely thing a future implementor or
reviewer reads first.

**Fix:** change "link-domain and liveness clauses" to "link-domain, liveness,
and exclusive-ownership clauses" (or "three-clause caller-side contract").

---

## 2. `debug_assert!` condition in `pack_truncating` allows `INDEX_MASK` — correct for some callers, misleading for others (inaccuracy)

**File:** `src/imp.rs:314-319`
**Severity:** low — the assertion is correct, but the comment's wording
suggests `INDEX_MASK` is a broadly-legal input when in practice only two
callers (`empty()` and the H-2 drain path in `pop_index_impl`) ever pass it.

```rust
debug_assert!(
    index as u64 <= Self::INDEX_MASK,
    "pack_truncating: index out of range — must be <= INDEX_MASK \
     (the empty sentinel itself is legal: empty()/the H-2 drain \
     path pack it)"
);
```

The condition uses `<=`, which admits `INDEX_MASK` itself (the empty
sentinel). That IS correct — `empty()` and the H-2 drain path pass
`INDEX_MASK_U32` intentionally. But `push_index_impl` (the primary caller)
panics on `index >= INDEX_MASK` before ever calling `pack_truncating`, so it
NEVER passes `INDEX_MASK`. The `debug_assert!`'s comment could mislead a
future caller into thinking `INDEX_MASK` is an acceptable input for
`push_index_impl`'s code path.

**Fix:** reword the comment to say "must be < INDEX_MASK for push callers;
`<= INDEX_MASK` is admitted only because `empty()` and the H-2 drain path
pack the empty sentinel itself". Alternatively, split the check: a
`debug_assert!` for `< INDEX_MASK` for the general case, with a separate
named allow for the empty sentinel.

---

## 3. `Backoff::new()` early-return style (style)

**File:** `src/imp.rs:74-80`
**Severity:** low — valid Rust, but the function has two `return` statements
behind `#[cfg]` branches instead of a single expression.

```rust
fn new() -> Self {
    #[cfg(not(any(feature = "test-internals", loom)))]
    return Backoff(0);

    #[cfg(any(feature = "test-internals", loom))]
    return Backoff(0, false);
}
```

This is clear and correct. No fix needed. Worth noting only because the
`Backoff` type was previously a simple one-field tuple struct with a trivial
`new()` — the conditional second field makes the constructor slightly more
involved.

---

## 4. `Backoff` tuple struct with `#[cfg]`-gated second field — correct but unusual (informational)

**File:** `src/imp.rs:65-71`
**Severity:** informational — the struct now has a conditionally-present
`bool` field for the oracle verdict. In production it is one `u32`; in test
builds it is `(u32, bool)`.

```rust
struct Backoff(
    u32,
    #[cfg(any(feature = "test-internals", loom))] bool,
);
```

This is valid Rust and the `#[cfg]` on the field guarantees the field is
either present or absent consistently across all methods. The `spin()` method
writes to `.1` inside a `#[cfg]` block, and `spun_at_cap()` only exists
under the same gate. No correctness issue.

Worth noting only because this pattern (a `#[cfg]`-gated tuple-struct field)
is unusual in this codebase. It works, but a future reader might wonder why
`Backoff` has different ABI in different cfgs. The struct-level doc comment
explains it.

---

## 5. `push_index`'s `# Errors` section: stale reference to "this method's own
`# Safety` contract" phrasing (doc drift)

**File:** `src/imp.rs:1125-1131`
**Severity:** low — the doc says "this method's own `# Safety` contract (its
two caller-side clauses" — wait, let me re-read.

Actually, looking at the current code, the `push_index` doc already correctly
says "three clauses". So this is not an issue. Let me re-check.

Looking at lines 1076-1171 of the current `imp.rs`, the `push_index` doc
correctly says "three clauses" and lists all three. So this is fine.

---

## Summary

| # | Finding | Severity | Fix required |
|---|---------|----------|--------------|
| 1 | Stale "two-clause" reference in `push_index_impl`'s `# Safety` doc | medium | update doc to three clauses |
| 2 | `debug_assert!` comment in `pack_truncating` allows `INDEX_MASK` — correct for some callers, misleading for `push_index_impl` | low | reword comment |
| 3 | `Backoff::new()` early-return style | low | none (valid style) |
| 4 | `Backoff` tuple struct with `#[cfg]`-gated second field | info | none (valid pattern) |

### What I did NOT find

- No ABA bugs — the strictly monotonic tag + H-2 empty-transition preservation is correctly implemented.
- No release-sequence violations — every write to `head` is a CAS (RMW).
- No `unsafe` outside the eight audited regions.
- No data races — the `Acquire`/`Release` pairing and CAS orderings are correct.
- No correctness regressions from the R1 fixes — `pack_truncating`'s mask
  removal is safe because all callers prove their inputs are in range; the
  `debug_assert!` is a debug-build safety net, not a release-build
  guarantee (which is the correct posture for a function that "trusts its
  precondition").
- The new clause 3 (exclusive temporal ownership) is correctly documented,
  proven by the loom counterfactual, and consistently named across all public
  surfaces (`lib.rs`, `README.md`, `StackOps::push_index`'s `# Safety`,
  `ArrayIndexStack::push`'s `# Safety`). The ONLY stale reference is
  `push_index_impl`'s own `# Safety` doc (finding #1 above).

### R1 findings status

| R1 # | Description | Status |
|------|-------------|--------|
| 1 | Misleading `wrapping_add` comment | **Fixed** — comment rewritten |
| 2 | Redundant mask in `pack_truncating` | **Fixed** — mask removed, `debug_assert!` added |
| 3 | Hot-path `unsafe {}` noise | None (lint tradeoff, not fixable) |
| 4 | `Backoff::spin` dual responsibility | **Fixed** — split into `spin()` + `spun_at_cap()` |
| 5 | `store_next` retry-overwrite not named in SAFETY | **Fixed** — comment extended |
| 6 | Double-push of current head silent on push | Info (caller obligation, not fixable) |
| 7 | Pop CAS ordering asymmetry easy to misread | **Fixed** — cross-reference added |
| 8 | Doc-comment bloat | Subjective (not fixed) |
| 9 | Proptest strategies lack `TAG_BITS < 64` const assert | **Fixed** — const asserts added |
| 10 | `store_next` SAFETY proof doesn't mention retry | **Fixed** — comment extended |
| 11 | `Backoff::spin` saturation logic — correct | None |
| 12 | `ArrayLinks::new` loom path uses `from_fn` | None (MSRV covers it) |

All fix-required R1 findings are fixed. Finding #1 (stale two-clause doc in
`push_index_impl`) is a NEW issue introduced by the clause-3 addition — the
normative `StackOps::push_index` doc was updated, but the internal
`push_index_impl` doc was missed.
