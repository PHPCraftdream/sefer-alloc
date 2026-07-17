# size-classes

Const-built mimalloc-style **size-class tables** with a compile-time-derived
**O(1) size→class lookup** and an **alignment-divisibility classifier** — the
trio every slab / pool / arena allocator reinvents, packaged as a `no_std`,
zero-dependency, `#![forbid(unsafe_code)]` unit.

- `build_table` — a `const fn` sorted-merge of a geometric progression
  (`round_up(prev * num / den, min_block)`) with an arbitrary sorted list of
  explicit extra classes (page-aligned classes, an exact size the geometric run
  skips, a medium tier …).
- `build_size2class` — derives the O(1) `size→class` lookup from a table at
  compile time (monotone-pointer, `O(buckets + classes)`), with a compile-time
  `u8` pin on the class count.
- `SizeClasses::class_for(size, align)` — O(1) fast path for `align <=
  min_block`, and a provably-equivalent **jump** slow path for larger
  alignments: round up to the next multiple of `align` and re-seed through the
  lookup, skipping whole runs of non-divisible classes. Without it every
  `align >= 512` request silently falls through to the caller's whole-segment
  path — a real bug class in hand-rolled allocators.

The "huge" threshold is a **policy parameter** (`Params::huge_threshold`); the
crate has no notion of an OS segment size.

## Example

```text
use size_classes::{Params, SizeClasses, size2class_len};

const MIN_BLOCK: usize = 16;
const EXTRAS: &[usize] = &[256, 512, 1024, 2048, 4096];
const N: usize = 40 + EXTRAS.len();
const MAX_CLASS: usize = /* table[N-1] — compute or pin */ 258_752;
const L: usize = size2class_len(MAX_CLASS, MIN_BLOCK);

const SC: SizeClasses<N, L> = SizeClasses::build(Params {
    min_block: MIN_BLOCK,
    growth: (5, 4),
    geo_count: 40,
    extras: EXTRAS,
    huge_threshold: 4 * 1024 * 1024,
});

// SC.class_for(size, align) -> Option<usize>
// SC.block_size(idx) -> usize;  SC.count() -> usize;  SC.small_max() -> usize;
```

Runnable forms live in `tests/`.

## License

MIT OR Apache-2.0.
