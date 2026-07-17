# aligned-vmem

[![Crates.io](https://img.shields.io/crates/v/aligned-vmem.svg)](https://crates.io/crates/aligned-vmem)
[![Documentation](https://docs.rs/aligned-vmem/badge.svg)](https://docs.rs/aligned-vmem)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Cross-platform **aligned anonymous virtual memory** — reserve a span whose base
is aligned to an arbitrary power of two, commit/decommit its pages, release it.
Directly through the OS, no file-mapping machinery, **zero dependencies, 100 %
Rust** (no C / C++ libraries pulled in — the OS syscalls are declared locally
through `extern "system"` / `extern "C"`, the same way `std` itself links
`kernel32` / `libc`), miri-friendly.

```toml
[dependencies]
aligned-vmem = "0.2"
```

```rust
use aligned_vmem::{reserve_aligned, release};

// Reserve 4 MiB aligned to 4 MiB — e.g. one allocator segment.
let span = 4 * 1024 * 1024;
let r = reserve_aligned(span, span).expect("OOM");
let base = r.as_ptr();
assert_eq!(base as usize % span, 0);

// SAFETY: base is valid for r.len() bytes, owned exclusively.
unsafe { base.write(0xAB); assert_eq!(base.read(), 0xAB); }

// RAII release on drop — or take the parts for self-hosted manual release:
let (raw, raw_len, raw_align) = r.into_parts();
unsafe { release(raw, raw_len, raw_align) };
```

## What it does

| API | Purpose |
|---|---|
| `reserve_aligned(size, align) -> Option<Reservation>` | Reserve `size` bytes whose base is `align`-aligned (over-reserve + trim). |
| `Reservation::as_ptr / len / reservation_ptr / reservation_len` | The usable span and the underlying OS reservation. |
| `Reservation::into_parts() -> (ptr, len, align)` | Take the raw reservation, suppress `Drop`, for self-hosted release. |
| `release(ptr, len, align)` (unsafe) | Release a reservation taken via `into_parts`, exactly once. |
| `decommit(base, start, end)` / `recommit(base, start, end)` (unsafe) | Return page-granular physical backing to the OS / re-commit it. |
| `decommit_lazy(base, start, end)` (unsafe) | Cheaper lazy reclaim — Linux `MADV_FREE`, macOS `MADV_FREE_REUSABLE`, Windows falls back to `decommit`. |
| `page_size() -> usize` | Real OS page size, queried once (`sysconf`/`GetSystemInfo`) — 16 KiB on Apple Silicon, not the 4 KiB `PAGE` minimum. |
| `PAGE` | Minimum decommit/recommit granularity constant (4 KiB). |
| `leak_zeroed_pages(size) -> Option<NonNull<u8>>` | Reserve zeroed, process-lifetime-leaked pages (for pre-main / `GlobalAlloc` bookkeeping). |
| `try_reserve_aligned` / `try_recommit` / `try_commit_range` … `-> Result<_, VmemError>` | Fallible forms carrying the OS `errno`/`GetLastError` cause. |

Every fallible entry point has an infallible `Option`/`bool` counterpart that
discards the cause. Optional features: `lazy-commit` (incremental commit:
`reserve_aligned_lazy` + `commit_range`; formerly `alloc-lazy-commit`, still
accepted as an alias), `huge-pages` (`reserve_aligned_huge` — `MAP_HUGETLB` /
`MEM_LARGE_PAGES`, best-effort with fallback), and `mock` (recording call log +
`fail_next_reserve` / `fail_next_commit` fault injection for deterministic
OOM-path tests on any target).

Backends: `mmap`/`munmap`/`madvise` on Unix,
`VirtualAlloc`/`VirtualFree(MEM_DECOMMIT/MEM_RELEASE)` on Windows, `std::alloc`
fallback under miri (so consumers stay miri-testable).

## Why not `region` / `memmap2` / `mmap-rs`?

Those are excellent for **file mappings** and **page-protection changes**.
`aligned-vmem` does one different, narrow thing: hand you an **anonymous span
aligned to a power of two you choose** plus page-granular decommit/recommit.
That is exactly what an **allocator / arena / slab** needs ("give me a 4
MiB-aligned 4 MiB span, let me hand pages back to the OS, keep the address
reservation"), and what the file-mapping crates don't directly offer.

## Alignment contract

- `align` must be a power of two `>=` `PAGE` (4 KiB).
- `size` must be a non-zero multiple of `PAGE`.
- `decommit`/`recommit` offsets must be multiples of `PAGE`.

Violations return `None` / are no-ops — never a panic, so this is safe to call
from inside a `GlobalAlloc::alloc` body.

## Provenance & safety

Every `unsafe` block carries a `// SAFETY:` proof. The crate is the OS aperture
extracted from [`sefer-alloc`](https://crates.io/crates/sefer-alloc); it is
deliberately the one place where the raw OS calls live, so consumers can stay
`#![forbid(unsafe_code)]` above it. The returned pointers preserve provenance
(no exposed-address `as usize` round-trips in the public API).

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option.
