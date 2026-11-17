# sefer-platform

**The platform seam of [`sefer-alloc`](https://github.com/PHPCraftdream/sefer-alloc):
the sanctioned process terminator, the Windows process-exit probe, and the
SEGMENT-aligned OS aperture on top of [`aligned-vmem`](https://crates.io/crates/aligned-vmem).**

## Status: internal crate, no stability guarantees

This crate exists so that `sefer-alloc` can keep its OS-facing code in one
small, separately checked unit (plan step P1.4, owner decision G0 of
2026-11-15). It is an **internal** crate: its public items are `pub` only so
that the root crate can re-export them, and **no API stability, SemVer
compatibility or support is promised** for any item. Depend on `sefer-alloc`,
not on this crate.

## What it contains

| Module | Role |
|--------|------|
| `fatal` | `fatal(msg)` — writes a best-effort static diagnostic and terminates without unwinding. The one sanctioned terminator. |
| `process_exit` | `process_exit_in_progress()` — the Windows loader-shutdown FFI probe (`kernel32`); constant `false` on other targets. |
| `record_exit_lock_giveup` | `record_exit_lock_giveup()` — the `#[cold]`, `#[doc(hidden)]` counter increment for the sanctioned shard-lock give-up during process exit. |
| `dbg_exit_lock_giveups` | `dbg_exit_lock_giveups()` — reads the give-up counter (`bench-internals` only; `#[doc(hidden)]`). |
| `os` | The SEGMENT-aligned OS aperture: `Segment`, `SEGMENT` / `PAGE` / `MAX_REALISTIC_PAGE_SIZE`, the `SEGMENTS_*` reservation counters, release / commit / decommit / recommit wrappers and the depot-overflow storage. Built on `aligned-vmem`. |

The code was moved verbatim from `sefer-alloc`'s `alloc_core::platform`
(`fatal`, the "backup" half of `os`) and `global::process_exit`. The
directory-sidecar functions that depend on allocator-internal types stay in
the root crate.

## Features

The feature names mirror the root `sefer-alloc` features one-for-one; the root
forwards each of them. Gates only select which moved items compile.

```text
alloc-decommit            empty toggle
numa-aware                empty toggle
exact-span-large          empty toggle
large-reserved-capacity   enables aligned-vmem/lazy-commit
primordial-lazy-commit    enables aligned-vmem/lazy-commit
small-segment-lazy-commit enables aligned-vmem/lazy-commit
bench-internals           enables aligned-vmem/bench-internals, aligned-vmem/lazy-commit
internals                 empty toggle
alloc-global              empty toggle
fastbin                   empty toggle
```

There are no default features.

## Where `unsafe` lives

- `src/os.rs` — FFI-adjacent calls into `aligned-vmem` (release,
  commit, decommit, recommit).
- `src/process_exit.rs` — `kernel32` FFI (`GetModuleHandleW`, `GetProcAddress`)
  and the call through the resolved function pointer.
- `src/fatal.rs` — no `unsafe`.
- `src/exit_lock_giveups.rs` / `src/record_exit_lock_giveup.rs` /
  `src/dbg_exit_lock_giveups.rs` — no `unsafe` (relaxed RMWs on the
  give-up counter).

## Non-goals

- Not a general-purpose virtual-memory or NUMA library: this crate does not
  extend `aligned-vmem` or `numa-shim`, and must not grow their surface.
- No NUMA code lives here (`numa.rs` stays in the root crate).
- No stable public API (see above).

## License

Licensed under either of MIT or Apache-2.0, at your option.
