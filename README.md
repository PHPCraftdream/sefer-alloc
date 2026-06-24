# sefer-alloc

> A safe, handle-addressed region store — generational handles instead of
> pointers, a dense cache-friendly core with **zero `unsafe`**, and a
> verification-first build.

`sefer-alloc` hands out small, copyable [`Handle`] values instead of raw
pointers. The data lives in a dense backing store the region is free to move;
a stale handle never dereferences freed memory — it returns `None`, never
undefined behaviour. The single-threaded core is `#![forbid(unsafe_code)]`:
the `Vec` does all the initialization and dropping, so there is no `unsafe` to
audit at all.

```rust
use sefer_alloc::Region;

let mut region = Region::new();
let a = region.insert("alpha");
let b = region.insert("beta");

assert_eq!(region.get(a), Some(&"alpha"));

region.remove(a);
assert_eq!(region.get(a), None);        // stale handle → None, never UB
assert_eq!(region.get(b), Some(&"beta")); // others stay valid
```

## Why

Building self-referential structures in Rust — linked lists, graphs, trees,
slabs — fights the borrow checker because raw pointers dangle. The established
answer is to store references **as indices** into a backing array, and to make
those indices safe against reuse with a **generation** counter. That is what
this crate is: a clean, verified, well-documented vessel for that pattern.

Prior art that does the single-threaded job today: [`slotmap`],
[`thunderdome`], [`generational-arena`]. `sefer-alloc` builds its core from
scratch as a craft and a foundation, then aims past them at the parts that
have **no safe, ready-made answer**: a concurrent (epoch-reclamation) tier and
an experimental byte / global-allocator mode.

## Design in one breath

Three organs:

- **Cartographer** (safe) — all placement and free-list logic; pure integer
  arithmetic over indices, never touches memory.
- **Membrane** (safe) — the typed `Handle` API and generation checks; *total*,
  cannot express UB.
- **Hand** (unsafe) — a single audited organ that touches raw memory, present
  **only** in the lower tiers (concurrent epoch reclamation; byte/allocator
  mode). The core you use today has no Hand.

See [`docs/DESIGN.md`](docs/DESIGN.md) and [`docs/INVARIANTS.md`](docs/INVARIANTS.md).

## Scope (honest)

This is an **application-level** store, not a drop-in global allocator.
`GlobalAlloc`'s contract is "give me a raw `*mut u8`", which is the one place a
handle cannot survive — so a process-wide allocator built on these principles
keeps a single, irreducible raw aperture at the very bottom. That descent is a
later, **research-flagged** phase (see [`docs/PLAN.md`](docs/PLAN.md)); it may
never beat `mimalloc`, and it exists to learn, not to replace it.

## Status

Early. Phase 0–1: the single-threaded `Region<T>` is implemented and tested
(unit tests + a proptest differential harness). The roadmap and its
verification gates (miri, loom, fuzz, multi-arch) live in
[`docs/PLAN.md`](docs/PLAN.md).

## License

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE),
at your option.

[`Handle`]: https://docs.rs/sefer-alloc
[`slotmap`]: https://crates.io/crates/slotmap
[`thunderdome`]: https://crates.io/crates/thunderdome
[`generational-arena`]: https://crates.io/crates/generational-arena
