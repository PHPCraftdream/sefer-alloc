# ring-mpsc

A bounded, allocation-free, `no_std` **MPSC index ring** (Vyukov-style
CAS-reserve push / single-consumer drain) usable over an **owned array** OR
**caller-supplied raw memory** — plus a lost-wakeup-safe `DirtyRouter`.

Unlike `crossbeam`, `heapless::mpmc`, or `rtrb` (all of which *own* their
storage), `ring-mpsc` can sit **in-place over memory you point it at**: shared
memory for IPC, a DMA/driver mailbox, or metadata carved out of an arena. It is
explicit about the two things allocator-grade rings care about and most crates
leave implicit: the **reserved-but-unpublished drain semantics** (a drain stops
at the first slot a producer reserved but hasn't published, and a later drain
picks it up — order is never violated) and the **overflow-as-bounded-loss**
policy (a full ring returns `Err(Full)`; it never overwrites an undrained slot,
and the caller owns the loss policy).

It ships **executable loom proofs run against the real type** (the crate aliases
its atomics to `loom::sync::atomic` under `--cfg loom`), with `#[should_panic]`
counterfactuals proving the harnesses are non-vacuous.

## Two tiers

```rust
use ring_mpsc::{MpscRing, Owned, Raw, U32Entry};

// (a) Safe owned storage.
let ring = MpscRing::<Owned<U32Entry, 256>>::new();
ring.push(42).unwrap();                 // any producer; Err(Full) => your policy
let stop = ring.drain(|off| { /* reclaim */ });   // single consumer; returns new head
if ring.tail_relaxed() != stop { /* a real drain is needed */ }  // guard idiom

// (b) In-place over raw memory (unsafe: you guarantee the bytes).
const CAP: usize = 256;
let footprint = MpscRing::<Raw<U32Entry, CAP>>::FOOTPRINT;
// ... obtain `base: *mut u8` to `footprint` aligned, exclusively-owned bytes ...
// let ring = unsafe { MpscRing::<Raw<U32Entry, CAP>>::over_raw(base) };
```

Two payload entries are provided: `U32Entry` (a single `u32`, one-word publish)
and `UsizeU32Entry` (a `(usize, u32)` pair, two-word pair-publish with the
Release-sequence torn-read guard). Implement `RingEntry` for your own `Copy`
payload.

## DirtyRouter

A lock-free "ready-set" over `WORDS * 64` keys: producers `mark(key)` a key
**after** publishing into its channel; the consumer `for_each_dirty(|key| ...)`
processes exactly the set keys via a per-word `swap(0, Acquire)`.

**Honest contract — at-least-once wakeup with bounded deferral.** A producer
stalled between its channel publish and its `mark` is *boundedly deferred* — its
entry is in the channel but invisible to a `for_each_dirty` that runs in that
window. It becomes visible on the next pass, via another producer's mark of the
same key, or — the caller's obligation — via a periodic unconditional full sweep
of every channel. **The consumer must either tolerate deferral until the next
mark, or run that periodic full sweep as a backstop.** This is exactly how
sparse epoll-style ready-lists and deferred-free queues behave; it is a
contract, not magic.

## `#![no_std]`, zero deps

A normal build pulls in zero non-std crates. `loom` is a `cfg(loom)`-gated
dependency used only when you run the shipped loom proofs.

## License

MIT OR Apache-2.0.
