# tagged-index-stack

A lock-free LIFO free-list of small **indices** — a *slot recycler* — whose head
is a single atomic word packing an `(index | tag)` pair, where a wrapping
generation **tag** in the high bits structurally defeats the ABA problem for
every permitted `INDEX_BITS`. That is a derived claim, not a slogan: the
enforced `1..=16` cap on `INDEX_BITS` guarantees every legal configuration a
tag of at least 48 bits, and the "Tag-width budget" section below derives,
from cache-coherence throughput on the single head cache line, that such a tag
cannot repeat within any physically plausible observation window. (The tag is
not strictly monotonic — a strictly monotonic counter never repeats a value,
and this one wraps — it just never repeats on a timescale the coherence
protocol can deliver.) Allocation-free, `no_std`,
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

## Slot-resident OR owned links

The stack stores only the HEAD. Each pushed index's "next" link lives in caller
storage, reached through the `Links` trait — so a production allocator keeps its
links **slot-resident** (an `AtomicU32` field inside a slot it already owns)
rather than paying for a second array. For standalone use, `ArrayLinks<N>`
provides an owned `[AtomicU32; N]` backing.

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
  this saved a ~16 MiB bootstrap first-touch.) A fresh stack is therefore EMPTY.

### The rule that is NOT one of the two: no double-push (caller-enforced)

- **No double-push (caller-enforced).** An index that is still reachable from
  the stack must never be pushed again: `push` overwrites the pushed index's
  link with the current head, so re-pushing a live index closes a cycle in the
  link chain — `pop` stops returning `None` and hands the same index to two
  callers. Checking liveness would cost an O(n) chain walk on every push, so
  the crate cannot enforce this (unlike H-2 and RAD-1); `push` checks only the
  `index < INDEX_MASK` bound. Every live index comes from exactly one `push`
  and is re-pushed only after the matching `pop`.

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
regime; the fastest one is its opposite — the UNCONTENDED single-threaded
case, where the head line stays resident and exclusive in one core's L1 and
no cross-core ownership transfer ever happens — a regime governed not by
coherence transfer but by the latency of the bare RMW instruction itself
(`lock cmpxchg` on x86-64, or the target's equivalent CAS instruction):
materially faster, but still bounded. The wrap-time conclusion survives both
regimes: this crate's own bench measures even that fastest one at
~`2 × 10^7` successful pushes/sec, an order of magnitude under the working
ceiling the next paragraph adopts. (The single-threaded `churn` bench row:
`50.75 ns` per pop+push pair — exactly one successful push per pair.)

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
model-checks the real `TaggedIndexStack` / `TaggedIndex` code exhaustively (no
`preemption_bound`). One model runs end-to-end through the shipped
`push`/`pop`; the rest drive the real head atomic and the real packing through
`cas_head_for_test` so an interleaving can be pinned. `#[should_panic]`
counterfactuals (untagged corruption, the H-2 tag-reset ABA, and a
Relaxed-CAS-failure-ordering regression) prove the harness is non-vacuous:

```sh
RUSTFLAGS="--cfg loom" cargo test -p tagged-index-stack --release --features loom --test loom_aba
```

## Notes

`TaggedIndexStack::raw_head` is a `#[doc(hidden)]` test-only probe: the attribute only hides it from rustdoc's rendered navigation, and the function remains publicly callable — it carries no semver stability guarantee and exists only for this crate's own `tests/`, so do not depend on it.

`TaggedIndex::empty()` is the crate's other `#[doc(hidden)]` `pub` item, and the same rules apply — publicly callable, no semver stability guarantee, do not treat it as stable public API. It is not test-only: it is used internally by this crate's bootstrap path (`TaggedIndexStack::new`) and by known in-workspace consumers for the same purpose (a const-capable bootstrap-empty head word), so it is not freely removable — but do not depend on it.

## Example

```rust
use tagged_index_stack::{ArrayLinks, TaggedIndexStack};

let links = ArrayLinks::<1024>::new();
let stack = TaggedIndexStack::<16>::new();   // 16-bit index, 48-bit ABA tag

stack.push(&links, 7);                        // recycle index 7
let idx = stack.pop(&links);                  // -> Some(7)
```

## MSRV

Rust 1.88.

## License

MIT OR Apache-2.0.
