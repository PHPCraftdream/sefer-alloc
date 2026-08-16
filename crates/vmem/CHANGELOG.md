# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.2.0 - 2026-08-16

### Added

- Safe `Reservation` methods for page-level memory management: `decommit`, `decommit_lazy`, `recommit`, `try_recommit`, `commit_range`, `try_commit_range`
- `MIN_PAGE` constant as an alias for `PAGE` with clearer semantics
- `page_size()` function to query the actual OS page size at runtime
- `ReservationParts` typed wrapper and `into_reservation_parts()` method
- `Reservation::into_reservation_parts()` typed form for manual release
- `ReservationFullParts` + `Reservation::into_full_parts()` — lossless six-field
  round-trip that preserves `base`, usable `len` and `granted_huge`, which
  `ReservationParts` discards (R4-11)
- `Reservation::decommit_reclaims_and_zeroes()` — `const fn` capability query
  reporting whether the current platform's ordinary native backend actually delivers
  decommit's reclaim + zero-fill semantics (`false` on Darwin and the four BSDs,
  where `MADV_DONTNEED` is advisory-only, and under miri where the backend is a no-op).
  Makes a guarantee that was previously documented in prose only something callers
  can branch on (R4-3, R5-1)
- `Reservation::can_decommit_reclaim_and_zero()` — instance-level query that combines
  the platform guarantee with the reservation's huge-page status, returning `false`
  for huge-page reservations where decommit silently fails (R5-1)
- `VmemError::last_os_error()` for OS error capture with preserved errno
- `bench-internals` feature with diagnostic counters for path activation:
  - `unix_exact_reserve_attempts()` / `unix_exact_reserve_hits()`
  - `windows_reserve_commit_calls()` / `windows_reserve_commit_single_calls()` / `windows_reserve_commit_two_call_pairs()`
  - `windows_large_page_retry_failures()` / `windows_large_page_alignment_failures()` — separate counters for "both initial and retry failed" vs "succeeded but misaligned" large-page failure modes (R4-5/R5-4)
  - `unix_madvise_attempts()` / `unix_madvise_successes()`
  - `windows_virtualfree_decommit_attempts()` / `windows_virtualfree_decommit_failures()`
  - `windows_virtualfree_release_failures()`
  - `unix_munmap_failures()`
  - `huge_decommit_attempts()` for tracking decommit calls on huge-page reservations
  - `reset_bench_internals_counters()`
  - `validate_page_size()` for testing page size validation logic
- Mock backend converted from Cargo feature to build-time `--cfg aligned_vmem_mock` flag (no Cargo feature unification risk)
- `fault-injection` feature for deterministic OOM testing on the real commit path
- `try_reserve_aligned()` / `try_reserve_aligned_huge()` / `try_reserve_aligned_lazy()` fallible forms returning `Result<_, VmemError>`
- `reserve_aligned_huge()` for requesting OS large pages (Linux `MAP_HUGETLB`, Windows `MEM_LARGE_PAGES`)

### Changed

- `page_size()` granularity is now used for decommit/recommit validation instead of compile-time `PAGE` constant
- **lazy-commit contract tightened:** `reserve_aligned_lazy` now requires both `size` and `initial_commit` to be multiples of the runtime `page_size()` (not just `PAGE`). This prevents unwritable tails on systems where `page_size() > PAGE` (e.g., 64 KiB Windows configurations or 16 KiB macOS), where `commit_range` only accepts page_size()-aligned offsets. Mainstream Windows (page_size() == 4096) is unaffected. (R6-2)
- All OS error paths now capture `errno`/`GetLastError` immediately before cleanup FFI
- Unix 64-bit fast path disabled: syscall economy (one `mmap` call at the cost of extra virtual address space held per reservation). Exception on Linux: when `align == LINUX_HUGE_PAGE_SIZE` with huge pages requested, an exact-size `MAP_HUGETLB` fast path avoids the over-reserve (kernel guarantees huge-page-aligned base).
- `reserve_aligned_huge()` semantics fixed: reports actual huge-page grant only on platforms where it's observable
- `granted_huge` tracking added for Linux huge pages; non-Linux Unix correctly reports `is_huge() == false`
- **BREAKING**: `Reservation::from_raw_parts` signature changed to require new `granted_huge: bool` parameter
- `decommit`/`recommit` contract narrowed on Darwin/BSDs: explicitly best-effort hint with no zero-fill guarantee (Linux/Windows guarantees unchanged)
- Windows single-call fast path for `align <= WIN_ALLOCATION_GRANULARITY` (typically 64 KiB) on full-span commit; when requesting large pages (`MEM_LARGE_PAGES`), the threshold widens to `GetLargePageMinimum()` (typically 2 MiB).

