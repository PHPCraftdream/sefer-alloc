# size-classes

Const-built mimalloc-style **size-class tables** with a compile-time-derived
**O(1) size→class lookup** and an **alignment-divisibility classifier** — the
trio every slab / pool / arena allocator reinvents, packaged as a `no_std`,
zero-dependency, `#![forbid(unsafe_code)]` unit.

- `build_table` — a `const fn` sorted-merge of a geometric progression
  (`round_up(ceil(prev * num / den), min_block)`) with a strictly increasing list of
  `min_block`-multiple, `>= min_block` explicit extra classes (page-aligned
  classes, an exact size the geometric run skips, a medium tier …) — all three
  preconditions are machine-checked, so violations panic identically in
  `const` evaluation and at runtime, never silently accepted input.
- `build_size2class` — derives the O(1) `size→class` lookup from a table at
  compile time (monotone-pointer, `O(buckets + classes)`), with a compile-time
  `u8` pin on the class indices (up to 256 classes).
- `SizeClasses::class_for(size, align)` — O(1) fast path for `align <=
  min_block`, and a provably-equivalent **jump** slow path for larger
  alignments: round up to the next multiple of `align` and re-seed through the
  lookup, skipping whole runs of non-divisible classes. Without it a request
  whose `align` exceeds what the caller's classifier happens to handle
  silently falls through to the caller's whole-segment path — a real bug
  class in hand-rolled allocators (`sefer-alloc`'s own motivating case, the
  allocator this crate was extracted from: `align >= 512`). The classifier
  chooses an align-**divisible** stride; block **addresses** are
  align-aligned only if the base you carve from is too — a caller-owned
  precondition the crate cannot check itself (see `SizeClasses::class_for`'s
  `# Preconditions` in the crate docs for the exact requirement).
- `SizeClasses::try_class_for(size, align)` — the checked twin of
  `class_for`: rejects a non-power-of-two `align` (including `0`) with
  `Err(InvalidAlign(align))` before any arithmetic runs, instead of assuming
  a valid `align` and risking unspecified behavior (a wrong class choice, or
  a panic in a debug build). Never panics, for any `(size,
  align)`. Use this one unless `align` is already known-valid by
  construction (e.g. taken from a `core::alloc::Layout`) — `class_for`
  stays the zero-validation hot-path variant for that case.

The "huge" threshold is a **policy parameter** (`Params::huge_threshold`); the
crate has no notion of an OS segment size.

## Memory cost

`SizeClasses` embeds a `size2class` LUT of length `L = max_class / min_block
+ 1` (one `u8` per bucket) plus the `table` itself (`N * size_of::<usize>()`,
usually a few hundred bytes on a 64-bit target). `L` isn't chosen directly —
it falls out of `min_block`/`growth`/`geo_count`/`extras` together, and it
scales with `max_class / min_block`, **not** with the class count `N`: a
scheme with *fewer* classes can produce a *larger* LUT than one with more. A
realistic scheme (`min_block = 16`, 49 classes — the crate's own tests'
`SEFER` fixture; the crate itself has no defaults) gives `L = 16173`, ~16.18
KiB total on a 64-bit target; a 24-class scheme with `min_block = 8` reaches
`L = 18207`, a larger object despite having half the classes. See
[`size2class_len`'s rustdoc](https://docs.rs/size-classes/latest/size_classes/fn.size2class_len.html)
for the full worked comparison.

`Params` is `#[non_exhaustive]` (field growth is plausible, e.g. a future
`small_align_max` knob) — construct it via `Params::new(..)`, not a struct
literal.

## Example

```rust
use size_classes::{build_table, size2class_len, Params, SizeClasses};

const MIN_BLOCK: usize = 16;
const GEO_COUNT: usize = 40;
const EXTRAS: &[usize] = &[256, 512, 1024, 2048, 4096];
const PARAMS: Params = Params::new(MIN_BLOCK, (5, 4), GEO_COUNT, EXTRAS, 4 << 20);

// Both generics are pure functions of PARAMS — derive them, don't pin them.
const N: usize = GEO_COUNT + EXTRAS.len();
const TABLE: [usize; N] = build_table::<N>(PARAMS);
const L: usize = size2class_len(TABLE[N - 1], MIN_BLOCK);

// `static`, not `const`: a `const` this size re-materializes its embedded
// tables at every use site; `static` keeps one fixed-address copy.
static SC: SizeClasses<N, L> = SizeClasses::build(PARAMS);

// SC.class_for(size, align) -> Option<usize>
// SC.block_size(idx) -> usize;  SC.count() -> usize;  SC.small_max() -> usize;
```

Runnable forms live in `tests/`.

## MSRV

Rust 1.88.

## License

MIT OR Apache-2.0.
