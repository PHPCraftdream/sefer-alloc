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
| `reserve_aligned(size, align) -> Option<Reservation>` | Reserve `size` bytes whose base is `align`-aligned (exact-size mmap fast path on Unix, over-reserve on Windows). |
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
`MEM_LARGE_PAGES`, best-effort with fallback — **on Linux, `size` and `align`
must both additionally be multiples of the huge-page size (2 MiB), or the
request is rejected up front**; **on Windows the `MEM_LARGE_PAGES` request is
not currently functional and always falls back to ordinary pages** — see the
function's own rustdoc for why; use `Reservation::is_huge` to detect
whether a reservation actually got large/huge pages on either platform),
`mock`
(recording call log +
`fail_next_reserve` / `fail_next_commit` fault injection for deterministic
OOM-path tests on any target — **replaces** the commit/decommit/recommit
backend with a stub — **⚠ Cargo feature-unification hazard: enable only in a
leaf test/dev target, never in a library's own `[dependencies]` or a shared
`[dev-dependencies]` entry — see `mock`'s own module doc and its `Cargo.toml`
feature comment for the full reasoning, task #715**), and `fault-injection`
(`fault_injection::arm_fail_next` /
`arm_fail_at` — an armed hook on the REAL `try_commit_range` syscall path,
DISTINCT from `mock`: it changes nothing about which backend runs, it only
forces a specific real commit call to report failure, for a consumer that
needs the genuine OS backend under test).

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
- `decommit`/`recommit`/`commit_range` offsets must be multiples of `PAGE`.
- On Linux with `huge-pages` enabled, `reserve_aligned_huge`/
  `try_reserve_aligned_huge` additionally require `size` and `align` to both
  be multiples of the huge-page size (2 MiB) — see that function's own
  rustdoc.

Most violations return `None` / `false` / `Err(_)` — never a panic, so this
is safe to call from inside a `GlobalAlloc::alloc` body: `reserve_aligned`
and its siblings, and `decommit`/`decommit_lazy` (which silently no-op on a
violated range, an intentional asymmetry with `recommit`/`commit_range`
below — since `decommit`'s `()` return has no write-permitting sentinel to
misuse, silently skipping is safe, whereas `recommit`/`commit_range`'s
boolean/`Result` return previously clamped a contract violation to the same
value a genuine success reports, which crashed an in-repo consumer — see
`recommit`'s own rustdoc). Two exceptions to "never panics": `recommit` and
`commit_range` (and their fallible `try_*` forms) now **reject**, rather
than silently accept, a violated offset range (`start > end` or misaligned)
— they still don't panic, but callers relying on the old silent-no-op shape
should check the return value; and `Reservation::from_raw_parts` (an
`unsafe fn` for adopting a foreign OS reservation, not part of the ordinary
reservation flow) panics immediately on a contract-violating `align`/
`reservation_len` pair.

## Provenance & safety

Every `unsafe` block carries a `// SAFETY:` proof. The crate is the OS aperture
extracted from [`sefer-alloc`](https://crates.io/crates/sefer-alloc); it is
deliberately the one place where the raw OS calls live, so consumers can stay
`#![forbid(unsafe_code)]` above it. The returned pointers preserve provenance
(no exposed-address `as usize` round-trips in the public API).

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option.
