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
spent.

Allocation-free and `no_std`. Slab allocators, object pools, entity-component
stores, id allocators, and connection tables all need to recycle small
integer ids. Crates like `sharded-slab` embed one privately; this ships the
primitive standalone, with a loom model-check of the real type — exhaustive
within each of several bounded, individually-scoped models (not one
unbounded check of the whole behavior space; see the "## loom — real-type
model-check" section below for the precise scope, including the one
counterfactual that drives a buggy stand-in stack instead of the real type).

## Example

```rust
use tagged_index_stack::ArrayIndexStack;

let stack = ArrayIndexStack::<16, 1024>::new(); // 16-bit index, 48-bit ABA tag

// SAFETY: on this fresh stack, 7 is inside the 0..1024 link domain and no
// caller has pushed it: this first publish is backed by a freshly minted
// publish/recycle authority, which is exactly what `push`'s `# Safety`
// contract requires of a first push.
unsafe { stack.push(7) }.expect("fresh head has tag budget"); // recycle index 7
assert_eq!(stack.pop(), Some(7));         // recycled index comes back out
```

`push` is an `unsafe fn` because the compiler cannot check its caller-side
contract: the pushed index must live in the implementor's link domain, must
not already be reachable from any stack that reads and writes the same link
cells, and each push must carry a unique, not-yet-consumed publish/recycle
authority over the index (freshly minted, or obtained from one successful
`pop` that returned the index to this caller). The full contract is
`push_index`'s `# Safety` section in the crate docs; the crate's complete
unsafe-surface inventory is in "The unsafe surface" section below.

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
`push_index`/`pop_index` are crate-owned through the blanket `StackOps`
implementation, so the CAS-loop bodies cannot be overridden downstream. The
value-level safety obligation is simple: bind each head to one backing for its
whole life, never rebind it, and never let two bindings reach the same index
through shared link cells. Sharing cells for disjoint reachable populations is
fine; these binding-level obligations must be discharged by construction.

All three `StackStorage` hooks are `unsafe fn` with caller-side `# Safety`
contracts. The owned `ArrayIndexStack` does not implement the trait, so a
competing binding around it is rejected by the type system; custom
implementors can express that shape only behind an `unsafe impl`. The
`StackStorage` trait doc's "The shared-storage hazard class" section is the
source of truth for the full inventory and its runtime detection boundary.

A production allocator keeps its links **slot-resident** (an `AtomicU32` field
inside a slot it already owns) rather than paying for a second array, via a
custom `StackStorage` impl. For standalone use, `ArrayIndexStack<INDEX_BITS,
N>` is the owned stack that fuses the head and an `ArrayLinks<N>` backing,
with `push`/`pop` methods — `push` is `unsafe fn` (the caller upholds the
link-domain + liveness + exclusive-ownership contract, see below); `pop`
stays safe.

For a slot-resident implementation, the `StackOps` import provides the
blanket `push_index`/`pop_index` operations:

```rust
use core::sync::atomic::{AtomicU32, Ordering};
use tagged_index_stack::{StackHead, StackOps as _, StackStorage, TAIL};

struct SlotStorage {
    head: StackHead<16>,
    links: [AtomicU32; 8],
}

impl SlotStorage {
    fn new() -> Self {
        Self {
            head: StackHead::new(),
            links: [const { AtomicU32::new(TAIL) }; 8],
        }
    }
}

// SAFETY: one private head has one stable backing; each index in 0..8 has a
// dedicated atomic link cell with Acquire/Release access, and callers provide
// disjoint publish/recycle authority for the in-domain indices.
unsafe impl StackStorage<16> for SlotStorage {
    unsafe fn head(&self) -> &StackHead<16> {
        &self.head
    }

    unsafe fn load_next(&self, index: u32) -> u32 {
        self.links[index as usize].load(Ordering::Acquire)
    }

    unsafe fn store_next(&self, index: u32, next: u32) {
        self.links[index as usize].store(next, Ordering::Release);
    }
}

let storage = SlotStorage::new();
for index in 0..4 {
    // SAFETY: each index is in 0..8, fresh, and published exactly once.
    unsafe { storage.push_index(index) }.expect("fresh head has tag budget");
}
assert_eq!(storage.pop_index(), Some(3));
```

