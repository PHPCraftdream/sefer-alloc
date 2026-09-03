# R1 — tagged-index-stack code review

Round 1. Read-only. Findings sorted by severity.

---

## 1. Misleading comment on `wrapping_add` (inaccuracy)

**File:** `src/imp.rs:1424`
**Severity:** low — correctness not affected, but the documented rationale is wrong.

```rust
let new_tag = tag.wrapping_add(1);
```

The comment says:

```rust
// `wrapping_add` is used only to avoid a debug-build overflow panic on
// the increment itself, NOT as a wrap-on-truncate mechanism
```

This is **not true** for any legal `INDEX_BITS`. The seal check at line 1389 guarantees `tag <= TAG_MAX` before this line runs. For every legal width (`INDEX_BITS = 1..=16`), `TAG_BITS = 64 - INDEX_BITS` is in `48..=63`, so `TAG_MAX = 2^TAG_BITS - 1` satisfies `TAG_MAX + 1 <= 2^63 < u64::MAX`. Plain `tag + 1` would never overflow — not in debug, not in release. The `wrapping_add` is harmless but the comment invents a non-existent hazard to justify it.

**Fix:** either replace `wrapping_add(1)` with `+ 1` and drop the misleading paragraph, or keep `wrapping_add` and rewrite the comment to say "defensive against hypothetical future removal of the seal check".

---

## 2. Redundant mask in `pack_truncating` (minor perf / dead-code-smell)

**File:** `src/imp.rs:275-278`
**Severity:** low — the compiler will likely optimize it away, but the source still carries dead arithmetic on the hot path.

```rust
pub(crate) const fn pack_truncating(index: u32, tag: u64) -> u64 {
    let () = Self::_CHECK_BITS;
    (tag << INDEX_BITS) | ((index as u64) & Self::INDEX_MASK)
}
```

Every caller (`push_index_impl`, `pop_index_impl`, `empty()`) has already proven `index` is in range — `push_index_impl` panics on `index >= INDEX_MASK`, `pop_index_impl` unpacks the index from a trusted head word, and `empty()` passes `INDEX_MASK_U32` which is `INDEX_MASK` itself. The `& Self::INDEX_MASK` mask is therefore a guaranteed no-op for every in-crate call site. It exists as "defense in depth" against hypothetical future misuse, but the function is `pub(crate)` and its doc explicitly says "this silently produces a VALID-LOOKING word from invalid input" — the mask does not actually defend against anything, because an over-wide index masked to `INDEX_MASK` would still produce the empty-sentinel word, which is exactly the silent-truncation failure mode the doc warns about.

**Fix:** drop the mask. The function name (`pack_truncating`) already documents that the caller must prove its precondition. If defense in depth is desired, a `debug_assert!(index < (1u32 << INDEX_BITS))` is the right shape — it catches misuse in debug builds without silently producing a wrong word in release.

---

## 3. Hot-path `unsafe {}` noise from `#![deny(unsafe_op_in_unsafe_fn)]` (style / readability)

**Files:** `src/imp.rs:1421`, `src/imp.rs:1569`, `src/imp.rs:1722`
**Severity:** low — zero runtime cost, but the hot retry loop in `push_index_impl` now has an `unsafe {}` block on every iteration that adds visual noise to the critical section.

```rust
loop {
    ...
    unsafe {
        s.store_next(index, next_link);
    }
    let new_tag = tag.wrapping_add(1);
    ...
}
```

The `#![deny(unsafe_op_in_unsafe_fn)]` lint forces this block even inside an `unsafe fn`. The safety proof is identical on every iteration — the call site never changes. This is a known tradeoff of that lint: it makes every unsafe call site explicit, at the cost of wrapping the innermost hot-path call in a block that never varies.

**Fix:** none required. This is the intended behavior of the lint. Worth noting only because the crate's own "Where unsafe lives" inventory counts regions, not blocks, and the block count rose from 3 to 6 when this lint was added — the hot path is now visually noisier even though nothing semantic changed.

