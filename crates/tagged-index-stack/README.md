# tagged-index-stack

A lock-free LIFO free-list of small **indices** — a *slot recycler* — whose head
is a single atomic word packing an `(index | tag)` pair, where a STRICTLY
MONOTONIC generation **tag** in the high bits eliminates the ABA problem
outright, for every permitted `INDEX_BITS` — it never wraps. Every successful
push installs a tag exactly one greater than the one it observed, and a push
that observes the ceiling (`TaggedIndex::TAG_MAX`) is refused
(`Err(TagExhausted)`) instead of wrapping back to 0, sealing the stack
(pops are unaffected and keep draining). Consequently every `(index, tag)`
head word occurs in at most one contiguous interval of the head's history, so
a stale CAS can never be reinstated by a later cycle. The enforced
`INDEX_BITS` cap guarantees every legal configuration a large tag — the crate
docs' "Tag-width budget" section derives, from cache-coherence throughput on
the single head cache line, a hardware-bounded floor of days of continuously
saturated pushes at the widest permitted width before a head this width
seals. That is a LIFETIME bound, not a risk bound: because the tag never
recurs, there is no collision to reason about at any point in that lifetime
or beyond it — a head just stops accepting pushes, loudly, once its budget is
spent. Allocation-free, `no_std`;
the production library source (`src/`) is `#![deny(unsafe_code)]` with
exactly EIGHT audited, item-scoped `#[allow(unsafe_code)]` lint-exception
regions, all in `src/imp.rs` — the `unsafe trait StackStorage` declaration
(whose three hooks are `unsafe fn`) plus the sealed `SealedStorage`
trait/bridge surface, and the caller-facing push boundary (`push_index` and
`ArrayIndexStack::push`, both `unsafe fn` under a three-clause link-domain +
liveness + exclusive-ownership contract). The repository's integration tests are separate crate
targets outside that deny and intentionally carry additional `unsafe impl
StackStorage` test fixtures (correct implementor examples plus
deliberately-broken compile-fail fixtures). A region is a lint-exception
boundary, not a count of unsafe declarations/blocks/operations inside it —
see the crate documentation's "Where unsafe lives" section (`src/lib.rs`)
for the authoritative declaration/block-count breakdown and the
self-verifying inventory commands.

Slab allocators, object pools, entity-component stores, id allocators, and
connection tables all need to recycle small integer ids. Crates like
`sharded-slab` embed one privately; this ships the primitive standalone, with
a loom model-check of the real type — exhaustive within each of several
bounded, individually-scoped models (not one unbounded check of the whole
behavior space; see the "## loom — real-type model-check" section below for
the precise scope, including the one counterfactual that drives a buggy
stand-in stack instead of the real type).

## The packed word

The stack head is one `AtomicU64` holding a `TaggedIndex<INDEX_BITS>`: the low
`INDEX_BITS` bits carry a slot index, the high `64 - INDEX_BITS` bits carry a
strictly monotonic generation tag bumped on every successful push — it never
wraps (see above). The index half's
all-ones value is the reserved "stack empty" sentinel. The classic ABA scenario
(A reads `head = X`; B pops X then re-pushes X) is defeated because B's re-push
bumps the tag, so A's CAS on `(X, old_tag)` fails and retries.

`pack`/`unpack` convert between an `(index, tag)` pair and the packed word;
`pack` is checked, returning `None` for an out-of-range index or tag instead
of silently truncating it.

## One implementor owns the head AND the links

