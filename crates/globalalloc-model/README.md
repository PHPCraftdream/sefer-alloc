# globalalloc-model

Differential-test any Rust allocator against a trivial reference model.

Apply a random stream of `alloc` / `dealloc` / `realloc` / `alloc_zeroed`
operations to the allocator under test **and** to a reference model (a `Vec` of
live blocks), asserting the **M1–M4 correctness oracles** on every step:

- **M1 (validity):** every returned pointer is non-null, aligned to the
  requested align, and writable for the requested size (fill-pattern read-back).
- **M2 (no double-free / UAF):** a second `dealloc` of the same pointer is a
  no-op that must not corrupt the allocator.
- **M3 (no overlap):** two simultaneously-live allocations never share a byte
  (checked against every live block, plus per-block fill re-checked at run end).
- **M4 (alignment & size fidelity):** the returned pointer satisfies the
  requested size and align.
- **`alloc_zeroed` contract:** every byte of a zeroed allocation reads as 0.
- **`realloc` prefix preservation:** the `min(old, new)` prefix is preserved.

This is the correctness twin of
[`malloc-bench-rs`](https://crates.io/crates/malloc-bench-rs) (the performance
side). Nothing else on crates.io offers a ready "differential-test your
`GlobalAlloc` against a model with UAF/overlap/zeroed/realloc oracles" kit.

## One model, two front-ends

The same `drive()` loop powers both:

- a **proptest** `Strategy<Value = Vec<Op>>` (feature `proptest`) — for
  `cargo test` and the bounded miri run, and
- an **`impl Arbitrary for OpStream`** (feature `arbitrary`) — for `cargo fuzz`
  / libFuzzer.

So an oracle improvement reaches proptest, miri, and libFuzzer at once. A normal
build (no features) has **zero non-dev dependencies** — both front-ends are
optional.

## The allocator seam

`drive()` is generic over a minimal `unsafe trait RawAllocator` — exactly the
`alloc` / `dealloc` / `realloc` / `alloc_zeroed` surface of `GlobalAlloc`, for
which a blanket impl is provided. A plain owned allocator with the same four
methods can implement the trait directly.

## Usage

```text
use globalalloc_model::{drive, Config};
use std::alloc::System;

// proptest:
proptest! {
    #[test]
    fn matches_model(ops in globalalloc_model::op_strategy(Config::default(), 0..200)) {
        drive(&System, Config::default(), &ops);
    }
}

// libFuzzer:
fuzz_target!(|stream: globalalloc_model::OpStream| {
    drive(&System, Config::default(), &stream.ops);
});
```

## License

MIT OR Apache-2.0.
