# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.2.0 - 2026-08-16

### Added

- Safe `Reservation` methods for page-level memory management: `decommit`, `decommit_lazy`, `recommit`, `try_recommit`, `commit_range`, `try_commit_range`
- `MIN_PAGE` constant as an alias for `PAGE` with clearer semantics
- `page_size()` function to query the actual OS page size at runtime
- `ReservationParts` typed wrapper and `into_reservation_parts()` method
- `Reservation::into_reservation_parts()` typed form for manual release
- `VmemError::last_os_error()` for OS error capture with preserved errno
- `bench-internals` feature with diagnostic counters for path activation:
  - `unix_exact_reserve_attempts()` / `unix_exact_reserve_hits()`
  - `windows_reserve_commit_calls()` / `windows_reserve_commit_single_calls()` / `windows_reserve_commit_two_call_pairs()`
  - `unix_madvise_attempts()` / `unix_madvise_successes()`
  - `reset_bench_internals_counters()`
  - `validate_page_size()` for testing page size validation logic
- Mock backend converted from Cargo feature to build-time `--cfg aligned_vmem_mock` flag (no Cargo feature unification risk)
- `fault-injection` feature for deterministic OOM testing on the real commit path
- `try_reserve_aligned()` / `try_reserve_aligned_huge()` / `try_reserve_aligned_lazy()` fallible forms returning `Result<_, VmemError>`
- `reserve_aligned_huge()` for requesting OS large pages (Linux `MAP_HUGETLB`, Windows `MEM_LARGE_PAGES`)

### Changed

- `page_size()` granularity is now used for decommit/recommit validation instead of compile-time `PAGE` constant
- All OS error paths now capture `errno`/`GetLastError` immediately before cleanup FFI
- Unix 64-bit fast path disabled: syscall economy (one `mmap` call at the cost of extra virtual address space held per reservation). Exception on Linux: when `align == LINUX_HUGE_PAGE_SIZE` with huge pages requested, an exact-size `MAP_HUGETLB` fast path avoids the over-reserve (kernel guarantees huge-page-aligned base).
- `reserve_aligned_huge()` semantics fixed: reports actual huge-page grant only on platforms where it's observable
- `granted_huge` tracking added for Linux huge pages; non-Linux Unix correctly reports `is_huge() == false`
- **BREAKING**: `Reservation::from_raw_parts` signature changed to require new `granted_huge: bool` parameter
- `decommit`/`recommit` contract narrowed on Darwin/BSDs: explicitly best-effort hint with no zero-fill guarantee (Linux/Windows guarantees unchanged)
- Windows single-call fast path for `align <= WIN_ALLOCATION_GRANULARITY` (typically 64 KiB) on full-span commit; when requesting large pages (`MEM_LARGE_PAGES`), the threshold widens to `GetLargePageMinimum()` (typically 2 MiB).

### Fixed

- Android build support added with correct `_SC_PAGESIZE` constant wiring
- 32-bit Linux glibc/musl FFI fix: `off_t` type correctly declared as 64-bit on all musl targets (was mismatched for 32-bit musl)
- BSD `decommit_lazy` no-ops fixed: FreeBSD/DragonFly/NetBSD/OpenBSD now dispatch to their own real `MADV_FREE` constant values instead of an undefined/wrong one
- macOS CI failures caused by `PAGE` vs `page_size()` validation mismatch
- `release()` panic hardened with informative multi-clause assert under miri
- Fixed data race in `SERIAL` mutex for bench-internals tests
- Wire-thread drop split documented in mock module
- FFI struct layout for `SystemInfo` matches real `SYSTEM_INFO` (union head flattened)
- Over-reserve documentation corrected to reflect "no trim" behavior
- Documentation gaps fixed: huge-pages semantics, alignment contract, API completeness

### Removed

- Deprecated `Reservation::is_empty()` method (use `len() == 0` instead)
- `mock` Cargo feature (replaced by `--cfg aligned_vmem_mock` build flag)

[0.1.0]: https://github.com/PHPCraftdream/sefer-alloc/releases/tag/v0.1.0-aligned-vmem