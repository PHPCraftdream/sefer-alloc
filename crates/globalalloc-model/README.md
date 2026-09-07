# globalalloc-model

`globalalloc-model` drives a Rust allocator with allocation, deallocation,
reallocation, and zeroed-allocation operations while tracking the expected live
blocks in a small reference model.

The driver checks the M1-M4 oracles at the points where each property is
observable:

- **M1 (allocation response and read-back):** the driver treats null from
  `alloc` or `alloc_zeroed` as a test failure, even though `GlobalAlloc` permits
  null as its allocation-failure result. The driver fills each non-null extent
  and reads the fill back. A null `realloc` is accepted and leaves the old
  block live.
- **M2 (authorized double-free is a no-op):** when explicitly enabled with
  `Config::double_free: Some(unsafe { DoubleFreeOk::new() })`, the driver
  immediately repeats a completed `dealloc`. This is off by default because an
  ordinary `GlobalAlloc`, including `System`, makes double-free undefined
  behavior. The oracle observes only later corruption; it cannot directly prove
  that the repeated call did nothing.
- **M3 (no live-block overlap):** every non-null extent returned by `alloc`,
  `alloc_zeroed`, or `realloc` is compared with the other currently live
  extents before it is accepted into the model.
- **M4 (alignment):** every non-null return is checked against the requested
  alignment before the driver accesses it.
- **`alloc_zeroed`:** every returned byte is checked for zero before the block
  receives its fill pattern.
- **`realloc`:** the initialized `min(old_size, new_size)` prefix is checked for
  preservation on a non-null return.

Live fills are checked before a block is passed to `dealloc` or `realloc`, and
again before each teardown deallocation. These are observation points, not
continuous monitoring: corruption that occurs and is repaired between checks
is invisible.

