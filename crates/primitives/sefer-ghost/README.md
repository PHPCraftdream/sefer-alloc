# sefer-ghost

`sefer-ghost` is the *ghost check* infrastructure of the sefer allocator:
checking-build-only assertion and event-counter macros (`ghost_assert!`,
`ghost_event!`) plus a shadow event side table. It was extracted verbatim from
`sefer-alloc`'s former `src/ghost` module (plan step P1.1 of
docs/plans/2026-11-15-workspace-restructure-plan.md); `sefer-alloc` re-exports
it as `sefer_alloc::ghost`, so every in-tree `crate::ghost::…` call site keeps
resolving unchanged.

The crate compiles code only in *checking* builds —
`cfg(any(loom, miri, kani, sefer_ghost))`. In production both macros expand to
nothing (their arguments are not even evaluated), so ghost checks cost no
code, no branches, no allocation (rule R9). The shadow table adds no fields to
production structs, never allocates (it may run inside the global allocator),
and uses `core` atomics only — never loom's — so a ghost check adds no
happens-before edge a model checker could see.

The only feature, `internals`, gates the `probe` self-test entry
(`sefer_ghost::ghost::probe`); `sefer-alloc`'s own `internals` feature forwards
to it so the gate stays identical across the crate boundary.

Ghost builds are enabled with a global `--cfg` flag:
`RUSTFLAGS="--cfg sefer_ghost" cargo ...`.

The crate is `#![no_std]` with zero dependencies and `#![forbid(unsafe_code)]`.
