# Design

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

## Dense generational layout

```text
slots:         [ gen, Occupied{dense} | Vacant{next_free} ]   stable; handle.index → here
dense:         [ T, T, T, ... ]                               compact value storage
dense_to_slot: [ slot, slot, slot, ... ]                      back pointer dense[i] → slot
free_head:     Option<u32>                                    head of the vacant free list
```

- **insert** — push the value onto `dense`; claim a slot (reuse a vacant one or
  grow `slots`); wire the cross pointers. `O(1)`.
- **get** — bounds-checked slot lookup, generation compare, then index into
  `dense`. `O(1)`, no `unsafe`.
- **remove** — bump the slot's generation, thread it onto the free list,
  `swap_remove` from `dense`, and repair the back pointer of the element the
  swap moved. `O(1)`.

Because values live in a `dense` `Vec<T>`, the store is **always compact**:
iteration is cache-friendly and there is no fragmentation to defragment. This
is the log-structured/compacted ideal, achieved by construction rather than by
a background pass.

## The descent of `unsafe` (where the Hand appears)

- **Typed single-threaded core (today):** zero `unsafe`
  (`#![forbid(unsafe_code)]`). The dense `Vec<T>` owns all init and drop.
- **Concurrent epoch tier (Phase 3b):** the first confined `unsafe` —
  lock-free reads via epoch reclamation (the read-copy-update / shadow-paging
  principle). Every block carries a `// SAFETY:` proof and the core is
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
