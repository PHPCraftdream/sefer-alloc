# aligned-vmem — independent engineering audit (fxx, 2026-08-17)

- **Audited tree:** `crates/aligned-vmem/` (src, tests, benches, examples, Cargo.toml, README, CHANGELOG)
- **Commit:** `8393aa3572a1da2bf1df5c81cef7c3b692ef024f` (`main`) — NOTE: the crate's `src/` is largely **uncommitted working-tree state** at audit time (modified `src/lib.rs`; untracked `src/api/`, `src/os/`, `src/bench_internals/`, `src/reservation.rs`, etc. per `git status`). This audit reads the working tree as-is.
- **Method:** full read of every `src/**/*.rs` file, `Cargo.toml`, `README.md`; tests read in part (see coverage note at end). Read-only audit — no builds or tests were run (user instruction), so nothing here is compile-verified; every claim is from source reading.

Findings are numbered in discovery order. Severity: HIGH / MEDIUM / LOW / INFO.

---

## F1 — `decommit_reclaims_and_zeroes()` ignores the `aligned_vmem_mock` cfg; sibling query `lazy_commit_is_honored()` does not

- **Severity:** MEDIUM
- **Category:** API | docs
- **Location:** `crates/aligned-vmem/src/reservation.rs:304-316` (`Reservation::decommit_reclaims_and_zeroes`), contrast `crates/aligned-vmem/src/lazy_commit_is_honored.rs:35-37`
- **What is wrong:** `decommit_reclaims_and_zeroes()` is documented as "programmatic access to the platform-specific guarantee" and its cfg! exclusion list is `macos/ios/tvos/watchos/freebsd/dragonfly/netbsd/openbsd/miri` — it does **not** exclude `aligned_vmem_mock`. Under the mock cfg, `decommit`/`recommit` never touch the OS (`#[cfg(not(aligned_vmem_mock))]` around `decommit_pages_impl` in `src/api/decommit.rs:107-112`), so on a Linux/Windows host with the mock enabled a decommit reclaims nothing and zero-fills nothing — yet this query returns `true`. The sibling capability query `lazy_commit_is_honored()` was written with exactly this in mind: `cfg!(all(windows, not(miri), not(aligned_vmem_mock)))`. The crate's own test suite proves the mock case is a real divergence: `tests/smoke.rs::decommit_recommit_roundtrip` must skip its zero-fill assertion under `#[cfg(not(any(miri, aligned_vmem_mock, ...)))]` — i.e. the tests already know the guarantee does not hold under mock, but the public query says it does.
- **Why it matters / failure scenario:** a consumer that follows the crate's own advice ("Use `Reservation::decommit_reclaims_and_zeroes()` to programmatically query...", README Platform caveats) and branches on it inside a mock-cfg test run (the documented use of the mock: "deterministically test its OOM-handling on any target") will take the "decommit is guaranteed" branch, then read back stale non-zero data — a confusing false failure, or worse, a false pass of a security-relevant "memory is zeroed" test.
- **Evidence:** `reservation.rs` cfg! list (no `aligned_vmem_mock`); `lazy_commit_is_honored.rs:36` (`not(aligned_vmem_mock)` present); `tests/smoke.rs:311-318` skip list includes `aligned_vmem_mock`.
- **Suggested direction:** add `aligned_vmem_mock` to the cfg! exclusion list (and mirror in the rustdoc's "determined by the target OS triple and whether miri is active" sentence), matching `lazy_commit_is_honored`'s pattern.
- **Confidence:** certain

## F2 — Zero-address `mmap` cleanup path violates the crate's own errno immediate-capture discipline (stale `VmemError` code)

- **Severity:** LOW
- **Category:** correctness | docs
- **Location:** `crates/aligned-vmem/src/os/unix.rs:904-911` (`libc_mmap`, the R7-11 address-zero branch), together with its callers' error capture at `unix.rs:191-199` (`unix_reserve`) and `unix.rs:288-291` (`try_reserve_aligned_exact`)
- **What is wrong:** task #713's rule (documented at every capture site) is that `VmemError::last_os_error()` is called "IMMEDIATELY after the syscall that produced it, before any cleanup FFI call that could clobber errno", and the capture-site comments assert "Nothing was mapped; no cleanup needed". The R7-11 branch breaks both statements: when `mmap` *succeeds* at address zero, `libc_mmap` unmaps it (`libc_munmap`) and returns null. The caller then executes `return Err(VmemError::last_os_error())` — but no syscall failed: `mmap` succeeded (errno untouched), and `munmap` either succeeded (errno untouched) or failed (errno now describes the cleanup, not the reservation). The returned error therefore carries whatever unrelated `errno` was lying around — exactly the "stale/irrelevant error code" class task #713 was written to eliminate.
- **Why it matters / failure scenario:** requires the kernel to return a successful mapping at address zero (`mmap_min_addr = 0` plus specific placement), so it is practically unreachable on hardened defaults — but on such a host every affected `try_reserve_aligned` reports a fabricated OS cause. The in-code comments ("no cleanup needed, so capturing here is already the immediate-capture") are now false for this sub-path.
- **Evidence:** `unix.rs:908-911` (`if p.cast::<u8>().is_null() { unsafe { libc_munmap(p.cast(), len) }; return core::ptr::null_mut(); }`) vs the caller comment at `unix.rs:192-193` "Nothing was mapped; no cleanup needed".
- **Suggested direction:** have `libc_mmap` return a richer result (or have the zero-address branch record `VmemError::os_refusal_unknown_code()` via an out-channel) so callers can distinguish "OS refused (errno valid)" from "crate rejected an address-zero grant (no valid errno)"; at minimum fix the two capture-site comments.
- **Confidence:** certain (about the code shape and doc mismatch); the triggering condition itself is exotic.

## F3 — `decommit_lazy` claims a contract "identical to `decommit`" but lacks the debug-build diagnostic `decommit` has

- **Severity:** LOW
- **Category:** docs | API
- **Location:** `crates/aligned-vmem/src/api/decommit_lazy.rs:51-58` (`decommit_lazy`) vs `crates/aligned-vmem/src/api/decommit.rs:91-99` (`decommit`)
- **What is wrong:** `decommit`'s body opens with a `debug_assert!(decommit_range_is_well_formed(...))` whose message explicitly advertises the behavior ("In a debug build say so loudly, so a consumer's own test fails at the mistake"). `decommit_lazy`'s rustdoc says "`start`/`end` contract and safety are identical to [`decommit`]" — but its body has no such `debug_assert`; a contract-violating call silently no-ops even in debug builds. So the two functions genuinely differ in debug-build observable behavior while documented as identical, and the diagnostic rationale given for `decommit` applies equally to `decommit_lazy`.
- **Why it matters / failure scenario:** a consumer's debug test that mistakenly passes misaligned offsets to `decommit` fails loudly (good); the same mistake with `decommit_lazy` passes silently and the memory quietly stays resident — the exact silent-skip failure mode the `decommit` assert was added to catch. Also, there is a `try_decommit` but no `try_decommit_lazy` (acknowledged in the rustdoc as future work), so the lazy variant is the only decommit entry point with neither a loud debug diagnostic nor a fallible twin.
- **Evidence:** grep: `debug_assert!` appears in `api/decommit.rs` but not in `api/decommit_lazy.rs`.
- **Suggested direction:** add the same `debug_assert!` (sharing `decommit_range_is_well_formed`) to `decommit_lazy`, or amend its rustdoc to state the debug-build difference.
- **Confidence:** certain

## F4 — Stale performance claim: `try_commit_range`'s fault-injection hook is described as "two relaxed loads" but now takes a `Mutex` on every real commit

- **Severity:** LOW
- **Category:** docs | performance | concurrency
- **Location:** `crates/aligned-vmem/src/api/commit_range.rs:91-95` (comment in `try_commit_range`), `crates/aligned-vmem/src/fault_injection.rs:121-158` (`should_fail_commit`)
- **What is wrong:** the call-site comment says "When neither hook is armed this is two relaxed loads that branch-predict not-taken — negligible on the production path". Since task #1021/R4-8 replaced the second hook's two atomics with `Mutex<FaultState>`, `should_fail_commit` unconditionally executes `FAULT_STATE.lock()` on **every** real commit call whenever `fault-injection` is compiled in — even when nothing is armed (the lock is taken after the `FAIL_NEXT` check regardless of `state.target`). That is a full uncontended mutex acquire/release (atomic RMW + fence), not a relaxed load, and it serializes concurrent `commit_range` callers — in tension with `commit_range`'s own "Concurrent calls are safe... not itself a new hazard" doc, which is still true for safety but now false for scalability under the feature.
- **Why it matters / failure scenario:** only bites builds with `fault-injection` enabled (test-only by design), so production impact is nil; but a consumer running multi-threaded commit-heavy tests under `fault-injection` gets an invisible global serialization point, and the comment actively misinforms anyone auditing the hot path.
- **Evidence:** `fault_injection.rs:146` — `let mut state = FAULT_STATE.lock()...` is unconditional; the `state.target > 0` check happens *after* acquiring the lock.
- **Suggested direction:** guard the lock behind a relaxed `AtomicBool`/`AtomicU32` "armed" flag (store it in `arm_fail_at`), restoring the two-relaxed-loads fast path; or just fix the comment.
- **Confidence:** certain

## F5 — `reserve_aligned_lazy` mock chaining defeats reserve-shape observation, and the recorded `ReserveLazy` disagrees with what actually ran

- **Severity:** INFO
- **Category:** API | tests
- **Location:** `crates/aligned-vmem/src/api/reserve_aligned_lazy.rs:71-80`
- **What is wrong (observation, not a bug):** under `aligned_vmem_mock`, `try_reserve_aligned_lazy` records `Call::ReserveLazy { initial_commit, .. }` but then deliberately chains to the *eager* backend (documented, with a sound rationale: a mocked no-op commit must not leave a genuinely-uncommitted Windows tail). The consequence worth stating for consumers: under mock, the recorded call log asserts a lazy reservation happened while the OS-visible behavior was eager, and `LazyReservation::committed_len()` reports `len()` (via `lazy_commit_is_honored() == false` under mock) — so a mock-based test cannot exercise any watermark-growth logic (`ensure_committed` is always a no-op below `len()`). This is coherent and documented across three files, but only implicitly; nothing in `LazyReservation`'s own rustdoc says "under the mock cfg the watermark starts at len()".
- **Why it matters:** a consumer using the mock to test incremental-commit bookkeeping will find their `ensure_committed` paths structurally untestable under mock and may not understand why.
- **Suggested direction:** one sentence in `LazyReservation`'s rustdoc (or `mock`'s module doc) naming the mock case explicitly alongside Unix/miri.
- **Confidence:** certain about behavior; INFO because it is a deliberate, internally-consistent design.