### Fixed

- **Lazy reservation documentation corrected:** `Reservation` type and `as_ptr()` docs now explicitly state that lazy reservations on Windows only commit the `initial_commit` prefix; the tail must be committed via `commit_range` before it's writable. README platform caveats updated with this information. (R6-1 variant 1)
- **`from_raw_parts` Windows commit-state documentation rewritten:** No longer inaccurately requires all Windows reservations to be created with `MEM_RESERVE | MEM_COMMIT`. Instead documents that partial-commit (lazy) reservations are valid, and explains the `granted_huge` compatibility requirements. (R6-1 variant 2)
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
- 32-bit Linux/Android no longer issues the same `MAP_HUGETLB` mmap twice for a
  single 2 MiB-aligned huge reservation: the generic 32-bit exact-size fast path
  is now skipped when the huge-page exact-size path above already attempted it.
  Saves a syscall and a second draw against a scarce hugetlb pool, and makes
  `UNIX_EXACT_RESERVE_ATTEMPTS` count one attempt per logical reserve on every
  platform (R4-2/R3-1)
- MIPS targets now fail to compile with an explanatory `compile_error!` instead
  of building successfully and then failing every `reserve_aligned` call at
  runtime with an undiagnosed `EBADF` (MIPS `MAP_ANON`/`MAP_HUGETLB` values
  differ from the `asm-generic` constants this crate hardcodes) (R4-1)
- `mock::drain()` no longer holds the `RefCell` borrow across the returned
  `Vec`'s allocation, which could reenter `record()` and panic with
  `BorrowMutError`; it now `mem::take`s the log under a short borrow (R4-9)
- `fault_injection`'s one-shot self-disarm can no longer cancel a concurrent
  `arm_fail_at`: the two-atomic target/counter protocol is replaced by a single
  mutex-guarded state, closing the last of the three races this module
  documented (R4-8)
- Three `// SAFETY:` comments on the Windows decommit path claimed the caller
  must pass a COMMITTED range — a precondition the crate deliberately violates
  in a CI-covered test; all now state the real `MEM_RESERVE`d-region contract
- `from_raw_parts`'s contract documentation corrected: it takes six arguments,
  not five, and now requires runtime `page_size()` alignment rather than only
  the compile-time `PAGE` lower bound (R4-10, R4-6)
- `from_raw_parts` documentation fixed to accurately reflect what the constructor's
  `assert!` checks versus what remains the caller's responsibility. The documentation
  previously required `len` and `reservation_len` to be multiples of both `PAGE`
  and `page_size()`, and claimed "both are asserted at construction", but the
  actual `assert!` only checks against `PAGE`. The fix clarifies: (a) logical
  lengths (`len`, `reservation_len`) require only `PAGE` multiple (checked by
  the assert), (b) addresses and operations (`base`, `reservation`, `decommit`/
  `decommit_lazy` arguments) require `page_size()` alignment (NOT checked by the
  assert, remains caller responsibility), and (c) `reservation_len` may under-report
  the actual OS mapping size on hosts where `page_size()` > `PAGE` (harmless for
  correctness, documented now) (R5-2)
- `into_full_parts` documentation fixed: replaced "persists metadata across restarts"
  (misleading — raw pointers don't survive process restarts) with "hands off
  reservations between components within the same process", and added explicit
  warning that dropping or forgetting `ReservationFullParts` does NOT release the
  underlying OS reservation (R5-5)

### Removed

- Deprecated `Reservation::is_empty()` method (use `len() == 0` instead)
- `mock` Cargo feature (replaced by `--cfg aligned_vmem_mock` build flag)