---

## 4. `Backoff::spin` has dual responsibility (design smell)

**File:** `src/imp.rs:97-107`
**Severity:** low — the method both spins AND reports whether it spun at full depth. The return value is used only for test counters.

```rust
fn spin(&mut self) -> bool {
    let at_cap = self.at_cap();
    for _ in 0..(1u32 << self.0) {
        core::hint::spin_loop();
    }
    if !at_cap {
        self.0 += 1;
    }
    at_cap
}
```

In production builds (`#[cfg(not(any(feature = "test-internals", loom)))]`), the return value is discarded:

```rust
#[cfg(not(any(feature = "test-internals", loom)))]
backoff.spin();
```

The method conflates "do the work" with "report whether you did it at max depth". The report arm exists solely for the `PUSH_BACKOFF_CAP_REACH_COUNT` / `POP_BACKOFF_CAP_REACH_COUNT` test counters.

**Fix:** split into `spin(&mut self)` (returns `()`) and `spun_at_cap(&self) -> bool` (query-only). In production, call `spin()` and ignore the result. In test builds, call `spin()` then check `spun_at_cap()`. This makes the production path explicitly ignore the report, rather than silently discarding a return value.

---

## 5. `store_next` before CAS means every retry iteration writes link storage (correct but worth naming)

**File:** `src/imp.rs:1406-1423`
**Severity:** informational — this is correct Treiber push behavior, but it means a CAS-failure retry re-writes the link cell before the next CAS, and a concurrent pop that reads the link between the stale write and the next iteration's overwrite will see the stale value. That pop's CAS will fail (the head changed), so it retries — but the stale link read is observable under contention.

This is the standard Treiber stack analysis and the algorithm is correct. The `store_next` before CAS is safe because:
- If the CAS succeeds, the link is correctly written before the head update.
- If the CAS fails, the index is not the head, so no pop in the stack's read-set can observe the stale link as part of a valid chain.

The comment at lines 1403-1405 ("This is the ONLY link write — never an eager init (RAD-1)") is slightly ambiguous: it says "never an eager init" but doesn't explicitly say "this write may happen multiple times on retry". A one-line note ("on a CAS failure the next iteration overwrites this cell before its own CAS — the stale write is never observable in the stack's read-set") would close the question for a reader tracing the retry path.

**Fix:** add a one-line comment after the `store_next` call noting that on CAS failure the next iteration overwrites this cell, and the stale write is never observable.

---

## 6. `push_index_impl` does not guard against `index == cur_idx` (double-push of current head)