## F6 — `tests/mock.rs` comment asserts the wrong `initial_commit` contract; the test is a latent failure on any 16 KiB-page host under the mock cfg

- **Severity:** LOW
- **Category:** tests | docs
- **Location:** `crates/aligned-vmem/tests/mock.rs:118-146` (`fail_next_commit_injects_commit_range_failure`), contract at `crates/aligned-vmem/src/api/internal.rs:75-85` (`validate_initial_commit`)
- **What is wrong:** the test's comment says "`initial_commit` (the 3rd argument) validates against the compile-time `PAGE` constant (task #947/A-1's contract for `reserve_aligned_lazy`), so `PAGE` is correct there" and then calls `reserve_aligned_lazy(4 * MIB, 4 * MIB, PAGE)`. That claim contradicts the actual contract: `validate_initial_commit` rejects any `initial_commit` that is not a multiple of the **runtime** `page_size()`, and `internal.rs`'s own doc comment spells out the exact consequence: "on a 16 KiB-page host, `reserve_aligned_lazy(size, align, PAGE)` is now rejected". So on Apple Silicon macOS (or 64 KiB-page Linux) with `--cfg aligned_vmem_mock`, `.expect("lazy reserve")` panics and the test fails — for a reason unrelated to what it tests. It does not fail today only because CI's mock-cfg test row runs exclusively on `ubuntu-latest` (4 KiB pages) — verified in `.github/workflows/ci.yml:184-186` (the `aligned-vmem package gates` job).
- **Why it matters / failure scenario:** the moment anyone adds a macOS mock-cfg CI row (a natural extension — `src/mock.rs`'s module doc advertises the mock as usable "on any target (including macOS and miri)"), this test goes red with a misleading message; and the comment actively teaches the wrong contract to the next test author.
- **Evidence:** `mock.rs:123-127` (the comment) vs `internal.rs:77-79` (`!initial_commit.is_multiple_of(ps)` where `ps = page_size()`); README "Alignment contract": "`reserve_aligned_lazy`'s `size` and `initial_commit` must BOTH be multiples of the runtime page size".
- **Suggested direction:** pass `page_size()` as `initial_commit` (as `tests/lazy_commit.rs`'s task-#959-corrected tests already do) and fix the comment.
- **Confidence:** certain

## F7 — `bench_internals_counters.rs`'s "accessor surface completeness" mechanism silently rotted: the R7-3 counter is missing from both the import list and the reset assertions

- **Severity:** LOW
- **Category:** tests
- **Location:** `crates/aligned-vmem/tests/bench_internals_counters.rs:59-66` (import list) and `:112-125` (post-reset assertions)
- **What is wrong:** the test's comment claims "every accessor name must appear exactly once, creating a compile-time check that the accessor surface matches the counter surface. If a new counter is added without a corresponding accessor, this line will fail to compile." Two problems: (a) the mechanism is inverted — an import list can only detect a *removed* accessor, never a counter *added* without one; (b) even as an accessor roster it is stale: `windows_large_page_plain_fallback_successes` (added by R7-3, exported from `src/lib.rs` and reset by `reset_bench_internals_counters`) appears in NEITHER the import list NOR the reset-to-zero assertion block. The block's stated purpose is to catch "the R3-6 class of bug: a new counter is added, but its `store(0, ...)` line is omitted from `reset_bench_internals_counters`" — for the plain-fallback counter, exactly that regression would now go uncaught (deleting its `store(0, ...)` line in `src/bench_internals/reset.rs:38` fails no test).
- **Why it matters:** the crate's own history shows this exact bug class occurred once (R3-6); the guard built for it does not cover the newest counter, and its self-description overstates what it can catch.
- **Evidence:** grep for `plain_fallback` in `tests/bench_internals_counters.rs` — zero hits; `src/bench_internals/reset.rs:38` resets it; `src/lib.rs:169-177` exports it.
- **Suggested direction:** add the missing accessor to both lists; reword the "compile-time check" claim to what the import actually guarantees.
- **Confidence:** certain

## F8 — `bench_internals_counters.rs` calls `decommit` with `reservation_ptr()` as `base`, violating `decommit`'s documented `# Safety` contract

- **Severity:** INFO
- **Category:** tests
- **Location:** `crates/aligned-vmem/tests/bench_internals_counters.rs:90-93`
- **What is wrong:** `unsafe { aligned_vmem::decommit(reservation.reservation_ptr(), 0, ps) }` — `decommit`'s `# Safety` says "`base` must be the `as_ptr` of a live reservation". On 64-bit Unix, `reserve_aligned(4*ps, 4*ps)` over-reserves, so `reservation_ptr() != as_ptr()` whenever the kernel's address is not already `4*ps`-aligned; the call then decommits the first page of the slack region below the usable span, not of the usable span. Not UB (the whole over-reserved mapping is mapped, so the madvise is in-bounds and the counter increments as intended), but the crate's own test violates the contract the crate documents — and would break for real if the crate ever returned to trimming the over-reserve head (the pre-task-#842 design).
- **Suggested direction:** use `reservation.as_ptr()`; the counter increment works identically.
- **Confidence:** certain

## F9 — Linux huge-page over-reserve path charges the scarce hugetlb pool with up to `align` bytes of extra huge pages; the pool-exhausted path also pays a guaranteed-doomed extra syscall

- **Severity:** LOW
- **Category:** performance
- **Location:** `crates/aligned-vmem/src/os/unix.rs` — `unix_reserve` over-reserve branch with `huge == true` (lines 174-256), II-4 fast path (lines 107-142)
- **What is wrong / cost:** two related costs on Linux/Android with `huge-pages`:
  1. For `align > LINUX_HUGE_PAGE_SIZE` (e.g. a 4 MiB-aligned request), the only path is the over-reserve `mmap(size + align, MAP_HUGETLB|MAP_HUGE_2MB)`, and the whole mapping is kept for the reservation's lifetime. For private hugetlb mappings the kernel reserves pages from the hugetlb pool at mmap time for the entire mapping length — so the `align` bytes of slack (2 extra 2 MiB pages for a 4 MiB align) are charged against the pool even though never touched. The in-code comment "Cost: up to `align` bytes of untouched VA held for the reservation's lifetime (no RSS)" is written for the ordinary-pages case and understates the huge case: hugetlb slack is not merely VA, it is pool reservation — the same scarce resource the II-4 fast path exists to conserve (its own comment: "avoids charging `size + align` against the scarce hugetlb pool", which only covers `align == 2 MiB`). Note that since the guard forces `size`/`align` to be 2 MiB multiples and hugetlb bases are 2 MiB-aligned, huge-aligned head/tail trims WOULD be munmap-conformant here, unlike the ordinary-page case that motivated task #842's keep-whole-mapping design.
  2. When the II-4 exact-size fast path fails because the pool is exhausted, the fall-through immediately issues a LARGER `MAP_HUGETLB` request (`size + align`) that is guaranteed to fail for the same reason, before the ordinary-page retry — one wasted syscall per reservation on the pool-exhausted path.
- **Why it matters / workload:** an allocator reserving many 4 MiB-aligned huge segments on a host with bounded `nr_hugepages` silently consumes ~2 pool pages of reservation per segment beyond need (33% pool overhead at size = 4 MiB), hitting pool exhaustion — and the huge→ordinary fallback — earlier than necessary.
- **Evidence:** the validation guard at `unix.rs:79-84`; the over-reserve keeps `over = size + align` (`:239-255`); no head/tail trim exists anywhere in the file.
- **Suggested direction:** (a) trim huge-aligned head/tail slack for the hugetlb case specifically (provably conformant given the 2 MiB-multiple guard), or at minimum document the pool-reservation cost in `reserve_aligned_huge`'s rustdoc; (b) skip the doomed `size + align` huge retry when the exact-size huge attempt with identical flags just failed. REASONED-FROM-SPEC (hugetlb reservation accounting per Linux hugetlbpage documentation); no hugetlb host available to measure, matching the crate's own verification-honesty convention.
- **Confidence:** likely (pool-reservation-at-mmap-time is standard documented Linux behavior for non-`MAP_NORESERVE` private hugetlb mappings, but unverified empirically here); the wasted-syscall half is certain from the code.

## F10 — `LazyReservation` has no `Debug` impl (its inner `Reservation` deliberately grew one)

- **Severity:** INFO
- **Category:** API
- **Location:** `crates/aligned-vmem/src/lazy_reservation.rs:58-65` (`LazyReservation` struct)
- **What is wrong:** `Reservation` carries a hand-written `Debug` (added as "V7 fix", pinned by `tests/smoke.rs::reservation_has_debug_output`), and `ReservationParts`/`ReservationFullParts` derive it — but `LazyReservation`, the newest public handle, implements no formatting trait, so a consumer cannot `{:?}` it (nor its watermark, the one piece of state the type exists to track). The argument that motivated V7 applies verbatim.
- **Suggested direction:** hand-write `Debug` forwarding `inner`'s fields plus `committed`.
- **Confidence:** certain

## F11 — Assorted doc rot (single finding for the small items)

- **Severity:** INFO
- **Category:** docs
- **Location / items:**
  1. `src/bench_internals/mod.rs:3` — "Three independent questions, one instrument family each:" followed by FIVE bullets (the list grew in rounds 3-7; the header count never updated).
  2. `src/os/windows.rs:576-579` — trailing banner comment "Unix path: mmap / munmap / madvise. Raw bindings declared locally — no libc dependency" sits at the END of the WINDOWS file, describing nothing below it (a leftover from the pre-split monolith; the Unix bindings live in `os/unix.rs`).
  3. `src/os/unix.rs:148-152` — the R3-1/R4-2 comment cites "lines 2707-2738", stale line numbers from the deleted 4656-line `lib.rs` monolith; the crate's own convention (see `tests/smoke.rs:73-89`, task #908/V2C1) is to name symbols, not line numbers, precisely because two prior line citations drifted stale within one round.
  4. `src/api/decommit.rs:24-25` — "`start` and `end` must be multiples of [`page_size()`] and within the span. A no-op if the range is empty." — does not say that a VIOLATING (not merely empty) range is also a silent no-op; the reader learns that only from the later debug-assert paragraph.
- **Why it matters:** none is load-bearing alone; collectively this is the drift class the repo's own reviews repeatedly flag (R7-9: "the two drifted apart once already").
- **Suggested direction:** one comment-only cleanup pass.
- **Confidence:** certain

---

## Verified sound (probed and found correct)

Areas explicitly checked with a defect hypothesis in mind, where the code held up:

- **FFI signatures, both platforms.** `mmap`/`munmap`/`madvise`/`sysconf` (`extern "C"`, the two-arm `OffT` alias: i32 for 32-bit glibc/bionic, i64 for everything else including 32-bit musl/BSD/Darwin) and `VirtualAlloc`/`VirtualFree`/`GetSystemInfo`/`GetLargePageMinimum` (`extern "system"`, SIZE_T→usize, BOOL→i32) match the real ABIs. `SystemInfo`'s `#[repr(C)]` layout matches `SYSTEM_INFO` field-for-field, including the leading union-as-two-u16s and `DWORD_PTR dwActiveProcessorMask` as `usize`. `#[derive(Default)]` on raw-pointer fields is valid at the crate's MSRV (1.88, where `Default for *mut T` is stable) — consistent with `rust-version = "1.88"`.
- **Per-target constants.** `MAP_ANON` (0x20 Linux/Android, 0x1000 Darwin/BSD), `MADV_DONTNEED = 4` (all supported Unix), `MADV_FREE = 8` (Linux), `MADV_FREE_REUSABLE = 7` (Darwin), BSD `MADV_FREE` 5 (FreeBSD/DragonFly) and 6 (NetBSD/OpenBSD), the per-OS `_SC_PAGESIZE` table (29 Darwin / 47 FreeBSD-DragonFly / 28 NetBSD-OpenBSD / 39 bionic / 30 glibc-musl), `MAP_HUGETLB`/`MAP_HUGE_2MB`, and the Windows `MEM_*`/`PAGE_READWRITE` values — all checked; the MIPS and unknown-Unix `compile_error!` guards close the known non-portable arms.
- **Reserve/release lifecycles.** Every error path in `unix_reserve`, `try_reserve_aligned_exact`, and `win_reserve_commit` unmaps/releases exactly once before returning (each early-return checked for leaks and double-frees); the II-4 huge fast path munmaps its alignment-miss mapping with a huge-aligned length; the Windows single-call→two-call fallthrough releases the misaligned allocation before retrying. No leak or double-free found on any path.
- **Overflow discipline.** `validate_size_align` (checked_add + `isize::MAX` cap), `align_up_addr` (checked), fit computations in both backends (checked_add chains), `leak_zeroed_pages` rounding (checked_add), `from_raw_parts`'s assert (checked_add for `len + (base - reservation)` AFTER the ordering check — the task #929 reorder is present).
- **`from_raw_parts` / `release` validation** matches the documented contract; every cheaply-checkable invariant is asserted, and each assert clause has a dedicated counterfactual `#[should_panic]` test in `tests/smoke.rs`.
- **Watermark logic (`LazyReservation`).** `ensure_committed` monotonicity/round-up/failure-leaves-watermark, `shrink_committed` round-up-keeps-kept-bytes, `new`'s derivation from `lazy_commit_is_honored()` — all consistent with the invariant (`committed` always a `page_size()` multiple ≤ `len()`); rounding cannot escape the span because `validate_initial_commit` forces `size` to a `page_size()` multiple.
- **Mock TLS/reentrancy.** `record`'s `RECORDING` guard + `try_with` teardown handling, `drain`'s borrow-scope fix (R4-9), and the dedicated `#[global_allocator]` reentrancy construction in `tests/mock_reentrancy.rs` (genuinely non-vacuous; counterfactually verified per its own doc).
- **Fault-injection atomics.** `FAIL_NEXT`'s `fetch_update` RMW closes the load/store race, and `tests/fault_injection.rs::fail_next_is_atomic_under_concurrent_callers` is a genuinely discriminating oracle (arms TOTAL/2; its doc records the counterfactual run against the reverted racy implementation). The `arm_fail_at` mutex serialization is correct — only the stale "two relaxed loads" claim is wrong (F4).
- **`page_size()` cache.** Benign racy-init on a relaxed atomic (idempotent value); 0-sentinel valid; `validate_page_size_impl` fail-closed fallback to `PAGE`.
- **Safe-API soundness.** No safe `pub fn` can trigger UB from 100%-safe downstream code: every entry point that trusts a caller pointer is `unsafe fn` with a `# Safety` contract; the safe `Reservation::*` methods bounds-check against `len()` before delegating; `Reservation: Send + !Sync` is coherent with the `&self` decommit methods because materializing any reference into the span already requires `unsafe`.
- **Test quality overall** is unusually high: path-activation oracles instead of vacuous success assertions, counterfactual `#[should_panic]` coverage per assert clause, per-file SERIAL mutexes for the process-global counters (with documented reproductions of the races they prevent), and platform-gated tests that assert the OTHER half of the contract instead of skipping (`tests/lazy_reservation.rs`). The exceptions found are F6-F8.

## Coverage honesty — what this audit did NOT do

- **Nothing was compiled or executed** (read-only run per the operator's instruction): no `cargo check`/`clippy`/`test`, no miri, no cross-target checks. All findings are from source reading; any claim about what compiles is reasoning, not a build receipt.
- **Not fully read:** `tests/mock.rs` beyond line 180 (constructor-coverage tail), `tests/mock_reentrancy.rs` beyond line 80 (the probe-allocator body), `CHANGELOG.md` (not audited against the code at all), and the repo-level CI file beyond the `aligned_vmem_mock`/gates excerpts cited in F6.
- **Not verified empirically (by anyone, per the crate's own honesty notes):** BSD/Android/tvOS/watchOS constants, hugetlb behavior on a configured host, non-2 MiB `default_hugepagesz` hosts, 16 KiB/64 KiB-page Linux. This audit checked the REASONED-FROM-SPEC citations against header values from memory; it adds no new empirical verification.
- **Not audited:** the rest of the workspace's consumption of this crate; whether the `bench-scale-tool = "0.1"` dev-dependency resolves on crates.io (a publish-time concern worth one check before `cargo publish`, since packaged benches need it); license/metadata files.

## Summary

| Severity | Count | Findings |
|---|---|---|
| HIGH | 0 | — |
| MEDIUM | 1 | F1 |
| LOW | 6 | F2, F3, F4, F6, F7, F9 |
| INFO | 4 | F5, F8, F10, F11 |

Total: 11 findings. Top three by importance: **F1** (capability query wrong under the mock cfg — the one place a public API actively misleads a consumer), **F9** (hugetlb pool over-charge — a real resource cost on the huge-page feature's flagship platform), **F6** (latent cross-platform test failure plus a comment teaching the wrong contract).
