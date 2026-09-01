# tagged-index-stack

A lock-free LIFO free-list of small **indices** — a *slot recycler* — whose head
is a single atomic word packing an `(index | tag)` pair, where a wrapping
generation **tag** in the high bits mitigates the ABA problem for every
permitted `INDEX_BITS`: the tag defeats the ordinary short-window ABA
pattern, but it is finite and demonstrably wraps, so ABA is reduced to a
quantified recurrence risk, not eliminated. A collision requires a FULL tag
wrap — `2^TAG_BITS` successful pushes anywhere on the stack — occurring WHILE
one specific victim thread stays parked holding its stale snapshot for that
entire span. The mitigation is a derived, quantified bound, not a slogan: the
enforced `1..=16` cap on `INDEX_BITS` guarantees every legal configuration a
tag of at least 48 bits, and the "Tag-width budget" section below derives,
from cache-coherence throughput on the single head cache line, a
hardware-bounded floor on that recurrence window — roughly 3.3-16 days of
continuously saturated pushes at the widest permitted width. The floor is an
engineering risk-reduction argument, not a proof of impossibility:
suspending a thread is outside the crate's control (a debugger pause, a
stop-the-world pause, extreme starvation, instrumentation) and can stretch
the observation window past it; accepting that residual risk is part of the
caller's contract. (The tag is not strictly monotonic — a strictly monotonic
counter never repeats a value, and this one wraps — it just does not repeat
until a full `2^TAG_BITS` pushes have elapsed, days of continuously
saturated operation at every permitted width.) Allocation-free, `no_std`;
`#![deny(unsafe_code)]` with exactly ONE audited `unsafe` token — the
`unsafe trait StackStorage` declaration (see the crate documentation's
"Where unsafe lives" section for the self-verifying inventory).

This is the canonical "recycle a small integer id" primitive that slab
allocators, object pools, entity-component stores, id allocators, and connection
tables all reinvent — and routinely reinvent *wrong*. Crates like `sharded-slab`
embed one privately; this ships it as a standalone primitive **with an
exhaustive loom model-check run against the real type**.

## The packed word