This is the correctness counterpart to
[`malloc-bench-rs`](https://crates.io/crates/malloc-bench-rs), the performance
harness. The default build has no dependencies.

## Installation and features

For a manual operation stream, no feature is needed:

```toml
[dev-dependencies]
globalalloc-model = "0.1"
```

The optional front-ends are independent:

| Feature | Adds | Intended use |
| --- | --- | --- |
| `proptest` | `op_strategy(Config, Range<usize>)` | Property tests |
| `arbitrary` | `Arbitrary` for `OpStream` | `cargo fuzz` / libFuzzer |

For proptest, enable the crate feature and add `proptest` itself because the
test uses its macros and traits:

```toml
[dev-dependencies]
globalalloc-model = { version = "0.1", features = ["proptest"] }
proptest = "1"
```

For a cargo-fuzz target:

```toml
[dependencies]
globalalloc-model = { version = "0.1", features = ["arbitrary"] }
libfuzzer-sys = "0.4"
```

## Runnable manual `System` example

This complete program uses only the default feature set. Put it in
`examples/manual_system.rs` and run `cargo run --example manual_system`:

```rust
use globalalloc_model::{drive, Config, Op};
use std::alloc::System;

fn main() {
    let ops = [
        Op::Alloc { size: 64, align: 8 },
        Op::AllocZeroed {
            size: 128,
            align: 16,
        },
        Op::Realloc {
            i: 0,
            new_size: 96,
        },
        Op::Dealloc(1),
        Op::Dealloc(0),
    ];

    drive(&System, Config::default(), &ops);
}
```

Do not enable the double-free oracle for `System`.

## Proptest example

With the `proptest` setup above:

```rust
use globalalloc_model::{drive, op_strategy, Config};
use proptest::prelude::*;
use std::alloc::System;

proptest! {
    #[test]
    fn system_matches_model(ops in op_strategy(Config::default(), 0..200)) {
        drive(&System, Config::default(), &ops);
    }
}
```

## libFuzzer example

With the `arbitrary` setup above, a cargo-fuzz target can be:

```rust
#![no_main]

use globalalloc_model::{drive, Config, OpStream};
use libfuzzer_sys::fuzz_target;
use std::alloc::System;

fuzz_target!(|stream: OpStream| {
    drive(&System, Config::default(), &stream.ops);
});
```

`Arbitrary` itself has no configuration parameter. For a custom distribution,
wrap `OpStream` in a local newtype and delegate its `Arbitrary` implementation
to `OpStream::arbitrary_with_config`.

## `no_std` and portability

The default model uses only `core` and `alloc`. The `proptest` dependency is
also configured with its `alloc` and `no_std` features, so both configurations
can be used in a `no_std` allocator's test setup. The `arbitrary` front-end
currently requires `std` because its derive-generated recursion guard uses
`std::thread_local!`.

Without `std`, proptest uses deterministic seeding. Consumers that want random
OS-seeded property runs should also depend on a std-enabled `proptest`, as this
crate's own test suite does. CI runs all integration targets on 32-bit i686
and all feature combinations on 64-bit Windows and Linux, while the
repository's broad CI retains the bare-metal and Miri checks.

## The allocator seam and reentrancy

`drive` is generic over the unsafe `RawAllocator` trait, the four-method
`GlobalAlloc` surface. Every `GlobalAlloc` receives a blanket implementation;
an allocator engine may implement `RawAllocator` directly. Because of that
blanket implementation, a type that already implements `GlobalAlloc` needs a
newtype if it also needs custom `RawAllocator` behavior.

The driver allocates its `Vec` bookkeeping through the installed global
allocator. If that allocator is also under test, bookkeeping allocations can
interleave with the explicit operation stream. This does not inherently
deadlock: `System` in the runnable example is a normal case. It can, however,
perturb allocator state, and an implementation that is not safe against its
own re-entry may recurse or deadlock. Drive the underlying engine directly
when the test needs to isolate allocator operations from driver bookkeeping.

## Limits of the oracle

`RawAllocator` is unsafe because several properties cannot be diagnosed safely.
A non-null return must already denote a live allocation with the documented
extent. If it is dangling, undersized, or otherwise invalid, the driver's first
write or read can itself be undefined behavior rather than a useful failure.

Likewise, `alloc_zeroed` must return initialized bytes, and the old prefix passed
to `realloc` must be initialized. Reading genuinely uninitialized bytes is
undefined behavior both natively and under Miri; neither environment is
promised to turn that contract violation into a reliable oracle report.

Each block's expected contents are a position-dependent pattern derived from a
fill identifier in `1..=255` (the byte at offset `o` of a block carrying
identifier `f` is `f + o`, wrapping), so a repeated single byte, a shifted
copy, or a permuted prefix no longer reads back as a whole block's
expectation. Residual collisions remain: the pattern has period 256 in the
offset, and after 255 fill assignments two live blocks can share an
identifier, so corruption aligned to that period (or from one such block into
the other) can still collide with the expected values. Direct extent-overlap
checks still run when a block is created. Any corruption that is restored
between observation points is also undetectable.

On normal return, every surviving modeled block is deallocated. On an oracle
panic, allocations may be leaked deliberately. Generic cleanup is not safe
after faults such as an invalid or overlapping pointer because attempting to
deallocate the suspect set could cause undefined behavior or a double-free.
This matters if a caller catches oracle panics and keeps the process alive.

The generators bound individual requests, but that cannot guarantee that the
allocator or process will not run out of memory: many live operations can
accumulate, and an allocator may impose tighter limits. `drive` treats null
from `alloc` or `alloc_zeroed` as a test failure; null from `realloc` is the
supported failure result and leaves the old block live.

`Config::validate` rejects an invalid `max_align`, all-zero weights, and size
bounds above `isize::MAX`. Hand-built operations with zero or oversized sizes
are clamped into the `GlobalAlloc` size contract, but a zero, non-power-of-two,
or otherwise inadmissible alignment is rejected. Two accepted degenerate
generator configurations are worth calling out: `small_max == 0` produces a
size-1 small arm, and `large_max <= small_max` produces a large arm at
`small_max + 1`, which can exceed the configured `large_max`.

## Compatibility

Fuzzer bytes are not a stable serialized operation format. Decoder changes
can reinterpret existing corpus entries; retain an `Op` stream when an exact
sequence must be replayed.

`Op`, `Config`, and `OpStream` are exhaustive types with public fields by
design. Adding a field or variant is a breaking change and requires a minor
version bump while the crate is on 0.x.

## License

MIT OR Apache-2.0.
