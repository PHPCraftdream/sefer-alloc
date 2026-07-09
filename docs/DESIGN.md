# Design

> ⚠️ **Historical design document** — see
> [`docs/ARCHITECTURE.md`](ARCHITECTURE.md) for the current unsafe-seam inventory
> and architecture. Sections below describe the early plan (e.g. a "byte tier
> (Phase 4)" that was superseded, and a "one screenful of `unsafe`" promise that
> predates the current inventory of 10 confined-unsafe `src` modules plus 3
> unsafe companion crates). Kept for provenance, not as the current spec.

## Three organs

| Organ | Responsibility | Safety |
| --- | --- | --- |
| **Cartographer** | All placement / free-list / (later) compaction logic — pure integer arithmetic over indices. Never touches memory. | safe |
| **Membrane** | The typed API: `Handle<T>`, generation checks, lifetimes. *Total* — cannot express UB. | safe |
| **Hand** | The single, audited `unsafe` organ that touches raw memory. | confined |

The deliberate inversion: **all the intelligence lives in the safe
Cartographer** (it is just arithmetic on `u32`s), so the Hand stays mechanical
and tiny. That is what makes verification tractable — you prove a total
membrane and an integer algorithm, not a tangle of pointer math.

**Organ attribution in the single-threaded core:** the Cartographer + Hand are
now **provided by `slotmap`** (an audited dependency with years of production
exposure and fuzzing). Our own code contributes only the typed **Membrane** —
`Region<T>` wraps `slotmap::SlotMap<DefaultKey, T>`, and `Handle<T>` is a
newtype over `slotmap::DefaultKey` + `PhantomData<fn() -> T>` so handles stay
generic-over-`T` (which raw slotmap keys are not). A battle-tested slotmap is
*safer* than fresh hand-rolled code, even though slotmap uses internal
`unsafe`. Our wrapper therefore stays `#![forbid(unsafe_code)]`; our own Hand
organ appears ONLY in the concurrent epoch tier (3b-II) and the byte tier (4).

## Dense generational layout

```text
slots:         [ gen, Occupied{dense} | Vacant{next_free} ]   stable; handle.index → here
dense:         [ T, T, T, ... ]                               compact value storage
dense_to_slot: [ slot, slot, slot, ... ]                      back pointer dense[i] → slot
free_head:     Option<u32>                                    head of the vacant free list
```

This is the layout **`slotmap` gives us** (and what we would otherwise have
hand-built in Phases 0–2). We adopt it rather than re-implement it. The
machinery — the `slots` array, the free list, the cross pointers, generation
bump on remove, version-saturation retirement — is `slotmap`'s; our wrapper
contributes the typed `Handle<T>` boundary on top.

- **insert** — push the value onto `dense`; claim a slot (reuse a vacant one or
  grow `slots`); wire the cross pointers. `O(1)`.
- **get** — bounds-checked slot lookup, generation compare, then index into
  `dense`. `O(1)`, no `unsafe`.
- **remove** — bump the slot's generation, thread it onto the free list,
  `swap_remove` from `dense`, and repair the back pointer of the element the
  swap moved. `O(1)`.

Because values live in a `dense` `Vec<T>`, the store is **always compact**:
iteration is cache-friendly and there is no fragmentation to defragment — the
live values are contiguous in memory, so a stride through `dense` hits cache
lines maximally. This is the log-structured/compacted ideal, achieved by
construction rather than by a background pass, and now provided by `slotmap`.

## The descent of `unsafe` (where the Hand appears)

- **Typed single-threaded core (today):** **our wrapper is zero `unsafe`**
  (`#![forbid(unsafe_code)]`). Honest caveat: the engine's `unsafe` now lives
  in the audited `slotmap` dependency — `slotmap` uses internal `unsafe` to
  manage the dense generational layout, and we rely on its maturity rather than
  re-rolling it. The "one screenful of `unsafe`" structural promise applies to
  **OUR repo**: `#![forbid(unsafe_code)]` everywhere except the one documented
  `hand.rs` (which does not exist in the single-threaded core).
- **Concurrent tier (Phase 3b) — TWO stages:**
  - **3b-I** — lock-free reads via `arc-swap` RCU with page-granularity
    copy-on-write (the Btrfs-CoW principle): **zero `unsafe` of our own**.
    Readers load an immutable snapshot and look up lock-free; rare writers
    serialise, CoW only the touched page, and publish via `arc.store`;
    reclamation is plain `Arc` refcounting.
  - **3b-II** — the heavier `crossbeam-epoch` + per-slot-atomics design: the
    single confined `unsafe` Hand module (loom-gated + ThreadSanitizer +
    aarch64), taken ONLY if 3b-I's write cost / reader-pinning proves
    unacceptable. Every block carries a `// SAFETY:` proof and the core is
    loom-model-checked.
- **Byte / global-allocator mode (Phase 4):** raw byte ranges, and a single
  irreducible `*mut u8` handed to `std`. `GlobalAlloc`'s contract demands an
  address, so this is the one aperture a handle cannot replace — kept minimal,
  single, and documented.

## Verification-first

The build leans on tools that fit each tier, not on unit tests alone:

- **proptest** — differential testing against a reference model (every tier).
- **miri** — undefined-behaviour detection in any `unsafe` (CI gate).
- **loom** — bounded model-checking of the lock-free tier (Phase 3b gate).
- **cargo-fuzz** — random op sequences over CPU-hours (Phase 5).
- **multi-arch CI (x86_64 + aarch64)** — weak-memory bugs that x86 hides.

The structural promise: `#![forbid(unsafe_code)]` everywhere except one
documented module, so "the `unsafe` is one screenful" is checked by the
compiler, not asserted in prose.
