# globalalloc-model

Differential-test any Rust allocator against a trivial reference model.

Apply a random stream of `alloc` / `dealloc` / `realloc` / `alloc_zeroed`
operations to the allocator under test **and** to a reference model (a `Vec` of
live blocks), asserting the **M1–M4 correctness oracles** on every step:

- **M1 (validity):** every returned pointer is non-null and aligned to the
  requested align; size fidelity is established indirectly (fill-pattern
  read-back proves the requested size is writable, and the overlap check over
  requested extents catches an undersized block once a neighbour lands inside
  the missing tail).
- **M2 (no double-free / UAF):** a second `dealloc` of the same pointer must
  not corrupt the allocator (opt-in via `Config::double_free: Some(unsafe { DoubleFreeOk::new() })`; **off by
  default** — a real system malloc treats double-free as UB, so the harness
  only issues the second free when you hand it this `unsafe`-constructed token).
- **M3 (no overlap):** two simultaneously-live allocations never share a byte
  — the overlap check runs on **every block-creating op** (`alloc`,
  `alloc_zeroed`, and `realloc`'s new extent), plus a per-block fill re-checked
  at run end.
- **M4 (alignment):** every returned pointer is aligned to the requested
  align (extent fidelity is established indirectly — see M1).
- **`alloc_zeroed` contract:** every byte of a zeroed allocation reads as 0.
- **`realloc` prefix preservation:** the `min(old, new)` prefix is preserved.

This is the correctness twin of
[`malloc-bench-rs`](https://crates.io/crates/malloc-bench-rs) (the performance
side): a ready-made op-stream driver plus reference-model oracles for
differential-testing any `GlobalAlloc`. A normal build (no features) has
**zero dependencies** — both front-ends are optional.

## One model, two front-ends

The same `drive()` loop powers both:

- a **proptest** `Strategy<Value = Vec<Op>>` (feature `proptest`) — for
  `cargo test` and the bounded miri run, and
- an **`impl Arbitrary for OpStream`** (feature `arbitrary`) — for `cargo fuzz`
  / libFuzzer.

So an oracle improvement reaches proptest, miri, and libFuzzer at once.

Need a custom `Config` for the fuzz front-end? `Arbitrary` takes no
parameters, so a config cannot be threaded through the plain
`fuzz_target!(|stream: OpStream| ...)` form. Wrap the stream in a local
newtype whose `Arbitrary` impl delegates to
`OpStream::arbitrary_with_config` (keeping libFuzzer's structured crash
report) and pass the newtype to `fuzz_target!` — the pattern is spelled
out in `OpStream::arbitrary_with_config`'s rustdoc.

## `no_std` by default

The core model needs only `core` + `alloc`, so this crate can
differential-test a `no_std` allocator's own test suite without pulling in
`std` — verified against a real bare-metal target (`thumbv7em-none-eabi`)
for the default build AND for the `proptest` front-end (declared with
`default-features = false, features = ["alloc", "no_std"]`, which routes
proptest's float samplers through num-traits/libm). One consequence of that
no_std mode: without `std`, proptest seeds its RNG from a hardcoded constant,
so a consumer relying on this feature alone gets *deterministic* seeding;
this crate's own test suite dev-depends on a default-featured `proptest` so
its property runs are randomly seeded. The one exception is
the `arbitrary` front-end: `derive_arbitrary`'s generated recursion guard
unconditionally references `std::thread_local!` — an upstream limitation,
not this crate's choice.

## The allocator seam

`drive()` is generic over a minimal `unsafe trait RawAllocator` — exactly the
`alloc` / `dealloc` / `realloc` / `alloc_zeroed` surface of `GlobalAlloc`, for
which a blanket impl is provided. A plain owned allocator with the same four
methods can implement the trait directly.

Two caveats: `drive()` allocates its own bookkeeping through the *global*
allocator, so if the allocator under test is also the installed
`#[global_allocator]`, a reentrant allocator will deadlock or recurse — drive
the *engine* behind your `GlobalAlloc`, not the installed global allocator
itself. And because of the blanket impl, a type that already implements
`GlobalAlloc` cannot ALSO implement `RawAllocator` — wrap it in a newtype if
you need to override the forwarding.

## Compatibility

`Op`, `Config`, and `OpStream` are exhaustive types with public fields on
purpose (so `Config { double_free: Some(unsafe { DoubleFreeOk::new() }), ..Config::default() }` stays
ergonomic). Adding a field or variant is a breaking change and bumps the
minor version under 0.x.

## Usage

```text
use globalalloc_model::{drive, op_strategy, Config};
use std::alloc::System;

// proptest (the `proptest!` macro comes from the `proptest` crate):
proptest! {
    #[test]
    fn matches_model(ops in op_strategy(Config::default(), 0..200)) {
        drive(&System, Config::default(), &ops);
    }
}

// libFuzzer (the `fuzz_target!` macro comes from `libfuzzer-sys`):
fuzz_target!(|stream: globalalloc_model::OpStream| {
    drive(&System, Config::default(), &stream.ops);
});
```

## License

MIT OR Apache-2.0.