A `StackStorage<INDEX_BITS>` implementor supplies BOTH the head (its `head()`)
AND the links (`load_next` / `store_next`) in a single impl — the head↔links
binding is expressed once, in that impl, rather than re-asserted per call.
`push_index`/`pop_index` are crate-owned (a blanket `StackOps` impl over
every `StackStorage` implementor), so the CAS-loop bodies cannot be
overridden downstream. The old per-call repro — two independent calls, each
supplying a different link array against one head and double-issuing an
index — no longer compiles. The obligation moved rather than vanished, and the part that stayed live is
implementor/caller discipline at the VALUE level: a head must be reachable
through exactly ONE live implementor value at a time (the trait doc's `# Safety` clause 1),
and disjoint REACHABLE-index populations per binding over any shared
link-cell population — cell sharing per se is harmless (two stacks over the
same cells with disjoint populations coexist correctly); the hazard is one
index reachable from two bindings (the trait doc's `# Safety` clause 3). These are
obligations about head↔links BINDINGS — invisible to any audit of a single
impl block, discharged by construction. All three `StackStorage` hooks are
`unsafe fn` with per-method caller-side `# Safety` contracts — a call from
safe code is a compile error (E0133), and an `unsafe`-block call puts the
caller under the hook's own contract (for `head()`: no second, competing
binding built around the returned reference) — and the owned
`ArrayIndexStack` additionally does not implement the trait at all (its
`head` field is private, no trait impl hands it out), so a competing
binding around a standalone `ArrayIndexStack` still does not COMPILE
(pinned by the compile-fail fixture
`tests/compile_fail/array_index_stack_head/`).
For CUSTOM implementors the shared-head shape remains expressible — only
behind an `unsafe impl` asserting the very `# Safety` clause it violates. The
`StackStorage` trait doc's "The shared-storage hazard class" section is the
single source of truth for the full inventory and for what the runtime does
and does not detect (pinned by that compile-fail fixture and the pinning
tests in `tests/custom_storage_impl.rs`).

A production allocator keeps its links **slot-resident** (an `AtomicU32` field
inside a slot it already owns) rather than paying for a second array, via a
custom `StackStorage` impl. For standalone use, `ArrayIndexStack<INDEX_BITS,
N>` is the owned standalone stack that fuses the head and an `ArrayLinks<N>`
backing, with `push`/`pop` methods — `push` is `unsafe fn` (the caller
upholds the link-domain + liveness + exclusive-ownership contract, see below); `pop` stays safe.

**Storage requirement: dedicated, never payload-aliased.** Slot-resident means
the link lives in memory the slot owns, not that it may share bytes with the
slot's live payload — a backing that overlays the link on the popped slot's
first bytes (the classic free-block-header idiom) is not supported. `pop_index`'s
corruption-detection guard panics (release-active, not debug-only) on TWO
value shapes — an out-of-range link and a self-loop (`next == index`) — so a
corrupted-but-in-range ACYCLIC backing still passes silently; see the
[`StackStorage`] trait doc's "Storage requirement" and "The shared-storage
hazard class" sections for the exact catch/miss boundary.

`StackHead::is_empty()` (also reachable through `ArrayIndexStack::is_empty()`)
is an advisory, `Relaxed` emptiness check —
useful for diagnostics/monitoring, but a concurrent push or pop can make it
stale the instant it returns, so `pop_index`'s `None` remains the only
authoritative empty check.

## Two correctness-critical subtleties (H-2 and RAD-1)

- **H-2 empty-transition tag preservation.** When a pop drains the LAST element,
  the head goes "empty". Packing the empty sentinel with **tag 0** reopens the
  ABA window (a parked popper's stale tag can recur after a drain+refill). The
  fix packs the empty sentinel with the RUNNING tag the draining pop just
  observed, so the tag keeps climbing. The shipped loom counterfactual
  `counterfactual_empty_transition_tag_reset_lets_aba_recur` proves this is
  load-bearing.
- **Lazy link discipline (internally: RAD-1).** Links are NEVER eagerly written — only a push
  writes a link. A caller whose link backing is OS-zeroed memory never
  first-touches those pages merely to set up the free-list; they commit lazily,
  on first push of each index. (In the allocator this crate was extracted from,
  this saved a ~16 MiB bootstrap first-touch — because the links there were
  slot-resident, so eagerly chaining them would have first-touched every
  slot's page, and the SLOTS are what total ~16 MiB, not the link array
  itself.) A fresh stack is therefore EMPTY.

### No double-push — compiler-enforced unsafe boundary, still not runtime-checked

- **No double-push (caller-side `# Safety` clause).** An index must NOT
  already be reachable from ANY stack that reads and writes the same link
  cells this stack's `load_next`/`store_next` touch. This rule is no longer
  prose-only caller discipline: it is clause 2 of `push_index`'s caller-side
  `# Safety` contract, behind a compiler-enforced unsafe boundary —
  `push_index` is an `unsafe fn`, so a bare call from safe code is a compile
  error (E0133) — but the compiler checks only that an `unsafe` context
  exists, not the clause's substance: it is STILL not runtime-checked, no
  detector exists. Consequence:
  re-pushing a live index closes a cycle in the link chain — a
  deeper-than-head loop silently hands one index to two callers; re-pushing
  the current head trips `pop_index`'s self-loop detector on the first pop.
  Checking liveness would cost an O(n) chain walk per push, so `push_index`'s
  own unconditional check is only `index < INDEX_MASK` — necessary for the
  head-word encoding, but never sufficient proof of the implementor's
  (typically narrower) link domain.
  Full contract and consequences: `push_index`'s `# Safety` section (crate
  docs).

- **No duplicate authority over the same index (exclusive ownership epoch,
  caller-side `# Safety` clause 3).** Each push of an index must be backed
  by a unique, not-yet-consumed publish/recycle authority epoch over that
  index — freshly minted, or obtained from one specific successful `pop`
  that returned the index to this caller. Clause 2's "not reachable" is a
  point-in-time check at call entry only. The push consumes its epoch at its
  own successful head CAS — the linearization point, not physical return —
  so another thread MAY legitimately pop the just-published index and push
  it again (backed by its own epoch from that pop) even before the original
  push call has physically returned: that overlap is permitted. What
  clause 3 forbids is two pushes acting on the SAME epoch (no intervening
  successful pop): both satisfy the entry checks and still corrupt the
  free-list into a self-loop (`next[index] == index`), which `pop_index`'s
  detector panics on; pinned from both sides in the loom suite by
  `counterfactual_same_index_concurrent_push_self_loops` (the forbidden
  duplicate-authority race) and
  `pop_repush_after_publish_conserves` (the permitted overlap).
  Full contract: `push_index`'s `# Safety` section (crate docs).

## Tag-width budget

The tag never recurs, so it does not defend against ABA "while" some window
holds — it SEALS: a head accepts successful pushes until its tag reaches
`TaggedIndex::TAG_MAX`, then `push_index` refuses (`Err(TagExhausted)`)
rather than wrapping. The time a head's tag budget lasts is bounded by
hardware (cache-coherence throughput on the single head cache line), not by
the workload. This bound is why `INDEX_BITS > 16` is compile-time rejected
(`TaggedIndex::_CHECK_BITS`), not merely discouraged — an availability floor
(enough pushes-until-sealed lifetime for ordinary long-running use), not a
soundness floor: sealing is safe at any width, just impractically frequent
below it. The derivation and the figures (hardware rate bound across
contended and uncontended regimes, seal times at each permitted width, the
uncontended bench receipt, and the fresh-sample command) are in the crate
docs' "Tag-width budget" section.

### Why the default is not a wider packed word (128-bit CAS)

A 128-bit packed word was considered and explicitly rejected: `loom` has no
`AtomicU128`, so the real type would lose its model-check; it would add an
unsafe third-party dependency; and `cmpxchg16b` is not in the x86-64
baseline. Full rationale in the repository ADR
`docs/adr/2026-09-01-tagged-index-stack-doc-consolidation-and-review-history.md`
(repository file, not part of the published package). A genuine future need
for >65535 indices should be a separate opt-in, feature-gated type — not a
change to this default.

## Lock-freedom and starvation

`push_index`/`pop_index` never block on a lock — a losing CAS retries — but
lock-freedom is not starvation-freedom: a call can lose arbitrarily many
CASes in a row, and the exponential backoff deliberately makes an unlucky
call wait longer between retries. The shipped backoff cap trades worse
extreme outliers and a thread-count-dependent slow-pop tail-count band for
better latency through p99.9 and roughly 4-5x aggregate wall-clock
throughput. A latency-sensitive consumer must size its tolerance at its own
thread count — neither single thread count's story generalizes. Full
measurements and per-thread-count tables: the crate docs'
"Lock-freedom and starvation" section and
[`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` §3.4](https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md)
(repository file, not in the published package).

## Portability limit — requires 64-bit atomics

The stack head is a single `AtomicU64` (the packed `(index | tag)` word), so
this crate needs `target_has_atomic = "64"` and will **not compile** on a
target without native 64-bit atomic support — notably `thumbv6m-none-eabi`,
`thumbv7em-none-eabi`, `riscv32imc-unknown-none-elf`, and
`armv5te-unknown-linux-gnueabi`. `no_std`-compatible does not imply
64-bit-atomic support: several Cortex-M and RISC-V-without-A-extension
targets are `no_std` yet lack `AtomicU64` entirely. An unsupported-target
build fails fast with an explicit `compile_error!` naming the requirement.

## loom — real-type model-check

Under `--cfg loom` the atomics alias to `loom::sync::atomic`, so the loom suite
model-checks the real `ArrayIndexStack` / `StackHead` / `TaggedIndex` code
exhaustively (no
`preemption_bound`). Several models run end-to-end through the shipped
`push`/`pop`; most of the rest drive the real head atomic and the real
packing through `cas_head_for_test` so an interleaving can be pinned — the one
exception is the untagged-ABA counterfactual, which drives a locally-defined
buggy stand-in stack instead of the real type. `#[should_panic]`
counterfactuals (untagged corruption, the H-2 tag-reset ABA, and a
Relaxed-CAS-failure-ordering regression) prove the harness is non-vacuous.
See `tests/loom_aba.rs`'s own module doc for the per-model breakdown:

```sh
RUSTFLAGS="--cfg loom" cargo test -p tagged-index-stack --release --features loom --test loom_aba
```

## Notes

This crate's test-only surface is feature- and cfg-gated, not merely
`#[doc(hidden)]`: under DEFAULT features none of the test probes below
exists at all — a downstream consumer cannot name them, and a default
`cargo doc` render (docs.rs included) does not contain them. (The attribute
alone only hides an item from rustdoc's rendered navigation while it stays
publicly callable; the gate is what makes it genuinely absent, and each
gated item additionally carries no semver stability guarantee.)

The ONE `#[doc(hidden)]` item that remains in a default build is
`TaggedIndex::empty()` — not test-only: it is used internally by this
crate's bootstrap path (`StackHead::new` / `ArrayIndexStack::new`), and its
one out-of-crate consumer is `sefer-alloc`'s registry bootstrap — through
that crate's `#[cfg(loom)]` `bootstrap::loom_shim` TEST shim (its mirrored
const-capable `StackHead::new`, which keeps the const `REGISTRY` static
compiling under loom and is never on a modeled interleaving); a production
`sefer-alloc` build takes the real `StackHead` type directly and never
calls `empty()` itself. So it is not freely removable, but do not depend on
it either.

