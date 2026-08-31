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
saturated operation at every permitted width.) Allocation-free, `no_std`,
`#![forbid(unsafe_code)]`.

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
index — no longer compiles. The obligation moved rather than vanished, and
part of it stayed live: an implementor whose own `load_next`/`store_next`
read and write different backings is still expressible in safe Rust (the
trait doc's rules 3 and 4), and so is a subtler shape — two implementor
values whose `head()` methods return the SAME `StackHead` while their links
differ. Each value is individually coherent, yet the combination
double-issues indices exactly like the old repro, and nothing in the
compiler or the runtime guard catches it. A head must be reachable through
exactly ONE live implementor value at a time (the trait doc's rule 1) — an
implementor/caller obligation, discharged by construction (one storage
object per head), not by auditing impl blocks, which cannot see the
combination.

A production allocator keeps its links **slot-resident** (an `AtomicU32` field
inside a slot it already owns) rather than paying for a second array, via a
custom `StackStorage` impl. For standalone use, `ArrayIndexStack<INDEX_BITS,
N>` is the owned standalone stack that fuses the head and an `ArrayLinks<N>`
backing, with plain `push`/`pop` methods.

**Storage requirement: dedicated, never payload-aliased.** Slot-resident means
the link lives in memory the slot owns, not that it may share bytes with the
slot's live payload — a backing that overlays the link on the popped slot's
first bytes (the classic free-block-header idiom) is not supported and defeats
`pop_index`'s own corruption-detection guard, which panics unconditionally
(release-active, not debug-only) on a backing that violates it.

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

- **No double-push (caller-enforced).** An index that is still reachable from
  the stack must never be pushed again: `push_index` overwrites the pushed
  index's link with the current head, so re-pushing a live index closes a cycle
  in the link chain — `pop_index` stops returning `None` and hands the same
  index to two callers. Checking liveness would cost an O(n) chain walk on
  every push, so the crate cannot enforce this (unlike H-2 and RAD-1);
  `push_index` checks only the `index < INDEX_MASK` bound. Every live index
  comes from exactly one `push_index` and is re-pushed only after the matching
  `pop_index`.

## Tag-width budget

A tag defends against ABA only while it does not recur: a stale CAS can succeed
again only if the head word returns to the exact `(index, tag)` pair the victim
is holding, which takes a FULL tag wrap — `2^TAG_BITS` successful pushes
anywhere in the stack, the last of them re-pushing the victim's own index. The
time a wrap takes is

```text
wrap_time = 2^TAG_BITS / aggregate_successful_push_rate
```

and the rate term is bounded by HARDWARE, not by the workload. The tag is
GLOBAL to the whole stack, not per-slot: every successful push — of any index,
from any thread — is a compare-exchange (a locked RMW) on the ONE `AtomicU64`
head word, so the rate in the formula is the stack's AGGREGATE successful-push
rate across ALL slots, and every one of those pushes serializes on a single
cache line whose exclusive ownership must transfer between cores. That
transfer cost caps the aggregate rate at roughly `10^8` to `10^9` RMWs/sec no
matter how many threads contend — more contention only makes the line's
ownership transfers slower, never faster. That argument covers the CONTENDED
regime; the other bound to check is its opposite — the UNCONTENDED
single-threaded case, where the head line stays resident and exclusive in one
core's L1 and no cross-core ownership transfer ever happens — a regime
governed not by coherence transfer but by the latency of the bare RMW
instruction itself (`lock cmpxchg` on x86-64, or the target's equivalent CAS
instruction): materially faster, but still bounded. The wrap-time conclusion
survives both regimes: this crate's own single-threaded `churn` bench row
measures ~`2 × 10^7` successful pushes/sec in that uncontended regime (a
pop+push pair per iteration, so one successful push per pair — a push-only
rate would run faster still, but the pair rate is already an order of
magnitude under the working ceiling the next paragraph adopts, so the
argument does not need the tighter push-only number). Committed receipt:
the single-threaded `churn` rows in
`docs/perf/_raw_tis_backoff_cap_sweep_run1.log` — a file in this crate's
repository
(<https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/perf/_raw_tis_backoff_cap_sweep_run1.log>),
not in the published package (11th Gen Intel Core
i7-11800H, rustc 1.97.0, 2026-08-31) — e.g. 53.89 ns/pair in that log's
first arm, its 20 such samples spanning 51.41-64.72 ns/pair; re-run
`cargo bench -p tagged-index-stack --bench tagged_index_stack_bench` for a
fresh sample — the bound below only needs the order of magnitude, not the
exact figure.

Taking a generous `2 × 10^8` successful pushes/sec as the working ceiling: the
enforced `1..=16` cap on `INDEX_BITS` guarantees every legal configuration a
tag of at least **48 bits** — at the widest permitted `INDEX_BITS = 16`
(65535 usable indices with the `0xFFFF` empty sentinel reserved above them),
the tag wraps at `2^48 ≈ 2.8 × 10^14` and a wrap takes
`2^48 / (2 × 10^8) ≈ 16` days; even at the optimistic top of the hardware
range it is still `2^48 / 10^9 ≈ 3.3` days. And a wrap is only the
PRECONDITION for a collision: cashing one in further requires that the head
line stay saturated at the coherence ceiling continuously for the entire span
AND that one specific victim thread sit parked, motionless, holding its stale
snapshot the whole time. This bound is why `INDEX_BITS > 16` is REJECTED at
compile time (`TaggedIndex::_CHECK_BITS`) rather than merely discouraged: at
`INDEX_BITS = 24` the tag would be 40 bits, `2^40 / (2 × 10^8) ≈ 92` minutes
at the same ceiling — a long debugger pause or OS scheduling delay defeats
that — and the pre-cap `INDEX_BITS = 32` maximum gave only
`2^32 / (2 × 10^8) ≈ 21` seconds, within reach of ordinary scheduling jitter.
Within the permitted range a caller still trades index range against tag
headroom, but never below the 48-bit floor.

This derivation bounds the RECURRENCE window — the minimum time a victim
thread must stay parked, at saturated push rates, before its exact
`(index, tag)` snapshot can recur. It does not prove recurrence impossible:
the tag turns ABA into a quantified, engineering-manageable risk, and a
deployment whose threads can be parked indefinitely (debuggers,
stop-the-world pauses, extreme starvation) needs its own hazard/epoch-style
protection on top of this crate.

### Why the default is not a wider packed word (128-bit CAS)

A wider packed word — a 128-bit CAS, e.g. via `portable-atomic` or a nightly
intrinsic — was considered and explicitly rejected for this crate's default
primitive. It would drop loom's coverage of the real type (`loom` has no
`AtomicU128`), add an unsafe dependency to a crate that is currently
`#![forbid(unsafe_code)]`, and likely turn `pop`'s currently read-only head
observation into an RMW on targets without a guaranteed native 128-bit CAS
(`cmpxchg16b` is not in the x86-64 baseline). A genuine future need for more
than 65535 indices in one pool would be better served by a separate, explicitly
opt-in type gated behind a feature flag — not by changing this default.

## Lock-freedom and starvation

`push_index`/`pop_index` never block on a lock — a losing CAS retries — but lock-freedom
is not starvation-freedom: a call can lose arbitrarily many CASes in a row,
and the exponential backoff deliberately makes an unlucky call wait longer
between retries. The measured trade is a SMALL NUMBER OF VERY LARGE OUTLIERS
in exchange for better latency at every percentile through p99.9 AND better
aggregate throughput — not "tail latency for throughput" in general. On a
64-element `ArrayLinks` under this crate's own contention discipline (8
threads x 200k pop-then-repush iterations, `--release`): the single worst
`pop` blocked 41-60 ms across three runs under the shipped backoff cap, vs
0.6-24 ms with the backoff disabled — a handful of extreme outliers is the
one axis where disabling the backoff wins — while the same workload finished
~4.9x faster in aggregate under the cap, every percentile through p99.9 was
1-2 orders of magnitude better under the cap (p99.9 ≈ 1 µs vs 54-182 µs at
8-16 threads), and at 16 threads the backoff-free build produced ~2.4x
MORE pops over 1 ms median-to-median (1.9-2.6x across rep pairings). A
consumer recycling a slot on a latency-sensitive
request path should size its tolerance for those rare outliers, not fear a
broad tail; the full table is
[`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` §3.4](https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md)
— a file in this crate's repository, not in the published package.

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
(`StackHead::new` / `ArrayIndexStack::new`) and by the production allocator
this crate ships alongside (`sefer-alloc`, whose registry bootstrap free-list
is built on this crate's head) for the same purpose (a const-capable
bootstrap-empty head word), so it is not freely
removable, but do not depend on it either.

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

Rust 1.88.

## License

MIT OR Apache-2.0.