The owned array is convenient for small stacks. `ArrayIndexStack<16, 65535>`
is 256 KiB by value; a large local `let` can overflow a small debug thread
stack. Because `new` is `const`, prefer static placement for a large owned
stack, or use slot-resident `StackStorage` when the links already belong to
caller-owned slots. For example:

```text
static LARGE_STACK: ArrayIndexStack<16, 65535> = ArrayIndexStack::new();
```

The owned array checks its `N` link bound on each access; a slot-resident
implementor can use a proven domain to avoid that second bounds check in its
own link accessor.

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

## Two correctness-critical subtleties

- **Empty-transition tag preservation.** When a pop drains the LAST element,
  the head goes "empty". Packing the empty sentinel with **tag 0** reopens the
  ABA window (a parked popper's stale tag can recur after a drain+refill). The
  fix packs the empty sentinel with the RUNNING tag the draining pop just
  observed, so the tag keeps climbing. The shipped loom counterfactual
  `counterfactual_empty_transition_tag_reset_lets_aba_recur` proves this is
  load-bearing.
- **Lazy link discipline.** Links are NEVER eagerly written — only a push
  writes a link. OS-zeroed backing is not first-touched merely to initialize
  the free-list; links are committed lazily on each index's first push. A
  fresh stack is therefore EMPTY.

### Caller obligations at the unsafe boundary

`push_index` is `unsafe` because callers must prove three facts the compiler
cannot: the index is in the implementor's fixed link domain; it is not already
reachable from another stack using the same cells; and this call consumes a
unique publish/recycle authority (fresh or returned by a successful `pop`).
The compiler checks only that an unsafe context exists. Runtime guards catch
out-of-range links and current-head self-loops, but not every deeper cycle, so
the full contract in the crate docs remains required.

## Tag-width budget

The tag never recurs, so it does not defend against ABA "while" some window
holds — it SEALS: a head accepts successful pushes until its tag reaches
`TaggedIndex::TAG_MAX`, then `push_index` refuses (`Err(TagExhausted)`)
rather than wrapping. The time a head's tag budget lasts is bounded by
hardware: the fastest uncontended head RMW is the upper bound, while
contention on the single head cache line only lowers aggregate throughput.
This bound is why `INDEX_BITS > 16` is compile-time rejected
(`TaggedIndex::_CHECK_BITS`), not merely discouraged — an availability floor
(enough pushes-until-sealed lifetime for ordinary long-running use), not a
soundness floor: sealing is safe at any width, just impractically frequent
below it. The derivation and figures are in the crate docs' "Tag-width budget"
section.

### Why the default is not a wider packed word (128-bit CAS)

A 128-bit packed word was considered and explicitly rejected: `loom` has no
`AtomicU128`, so the real type would lose its model-check; it would add an
unsafe third-party dependency; and `cmpxchg16b` is not in the x86-64
baseline. Full rationale in the repository ADR
`docs/adr/2026-09-01-tagged-index-stack-doc-consolidation-and-review-history.md`.
A genuine future need
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
thread count — neither single thread count's story generalizes. The cap
counts `spin_loop` hint invocations, not portable time units, so this
trade is specific to the measured host and can differ across
microarchitectures and targets. Full measurements and per-thread-count
tables are in the crate docs'
"Lock-freedom and starvation" section and
[`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` §3.4](https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md).

## Portability limit — requires 64-bit atomics

The stack head is a single `AtomicU64` (the packed `(index | tag)` word), so
this crate needs `target_has_atomic = "64"` and will **not compile** on a
target without native 64-bit atomic support — notably `thumbv6m-none-eabi`,
`thumbv7em-none-eabi`, `riscv32imc-unknown-none-elf`, and
`armv5te-unknown-linux-gnueabi`. `no_std`-compatible does not imply
64-bit-atomic support: several Cortex-M and RISC-V-without-A-extension
targets are `no_std` yet lack `AtomicU64` entirely. An unsupported-target
build fails fast with an explicit `compile_error!` naming the requirement.

On AArch64, the portable baseline may lower atomic CAS operations to outlined
compiler/runtime atomic calls; baseline code must not assume LSE instructions.
Consumers may select `-C target-feature=+lse` (or an equivalent `target-cpu`)
only when their deployment guarantees LSE support. That is an explicit
deployment choice, not a crate requirement.

## loom — real-type model-check

Under `--cfg loom` the atomics alias to `loom::sync::atomic`, so the loom suite
model-checks the real `ArrayIndexStack` / `StackHead` / `TaggedIndex` code
exhaustively (no
`preemption_bound`). Several models run end-to-end through the shipped
`push`/`pop`; most of the rest drive the real head atomic and the real
packing through `cas_head_for_test` so an interleaving can be pinned — the one
exception is the untagged-ABA counterfactual, which drives a locally-defined
buggy stand-in stack instead of the real type. `#[should_panic]`
counterfactuals (untagged corruption, empty-transition tag-reset ABA,
Relaxed-CAS-failure-ordering regression, same-index concurrent-push
self-loop, and bypassed-seal stale-CAS double-issue) prove the harness is
non-vacuous.
See `tests/loom_aba.rs`'s own module doc for the per-model breakdown:

```sh
RUSTFLAGS="--cfg loom" cargo test -p tagged-index-stack --release --features loom --test loom_aba
```

## The unsafe surface

The production library source (`src/`) is `#![deny(unsafe_code)]`: every
`unsafe` token outside an audited set of item-scoped `#[allow(unsafe_code)]`
lint-exception regions — all in `src/imp.rs` — is a hard compile error. Those
regions cover the `unsafe trait StackStorage` declaration (its three hooks
are `unsafe fn`), the crate-private `SealedStorage` trait/bridge surface, and
the caller-facing push boundary (`push_index` and `ArrayIndexStack::push`,
both `unsafe fn` under a three-clause link-domain + liveness +
exclusive-ownership contract).

The repository's integration tests are separate crate targets outside that
deny, and intentionally carry additional `unsafe impl StackStorage` test
fixtures — correct implementor examples plus deliberately-broken compile-fail
fixtures.

A region is a lint-exception boundary, not a count of the unsafe
declarations, blocks, or operations inside it. The audited region count, the
declaration/block-count breakdown, and the self-verifying inventory commands
are stated in ONE place — the crate documentation's "Where unsafe lives"
section (`src/lib.rs`) — next to the grep that re-derives them, and are
deliberately not re-quoted here so they cannot drift from it.

## Notes

This crate's hidden test probes are absent from default builds. Under
`tagged_index_stack_test` or `loom`, the read/counter probes include
`raw_head`, `load_next_for_test`, `with_tag_for_test`, `retry_counts_for_test`,
and `backoff_spin_depths_for_test`;
`backoff_spin_count_for_test` is loom-only. The raw CAS/write probes
(`cas_head_for_test`, `store_next_for_test`) remain loom-only. When enabled,
all are `#[doc(hidden)]`; the cfg gates keep them out of default and docs.rs
builds. The `tagged_index_stack_test` cfg is an explicitly unstable,
repository-test escape hatch: its probes may be changed or removed without a
semver guarantee, and consumers must not build production code against them.
The default package test run therefore skips the seal and backoff oracles;
repository CI enables the test cfgs. The `loom` cfg/feature has the same
policy for its loom-only probes. These
surfaces remain public only because Cargo integration-test targets are separate
crates; no standalone harness crate is needed, and the cfg is not part of the
stable API contract.

The bootstrap empty word is crate-private. `StackHead::new` and
`ArrayIndexStack::new` construct it internally, while external consumers that
need the sentinel use the stable `TaggedIndex::empty_index()` plus
`TaggedIndex::pack(empty_index, 0)` contract instead of depending on an
undocumented empty-word helper.

## MSRV

Rust 1.79 — the measured LIBRARY-surface floor (the newest API the published
library itself uses is the inline `const` block in `ArrayLinks::new`'s array
repeat, stable in 1.79; verified with `cargo +1.79 check`, default and
`RUSTFLAGS="--cfg tagged_index_stack_test"`). The crate's dev/test/bench graph
is measured separately on Rust 1.88 by the pinned `cargo check`, `cargo test
--no-run`, and `cargo bench --no-run` rows; dev-only needs do not raise the
floor a library consumer pays. Details: the `rust-version` comment in this
crate's `Cargo.toml`.

## License

MIT OR Apache-2.0.
