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

```text
use aligned_vmem::{reserve_aligned, release};

// Reserve 4 MiB aligned to 4 MiB — e.g. one allocator segment.
let span = 4 * 1024 * 1024;
let r = reserve_aligned(span, span).expect("OOM");
let base = r.as_ptr();
assert_eq!(base.addr() % span, 0);

// SAFETY: base is valid for r.len() bytes (except decommitted ranges), owned exclusively.
unsafe { base.write(0xAB); assert_eq!(base.read(), 0xAB); }

// RAII release on drop — or take the parts for self-hosted manual release:
let (raw, raw_len, raw_align) = r.into_parts();
unsafe { release(raw, raw_len, raw_align) };
```

Runnable form: `tests/readme_example.rs`.

**Reservation ownership:** `Reservation` is a single-owner handle over its whole span with no built-in sub-span/re-derivation API. Sub-allocation on top of this crate is expected via raw pointer arithmetic within the reservation's bounds, done by the consumer.

## What it does

| API | Purpose |
|---|---|
| `reserve_aligned(size, align) -> Option<Reservation>` | Reserve `size` bytes whose base is `align`-aligned. On 32-bit Unix, first tries an exact-size mmap fast path; on a miss, or on 64-bit Unix (where the fast path is compiled out — see P-1 below), over-reserves `size + align` and keeps the full mapping. On Windows, over-reserves `size + align` and keeps the full mapping when `align > WIN_ALLOCATION_GRANULARITY` (typically 64 KiB); for `align <= WIN_ALLOCATION_GRANULARITY` uses a single-call fast path with no over-reserve (threshold widens to `GetLargePageMinimum()` when requesting large pages). On 32-bit Unix, a fast-path miss holds `size + align` bytes of virtual address space for the reservation's lifetime (measured hit rate: 34.4% at 64 KiB align, 46.7% at 1 MiB, 56.7% at 4 MiB — commit `35d51e6`, task #849; measured on WSL2/Linux, x86_64; 30-run aggregate; scope: 32-bit only — the hit rate is kernel- and ASLR-dependent and is not expected to transfer to other Unix platforms). On 64-bit Unix, every reservation usually over-reserves in one `mmap` call — the exact-size fast path never runs, so these hit-rate numbers do not apply there (task #944, finding P-1: `try_reserve_aligned_exact` is gated `target_pointer_width = "32"`). Exception on Linux: when `align == LINUX_HUGE_PAGE_SIZE` with huge pages requested, an exact-size `MAP_HUGETLB` fast path avoids the over-reserve (kernel guarantees huge-page-aligned base). |
| `Reservation::as_ptr / len / reservation_ptr / reservation_len` | The usable span and the requested reservation length (not necessarily the actual OS reservation size). |
| `Reservation::into_parts() -> (*mut u8, usize, usize)` | Take the raw reservation, suppress `Drop`, for self-hosted release (legacy tuple form). |
| `Reservation::into_reservation_parts() -> ReservationParts` | Take the raw reservation, suppress `Drop`, for self-hosted release (typed form). |
| `release(ptr, len, align)` (unsafe) | Release a reservation taken via `into_parts`, exactly once (legacy tuple form). |
| `release_parts(ReservationParts)` (unsafe) | Release a reservation taken via `into_reservation_parts`, exactly once (typed form). |
| `Reservation::is_huge() -> bool` | Detect whether a reservation actually got large/huge pages on either platform. |
| `impl From<VmemError> for std::io::Error` | Convert `VmemError` to `std::io::Error` for error-propagation convenience. |
| `decommit(base, start, end)` / `recommit(base, start, end)` (unsafe) | Hint the OS to return page-granular physical backing (guaranteed on Linux/Windows, best-effort on Darwin/BSDs with no zero-fill guarantee) / re-commit it. |
| `decommit_lazy(base, start, end)` (unsafe) | Cheaper lazy reclaim — Linux `MADV_FREE`, macOS/iOS `MADV_FREE_REUSABLE`, BSD (FreeBSD/DragonFly/NetBSD/OpenBSD) `MADV_FREE`, Windows falls back to `decommit` (eager `MEM_DECOMMIT`: a write before `recommit` is a hard crash there, not a re-fault). |
| `page_size() -> usize` | Real OS page size, queried once (`sysconf`/`GetSystemInfo`) — 16 KiB on Apple Silicon, not the 4 KiB `PAGE` minimum. |
| `PAGE` | Minimum decommit granularity constant (4 KiB) — superseded by `page_size()` on hosts with larger pages (see `MIN_PAGE` for the underlying constant). |
| `MIN_PAGE` | Underlying minimum page size constant (4 KiB). |
| `leak_zeroed_pages(size) -> Option<NonNull<u8>>` | Reserve zeroed, process-lifetime-leaked pages (for pre-main / `GlobalAlloc` bookkeeping). |
| `try_reserve_aligned` / `try_recommit` / `try_commit_range` … `-> Result<_, VmemError>` | Fallible forms carrying the OS `errno`/`GetLastError` cause. |

