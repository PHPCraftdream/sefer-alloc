# Safety invariants

These are the properties `sefer-alloc` upholds. They are encoded as tests
(unit tests in `src/lib.rs` plus the proptest harness in `tests/differential.rs`)
and form the spec that every future change must keep green.

- **I1 — resolution.** A handle returned by `insert` resolves via `get` to the
  inserted value until it is `remove`d.
- **I2 — tombstone.** After `remove(h)`, `get(h)` is `None` forever and a
  second `remove(h)` is a no-op `None`.
- **I3 — no ABA.** A stale handle — one whose slot has since been reused —
  never resolves to a live value. The slot's generation is bumped on removal,
  so the old handle fails the generation check and yields `None`.
- **I4 — accounting.** `len()` equals the number of live entries, and
  `is_empty()` agrees.
- **I5 — drop-once.** Every live value is dropped exactly once: on `remove`
  (returned to the caller) or on `Region` drop. None is dropped twice; none is
  leaked.
- **I6 — compaction (Phase 2, not yet implemented).** After compaction, every
  live handle still resolves to the same logical value, and reclaimed slots are
  reused. See `docs/PLAN.md`.

## Why handles, not pointers

A raw pointer into a `Vec` dangles the moment the `Vec` reallocates or the
element is removed — and dereferencing it is undefined behaviour. A handle is
an *index plus a generation*: the worst case is a checked lookup that returns
`None`. We trade one unconditional `unsafe` dereference for one safe integer
compare. That is the whole idea, and it is why the single-threaded core needs
no `unsafe` at all — the dense `Vec<T>` performs every initialization and drop.