Under the `test-internals` feature (off by default — a default build of the
crate carries no instrumentation at all) or a `--cfg loom` build:
`StackHead::raw_head` (also reachable through `ArrayIndexStack`'s gated
forwarder) is a test-only probe of the packed head word, used only by this
crate's own `tests/`; `ArrayIndexStack::load_next_for_test` is the matching
read-only link probe; `retry_counts_for_test` reads both CAS-retry counters
in one call; and `backoff_cap_reached_for_test` reads the matching
backoff-depth counters (non-zero only when a retry's spin loop ran at full
backoff depth) — these last two are the non-loom twins of the loom-only
accessors below and are what `tests/threaded_conservation.rs` uses as its
two-level activation oracle under real OS threads.

Under `--cfg loom` only (not present in a normal build or on docs.rs):
`StackHead::cas_head_for_test` (also reachable through `ArrayIndexStack`'s
gated forwarder) is a raw CAS on the head word that the shipped loom proof
(`tests/loom_aba.rs`) uses to split a pop's head-load from its CAS and drive
ABA counterfactuals; `pop_retry_count_for_test`/`push_retry_count_for_test`
are loom-only accessors over the same retry-activation counters that the
same suite asserts against; `ArrayIndexStack::store_next_for_test` is a raw
WRITE to a link cell (bypassing the stack algorithm entirely) that the same
loom proof uses to reconstruct a pre-seal wrapping counterfactual — it is
`loom`-only, not `test-internals`, because a safe write of this shape is
reachable exclusively for that one loom counterfactual; enabling it under
plain `test-internals` would let any downstream consumer construct a cycle
in the linked chain and make `pop()` double-issue an index.

## Example

```rust
use tagged_index_stack::ArrayIndexStack;

let stack = ArrayIndexStack::<16, 1024>::new(); // 16-bit index, 48-bit ABA tag

// SAFETY: 7 is in this stack's 0..1024 link domain and has never been
// pushed, so its publish/recycle authority is freshly minted and consumed
// by this one push (clause 3).
unsafe { stack.push(7) }.expect("fresh head has tag budget"); // recycle index 7
assert_eq!(stack.pop(), Some(7));         // recycled index comes back out
```

## MSRV

Rust 1.79 — the measured LIBRARY-surface floor (the newest API the published
library itself uses is the inline `const` block in `ArrayLinks::new`'s array
repeat, stable in 1.79; verified with `cargo +1.79 check`, default and
`--features test-internals`). The crate's own test/clippy target set needs
newer toolchains (dev-dependency graph, `std::panic::PanicHookInfo` at 1.81)
— dev-only needs do not raise the floor a library consumer pays. Details:
the `rust-version` comment in this crate's `Cargo.toml`.

## License

MIT OR Apache-2.0.