Every fallible entry point has an infallible `Option`/`bool` counterpart that
discards the cause. Optional features: `lazy-commit` (incremental commit:
`reserve_aligned_lazy` + `commit_range`; the `alloc-lazy-commit` name is kept
as a compat alias for one release and will be removed in 0.3.0/1.0.0 — migrate
to `lazy-commit` now), `huge-pages` (`reserve_aligned_huge` — `MAP_HUGETLB` /
`MEM_LARGE_PAGES`, best-effort with fallback — **on Linux, `size` and `align`
must both additionally be multiples of the huge-page size (2 MiB), or the
request is rejected up front**; **on Windows, large pages (`MEM_LARGE_PAGES`) are only ever requested and possibly granted via the single-call fast path (`align <= WIN_ALLOCATION_GRANULARITY`, typically 64 KiB); the two-call path used for `align >` that threshold never requests large pages, so `is_huge()` is always `false` for a reservation that takes it**; otherwise the request falls back
to ordinary pages — see the function's own rustdoc for the full technical
explanation; use `Reservation::is_huge` to detect whether a reservation actually
got large/huge pages on either platform), and `fault-injection` (`fault_injection::arm_fail_next` /
`arm_fail_at` — an armed hook on the REAL `try_commit_range` syscall path,
DISTINCT from the mock backend: it changes nothing about which backend runs, it only
forces a specific real commit call to report failure, for a consumer that
needs the genuine OS backend under test). The mock backend (recording call log +
`fail_next_reserve` / `fail_next_commit` fault injection for deterministic
OOM-path tests on any target — **replaces** the commit/decommit/recommit
backend with a stub) is enabled via the `aligned_vmem_mock` cfg flag
(`RUSTFLAGS="--cfg aligned_vmem_mock" cargo test ...`), following the same
pattern as this repo's `cfg(loom)`/`cfg(kani)` flags. A `--cfg` flag cannot be
silently unified into a build by another crate downstream — exactly why it
was converted (task #962).

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
- `decommit`/`decommit_lazy` offsets must be multiples of the runtime page size
  (`page_size()`).
- `recommit`/`commit_range` offsets must be multiples of the runtime page size
  (`page_size()`).

Note: `decommit`/`decommit_lazy` and `recommit`/`commit_range` validate the
same granularity, but respond differently to a violated range — this
asymmetry is intentional. `decommit`/`decommit_lazy` have an infallible
`()` return with no write-permitting sentinel to misuse, so silently skipping
on a violated range is safe; `recommit`/`commit_range`'s boolean/`Result`
return means a silent no-op could hide a real OOM, so they reject violations
instead.
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
`recommit`'s own rustdoc).

**Behavior change:** `recommit` and `commit_range` (and their fallible
`try_*` forms) now **reject**, rather than silently accept, a violated offset
range (`start > end` or misaligned) — they still don't panic, but callers
relying on the old silent-no-op shape should check the return value.

**Two panic exceptions:** `Reservation::from_raw_parts` (an `unsafe fn` for
adopting a foreign OS reservation, not part of the ordinary reservation
flow) panics immediately on a contract-violating `align`/`reservation_len`
pair; and `release` panics on a contract-violating `(reservation_len, align)`
pair (a null pointer remains a documented no-op) — see `release`'s own rustdoc
`# Panics` section for the full detail.

## Platform caveats

`decommit`'s single-line table entry above ("Return page-granular physical
backing to the OS / re-commit it") hides four platform divergences worth
knowing before you rely on it — see `decommit`'s own rustdoc for the full
technical explanation of each:

- **Windows: a write before recommit is a hard crash, not a soft re-fault.**
  `MEM_DECOMMIT` genuinely unmaps the pages, so writing into
  `[base+start, base+end)` before calling `recommit` raises a
  `STATUS_ACCESS_VIOLATION` on Windows. On Linux, `MADV_DONTNEED` keeps the
  mapping resident and transparently re-faults a fresh zero page on the next
  write, so code that is safe on Linux can crash on Windows. This exact
  divergence has already crashed an in-repo consumer — see
  <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md>
  item 6 for the incident record.
- **Huge pages: decommit does nothing, on either OS.** When a reservation
  came from `reserve_aligned_huge` (`Reservation::is_huge() == true`),
  `decommit` does not work on it on either Windows or Linux: `VirtualFree`
  fails on large-page regions on Windows, and `madvise` on a `MAP_HUGETLB`
  mapping only accepts huge-page-granular offsets on Linux, so a
  `page_size()`-granular call gets `EINVAL` and does nothing. The effect is
  indistinguishable from a silent no-op — RSS does not drop and reads return
  the old data. Use `reserve_aligned` instead if you need working decommit.
- **Darwin (macOS/iOS/tvOS/watchOS): no zero-fill, no RSS return, on ordinary
  reservations too.**
  `MADV_DONTNEED` is advisory-only for anonymous memory on Darwin, so unlike
  Linux it does not reliably unmap the physical pages: a decommit +
  `recommit` round trip on any Darwin target can still observe the old data
  instead of a fresh zero page, even for a non-huge reservation. Confirmed as a real,
  failing-test-level gap by this crate's first real-macOS CI run on
  2026-08-13 (the underlying hazard was already documented elsewhere in this
  repository since Round 9, before this crate was extracted); no fix is
  implemented yet — see
  <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md>
  for the open item.
- **BSD (FreeBSD/DragonFly/NetBSD/OpenBSD): the same advisory-only eager
  `decommit` caveat as Darwin, but lazy `decommit_lazy` genuinely reclaims.**
  `MADV_DONTNEED` is advisory-only for anonymous memory on the four BSDs too
  — like Darwin and unlike Linux, eager `decommit` does not reliably unmap
  the physical pages there, so the same "no zero-fill, no RSS return" gap
  applies. Unlike eager `decommit`, though, `decommit_lazy`'s `MADV_FREE`
  advice on BSD (as on Darwin's `MADV_FREE_REUSABLE`) DOES do something real:
  it drops the physical footprint rather than being a no-op — see
  `decommit`'s own rustdoc for the precise wording this caveat mirrors.
  REASONED-FROM-SPEC only (no BSD CI runner in this crate to verify against
  empirically); not independently confirmed the way the Darwin gap above was
  by a real CI run.

## Supported targets

This crate's platform support falls into two categories:

### CI-verified targets

The following platforms are verified by this project's CI and are tested on every commit:

- **x86_64 Linux** (Ubuntu, glibc)
- **x86_64 Windows** (Windows Server)
- **AArch64 macOS** (Apple Silicon, macos-latest CI)
- **i686 Linux** (compile-only check via `cargo check --target i686-unknown-linux-gnu`)

### Reasoned-from-spec targets

The following targets are supported but have not been empirically verified in CI. Support is based on published specifications and OS documentation:

- **AArch64 Linux** — the crate's FFI constants (`MAP_ANON`, `MADV_*`, `_SC_PAGESIZE`) are architecture-agnostic standard Linux values, not x86_64-specific, so the same 64-bit code path used by x86_64 Linux applies, but this has not been empirically CI-verified. (The project's CI has a cross-compilation matrix for aarch64, but no job runs on an ARM64 runner; AArch64 macOS is CI-verified — see the CI-verified list above.)
- **32-bit musl Linux** (e.g., `i686-unknown-linux-musl`, `armv7-unknown-linux-musleabihf`) — `off_t` is correctly declared as 64-bit (musl uses 64-bit `off_t` on all architectures); no CI runner currently tests these targets.
- **BSD family** (FreeBSD, DragonFly BSD, NetBSD, OpenBSD) — `MAP_ANON`, `_SC_PAGESIZE`, and `MADV_*` constants are derived from each OS's header files; no BSD CI runner exists for this project. `MADV_DONTNEED`'s advisory-only behavior is confirmed by BSD documentation but not empirically verified on BSD hardware.
- **Android (bionic)** — 32-bit targets use a 32-bit `off_t` by default (matching glibc behavior without `_FILE_OFFSET_BITS=64`); this is reasoned from bionic's design documentation, not CI-verified.
- **Apple platforms beyond macOS** (iOS, tvOS, watchOS) — constants and behavior are derived from Apple's unified documentation; no CI runner currently tests these targets.
- **MIPS** — **not supported**. The crate's `MAP_ANON`/`MAP_HUGETLB` constants (0x20 / 0x40000) are correct on all Linux architectures that use `asm-generic/mman-common.h` (x86, x86_64, aarch64, arm, riscv, powerpc) but are **wrong on MIPS** (MIPS uses 0x0800 / 0x80000 per `arch/mips/include/uapi/asm/mman.h`). A build on `mips*-unknown-linux-*` would fail closed at compile time via a `compile_error!` diagnostic explicitly naming this gap; adding support requires adding a `#[cfg(target_arch = "mips*")]` arm with the correct constants.

**Note:** The absence of CI verification for a target means only that this project's own test suite has not executed on that platform. The platform-specific constants (e.g., `MADV_DONTNEED = 4`, Linux's `_SC_PAGESIZE = 30`, macOS's `_SC_PAGESIZE = 29`) are sourced from the respective OS's official documentation and header files. If you use this crate on a reasoned-from-spec target and encounter issues, please file a bug report — the intent is to support the full `cfg(unix)` and `cfg(windows)` families.

## Provenance & safety

Every `unsafe` block carries a `// SAFETY:` proof. The crate is the OS aperture
extracted from [`sefer-alloc`](https://crates.io/crates/sefer-alloc); it is
deliberately the one place where the raw OS calls live, so consumers can stay
`#![forbid(unsafe_code)]` above it. The returned pointers preserve provenance
(no exposed-address `as usize` casts in the public API — the mock backend's
diagnostic-only call recorder stores addresses as `usize` for
comparison/logging, obtained via the non-exposing `.addr()`, and none of
those values is ever cast back into a pointer; this crate's own `tests/`
files, which are not part of the public API, still use `as usize` at a
few sites).

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option.
