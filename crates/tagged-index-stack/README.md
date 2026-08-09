# tagged-index-stack

A lock-free LIFO free-list of small **indices** — a *slot recycler* — whose head
is a single atomic word packing an `(index | tag)` pair, where a monotonic
**tag** in the high bits defeats the ABA problem. Allocation-free, `no_std`,
`#![forbid(unsafe_code)]`.

This is the canonical "recycle a small integer id" primitive that slab
allocators, object pools, entity-component stores, id allocators, and connection
tables all reinvent — and routinely reinvent *wrong*. Crates like `sharded-slab`
embed one privately; this ships it as a standalone primitive **with executable
loom proofs run against the real type**.

## The packed word

The stack head is one `AtomicU64` holding a `TaggedIndex<INDEX_BITS>`: the low
`INDEX_BITS` bits carry a slot index, the high `64 - INDEX_BITS` bits carry a
monotonic tag bumped on every successful push. The index half's all-ones value
is the reserved "stack empty" sentinel. The classic ABA scenario (A reads
`head = X`; B pops X then re-pushes X) is defeated because B's re-push bumps the
tag, so A's CAS on `(X, old_tag)` fails and retries.

## Slot-resident OR owned links

The stack stores only the HEAD. Each pushed index's "next" link lives in caller
storage, reached through the `Links` trait — so a production allocator keeps its
links **slot-resident** (an `AtomicU32` field inside a slot it already owns)
rather than paying for a second array. For standalone use, `ArrayLinks<N>`
provides an owned `[AtomicU32; N]` backing.

## Two hard-won subtleties (people get these wrong)

- **H-2 empty-transition tag preservation.** When a pop drains the LAST element,
  the head goes "empty". Packing the empty sentinel with **tag 0** reopens the
  ABA window (a parked popper's stale tag can recur after a drain+refill). The
  fix packs the empty sentinel with the RUNNING tag the draining pop just
  observed, so the tag keeps climbing. The shipped loom counterfactual
  `counterfactual_empty_transition_tag_reset_lets_aba_recur` proves this is
  load-bearing.
- **Lazy link discipline (RAD-1).** Links are NEVER eagerly written — only a push
  writes a link. A caller whose link backing is OS-zeroed memory never
  first-touches those pages merely to set up the free-list; they commit lazily,
  on first push of each index. (In the allocator this crate was extracted from,
  this saved a ~16 MiB bootstrap first-touch.) A fresh stack is therefore EMPTY.

## Tag-width budget

With `INDEX_BITS = 16` the tag gets 48 bits, wrapping at `2^48 ≈ 2.8 × 10^14`. A
wrap only reopens ABA if a victim is parked across an entire wrap's worth of
pushes on one slot: at an unrealistic 100k pushes/sec that is **~89 years** —
a structural non-hazard. A 32-bit tag, by contrast, gives only **~12 hours**
of frozen-victim churn at the SAME 100k pushes/sec rate (probabilistic).
Wider indices shrink this budget.

## Portability limit — requires 64-bit atomics

The stack head is a single `AtomicU64` (the packed `(index | tag)` word), so
this crate needs `target_has_atomic = "64"` and will **not compile** on a
target without native 64-bit atomic support — notably `thumbv6m-none-eabi`,
`thumbv7em-none-eabi`, `riscv32imc-unknown-none-elf`, and
`armv5te-unknown-linux-gnueabi`. `no_std`-compatible does not imply
64-bit-atomic support: several Cortex-M and RISC-V-without-A-extension
targets are `no_std` yet lack `AtomicU64` entirely. An unsupported-target
build fails fast with an explicit `compile_error!` naming the requirement.

## loom — real-type proofs

Under `--cfg loom` the atomics alias to `loom::sync::atomic`, so the loom suite
model-checks the REAL `TaggedIndexStack` / `TaggedIndex` code, with
`#[should_panic]` counterfactuals (untagged corruption + the H-2 tag-reset ABA)
proving the harness is non-vacuous:

```text
RUSTFLAGS="--cfg loom" cargo test -p tagged-index-stack --release --test loom_aba
```

## Test-only diagnostic surface

`TaggedIndexStack::raw_head` is `pub` (so this crate's own `tests/` — an
external crate from the library's own perspective — can reach it) but
`#[doc(hidden)]`: it is NOT a stable, documented part of the public API,
carries no semver guarantee, and is not exercised by any production caller.
It exists solely so this crate's own loom/unit tests can inspect the raw
packed head word. Do not depend on it from outside this crate.

## Example

```text
use tagged_index_stack::{ArrayLinks, TaggedIndexStack};

let links = ArrayLinks::<1024>::new();
let stack = TaggedIndexStack::<16>::new();   // 16-bit index, 48-bit ABA tag

stack.push(&links, 7);                        // recycle index 7
let idx = stack.pop(&links);                  // -> Some(7)
```

## License

MIT OR Apache-2.0.