**File:** `src/imp.rs:1376-1423`
**Severity:** informational — this is intentional and documented as caller-obligation (clause 2 of `push_index`'s `# Safety`), but the algorithm itself will happily write `store_next(index, index)` — a self-loop — and then CAS. `pop_index` catches this on the first pop via the clause-4 self-loop guard, but the push itself is silent.

The `push_index` doc says:

> Checking liveness would cost an O(n) chain walk per push, so `push_index`'s own unconditional check is only `index < INDEX_MASK`

This is the right tradeoff. Worth noting only that the guard fires one pop too late: the push succeeds, the index is now in a cycle, and the next pop panics. For the common case (caller upholds the contract), this is fine. For a violation, the failure mode is a loud panic on pop, not silent corruption — which is the correct safety posture.

---

## 7. `pop_index_impl` uses `Acquire` for both success and failure CAS orderings — correct but easy to misread

**File:** `src/imp.rs:1529`
**Severity:** informational — the asymmetry between push (`Release, Relaxed`) and pop (`Acquire, Acquire`) is deliberate and well-documented, but a reader skimming the CAS call might assume symmetry and conclude the failure ordering is a bug or oversight.

```rust
match head_ref.compare_exchange(head, new_head, Ordering::Acquire, Ordering::Acquire) {
```

The comment at lines 1520-1528 explains why success is `Acquire` (no Release half needed because every head write is an RMW, keeping the release sequence intact). The comment at lines 1431-1436 (in `push_index_impl`) explains why pop's failure is `Acquire` — pop follows a link on retry, so the failure load must synchronize with the push that wrote it. Push's failure is `Relaxed` because push never follows a link on retry.

This is correct. No fix needed. Worth a one-line cross-reference in the pop CAS comment pointing to the push CAS comment's asymmetry explanation, so a reader doesn't have to read both to understand why they differ.

---

## 8. Documentation bloat / "neural-network-slop" risk (style)

**Severity:** subjective — the crate's doc comments are extremely long and detailed. This is intentional for a safety-critical lock-free data structure, but several doc comments are long enough that a reader can't hold the full argument in working memory.

Specific examples:
- `StackStorage` trait doc: ~350 lines of `# Safety`, `# Ordering contract`, hazard-class inventory, binding obligations, mechanical requirements, storage requirements.
- `StackHead` doc: ~60 lines of layout notes, sealing policy, release-sequence invariant.
- `push_index` doc: ~130 lines of `# Safety`, `# Errors`, `# Panics`.
- `pop_index` doc: ~100 lines of `# Panics` alone.

The crate's own documentation policy deliberately centralizes the long-form argument in one place and cross-references it everywhere else. This is the right approach, but the source files still carry very long doc comments. The README and `lib.rs` crate doc repeat much of the same content.

**Fix:** none required. This is a documentation-density tradeoff, not a bug. The "single source of truth" pattern the crate uses (trait doc for the hazard inventory, `pop_index`'s `# Panics` for the self-loop disjunction, `push_index`'s `# Safety` for the no-double-push rule) is the right shape. The repetition between `lib.rs`, `README.md`, and the trait/type docs is the cost of making the crate's invariants discoverable from multiple entry points.

---

## 9. `proptest_pack_unpack.rs` uses `TaggedIndex::<16>::TAG_BITS` in a strategy expression — safe today, fragile if `_CHECK_BITS` widens

**File:** `tests/proptest_pack_unpack.rs:31,43-44,56-57,68-69,88,100`
**Severity:** low — `_CHECK_BITS` currently caps `INDEX_BITS` at 16, so `TAG_BITS` is in `48..=63` and `1u64 << TAG_BITS` is always a valid shift. If `_CHECK_BITS` were ever widened (e.g. to allow `INDEX_BITS = 32` for a specialized downstream), these strategies would shift by 32, which is fine — but if `_CHECK_BITS` were ever removed or widened to allow `INDEX_BITS = 0`, `TAG_BITS = 64` and `1u64 << 64` is a const-eval UB/shift-overflow panic.

This is a theoretical concern — `_CHECK_BITS` is explicitly part of the crate's public contract and is not going away. But the strategies are not themselves `const fn`, so they don't benefit from `_CHECK_BITS`'s compile-time guard. A `const { assert!(TaggedIndex::<16>::TAG_BITS < 64); }` at the top of the file would make the invariant explicit and self-checking.

**Fix:** add a `const` assert at the top of `proptest_pack_unpack.rs` pinning `TAG_BITS < 64` for each width used in strategies.

---

## 10. `push_index_impl`'s `store_next` is inside the retry loop, but the `# Safety` proof doesn't mention retry semantics

**File:** `src/imp.rs:1407-1423`
**Severity:** informational — the `unsafe {}` block's `// SAFETY:` comment says:

```rust
// SAFETY: (a) `SealedStorage::store_next` forwards to
// `StackStorage::store_next`, whose caller-side contract this
// discharges: we are in the CAS-valid push phase — `next_link` is
// [`TAIL`] or the just-unpacked head index `cur_idx`, and the head
// CAS that publishes `index` happens after, at the
// `compare_exchange` below; (b) the link-domain and liveness legs
// come from `push_index`'s caller-side `# Safety` contract, which
// this function's own `# Safety` forwards
```

This proof is correct for a single iteration, but on a CAS failure the next iteration will call `store_next` again with a (potentially different) `next_link` computed from the new head. The proof doesn't say "on retry, the next iteration overwrites this cell before its own CAS, and the stale write is never observable". The Treiber stack's safety argument for `store_next` before CAS is well-known, but this crate's own safety discipline (every `unsafe {}` block carries a standalone proof) would benefit from naming the retry-overwrite property explicitly.

**Fix:** extend the `// SAFETY:` comment to note that on CAS failure the next iteration overwrites `index`'s link cell before its own CAS, and the stale write is never observable in the stack's read-set.

---

## 11. `Backoff::spin` saturation logic is correct but the `at_cap` check is done before the loop, not after

**File:** `src/imp.rs:98-107`
**Severity:** informational — the method checks `at_cap` before the loop, then conditionally increments `self.0` after. This means the first call with `self.0 == BACKOFF_SPIN_CAP` spins `1 << BACKOFF_SPIN_CAP` times and does NOT increment `self.0`. Subsequent calls also spin `1 << BACKOFF_SPIN_CAP` times. This is correct — the cap is a ceiling, not a threshold that triggers one extra spin.

The comment says:

```rust
/// Returns whether this retry spun at FULL depth (the PRE-increment
/// `K` was already at the cap) — the oracle trigger for
/// `PUSH_BACKOFF_CAP_REACH_COUNT` / `POP_BACKOFF_CAP_REACH_COUNT`.
/// The check deliberately happens before the increment, so the oracle
/// does not fire one retry early.
```

This is correct and well-documented. No fix needed.

---

## 12. `ArrayLinks::new` under loom uses `core::array::from_fn` which requires Rust 1.63+

**File:** `src/imp.rs:1917`
**Severity:** informational — the crate's MSRV is 1.79, so `from_fn` is available. The non-loom path uses the const array repeat `[const { AtomicU32::new(0) }; N]`, which is stable since 1.79. No issue today, but if the MSRV ever drops below 1.63, the loom path would break. Not a current concern.

---

## Summary

| # | Finding | Severity | Fix required |
|---|---------|----------|--------------|
| 1 | Misleading `wrapping_add` comment | low | rewrite comment |
| 2 | Redundant mask in `pack_truncating` | low | drop mask or replace with `debug_assert!` |
| 3 | Hot-path `unsafe {}` noise | low | none (lint tradeoff) |
| 4 | `Backoff::spin` dual responsibility | low | split into two methods |
| 5 | `store_next` before CAS — retry overwrite not named in SAFETY comment | info | extend comment |
| 6 | Double-push of current head is silent on push, panics on pop | info | none (caller obligation) |
| 7 | Pop CAS ordering asymmetry easy to misread | info | add cross-reference |
| 8 | Doc-comment bloat | subjective | none (intentional) |
| 9 | Proptest strategies lack `TAG_BITS < 64` const assert | low | add const assert |
| 10 | `store_next` SAFETY proof doesn't mention retry semantics | info | extend comment |
| 11 | `Backoff::spin` saturation logic — correct, well-documented | none | none |
| 12 | `ArrayLinks::new` loom path uses `from_fn` — MSRV 1.79 covers it | none | none |

### What I did NOT find

- No ABA bugs — the strictly monotonic tag + H-2 empty-transition preservation is correctly implemented.
- No release-sequence violations — every write to `head` is a CAS (RMW), so the release sequence stays intact.
- No `unsafe` outside the eight audited regions — verified by reading all of `src/imp.rs`.
- No data races — the `Acquire`/`Release` pairing on `load_next`/`store_next` and the CAS orderings are correct.
- No false-sharing bugs in the hot path — the head is a single `AtomicU64`, and `ArrayLinks`'s 16-indices-per-cache-line layout is documented as a known tradeoff with a caller-side mitigation.
- No `unsafe_op_in_unsafe_fn` gaps — every `unsafe fn` call site inside an `unsafe fn` body carries its own `unsafe {}` block with a `// SAFETY:` proof.

The crate is sound. The findings above are polish, not correctness bugs.
