# `tagged-index-stack` — independent publish-readiness review

- **Reviewer:** Claude (Opus 5), blind review, no prior review docs read
- **Date:** 2026-08-30 13:28:47
- **Scope:** `crates/tagged-index-stack/**` (src, tests, benches, README, CHANGELOG, Cargo.toml)
  plus the in-tree consumers `src/registry/heap_registry.rs` and `src/registry/bootstrap.rs`
  where they touch this crate's public API.
- **Verification performed:** `cargo test -p tagged-index-stack` (18 tests, all green),
  `RUSTDOCFLAGS="-D warnings" cargo doc -p tagged-index-stack --no-deps` (clean), plus a
  read of the generated HTML in `target/doc/tagged_index_stack/` to confirm one rendering
  claim. Loom was **not** run (requires `RUSTFLAGS="--cfg loom"` and several minutes);
  all loom findings below are from static reading.

---

## Overall verdict: **CONDITIONAL-GO**

The core algorithm is correct. I attacked the ABA mechanism from several angles — tag
monotonicity across pops, the empty transition, the pop-preserves-tag choice, the
release-sequence status of every `head` modification, the `TAIL`/`empty_index` sentinel
split at every representable `INDEX_BITS`, the `INDEX_BITS = 32` boundary where the two
sentinels numerically coincide, and the `wrapping_add` tag wrap — and found no defect in
`src/lib.rs`. The tag is genuinely monotone (every push increments it, every pop preserves
it, including across the drain-to-empty transition), so the head word can only recur
exactly after a full `2^TAG_BITS` wrap; the H-2 fix is real and load-bearing, and the
`_CHECK_BITS` cap at 32 structurally removes the `index == TAIL` class of bug rather than
merely documenting it. The memory orderings are sufficient — including the two that look
suspicious at first glance (`pop`'s `Acquire`-without-`Release` success ordering, and
`push`'s `Relaxed` CAS-failure ordering), both of which I verified to be sound, though for
a subtle reason the code does not state (see P3-4).

What holds this back from an unconditional GO is not the algorithm; it is the **contract
documentation and the benchmark**. The single most important caller obligation of this
crate — *do not push an index that is already on the stack* — is documented nowhere, while
the crate's own benchmark comment asserts at length that it **is** documented (P1-1). And
the benchmark that comment lives in itself violates that obligation, deterministically, in
a way that turns `contention/churn`'s reported ops/sec into a measurement of a cyclic
free-list (P2-1); two of the four single-threaded rows also measure a different code path
from the one their comments name (P2-2, P2-3). None of that is a shipping-code bug, but
publishing a crate that markets itself as "the canonical primitive people reinvent *wrong*"
with its own uniqueness precondition unwritten is exactly the kind of gap that gets
reinvented wrong downstream. Fix P1-1 and P2-1..P2-4, decide P3-6/P3-7 before 0.1.0 freezes
the API, and this is a clean GO.

---

## P1 — blocking

### P1-1. The `push` uniqueness precondition is documented nowhere — and the repo's own bench comment claims it is

**Location:** `crates/tagged-index-stack/src/lib.rs:394-422` (`push`'s rustdoc + `# Panics`),
contradicted by `crates/tagged-index-stack/benches/tagged_index_stack_bench.rs:120-131`.

`push`'s documented contract is exactly one condition: `index < INDEX_MASK`. The rustdoc
spends 16 lines on that bound (including why it is a release-active `assert!` and why it
already implies `index != TAIL`), and says nothing about the *other* precondition every
Treiber free-list has: an index must not be pushed while it is still reachable from the
stack. I verified this is absent everywhere — a case-insensitive search of `src/lib.rs`,
`README.md` and `CHANGELOG.md` for `twice|already (on|in|live)|double|duplicat|must not.*push`
returns **zero** matches.

Meanwhile the bench file asserts the opposite, in prose, as settled fact:

> `benches/tagged_index_stack_bench.rs:127-129` — "…which silently corrupts the free-list's
> link structure (this crate's push() explicitly documents this as the caller's
> responsibility to avoid, checked only for the INDEX_MASK bound, not for 'is this index
> already live')."

`push()` explicitly documents no such thing. Someone reasoned about the hazard correctly,
wrote it down in the one file that is *not* the API surface, and left the API surface
silent.

**Failure scenario.** A downstream slab allocator uses `TaggedIndexStack<16>` as its slot
recycler and, on a double-free of slot 7, calls `push(&links, 7)` while 7 is still on the
stack. `push` takes the current head index `H`, writes `link[7] = H`, and CASes the head to
`(7, tag+1)`. The node that previously pointed at 7 still points at 7, so the chain is now
**cyclic**: `7 → H → … → 7 → H → …`. `pop` never returns `None` again, and it hands index 7
to two different threads concurrently. The crate's own `push` rustdoc
(`src/lib.rs:401-405`) already names this exact consequence — "handing out a slot that is
still live elsewhere — memory unsafety reachable from this `#![forbid(unsafe_code)]`
crate's 100% safe public API" — but attaches it to the `INDEX_MASK` bound, which is the
*less* likely of the two ways to get there.

**Why it matters at P1.** This is a `#![forbid(unsafe_code)]` crate whose whole pitch is
that it encodes the subtleties people get wrong. It ships two named subtleties (H-2, RAD-1)
in the crate doc and omits the one that a caller can actually trip on a normal Tuesday.
It is also a pure documentation fix with zero code risk, and it is cheapest before
publication.

**Suggested fix.** Add a `# Caller contract` (or extend `# Panics` with a preceding
paragraph) to `push`:

```text
/// # Caller contract
///
/// `index` must NOT already be reachable from the stack. This crate cannot check
/// that (it would require an O(n) chain walk on every push), and violating it
/// corrupts the free-list: `push` overwrites `index`'s link with the current head,
/// so the node that previously chained to `index` now closes a cycle — `pop` stops
/// returning `None` and hands the same index to two callers. Every index on the
/// stack must have been placed there by exactly one `push` with no intervening
/// `pop`. (`push` DOES check the `index < INDEX_MASK` bound — see `# Panics`.)
```

Mirror one sentence of it in `README.md` (a "Caller contract" bullet next to the two
subtleties) and in `CHANGELOG.md`'s `TaggedIndexStack` entry, then correct the bench comment
at `benches/tagged_index_stack_bench.rs:127-129` to cite the newly-real doc.

---

## P2 — important

### P2-1. `contention/churn` deterministically double-pushes live indices and measures a cyclic free-list

**Location:** `crates/tagged-index-stack/benches/tagged_index_stack_bench.rs:181-184`
(prefill), interacting with `:143-144` (phase-1 seeds) and `:109-110` (the shared stack is
created once and never drained between phases).

The two contention phases share one `shared_stack` / `shared_links` pair. Phase 1
(`contention/push_pop`) is itself clean: each thread seeds one distinct index
`seed_idx = thread_id * LINKS_SIZE / num_threads` and thereafter only re-pushes exactly what
`pop()` returned. But when phase 1 ends, **every seeded index is still on the stack** (the
`pop`/`push` pair inside the loop is always balanced, so no thread exits holding one), and
nothing drains it. Phase 2 then does:

```rust
let prefill_count = 64u32;
for i in 0..prefill_count {
    shared_stack.push(shared_links, i);   // :182-184
}
```

`seed_idx` for `thread_id == 0` is `0 * LINKS_SIZE / num_threads == 0` **for every thread
count**, and `0` is in `0..64`. So `push(shared_links, 0)` at `i == 0` is a guaranteed
double-push of a still-live index. (At `num_threads == 8` index `32` collides too; at 6, index
`42`; at 3, only 0. Index 0 always collides.)

**Concrete consequence.** Let the head at the end of phase 1 be `(X, t)`. `push(0)` sets
`link[0] = X` and the head to `(0, t+1)`. Index 0 is now the head *and* still the target of
whichever phase-1 node pointed at it, so the chain closes into a cycle. In the common case
where `0` was itself the head, `link[0] = 0` — a self-loop, and `pop` returns index 0
forever without ever emptying. Either way `contention/churn` then runs eight threads
"churning" a free-list in which the same index is simultaneously owned by several of them,
`fresh_idx_outstanding` never fires (the stack is never empty), and the printed
`contention/churn: N ops/sec` characterises a degenerate cycle rather than the LIFO
structure the benchmark claims to measure.

This is doubly notable because the surrounding comments (`:120-131` and `:194-203`) are two
long paragraphs about how carefully this exact hazard was avoided. The `fresh_idx_outstanding`
flag guards only *this thread's own* fallback push; it cannot see the prefill collision, nor
another thread's held index. (`fresh_idx` for `thread_id == 0` is also `0`, colliding with
prefill `i == 0` a second way.)

**Suggested fix.** Drain between phases and pick non-overlapping index spaces:

```rust
// Phase boundary: return the stack to a known-empty state before prefilling,
// so the prefill cannot double-push an index phase 1 left live.
while shared_stack.pop(shared_links).is_some() {}
```

and give phase 2's fallback indices a disjoint range from the prefill (e.g.
`prefill_count + thread_id as u32`, asserted `< LINKS_SIZE`). Optionally assert the
invariant the benches are supposed to preserve: after the drain,
`assert!(shared_stack.pop(shared_links).is_none())`.

### P2-2. `pop/nonempty` measures the empty-stack `None` path, not the success path

**Location:** `crates/tagged-index-stack/benches/tagged_index_stack_bench.rs:55-65`.

```rust
// pop/nonempty: pop from a nonempty stack.
// Measures the pop path with guaranteed success (no None case).
let stack = Stack::new();
stack.push(&links, 1u32);                    // ONE index pushed, once

h.bench("pop/nonempty", move || {
    black_box(stack.pop(&links));            // called N times
});
```

Exactly one index is pushed before the harness starts. Iteration 1 pops it; iterations
2..N all take `pop`'s `is_empty(head) → return None` early exit at `src/lib.rs:473-475` —
a single relaxed-free `Acquire` load and a mask compare, with **no CAS at all**. The row's
label ("pop from a nonempty stack") and its comment ("guaranteed success (no None case)")
describe the opposite of what is measured, and the reported ns/op will be an
empty-stack-probe number roughly an order of magnitude below the real pop cost. This is the
`bench-scale-tool` analogue of a fixture that decays to a trivial case after the first
iteration.

**Suggested fix.** Make the row self-restoring, like `churn` already is, and label it for
what it measures:

```rust
h.bench("pop/nonempty", move || {
    let idx = stack.pop(&links).expect("pop/nonempty: stack must never drain");
    stack.push(&links, idx);    // restore, so every iteration pops a real element
});
```

That does make the row a pop+push pair (so it duplicates `churn`); the alternative that
isolates `pop` is to prefill K indices, run exactly K iterations, and refill outside the
timed region if the harness supports it. If neither is practical, rename the row to
`pop/empty_fast_path` and fix the comment — a correctly-labelled empty-probe measurement is
a legitimate thing to track; a mislabelled one is not.

### P2-3. `push/empty_stack` measures the empty transition once out of N iterations, and builds a self-loop while doing it

**Location:** `crates/tagged-index-stack/benches/tagged_index_stack_bench.rs:44-53`.

```rust
// push/empty_stack: push onto an empty stack.
// Measures the full push path including the empty→non-empty transition.
h.bench("push/empty_stack", move || {
    stack.push(&links, black_box(1u32));
});
```

Nothing pops. Only iteration 1 sees `is_empty(head) == true` and takes the `next_link = TAIL`
branch at `src/lib.rs:430-431`; every subsequent iteration takes the non-empty branch. Worse,
because the index is always `1` and the head is always `1` after iteration 1, iteration 2
onward executes `store_next(1, 1)` — writing `link[1] = 1`, a self-referential link — and
bumps the tag. So the row measures "repeatedly re-push the index that is already the head",
which is (a) not the empty transition it names, and (b) a P1-1 contract violation inside the
crate's own bench.

**Suggested fix.** Restore the empty state each iteration, and note that the pop is part of
the measured cost:

```rust
// Cost = one empty-transition push + one drain-to-empty pop; subtract the
// `churn` row to isolate the transition if needed.
h.bench("push/empty_stack", move || {
    stack.push(&links, black_box(1u32));
    black_box(stack.pop(&links));   // returns the stack to empty
});
```

…which is then identical to `push_pop/single_thread` at `:33-42`, so the honest conclusion
may be that these two rows collapse into one. Either way the current row should not ship
with its present comment.

### P2-4. The loom shim's `pop` uses the exact CAS-failure ordering the crate's own counterfactual proves is corrupting — while claiming to be a byte-for-byte replica

**Location:** `src/registry/bootstrap.rs:536-537` (the shim's `pop` CAS) vs
`crates/tagged-index-stack/src/lib.rs:488-490` (the real `pop` CAS); the false claim is at
`src/registry/bootstrap.rs:460-469`.

The `#[cfg(loom)]` const-capable shim exists so `static REGISTRY: Registry = Registry::new()`
still const-evaluates under loom. Its header comment states:

> `bootstrap.rs:464-469` — "This keeps the shim's push/pop a **FAITHFUL byte-for-byte
> replica** of the crate's algorithm (same H-2 running-tag empty transition, **same
> Acquire/Release/Relaxed orderings**, same RAD-1 lazy links…), differing from the shipped
> type ONLY in which `AtomicU64` backs the head."

It does not. The shim's `pop`:

```rust
// bootstrap.rs:534-538
match self.head.compare_exchange(
    head,
    new_head,
    Ordering::Acquire,
    Ordering::Relaxed,     // <-- crate uses Ordering::Acquire here
) {
```

The shipped crate uses `compare_exchange(head, new_head, Ordering::Acquire, Ordering::Acquire)`.
The shim's `push` *is* faithful (`Release`/`Relaxed`, matching `lib.rs:448`); only `pop`
diverges — and it diverges to precisely the value that
`crates/tagged-index-stack/tests/loom_aba.rs:672-753`
(`counterfactual_relaxed_cas_failure_corrupts_free_list`) exists to prove corrupts the
free-list, and that `CHANGELOG.md:55-60` calls out by name as "the regression the shipped
counterfactual pins".

**Is it live today?** I checked: no `tests/loom_*.rs` file in the root crate actually calls
`claim`/`recycle`/`pop_free_slot`/`push_free_slot`/`free_slots` (the only match is a prose
mention in `tests/loom_remote_ring_drain_guard.rs:39`), so the shim is currently off every
modeled interleaving, as its comment claims. This is therefore **latent**, not active. But
the safety argument is a whole-suite property nothing enforces, and the comment actively
misdirects the next reviewer: anyone who adds a loom harness touching the registry will
read "faithful byte-for-byte replica" and reasonably conclude the modeled orderings are the
shipped ones. They would then be model-checking an ordering the production build does not
use, which is worse than having no model at all.

**Suggested fix.** Change `bootstrap.rs:537` to `Ordering::Acquire` so the comment becomes
true. It costs nothing (loom-only code path, `Acquire` on a CAS-failure read is free on
x86 and cheap everywhere). If for some reason the divergence is deliberate, the comment
must say so explicitly and name why — but I see no reason it would be.

---

## P3 — quality / perf / smell

### P3-1. Both CAS loops use the strong `compare_exchange` where `compare_exchange_weak` is the idiom (speculative perf)

**Location:** `crates/tagged-index-stack/src/lib.rs:446-449` (push) and `:488-491` (pop).

Both CASes are inside retry loops that already handle `Err` by reloading and recomputing, so
a spurious failure is indistinguishable from a real one — the textbook precondition for
`compare_exchange_weak`. On LL/SC targets (AArch64, RISC-V, PowerPC) the strong form forces
the compiler to emit an *inner* retry loop around the `ldxr`/`stxr` pair to mask spurious
SC failures, on top of the outer loop the code already has; the weak form compiles to a
single LL/SC pair. On x86-64 the two are identical (`lock cmpxchg`), so this is
architecture-conditional.

I cannot benchmark this here. **What a benchmark would need to show:** an AArch64 run of the
`contention/*` rows (after P2-1 is fixed) with `weak` vs `strong`, at 2/4/8 threads. My
expectation is single-digit-percent on the contended rows and no measurable change on x86.
Verify the drop-in is safe first — it is: push's `Err` path re-derives `next_link` and the
tag from `actual` and re-issues `store_next`; pop's `Err` path re-checks `is_empty(actual)`
at the loop top. Both are already correct under a spurious `Err(actual)` where
`actual == head`.

### P3-2. `push`'s initial head load is `Acquire` where `Relaxed` is sufficient (speculative perf)

**Location:** `crates/tagged-index-stack/src/lib.rs:423`.

```rust
let mut head = self.head.load(Ordering::Acquire);
```

`push` never dereferences anything reached through `head`: it uses the index half only as a
value to store into `link[index]`, and the tag half only as an integer to bump. It reads no
link and no caller data. The `Acquire` therefore buys no happens-before edge that `push`
uses — which is consistent with the fact that `push`'s CAS-*failure* ordering is already
`Relaxed` (`:448`), and the retry path re-enters the loop with a `Relaxed`-obtained value and
is correct. `pop`'s corresponding load at `:471` must stay `Acquire` (it reads a link
afterwards); `push`'s need not.

Cost is zero on x86 (all loads are acquire), non-zero on AArch64 (`ldar` vs `ldr`,
particularly the ordering constraint against subsequent stores — and `push` does a store to
`link[index]` right after). Marked speculative; needs the same AArch64 A/B as P3-1.

If this is changed, add a one-line comment saying *why* push can be `Relaxed` and pop cannot
— that asymmetry is exactly what a future maintainer would "harmonise" in the wrong
direction.

### P3-3. The `Links` ordering contract mandates `Acquire`/`Release` that the stack does not actually need

**Location:** `crates/tagged-index-stack/src/lib.rs:290-296` (the trait's `# Ordering contract`),
`:306` and `:312` (the per-method restatements), `:354` and `:358` (the `ArrayLinks` impl).

The trait says implementations **MUST** use `Acquire` on `load_next` and `Release` on
`store_next`. Neither is load-bearing:

- `store_next` is sequenced-before `push`'s `Release` CAS on the head (`:439` then `:446-449`).
  A release RMW publishes *all* prior operations of the issuing thread regardless of their own
  orderings, so a `Relaxed` link store is already published to any thread that acquires the head.
- `load_next` is sequenced-after `pop`'s `Acquire` head observation (`:471` or the `Acquire`
  CAS-failure read at `:490`). That acquire already establishes happens-before with the
  publishing push, so a `Relaxed` link load is guaranteed to see the published value, and
  cannot be hoisted above the acquire.

So the contract as written is *safe but strictly stronger than necessary*, and it exports
that strictness to every implementor — including the production consumer
(`src/registry/heap_registry.rs:576-600`), which pays `ldar`/`stlr` on `HeapSlot::next_free`
on AArch64 for a guarantee the stack does not consume.

**This is a genuine tradeoff, not a defect.** Relaxing the contract to "`Relaxed` is
sufficient; the stack's own head CAS orderings carry the publication" makes the crate faster
on weakly-ordered targets but couples the trait's contract to an internal implementation
detail of `push`/`pop` — if the head orderings ever changed, every external `Links` impl
would silently become wrong, with no compile error. My recommendation is to **keep the
`Acquire`/`Release` requirement** (defence in depth for an openly-implementable trait in a
memory-safety-adjacent crate) but **document that it is deliberately stronger than the
minimum**, so a reader benchmarking on ARM does not conclude the crate is naive:

```text
/// These orderings are stronger than the minimum the stack strictly needs
/// (`push`'s Release CAS on the head already publishes a Relaxed link store, and
/// `pop`'s Acquire head observation already orders a Relaxed link load). They are
/// required anyway so that a `Links` implementation stays correct independently of
/// the stack's internal head orderings.
```

### P3-4. `pop`'s `Acquire`-only success ordering is correct only via the release-sequence rule — undocumented and silently fragile

**Location:** `crates/tagged-index-stack/src/lib.rs:488-491`.

```rust
.compare_exchange(head, new_head, Ordering::Acquire, Ordering::Acquire)
```

`pop`'s successful CAS carries **no** `Release`. That is sound, but for a reason nothing in
the file states. Consider: thread P pushes 7 (`Release` CAS, publishing `link[7]`), thread Q
pushes 5, thread R pops 5 (head becomes `(7, tag)` — written by R's `Acquire`-only CAS),
thread S then acquires the head, reads `(7, tag)`, and must see `link[7]` as P wrote it.
S's acquire load reads a value written by R, and R's write is not a release operation.

The edge exists only because of the C++20/Rust release-sequence rule: the release sequence
headed by P's `Release` CAS is the maximal contiguous run of read-modify-writes following it
in `head`'s modification order, and **every** modification of `head` in this crate is an
RMW (`push` and `pop` both use `compare_exchange`; `new()` is an initialisation, `raw_head()`
is a load, `cas_head_for_test` is a CAS). So Q's and R's CASes stay inside P's release
sequence and S synchronizes-with P transitively.

The moment anyone adds a plain `store` to `head` — a `clear()`, a `reset()`, a
`set_head_for_test`, a `Drop` that zeroes it — that sequence is severed and `pop`'s
`Acquire`-only success ordering becomes *unsound* for every acquire that reads across the
break, with no compile error and no test failure on x86. The loom suite would likely catch
it, but only if someone thought to model that new API.

**Suggested fix.** Add an invariant comment at the `head` field (`:371`) and reference it
from `pop`'s CAS:

```text
/// INVARIANT: every modification of `head` is a read-modify-write
/// (`compare_exchange`). This keeps the release sequence headed by any `push`
/// unbroken, which is what lets `pop`'s successful CAS be `Acquire` rather than
/// `AcqRel`. Do NOT add a plain `store` to this field (a `clear()`/`reset()`
/// would sever the sequence and silently un-publish links on weakly-ordered
/// targets). If such an API is ever needed, promote `pop`'s success ordering to
/// `AcqRel` in the same change.
```

The cheap alternative is to just use `AcqRel` on pop's success and delete the subtlety
(costs a `dmb ish`-class barrier per pop on AArch64, nothing on x86). I prefer the comment —
the current code is faster and correct — but the invariant must be written down.

### P3-5. `TAG_BITS` is the one associated item outside the `_CHECK_BITS` guard, and closing that hole costs one line

**Location:** `crates/tagged-index-stack/src/lib.rs:222-224` (the item), `:196-200` (the
paragraph documenting the hole).

```rust
pub const TAG_BITS: u32 = 64 - INDEX_BITS;
```

`_CHECK_BITS` is forced through `INDEX_MASK`'s initializer, which every other mask-touching
item reaches. `TAG_BITS` does not touch `INDEX_MASK`, so `TaggedIndex::<0>::TAG_BITS`
evaluates cleanly to `64` and `TaggedIndex::<40>::TAG_BITS` to `24`, at widths the crate
otherwise rejects. The doc honestly records this as a residual exception — five lines of
hedging prose about a hole that is closable in one line:

```rust
pub const TAG_BITS: u32 = {
    let () = Self::_CHECK_BITS;
    64 - INDEX_BITS
};
```

That would let `:196-200`'s entire "TAG_BITS is the one associated item that does NOT touch
`INDEX_MASK` and so remains reachable, unguarded, at any out-of-range width" paragraph be
deleted, and would make the invariant "every public associated item of `TaggedIndex` forces
the width check" true without exception — a strictly simpler thing to document and to
remember.

Secondary inaccuracy in the same paragraph: "remains reachable, unguarded, **at any**
out-of-range width" is not true above 64 — `64u32 - 70u32` is a const-eval overflow, so
`TaggedIndex::<70>::TAG_BITS` *does* fail to compile, just with a confusing subtract-overflow
message instead of the `_CHECK_BITS` one. The guard fixes that too.

### P3-6. `TaggedIndex::empty()` is public API whose only correct use is bootstrap, and whose misuse *is* the crate's headline bug

**Location:** `crates/tagged-index-stack/src/lib.rs:251-262`.

`empty()` packs the empty sentinel with tag 0. The crate's most prominent documentation —
the H-2 section at `:43-62`, the README bullet at `README.md:33-39`, the CHANGELOG entry at
`CHANGELOG.md:29-35`, and `empty()`'s own doc at `:254-258` — all exist to tell the reader
that using this value anywhere except bootstrap reopens ABA. `empty()`'s own doc has to
shout: "**Only the bootstrap-time empty state uses tag 0 unconditionally.**"

The only in-crate caller is `TaggedIndexStack::new()` (`:381`, `:390`), which is already the
correct bootstrap path and is public. External callers get a function whose one legitimate
use is already covered by a safer API, and whose illegitimate use is the exact bug the crate
advertises fixing. The remaining users are this crate's own tests
(`tests/stack_unit.rs:73`, `tests/regression_counter_wrap.rs:62`, and
`tests/loom_aba.rs:489`'s deliberate counterfactual) — all external crates from the
library's perspective, which is precisely the situation `raw_head` already solves with
`#[doc(hidden)]` (`:510`).

**Suggested fix (decide before 0.1.0 freezes it).** Mark `empty()` `#[doc(hidden)]` with the
same rationale block `raw_head` carries, keeping it reachable from `tests/` but out of the
published API. If it must stay documented, rename it to something that carries the warning
in the name (`bootstrap_empty()`), so a reader skimming the method list cannot mistake it
for "the value to install when the stack becomes empty".

### P3-7. `pop` is the one resource-returning function in the crate without `#[must_use]`

**Location:** `crates/tagged-index-stack/src/lib.rs:470`.

`src/lib.rs` carries `#[must_use]` on ten items (`:238, 246, 259, 273, 280, 329, 338, 378,
387, 511`) — every `const fn` accessor, both `new`s, and `raw_head`. `pop` has none. `Option`
is *not* `#[must_use]` in `core` (`vec.pop();` compiles warning-free), so
`stack.pop(&links);` silently discards an index: it is gone from the free-list and the
caller never received it. For a *slot recycler* that is a leak of the exact resource the
crate exists to conserve, and it is the only place in the API where discarding a return
value has a side effect.

The crate's own test code demonstrates the shape at `tests/loom_aba.rs:397`
(`stack.pop(&*links);` inside the seeding loop — there it is deliberate, but it reads
identically to the bug).

**Suggested fix.**

```rust
#[must_use = "a popped index is removed from the free-list; discarding it leaks the slot"]
pub fn pop<L: Links + ?Sized>(&self, links: &L) -> Option<u32> {
```

and change the three deliberate discards in `tests/` to `let _ = stack.pop(...);`.

### P3-8. A loom test's name and assertion message both claim the property its own module doc says it does not test

**Location:** `crates/tagged-index-stack/tests/loom_aba.rs:87` (name), `:137-139` (assert
message), contradicted by `:21-26` (module doc, property (a)).

The module doc is explicit and was clearly written to correct an earlier overclaim:

> `:21-26` — "A's single-shot CAS attempt races against B's repush and may legitimately
> SUCCEED or FAIL depending on scheduling — there is no property requiring a specific outcome
> (a prior version of this list claimed A's CAS is unconditionally 'FORCED to fail', which is
> only true of the separate, rendezvous-pinned H-2 scenario (d) below)."

But the test is still named `aba_repush_forces_stale_cas_retry_and_stays_consistent`, and
its only assertion still fails with:

> `:137-139` — "free-list corrupted (loss or duplication): got {popped:?} — **the ABA tag
> guard failed to force A's stale CAS to retry**"

Both restate the disclaimed property. The correction was applied to the module doc and not
propagated to the two places a reader actually encounters — the test name in CI output and
the failure message they will read when it goes red. A reviewer trusting the name would
conclude this test pins "the stale CAS always retries", which it does not; the actual
oracle is conservation of `{0, 1}`.

**Suggested fix.** Rename to `aba_repush_keeps_free_list_conservation` (or
`..._no_loss_or_duplication`) and change the message to "free-list corrupted (loss or
duplication): got {popped:?} — the tagged head failed to preserve exactly {0, 1} across A's
racing single-shot CAS and B's pop+repush".

### P3-9. `single_slot_seeded`'s doc is garbled, self-contradictory, and names a parameter that does not exist

**Location:** `crates/tagged-index-stack/tests/loom_aba.rs:385-403`.

```text
/// A single-slot stack seeded at a caller-chosen running tag (models the
/// realistic steady state, not a bootstrap artifact). Built from the REAL crate
/// type by pushing once then re-seeding the tag via repeated push/pop is
/// fiddly; instead we drive the REAL `push`/`pop` and reason about the tag it
/// produces. Seeding is done by pushing index 0 `start_pushes` times through a
/// pop/push cycle so the running tag reaches the desired value.
```

Three separate defects in six lines:

1. Sentence 2 has no grammatical subject ("Built from … is fiddly") and is
   **self-contradictory**: it declares "pushing once then re-seeding the tag via repeated
   push/pop" fiddly and then says "instead we drive the REAL `push`/`pop`" — which is the
   same thing it just rejected. It reads like two drafts of an explanation spliced together.
2. `start_pushes` **is not a parameter**. The signature at `:389` is
   `fn single_slot_seeded(target_tag: u64)`. Stale name from an earlier revision.
3. The function is **incorrect for `target_tag == 0`**: `0u64.saturating_sub(1) == 0`, so the
   loop does not run, the unconditional `push` at `:399` sets the tag to 1, and the
   `assert_eq!(tag, target_tag, "seeded running tag")` at `:401` fails. Not currently
   reachable — the only call site is `run_h2`'s `single_slot_seeded(1)` at `:411` — but it is
   a latent trap in a helper that advertises "a caller-chosen running tag".

Related over-engineering: because `1` is the only argument ever passed, the entire
`for _ in 0..target_tag.saturating_sub(1) { push; pop; }` loop is dead. Either exercise it
(seed the H-2 scenario at a couple of tags, which would also be better coverage) or inline
the single push it actually performs.

**Suggested fix.** Rewrite as:

```text
/// A single-slot stack whose running tag is exactly `target_tag`, built by driving
/// the REAL `push`/`pop` (each push bumps the tag by 1; a pop preserves it), so the
/// scenario starts from a realistic steady state rather than a bootstrap artifact.
/// `target_tag` must be >= 1: a single push already leaves the tag at 1.
```

and add `assert!(target_tag >= 1, ...)` at the top, or handle 0 by returning before the
final push.

### P3-10. The head word has no cache-line isolation, and the production consumer places it 8 bytes from another hot RMW (speculative perf)

**Location:** `crates/tagged-index-stack/src/lib.rs:369-372` (`TaggedIndexStack`'s layout)
and `src/registry/bootstrap.rs` (`Registry { chunks: [...; NUM_CHUNKS], count: AtomicU32,
free_slots: TaggedIndexStack<16> }`).

`TaggedIndexStack` is a bare `AtomicU64` with default `repr(Rust)` and no alignment
attribute, so it inherits whatever line its embedder puts it on. In the production consumer
it sits immediately after `Registry::count: AtomicU32`, and `Registry::claim` drives both on
the same path: `pop_free_slot` (a CAS on `free_slots.head`) and, on the empty branch,
`bump_count` (`count.fetch_add(1, AcqRel)` at `src/registry/heap_registry.rs`). Following a
512-byte `[OncePtrCell; 64]` array, the two atomics land ~8 bytes apart, i.e. on the same
64-byte line with high probability — so every `bump_count` RMW invalidates the line every
concurrent `pop_free_slot` CAS is contending on, and vice versa.

Whether this is measurable depends entirely on how hot `claim`/`recycle` are; in this
allocator they are per-thread-lifetime, not per-allocation, so I would expect **no**
measurable effect here. It matters more for the crate's advertised general use (a slab
allocator's per-object free-list, where pop/push are per-allocation).

**What a benchmark would need to show:** the `contention/*` rows (post-P2-1) with the head
wrapped in a `#[repr(align(64))]` newtype vs. not, at ≥4 threads on a machine with real
cross-core traffic. The crate cannot fix this unilaterally — forcing 64-byte alignment on a
`pub struct` is a semver-visible layout change and wastes 56 bytes for every embedder that
does not need it. The right move for 0.1.0 is a **documentation note** on the type:

```text
/// # Layout note
///
/// This type is a bare `AtomicU64` with no cache-line padding, so it inherits the
/// line of whatever embeds it. If you place it adjacent to another
/// frequently-modified atomic, the two will false-share. Wrap it in a
/// `#[repr(align(64))]` newtype (or separate it with padding) when that matters.
```

### P3-11. `Cargo.toml`'s `description` is 556 characters

**Location:** `crates/tagged-index-stack/Cargo.toml:7`.

Measured: 556 characters, one paragraph, containing four separate technical claims
(the H-2 fix, the 89-year tag budget, the loom counterfactual list, the const-generic index
width). crates.io renders `description` as the one-line blurb under the crate name in search
results and at the top of the crate page; at this length it is truncated in the former and
reads as a wall of text in the latter. It also duplicates, near-verbatim, content that
`README.md` already presents with structure.

**Suggested fix.** Cut to ~120-160 characters covering what the crate *is*, and let the
README carry the rest. For example:

```toml
description = "Lock-free, allocation-free, no_std free-list of small indices (a slot recycler) with an ABA-defeating tag packed into one atomic word."
```

(148 chars.) Everything currently in the description is already in `README.md` and the crate
doc.

### P3-12. A first-release CHANGELOG cites internal task numbers for fixes to code that never shipped

**Location:** `crates/tagged-index-stack/CHANGELOG.md:55-60` (`task #698`, `task #703`) and
`:74-76` (`task #704`).

The file opens with "First release. Everything below is new in this version; **nothing has
shipped before it**" (`:9-10`), then describes internal history as if it were a change log:

> `:55-60` — "…a `Relaxed` retry **was the regression** the shipped counterfactual pins,
> **task #698**. Push's index-validity and sentinel guards **are release-active `assert!`s,
> not `debug_assert!`s** (**task #703**)."
> `:74-76` — "**`raw_head()`** — `#[doc(hidden)]` test-probe accessor…, its API posture
> **settled before first publish** (**task #704**)."

`#698`/`#703`/`#704` are this repository's session task IDs. They resolve to nothing a
crates.io reader can open — not a GitHub issue, not a commit, not a document in the
published tarball. And by the file's own first sentence, none of these describe a *change*:
there was no prior release in which the ordering was `Relaxed` or the guard was a
`debug_assert!`. A reader arriving at v0.1.0's changelog is told about regressions and
API-posture debates in code they have never seen.

**Suggested fix.** Drop the three task IDs and restate the two substantive facts as
properties of what ships, not as a history of what was fixed:

- "`pop`'s CAS-failure ordering is `Acquire` (a `Relaxed` retry would let the retry read a
  stale link — pinned by the `counterfactual_relaxed_cas_failure_corrupts_free_list` loom
  test)."
- "`push`'s index-validity guard is a release-active `assert!`, not a `debug_assert!`."
- "`raw_head()` — `#[doc(hidden)]` test-probe accessor for the packed head word; not stable
  API."

### P3-13. Review-response prose and extraction archaeology in shipped rustdoc, tests, and bench

**Locations (representative, not exhaustive):**

- `src/lib.rs:72-73` — "In **the extracting allocator** this saved a ~16 MiB
  bootstrap-commit first-touch."
- `src/lib.rs:81-83` — "or — **as the extracting allocator does** — mints fresh indices via a
  separate monotonic counter".
- `tests/loom_aba.rs:3-8` — "Unlike the in-tree shadow model this replaced
  (`tests/loom_free_slots_aba.rs` in the extracting allocator, which TRANSCRIBED the
  protocol into a local copy because it could not import the real registry code)…"
- `tests/loom_aba.rs:22-26` — "(a **prior version of this list** claimed A's CAS is
  unconditionally 'FORCED to fail'…)"
- `tests/loom_aba.rs:244-250` — "**A prior version of this oracle** asserted
  `!popped.contains(&1)` directly, which is scheduling-DEPENDENT…" (7 lines)
- `tests/loom_aba.rs:612` — "`Ordering::Acquire, // FIXED: was Relaxed, now Acquire`"
- `tests/stack_unit.rs:133-149` — "Regression for **the F1 finding (2026-08-06
  publish-readiness review)**… **see the crate's F1 write-up**… **the rust-intel audit (§D1,
  2026-08-07)** found that…"
- `tests/stack_unit.rs:188-194` — a 7-line "Compile-fail coverage note (F1, …)" comment
- `benches/tagged_index_stack_bench.rs:1-4` — "**(task #762)**. This crate **previously had
  zero benches of its own**…"

Two distinct problems:

1. **"The extracting allocator"** is opaque to anyone reading this on docs.rs. It reads like
   an allocator that performs extraction. `README.md:44` gets this right — "In the allocator
   this crate was extracted from" — and the crate doc should match, or better, name it and
   link it, or drop the provenance entirely and state the property ("a caller whose link
   backing is OS-zeroed memory avoids a first-touch commit proportional to the pool size").
2. **Review IDs and revision history** (`F1`, `§D1`, dated review names, `task #762`,
   "a prior version of this oracle", "FIXED: was Relaxed") are pointers into artifacts that
   do not exist in the published tarball. `tests/` and `benches/` **are** included in
   `cargo package` by default, so all of this ships. "See the crate's F1 write-up"
   (`tests/stack_unit.rs:141-142`) points at a document nobody outside this repo can open.

The *content* of several of these is worth keeping (why the oracle is conservation-based
rather than `!contains(&1)` is genuinely useful). The fix is to state the reason without the
revision narrative: "The oracle is conservation-based rather than `assert!(!popped.contains(&1))`
because the latter is scheduling-dependent — it fires on benign interleavings where A
completes before B's second pop, aborting the model check before the genuine ABA
interleaving is reached." Same information, no archaeology.

---

## P4 — minor / cosmetic

### P4-1. `[`INDEX_BITS`](Self)` renders as a link to the `TaggedIndex` struct page (verified)

**Location:** `crates/tagged-index-stack/src/lib.rs:208`.

```rust
/// Bit-mask for the low [`INDEX_BITS`](Self) (the index half), e.g. `0xFFFF`
```

Confirmed in the generated HTML (`target/doc/tagged_index_stack/struct.TaggedIndex.html`):

```html
Bit-mask for the low <a href="struct.TaggedIndex.html" title="struct tagged_index_stack::TaggedIndex"><code>INDEX_BITS</code></a>
```

A reader clicking a link labelled `INDEX_BITS` lands back on the page they are already on,
with no definition of `INDEX_BITS` in sight. Const generic *parameters* are not linkable
items; `Self` was used as a stand-in target. Fix: drop the link, use plain
`` `INDEX_BITS` ``.

`cargo doc` with `RUSTDOCFLAGS="-D warnings"` is clean, so this is the only rendering issue I
found; the `[`compile_error!`]` link at `:122` resolves correctly.

### P4-2. `proptest_pack_unpack.rs` misattributes which widths the test it complements covers

**Location:** `crates/tagged-index-stack/tests/proptest_pack_unpack.rs:3-5`.

> "complementing `stack_unit.rs`'s `pack_unpack_round_trip_16` test, **which only exercises
> hand-picked literals at widths 16/20/32**."

`pack_unpack_round_trip_16` (`tests/stack_unit.rs:19-33`) covers **width 16 only** — the name
says so. Widths 20 and 32 are covered by two *different* tests, `width_20_partitions` (`:120`)
and `width_32_index_mask_equals_tail_and_is_rejected` (`:151`). The intent (proptest adds
randomised coverage over hand-picked literals) is right; the attribution is wrong. Fix:
"complementing `stack_unit.rs`'s hand-picked literal coverage (`pack_unpack_round_trip_16` at
width 16, `width_20_partitions` at 20, `width_32_index_mask_equals_tail_and_is_rejected` at 32)".

### P4-3. Consumer doc calls a compile-time assert a `debug_assert`

**Location:** `src/registry/heap_registry.rs:564` describing the item at `:569-573`.

> "…the **`debug_assert`** below pins that equivalence so a future divergence fails loudly
> rather than silently corrupting a chain."

It is a `const _: () = assert!(NEXT_FREE_TAIL == tagged_index_stack::TAIL, ...)` — a
compile-time assertion. The doc *understates* its own guarantee (compile error in every
profile, vs. a debug-only runtime panic). Fix: "the compile-time `const` assert below".

### P4-4. README code fences are `text` where `rust` / `sh` would render

**Location:** `crates/tagged-index-stack/README.md:89-97` (the Rust example) and `:74-76`
(the loom command line).

The Rust example is fenced ```` ```text ````, so it renders unhighlighted on crates.io and
GitHub. `README.md` is **not** pulled in via `#![doc = include_str!("../README.md")]`
(verified — `src/lib.rs` has no such attribute), so a ```` ```rust ```` fence there would
**not** create a doctest and does not conflict with this repo's no-doctests rule. Same for
`:74`, which is a shell command and should be ```` ```sh ````.

Note the tradeoff: a ```` ```rust ```` fence in a README that is not `include_str!`'d is also
not compile-checked, so it can rot. If that is the reason for `text`, say so in a comment —
but the current state gets the cost (no highlighting) without the benefit.

### P4-5. The README devotes a section to a `#[doc(hidden)]` test hook

**Location:** `crates/tagged-index-stack/README.md:78-85`.

An eight-line section, "Test-only diagnostic surface", explaining that `raw_head` is `pub`
but hidden, carries no semver guarantee, and should not be depended on. That is
inward-facing rationale aimed at this repo's own reviewers; a prospective user reading the
README learns about an API they cannot see in the docs and are told not to use. The
`#[doc(hidden)]` attribute plus the existing rustdoc at `src/lib.rs:505-509` already carry
this. Suggest deleting the README section (or reducing it to one line under a "Notes"
heading).

### P4-6. Garbled sentence in `pack`'s doc, and it addresses only the in-crate caller

**Location:** `crates/tagged-index-stack/src/lib.rs:234-237`.

> "The caller — the stack — guarantees the `< 2^INDEX_BITS` precondition by construction,
> since indices come from [`push`](TaggedIndexStack::push)'s `< INDEX_MASK` contract, **which
> this truncation can never be reached through**."

The trailing clause is unparseable and its referent is ambiguous ("which" = the contract? the
truncation?). More substantively: `pack` is `pub`, so "the caller — the stack —" is only one
of its callers; an external user calling `TaggedIndex::<16>::pack(0x1_FFFF, tag)` gets the
silent truncation-to-empty-sentinel behaviour the paragraph above documents, with no
guarantee at all. Fix: "…so this truncation is unreachable through the stack's own API.
External callers of `pack` must uphold the precondition themselves."

Note also that the same paragraph uses two different bounds — `pack` says `< 2^INDEX_BITS`,
`push` says `< INDEX_MASK` (= `2^INDEX_BITS - 1`). Both are correct in their own scope, but
stating them one sentence apart without noting they differ invites misreading.

### P4-7. `push` decomposes the head word three times per iteration

**Location:** `crates/tagged-index-stack/src/lib.rs:430-441`.

```rust
let next_link = if TaggedIndex::<INDEX_BITS>::is_empty(head) {   // mask #1
    TAIL
} else {
    let (cur_idx, _tag) = TaggedIndex::<INDEX_BITS>::unpack(head);  // mask+shift #2
    cur_idx as u32
};
links.store_next(index, next_link);
let (_cur_idx, tag) = TaggedIndex::<INDEX_BITS>::unpack(head);      // mask+shift #3
```

LLVM will CSE all of this (it is pure arithmetic on one local), so I am **not** claiming a
runtime cost — this is readability. A single decomposition reads better and makes the
`_tag`/`_cur_idx` discard bindings unnecessary:

```rust
let (cur_idx, tag) = TaggedIndex::<INDEX_BITS>::unpack(head);
let next_link = if cur_idx == TaggedIndex::<INDEX_BITS>::INDEX_MASK { TAIL } else { cur_idx as u32 };
```

(That form does lose the explicit `is_empty` call, which the comment at `:425-429` deliberately
uses to keep the empty→`TAIL` mapping from resting on a numeric coincidence — so if that
comment's intent is to be preserved, keep `is_empty(head)` and merge only the two `unpack`s.)

Separately: `store_next` is re-issued on **every** loop iteration including retries where
`next_link` is unchanged (`:439`). Skipping a redundant store when the value has not changed
is safe (this thread wrote it, no one else can), and under contention would avoid a
`Release` store to a shared link line per failed CAS. Speculative — needs the contention
rows to show CAS-failure rates high enough to matter.

### P4-8. `Box::leak` is unnecessary inside `std::thread::scope`, and contradicts the comment above it

**Location:** `crates/tagged-index-stack/benches/tagged_index_stack_bench.rs:105-110`.

```rust
// Both are Sync, so we can share them by & reference directly.
// We create them in the outer scope and pass &'static references to threads.
let shared_links: &'static ArrayLinks<LINKS_SIZE> = Box::leak(Box::new(ArrayLinks::new()));
let shared_stack: &'static Stack = Box::leak(Box::new(Stack::new()));
```

The comment says "share them by `&` reference directly", the code then leaks two heap
allocations to manufacture `'static`. Since both contention phases use
`std::thread::scope` (`:135`, `:189`), scoped threads can borrow locals — `let shared_links =
ArrayLinks::<LINKS_SIZE>::new();` and `&shared_links` works with no leak and no `'static`.
(The `move` closures would need `&shared_links` captured explicitly, which scoped threads
support.) Minor, but it is a bench file whose comments are otherwise very careful.

### P4-9. Contention throughput divides by the nominal duration, not the measured elapsed

**Location:** `crates/tagged-index-stack/benches/tagged_index_stack_bench.rs:170-171` and
`:236-237`.

```rust
let total_ops_per_sec = total_ops / DURATION_SECS;   // DURATION_SECS == 1
```

The loop condition is `start.elapsed().as_secs() < DURATION_SECS`, so each thread runs until
elapsed **reaches** 1s and then finishes its current iteration; actual elapsed is ≥ 1s plus
thread spawn/join overhead, which is inside the window (`start` is taken before
`thread::scope`). At `DURATION_SECS = 1` the division is also a no-op, making the units
claim ("ops/sec") arithmetic-free. Use the real elapsed:

```rust
let elapsed = start.elapsed().as_secs_f64();
let total_ops_per_sec = total_ops as f64 / elapsed;
```

### P4-10. Two ~70-line loom tests differ only in one `Ordering` argument

**Location:** `crates/tagged-index-stack/tests/loom_aba.rs:576-667`
(`cas_retry_path_must_acquire_with_concurrent_push`) and `:672-753`
(`counterfactual_relaxed_cas_failure_corrupts_free_list`).

The bodies are near-identical: same setup, same A/B thread shapes, same oracle, same failure
message. The only differences are `Ordering::Acquire` vs `Ordering::Relaxed` at `:612`/`:704`
and `:640`/`:728`. The file already establishes the right idiom two sections earlier —
`run_h2(preserve_tag_on_drain: bool)` at `:405-471` parameterises exactly this
fixed-vs-counterfactual pair for the H-2 scenario. Extracting a
`run_cas_retry(failure_ordering: Ordering)` would remove ~70 duplicated lines and make the
one meaningful difference visible at the call sites.

### P4-11. "RAD-1" is never expanded

**Location:** `crates/tagged-index-stack/src/lib.rs:10, 39, 64, 311, 318, 366`;
`README.md:40`; `CHANGELOG.md:48`.

"H-2" is at least given a full explanation under its own heading. "RAD-1" appears eight
times across the published docs as a bare identifier with no expansion — it is an internal
project codename that means nothing to an external reader. The *concept* is explained (lazy
links, never eagerly written); only the label is opaque. Either expand it once at first use
("the lazy link discipline (internally: RAD-1)") or drop the tag and refer to it as "the lazy
link discipline" throughout. Same applies to "H-2" for consistency, though that one is at
least self-contained.

### P4-12. Small API-surface gaps and dead derives

**Locations:** `crates/tagged-index-stack/src/lib.rs:177-178`, `:369-372`, whole file.

- `#[derive(Debug, Clone, Copy)]` on `TaggedIndex` (`:177`), a unit struct the crate's own
  doc calls "a zero-sized namespace of `const fn` bit operations" and which is never
  instantiated anywhere in src, tests, or benches. Harmless, but three derives on a type with
  no values. (The struct-as-namespace shape itself is defensible — it enables the
  `type Tag = TaggedIndex<16>;` aliasing the tests use throughout — so I am not suggesting
  replacing it with free const-generic functions.)
- **No `is_empty()` on the stack.** The only way to observe emptiness without popping is
  `raw_head()`, which is `#[doc(hidden)]` and explicitly not-public-API. A user monitoring
  free-list exhaustion has no non-destructive option. `pub fn is_empty(&self) -> bool {
  TaggedIndex::<INDEX_BITS>::is_empty(self.head.load(Ordering::Relaxed)) }` is three lines and
  would let `raw_head` stay hidden without cost. (Document it as advisory-only — the answer is
  stale the moment it returns.)
- **No `Send`/`Sync` static assertion.** Both types are auto-`Send + Sync` today, but nothing
  pins it; adding a non-auto field (a `Cell`, a raw pointer) would silently remove `Sync` from
  a type whose entire purpose is cross-thread sharing. A
  `const _: () = { fn assert_sync<T: Sync + Send>() {} fn _check() { assert_sync::<TaggedIndexStack<16>>(); assert_sync::<ArrayLinks<4>>(); } };`
  in `tests/stack_unit.rs` costs nothing.

### P4-13. Test-coverage gaps

**Location:** `crates/tagged-index-stack/tests/`.

- **`TaggedIndexStack` is never exercised at `INDEX_BITS = 1`.** `proptest_pack_unpack.rs:20-33`
  covers the degenerate width for `TaggedIndex` packing only; no test pushes/pops through the
  actual stack at width 1 (where the sole valid index is `0` and `empty_index() == 1`). That is
  the width most likely to expose an off-by-one in the sentinel split.
- **`push`'s panic is only tested at width 32** (`tests/stack_unit.rs:151-186`), where
  `INDEX_MASK == TAIL` — i.e. at the one width where the guard's two purposes coincide. There
  is no test that `TaggedIndexStack::<16>::push(&links, 0xFFFF)` panics, which is the ordinary
  case the guard exists for.
- **No test touches either `Default` impl** (`src/lib.rs:346-350`, `:537-541`).
- The compile-fail gap for `INDEX_BITS > 32` is honestly recorded at
  `tests/stack_unit.rs:188-194` as manually verified. That is fine; noting it here only so the
  list is complete.

### P4-14. Undocumented panics in `Links` implementations and in `push`

**Location:** `crates/tagged-index-stack/src/lib.rs:352-360` (`ArrayLinks`'s impl), `:305-313`
(the trait), `:411-417` (`push`'s `# Panics`).

`ArrayLinks::load_next`/`store_next` index `self.next[index as usize]` and panic on
`index >= N`. Neither method has a `# Panics` section, and neither does the trait note that
implementations may panic on out-of-range indices. `push`'s `# Panics` covers only
`index >= INDEX_MASK`, so a user reading it concludes `push(&links, 100)` on an
`ArrayLinks<4>` is contract-compliant — it is (per `push`'s stated contract) and it panics
anyway, from a different layer.

This is a real usability gap because `N` and `INDEX_BITS` are independent const parameters
with nothing relating them: `TaggedIndexStack<16>` accepts indices up to 65534 while
`ArrayLinks<256>` accepts up to 255, and only the latter enforces its bound. Fix: add a
`# Panics` to both `ArrayLinks` methods ("Panics if `index >= N`"), and one sentence to
`push`/`pop` noting that the supplied `Links` implementation may impose a narrower bound than
`INDEX_MASK`.

---

## Things I checked and found correct (recorded so a later reviewer need not redo them)

- **Tag monotonicity / ABA.** `push` is the only writer that changes the tag, always by
  `wrapping_add(1)` (`:442`); `pop` preserves it in both branches (`:484`, `:486`). The head
  word `(idx, tag)` can therefore only recur after a full `2^TAG_BITS` wrap. A stale popper's
  CAS on `(X, t)` cannot succeed after any intervening push of X, because that push produced
  at least `t+1`.
- **H-2 empty transition.** `pop`'s drain branch packs `empty_index()` with the observed
  running tag (`:483-484`), and `is_empty` inspects only the index half (`:281-283`), so the
  empty word carries a live tag unambiguously. The crate-doc explanation at `:43-62` of *why*
  tag-reset reopens ABA (drain → 0, refill → 1, collides with a parked snapshot at tag 1) is
  correct.
- **Sentinel split at every width.** `_CHECK_BITS` caps `INDEX_BITS` at 32 (`:201-206`), so
  `INDEX_MASK <= u32::MAX == TAIL` always, so `index < INDEX_MASK` implies `index != TAIL` —
  the claim at `:405-409` holds. At width 32 the two coincide numerically and `push`'s guard
  still rejects `TAIL` (pinned by `tests/stack_unit.rs:151-186`). At widths < 32 they differ
  and the empty→`TAIL` mapping is spelled out explicitly rather than relying on the
  coincidence (`:425-435`).
- **`_CHECK_BITS` reachability.** Forced from `pack` (`:241`) and from `INDEX_MASK`'s own
  initializer (`:218`), which `unpack`/`empty_index`/`is_empty` all reference — so the guard
  fires on any mask-touching use. (`TAG_BITS` is the documented exception; see P3-5.)
- **`push`'s `Relaxed` CAS-failure ordering** (`:448`) is sufficient: `push` reads no link and
  no caller data through the retried head, only integer halves of it.
- **`pop`'s `Acquire` CAS-failure ordering** (`:490`) is necessary, and the loom counterfactual
  at `tests/loom_aba.rs:672-753` correctly demonstrates why (the retry's `load_next` can read
  the stale initial link value without it, duplicating an index).
- **Tag-width arithmetic in the docs.** `2^48 ≈ 2.81 × 10^14` ✓; `2^48 / 10^5 / (3600·24·365)
  ≈ 89.2 years` ✓; `2^32 / 10^5 / 3600 ≈ 11.9 hours` ✓ (`src/lib.rs:85-98`, `README.md:46-53`).
- **Portability claims.** `thumbv6m-none-eabi` (no atomic CAS), `thumbv7em-none-eabi` (ARMv7-M
  has no 64-bit exclusives), `riscv32imc-unknown-none-elf` (no A extension), and
  `armv5te-unknown-linux-gnueabi` (`max_atomic_width = 32`) all genuinely lack
  `target_has_atomic = "64"` (`src/lib.rs:109-124`, `:135-144`).
- **`[target.'cfg(loom)'.dependencies]`** (`Cargo.toml:27-28`) is the pattern loom's own
  documentation prescribes and does work with a `RUSTFLAGS`-supplied `--cfg` (Cargo passes
  RUSTFLAGS to the `rustc --print cfg` query it resolves target cfgs from).
- **Loom vacuity guard.** `.github/workflows/ci.yml:2416-2418` tees the run and
  `grep -F "test pop_empty_transition_preserves_tag ... ok"`, so a dropped `--cfg loom`
  (which would compile `#![cfg(loom)]` to `running 0 tests`, exit 0) fails the step. Good.
- **`#![cfg_attr(not(test), no_std)]`** (`:126`) is load-bearing, not cruft: `[lib] test` is
  not disabled, so `cargo test` builds a lib test harness that needs `std`. Real `no_std`
  coverage comes from the `cargo build -p tagged-index-stack --target x86_64-unknown-none` CI
  step.
- **`cargo test -p tagged-index-stack`**: 18 tests across 3 files, all green.
- **`RUSTDOCFLAGS="-D warnings" cargo doc -p tagged-index-stack --no-deps`**: clean, no broken
  intra-doc links.
