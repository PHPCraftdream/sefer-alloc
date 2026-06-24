# Implementation plan

`sefer-alloc` — a safe, handle-addressed region store over bytes. Built
verification-first: the tests and their tooling (proptest / miri / loom /
fuzz / multi-arch) are part of each phase, not an afterthought.

## Honest prior art

`slotmap`, `thunderdome`, and `generational-arena` already provide the
single-threaded vessel. Phases 0–2 deliberately re-tread that ground — we build
our own clean, verified core as craft and foundation. The genuinely novel,
no-safe-ready-answer work is Phases 3b–4: the concurrent epoch tier and the
byte / global-allocator descent.

## Architecture

See [`DESIGN.md`](DESIGN.md). Three organs — Cartographer (safe placement
logic), Membrane (the typed `Handle` API), Hand (the single confined `unsafe`,
present only in the lower tiers). The typed single-threaded core is
`#![forbid(unsafe_code)]`.

## Phases

Each phase has a **gate**: the objective, tool-checkable condition for "done".

| # | Phase | Gate ("green") | Task |
| --- | --- | --- | --- |
| 0 | Scaffold + verification harness (invariants as proptests) | harness compiles; properties are real (fail against a wrong impl) | #119 |
| 1 | Single-threaded dense generational `Region<T>` | invariants I1–I5 green + **miri clean** + `forbid(unsafe_code)` compiles | #120 |
| 2 | Compaction semantics, capacity/shrink, cache-locality benches | I6 green; benches show dense-iteration locality win | #121 |
| 3a | `RwLock` concurrent wrapper (baseline, always-shippable) | concurrent stress test clean; documented as the safe default | #122 |
| 3b | Lock-free read tier via epoch reclamation (RCU/CoW) — experimental | **only if loom-green** + TSan clean; else stays behind `experimental` | #123 |
| 4 | `ByteRegion` + `GlobalAlloc` experiment (the tzimtzum) | miri clean; benches vs system & mimalloc with an honest verdict | #124 |
| 5 | Hardening (fuzz, multi-arch CI, unsafe-confinement proof) + publish prep | fuzz clean; multi-arch green; `no_std`+`alloc`; docs build | #125 |
| 6 | Integrate into one resocks5 hot structure behind a feature flag + bench | resocks5 tests pass + before/after bench | #126 |

Dependency order: `0 → 1 → { 2 → { 4, 5 → 6 }, 3a → 3b }`.

## Risk register (stated up front)

- **Phase 4 may never beat `mimalloc`.** It is research-flagged; it exists to
  learn and to honour the design, not to ship. resocks5's global allocator
  stays on `mimalloc` (FFI) regardless.
- **Phase 3b is the time sink.** 3a (`RwLock`) is the always-shippable
  concurrent answer; 3b dives into lock-free only under loom's protection. No
  false confidence — if loom is not satisfied, 3b stays experimental.
- **Phases 0–2 overlap `slotmap`.** Accepted deliberately (craft + verification
  foundation). If priorities change, the core can be swapped for `slotmap` and
  effort refocused on 3b–4.

## Crate metadata

`no_std` + `alloc` (Phase 5; the arena needs only `alloc`); dual
MIT OR Apache-2.0; MSRV 1.88.

## A note on the generation counter

Generations are `u32` and wrap. A handle that outlives `2^32` reuses of *its*
slot could alias. Slot retirement at generation saturation (as `slotmap` does)
is a Phase 1 hardening item folded into the #120 gate.
