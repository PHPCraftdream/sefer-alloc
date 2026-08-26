# size-classes

Const-built mimalloc-style **size-class tables** with a compile-time-derived
**O(1) size→class lookup** and an **alignment-divisibility classifier** — the
trio every slab / pool / arena allocator reinvents, packaged as a `no_std`,
zero-dependency, `#![forbid(unsafe_code)]` unit.

- `build_table` — a `const fn` sorted-merge of a geometric progression
  (`round_up(prev * num / den, min_block)`) with a strictly increasing list of
  `min_block`-multiple, `>= min_block` explicit extra classes (page-aligned
  classes, an exact size the geometric run skips, a medium tier …) — all three
  preconditions are machine-checked, so violations panic identically in
  `const` evaluation and at runtime, never silently accepted input.
- `build_size2class` — derives the O(1) `size→class` lookup from a table at
  compile time (monotone-pointer, `O(buckets + classes)`), with a compile-time
  `u8` pin on the class count.
- `SizeClasses::class_for(size, align)` — O(1) fast path for `align <=
  min_block`, and a provably-equivalent **jump** slow path for larger
  alignments: round up to the next multiple of `align` and re-seed through the
  lookup, skipping whole runs of non-divisible classes. Without it a request
  whose `align` exceeds what the caller's classifier happens to handle
  silently falls through to the caller's whole-segment path — a real bug
  class in hand-rolled allocators (SEFER's own motivating case: `align >=
  512`). The classifier chooses an align-**divisible** stride; block
  **addresses** are align-aligned iff the base you carve from is
  (`address(k) = base + k * block_size` — stride divisibility preserves the
  base's alignment, it cannot create it). This is the caller's documented
  precondition, not a crate check — the crate never sees an address.

The "huge" threshold is a **policy parameter** (`Params::huge_threshold`); the
crate has no notion of an OS segment size.

`Params` is `#[non_exhaustive]` (field growth is plausible, e.g. a future
`small_align_max` knob) — construct it via `Params::new(..)`, not a struct
literal.

## Example

```text
use size_classes::{build_table, size2class_len, Params, SizeClasses};

const MIN_BLOCK: usize = 16;
const GEO_COUNT: usize = 40;
const EXTRAS: &[usize] = &[256, 512, 1024, 2048, 4096];
const PARAMS: Params = Params::new(MIN_BLOCK, (5, 4), GEO_COUNT, EXTRAS, 4 << 20);

// Both generics are pure functions of PARAMS — derive them, don't pin them.
const N: usize = GEO_COUNT + EXTRAS.len();
const TABLE: [usize; N] = build_table::<N>(&PARAMS);
const L: usize = size2class_len(TABLE[N - 1], MIN_BLOCK);

const SC: SizeClasses<N, L> = SizeClasses::build(PARAMS);

// SC.class_for(size, align) -> Option<usize>
// SC.block_size(idx) -> usize;  SC.count() -> usize;  SC.small_max() -> usize;
```

Runnable forms live in `tests/`.

## License

MIT OR Apache-2.0.
