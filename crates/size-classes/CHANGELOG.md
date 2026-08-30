# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0 - Unreleased

First release. Everything below is new in this version; nothing has shipped
before it.

### Added

- **`build_table(params) -> [usize; N]`** — a `const fn` that builds a
  mimalloc-style size-class table: a geometric progression
  (`round_up(ceil(prev * num / den), min_block)`, minimum step `min_block`, starting
  at `min_block`) sorted-merged with an explicit `extras` list. `extras`
  preconditions — every entry a multiple of `min_block` AND `>= min_block`,
  the list strictly increasing — plus `min_block` being a power of two,
  `geo_count > 0`, a non-zero growth denominator, `N == geo_count +
  extras.len()`, and the FINAL MERGED table being strictly increasing (an
  `extras` entry that DUPLICATES a geometric value is rejected at this
  function's own chokepoint, not left for a caller of `build_size2class` to
  discover downstream; an extra landing strictly between two geometric
  values is valid and expected) are **machine-checked**: a
  violation panics identically in `const` evaluation (a compile error at
  the consumer's table definition) and at runtime, never a silently
  accepted bad table — every precondition is a named assert, not a bare
  division/index panic.
- **`build_size2class(table, min_block) -> [u8; L]`** — derives the O(1)
  `size → class` lookup from a table at compile time using the
  monotone-pointer technique (`O(buckets + classes)` const-eval), with a
  compile-time pin that every class INDEX fits a `u8` (up to 256 classes,
  indices `0..=255`), and a machine-checked global-monotonicity/disjointness
  pass over the merged table — this stays in place as defense-in-depth for a
  table a caller assembles by hand rather than through `build_table`.
- **`size2class_len(max_class, min_block)`** — the `const fn` a consumer uses
  to pin the lookup length `L` (`max_class / min_block + 1`) as a `const`
  expression; asserts `min_block` is a power of two like its siblings, and
  that the `+ 1` itself does not overflow `usize`.
- **`Params<'a>`** — the scheme's parameter set (`min_block`, `growth` as
  `(num, den)` — `(5, 4)` is the classic mimalloc 1.25× spacing — `geo_count`,
  `extras`, `huge_threshold`), all plain data so the whole scheme is usable in
  `const` context. `#[non_exhaustive]` **with** a `const fn new` constructor:
  future field additions are semver-MINOR, and the const constructor keeps
  the type constructible in `const` context where struct literals no longer
  compile.
- **`SizeClasses<N, L>`** — the built scheme, generic over table length `N`
  and lookup length `L`, both pure functions of `Params`:
  - `const fn build(params)` — construct the whole scheme at compile time;
    intended placement is a `static` (a `const` this size re-materializes at
    every use site -- see the `SizeClasses` doc);
  - accessors `table()`, `size2class()`, `min_block()`, `min_block_shift()`,
    `small_align_max()`, `small_max()`, `huge_threshold()`, `count()`,
    `block_size(idx)`, `is_huge(size)`;
  - **`class_for(size, align) -> Option<usize>`** — resolve a request to the
    smallest class whose block is `>= max(size, align)` **and** a multiple of
    `align` (`None` routes to the caller's large path). O(1) fast path for
    `align <= min_block` (every class size is a multiple of `min_block`, so
    the stride trivially satisfies divisibility -- see below for the
    separate base-address requirement); for larger
    power-of-two alignments, a provably
    equivalent **jump** slow path rounds the block up to the next multiple of
    `align` and re-seeds through the lookup, skipping whole runs of
    non-divisible classes instead of stepping one class at a time (see
    `class_for`'s own rustdoc for why this matters -- a request whose
    `align` exceeds what a hand-rolled classifier happens to handle is a
    real bug class this crate exists to remove).
    `align` must be a power of two (the `Layout` contract), enforced by a
    `debug_assert!` — with `debug_assertions` off, a non-zero
    non-power-of-two `align` is unspecified: it can return a wrong
    `Some`/`None`. The `(size, align) == (0, 0)` corner does not panic, but
    only with `overflow-checks` ALSO off (never memory unsafety either way,
    regardless of profile). `try_class_for` below closes this for callers
    that don't already know `align` is valid. The divisibility check is a
    STRIDE property,
    not an address guarantee: it preserves whatever alignment the caller's
    carve base already has, it does not create it -- `base % align == 0` for
    every served `align` is the caller's own documented precondition, which
    this crate (pure size arithmetic, no addresses) cannot check.
  - **`try_class_for(size, align) -> Result<Option<usize>, InvalidAlign>`** —
    the checked twin of `class_for`: validates `align` instead of assuming
    it (`Err(InvalidAlign(align))` for a non-power-of-two `align`, including
    `0`, before any arithmetic runs), then delegates. Never panics, for any
    `(size, align)` pair — the substantive reason to prefer it over
    `class_for` whenever `align` is not already known-valid by construction
    (e.g. taken directly from a `core::alloc::Layout`). Mirrors
    `Layout::from_size_align` (checked) next to
    `Layout::from_size_align_unchecked` (trusted) in `core::alloc`; being a
    separate function, it adds zero cost to `class_for`'s own hot path --
    `try_class_for` itself does strictly more work than `class_for` (an
    added power-of-two check before delegating). `InvalidAlign` implements
    `Display` and `core::error::Error`, and is a plain `pub` tuple struct
    (not `#[non_exhaustive]`) — a deliberate pre-0.1.0 decision, since it
    has exactly one reason to exist and no foreseeable second field.
- `is_huge` compares against the caller-supplied `huge_threshold` policy
  parameter — the crate has no notion of an OS segment size; the consumer
  decides where "large" ends and "huge" begins for its own segment policy.
- The whole crate is `no_std`, zero-dependency, and `#![forbid(unsafe_code)]`;
  the geometric-advance step is computed in `u128` and range-checked, so an
  overflowing scheme is a loud error rather than a silently wrapped
  (wrong-but-valid-looking) table, and a scheme whose next class fits is not
  rejected merely because an intermediate product does not.
- `SizeClasses` is `Clone` but deliberately **not** `Copy`: an instance
  embeds both tables (~16 KiB for a realistic scheme), so `Copy` would make
  a full-object duplicate look as cheap as a move. Settled before the first
  release, since removing `Copy` afterwards would be a breaking change.
- `SizeClasses` also implements `Debug`, hand-written rather than derived:
  a derive would print both raw tables on any accidental `{:?}`/`dbg!`,
  so the impl instead prints a short summary (`N`, `L`, `min_block`,
  `small_max`, `huge_threshold`) and marks the rest `..` via
  `finish_non_exhaustive`.

### MSRV

- Rust 1.88.