The stack head is one `AtomicU64` holding a `TaggedIndex<INDEX_BITS>`: the low
`INDEX_BITS` bits carry a slot index, the high `64 - INDEX_BITS` bits carry a
wrapping generation tag bumped on every successful push. The index half's
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
through exactly ONE live implementor value at a time (the trait doc's rule 1),
and disjoint REACHABLE-index populations per binding over any shared
link-cell population — cell sharing per se is harmless (two stacks over the
same cells with disjoint populations coexist correctly); the hazard is one
index reachable from two bindings (the trait doc's rule 3). These are
obligations about head↔links BINDINGS — invisible to any audit of a single
impl block, discharged by construction. `head()` is not reachable from
outside this crate on ANY implementor: all three `StackStorage` hooks are
witness-gated (each takes a `Hook` witness no code outside this crate can
construct), and the owned `ArrayIndexStack` additionally does not implement
the trait at all (its `head` field is private, no trait impl hands it out),
so a competing binding around a standalone `ArrayIndexStack` does not
COMPILE (pinned by the compile-fail fixture
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
backing, with plain `push`/`pop` methods.

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

## The two hard-won subtleties (people get these wrong)

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

### The rule that is NOT one of the two: no double-push (caller-enforced)

- **No double-push (caller-enforced).** An index must NOT already be reachable
  from ANY stack that reads and writes the same link cells this stack's
  `load_next`/`store_next` touch. Consequence: re-pushing a live index closes
  a cycle in the link chain — a deeper-than-head loop silently hands one
  index to two callers; re-pushing the current head trips `pop_index`'s
  self-loop detector on the first pop. Checking liveness would cost an O(n)
  chain walk per push, so `push_index` checks only `index < INDEX_MASK`.
  Full contract and consequences: `push_index`'s "# Caller contract"
  (crate docs).

## Tag-width budget

A tag defends against ABA only while it does not recur: a stale CAS succeeds
again only after a FULL tag wrap — `2^TAG_BITS` successful pushes anywhere on
the stack. The time a wrap takes is

```text
wrap_time = 2^TAG_BITS / aggregate_successful_push_rate
```

with the rate bounded by HARDWARE, not by the workload. Headline facts: the
enforced `1..=16` cap on `INDEX_BITS` guarantees every legal configuration a
tag of at least **48 bits** at the widest permitted width; at a `2 × 10^8`
pushes/sec working ceiling a wrap takes ≈ 16 days (≈ 3.3 days even at `10^9`
pushes/sec). This bound is why `INDEX_BITS > 16` is compile-time REJECTED
(`TaggedIndex::_CHECK_BITS`), not merely discouraged: width 24 would give a
40-bit tag (≈ 92 minutes at the same ceiling) and the old width-32 cap only
≈ 21 seconds — within reach of ordinary scheduling jitter. The floor is a
risk-reduction argument, not a proof of impossibility: a debugger pause,
stop-the-world pause, or extreme starvation can stretch the observation
window past it, and accepting that residual risk is part of the caller's
contract. Full derivation — the hardware rate bound across contended and
uncontended regimes, the uncontended ~`2 × 10^7` pushes/sec bench receipt,
and the fresh-sample command — is the crate docs' "Tag-width budget"
section.

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
extreme outliers AND a thread-count-dependent slow-pop tail-COUNT band for
better latency through p99.9 (by 1-2 orders of magnitude: p99.9 ≈ 1 µs vs
54-182 µs at 8-16 threads) and roughly 4-5x aggregate wall-clock throughput
(median speedup 4.85x at 8 threads, 4.05x at 16; the backoff-free build
produced ~2.4x more pops slower than 1 ms median-to-median, 1.9-2.6x across
rep pairings). A latency-sensitive consumer must size its tolerance at ITS
OWN thread count — neither single thread count's story generalizes. Full
per-thread-count tables: the crate docs' "Lock-freedom and starvation"
section and
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

This crate has several `#[doc(hidden)]` `pub` items — some under default
features, more under `--cfg loom` (see below). In every case the attribute
only hides the item from rustdoc's rendered navigation — the item remains
publicly callable, carries no semver stability guarantee, and should not be
depended on.

Under default features: `StackHead::raw_head` (also reachable through
`ArrayIndexStack`'s `#[doc(hidden)]` forwarder) is a test-only probe,
used only by this crate's own `tests/`. `TaggedIndex::empty()` is not
test-only — it is used internally by this crate's bootstrap path
(`StackHead::new` / `ArrayIndexStack::new`), and its one out-of-crate
consumer is `sefer-alloc`'s registry bootstrap — but through that crate's
`#[cfg(loom)]` `bootstrap::loom_shim` TEST shim (its mirrored
const-capable `StackHead::new`, which keeps the const `REGISTRY` static
compiling under loom and is never on a modeled interleaving); a production
`sefer-alloc` build takes the real `StackHead` type directly and never
calls `empty()` itself. So it is not freely removable, but do not depend
on it either.

Under the `test-internals` feature (off by default — a default build of the
crate carries no instrumentation at all): `retry_counts_for_test` reads both
CAS-retry counters in one call, and `backoff_cap_reached_for_test` reads the
matching backoff-depth counters (non-zero only when a retry's spin loop ran
at full backoff depth). These are the non-loom twins of the loom-only
accessors below and are what `tests/threaded_conservation.rs` uses as its
two-level activation oracle under real OS threads.

Under `--cfg loom` (not present in a normal build or on docs.rs):
`StackHead::cas_head_for_test` (also reachable through `ArrayIndexStack`'s
`#[doc(hidden)]` forwarder) is a raw CAS on the head word that the
shipped loom proof (`tests/loom_aba.rs`) uses to split a pop's head-load from
its CAS and drive ABA counterfactuals;
`pop_retry_count_for_test`/`push_retry_count_for_test` are loom-only
accessors over the same retry-activation counters (which themselves compile
only under the `test-internals` feature or a loom build) that the same suite
asserts against.

## Example

```rust
use tagged_index_stack::ArrayIndexStack;

let stack = ArrayIndexStack::<16, 1024>::new(); // 16-bit index, 48-bit ABA tag

stack.push(7);                            // recycle index 7
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
