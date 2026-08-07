# rust-cc-audit report — `aligned-vmem`

**Date:** 2026-08-07
**Produced by:** the `/rust-intel` skill's fan-out audit workflow (`audit-project.workflow.js`,
run `wf_785991fc-92f`) — 14 agents total. ~1.17M subagent tokens, 124 tool calls.
**Audited tree:** `main` @ current HEAD, `crates/vmem`.

---

**Scope:** D:\dev\rust\sefer-alloc\crates\vmem
**Pinned versions:** aligned-vmem v0.2.0 (zero-dependency by design — `[dependencies]` empty; syscalls declared locally via `extern "system"`/`extern "C"`), MSRV 1.88, edition 2021
**Found:** 0 critical, 1 high, 11 medium, 9 info

---

## CRITICAL

_none_

## HIGH

### [§D1] crates/vmem/tests/fault_injection.rs:15 — real-path fault-injection tests run in ZERO CI configurations

`#![cfg(all(feature = "fault-injection", feature = "lazy-commit", not(feature = "mock")))]` vs ci.yml:733-735's claim "so all 4 test files actually run … plus `fault_injection.rs` compiling and running under `mock`". The default-features CI step cfg's the file out, and the `--all-features` step (ci.yml:740) turns `mock` ON, so the file's own `not(mock)` gate compiles it to an empty binary — §D1's "CI stays green because the test never runs" rot, with a false coverage claim on top. The hook's subtle semantics (fail_next priority over fail_at, one-shot disarm, k-th counting) are load-bearing for the root crate's grow-on-carve rollback tests.

**Fix:** add a CI step `cargo test -p aligned-vmem --features fault-injection,lazy-commit --no-fail-fast` (mock OFF) to the test-workspace job, and correct the ci.yml:725-739 comment to state that fault_injection.rs is deliberately excluded under `--all-features` and covered by the new step.

## MEDIUM

### [§B5] crates/vmem/src/lib.rs:1279 — unsafe block without a `// SAFETY:` comment

`let addr = unsafe { base.add(start) };` in the unix `decommit_pages_impl` has no preceding `// SAFETY:` (the match arms at :1283/:1284 have one; the Windows twin at :1005 has one at :1003-1004). §B5 BANNED verbatim; the crate's own module doc (lib.rs:12-13) publicly claims "every unsafe block carries a // SAFETY: proof", and repo CLAUDE.md's seam rule makes a missing proof a finding — this one block breaks both, on the eve of the 0.2.0 publish.

**Fix:** add `// SAFETY: caller guarantees start < end <= reservation len, so base.add(start) stays within the same live allocation (no wrap, in-bounds provenance).` above line 1279, mirroring lib.rs:1003-1004.

### [§B13] crates/vmem/src/fault_injection.rs:69 — Relaxed payload-then-flag publish across FAIL_AT_COUNTER/FAIL_AT_TARGET

`FAIL_AT_COUNTER.store(0, Relaxed); FAIL_AT_TARGET.store(k, Relaxed)` armed on one side; `should_fail_commit` loads the flag Relaxed then touches the payload — a cross-thread reader can see `target=k` while the counter reset is not yet visible, firing the fault at the wrong call index. The owner-only same-thread contract (fault_injection.rs:26-30) is stated but unenforced on safe pub fns over process-wide statics; the crate's intended consumer is a concurrent allocator, and the crate's own test file needed a SERIAL Mutex (tests/fault_injection.rs:34) precisely because parallel libtest threads interleave against this state.

**Fix:** Release on the stores in `arm_fail_at`, Acquire on the loads in `should_fail_commit` (ordering fix, not UB — cheap); or enforce the single-thread contract structurally (thread-locals like mock.rs) or debug_assert the arming thread id.

### [§B25a] crates/vmem/src/lib.rs:488 — `last_os_error()` read after intervening cleanup FFI (also flagged by §F2 semantics and §B26 data-and-types)

`None => Err(VmemError::last_os_error())` — but `win_reserve_commit` runs `unsafe { winapi_virtual_release(region_ptr) }` (lib.rs:979) between the failing VirtualAlloc(MEM_COMMIT) and this errno read; same shape at lib.rs:795 and lib.rs:844, and on Unix `unix_reserve` runs `libc_munmap` cleanup (lib.rs:1205) before returning None. §B25a BANNED verbatim: the cleanup FFI can clobber errno/GetLastError, so the VmemError's entire purpose (carrying the OS cause "captured at the point of failure", per error.rs's documented guarantee) is unreliable. Worse, the `fits`/checked_add-overflow paths involve NO failing syscall at all, yet mint a stale errno as an "OS refusal" (often code 0/ERROR_SUCCESS).

**Fix:** change the raw helpers (`win_reserve_commit`, `unix_reserve`, `try_reserve_aligned_exact`, `reserve_aligned_lazy_raw`, `reserve_aligned_huge_raw`) to return `Result<_, VmemError>`, capturing the error immediately after the failing syscall and BEFORE any cleanup; map the internal fit/overflow failure to a distinct non-OS cause (e.g. `VmemError::invalid_argument()`). The Windows `recommit_pages_impl` (lib.rs:1027) already does immediate capture — mirror it.

### [§B26] crates/vmem/src/lib.rs:627 — recommit/commit_range clamp a violated offset contract to the SUCCESS sentinel

`if start >= end || !start.is_multiple_of(PAGE) || !end.is_multiple_of(PAGE) { return Ok(()); }` — the exact "saturating hides the logic error" shape: the API's own contract says the false/Err sentinel means "caller MUST NOT write", yet a violated contract returns the write-permitting value, so on Windows the caller writes into still-decommitted pages and takes a hard STATUS_ACCESS_VIOLATION (the divergence lib.rs:534-542 itself documents as having already crashed an in-repo consumer). Same pattern at lib.rs:694 (`try_commit_range`) and, lower-stakes, lib.rs:544/578 (decommit silently skipped).

**Fix:** return `Err(VmemError::invalid_argument())` (and `false` from the bool wrappers) for misaligned/inverted offsets; keep Ok only for the genuinely-empty `start==end` case. Update the doc lines and the tests pinning current behavior (tests/smoke.rs:95-97, tests/lazy_commit.rs:96-110) in the same change.

### [§C1a] crates/vmem/src/mock.rs:30 — `Call`'s struct-like variants lack variant-level `#[non_exhaustive]`

Enum-level `#[non_exhaustive]` only reserves the right to add VARIANTS; adding a field to an existing variant (e.g. a huge/commit-flags field on `Reserve` — plausible given `ReserveLazy` already grew `initial_commit`) is still a semver-major break for every downstream `Call::Reserve { size, align }` match (`mock` is a published feature of a published crate).

**Fix:** add `#[non_exhaustive]` to each struct-like variant (variant-level form is stable), forcing `Call::Reserve { size, align, .. }` patterns; update tests/mock.rs. Decide now at 0.2.x — adding it later to already-published exhaustive variants is itself breaking.

### [§C10] crates/vmem/Cargo.toml:59 — `mock` is a non-additive backend-replacing feature exposed to Cargo feature unification

`mock = []` "replaces the real OS syscalls with a thread-local recording backend". Cargo unifies features across a consumer's graph, so any crate anywhere downstream enabling `aligned-vmem/mock` (e.g. in dev-dependencies) silently swaps the real mmap/VirtualAlloc backend out from under EVERY other consumer in that build — an allocator whose reserve/commit calls become recording stubs, with no compile error. Nothing warns the external consumer, and the crate is queued for crates.io publish (task #658).

**Fix:** at minimum add an explicit unification warning to the `mock` feature doc in Cargo.toml, lib.rs and README ("enable it only in leaf test crates/targets"). The stronger §C10-prescribed shape is a cfg flag (`--cfg vmem_mock` via RUSTFLAGS, declared in `[lints]` check-cfg like the workspace already does for `cfg(loom)`/`cfg(kani)`), since cfg flags do not unify.

### [§D1] crates/vmem/tests/lazy_commit.rs:246 — OS-zero-fill assertion on a never-written byte, ungated under miri/mock

`assert_eq!(base.read(), 0, "initial region byte not overwritten")` — byte 0 is never written by this test. Under the miri backend (`reserve_aligned_lazy_raw` → `std::alloc::alloc`, lib.rs:1527-1533, documented as NOT zeroing) this is an uninitialized read: miri reports UB and the value is arbitrary. The sibling assertion in smoke.rs:72-77 carries exactly the `#[cfg(not(any(miri, feature = "mock")))]` gate this one omits — and the crate's headline claim is that consumers "stay miri-testable".

**Fix:** gate the assertion like smoke.rs:72, or write byte 0 first and assert the written value.

### [§D1a] crates/vmem/src/lib.rs:824 — public huge-pages API has zero tests

Grep over tests/ finds zero calls to `reserve_aligned_huge`/`try_reserve_aligned_huge`; mock.rs:50 defines `Call::ReserveHuge` but no test drains it. The headline behavioral promise ("best-effort: falls back to ordinary pages, never fails purely because huge pages are unavailable" — retry paths lib.rs:961-977 Windows, lib.rs:1178-1189 Unix) has no evidence; CI's `--all-features` step only closed the COMPILE gap. No mutation of the fallback logic would be caught by any test.

**Fix:** add (a) smoke — `reserve_aligned_huge(4 MiB, 4 MiB)` on a runner without configured huge pages must succeed via fallback, aligned and writable; (b) contract rejection mirroring `rejects_bad_contracts`; (c) mock-recording — assert `Call::ReserveHuge` is logged and `fail_next_reserve` injects through the huge path.

### [§F1] crates/vmem/src/lib.rs:1392-1403 — hardcoded `_SC_PAGESIZE` wrong on all four BSDs the crate targets

`const _SC_PAGESIZE: i32 = { … 29 (macOS) … 30 };` — 30 is Linux/musl only; FreeBSD/DragonFly use 47, NetBSD/OpenBSD 28 — all four are in the crate's MAP_ANON cfg list (lib.rs:1356-1370). `sysconf(30)` there returns an unrelated limit; if it happens to be a power of two it passes `page_size()`'s guard and poisons exactly the decommit-offset rounding the doc (lib.rs:227-230) tells callers to base on `page_size()`. BSDs are never in CI, so the smoke test that would catch it never runs there.

**Fix:** per-OS cfg the constant (47 freebsd/dragonfly, 28 netbsd/openbsd, citing each OS's sys/unistd.h per §F1 REQUIRED) or call `getpagesize()`; tighten `page_size()`'s guard to require `queried >= PAGE`.

### [§F1] crates/vmem/src/lib.rs:1263,1266-1271,1213-1219 — hugetlb munmap violates mmap(2)'s huge-page alignment mandate, silently leaking mappings

mmap(2) "Huge TLB" mappings require munmap's addr AND length to be multiples of the huge page size, while mmap auto-rounds length UP. `reserve_aligned_huge` accepts any PAGE-multiple size: e.g. size=2MiB+4KiB maps 4MiB of hugetlb but records `reservation_len=size`; release's `munmap(base, 2MiB+4KiB)` fails EINVAL — swallowed by `let _ = munmap(...)` (lib.rs:1451) — and the whole mapping plus pinned physical huge pages leak on every release. Same for the over-reserve path's 4KiB-granular head/tail trims. No `// DEVIATION` comment, no doc constraint.

**Fix:** reject (invalid_argument) size/align not multiples of the huge page size, or round size/trim boundaries up and record the rounded `reservation_len`; cite mmap(2)'s Huge-TLB munmap rule at the site.

### [§F2] crates/vmem/src/lib.rs:948, 1210 — README's "pointers preserve provenance / no as-usize round-trips" guarantee is contradicted by both native base-pointer constructions

`let base = unsafe { NonNull::new_unchecked(base_addr as *mut u8) };` where `base_addr` is computed from `region_ptr as usize`. README §"Provenance & safety" states verbatim: "The returned pointers preserve provenance (no exposed-address `as usize` round-trips in the public API)" — yet the returned pointer is constructed by exactly such a round-trip on both native paths; legal only via exposed-provenance, and under strict provenance the base has no provenance. This sits in the crate's safety-guarantee prose.

**Fix:** derive the base by offsetting the original pointer (`region_ptr.add(base_addr - region_addr)`) so provenance is genuinely carried, or amend the README sentence to scope the claim to the miri path / API signatures.

## INFO

### [§A3] crates/vmem/src/lib.rs:161 — bench-internals counters exposed as directly-writable `pub static AtomicU64`

`UNIX_EXACT_RESERVE_ATTEMPTS` (also `UNIX_EXACT_RESERVE_HITS` :169, `WINDOWS_RESERVE_COMMIT_CALLS` :180) are writable by any consumer, a wider semver commitment than needed and redundant with the existing pub accessor/reset fns.

**Fix:** make the statics private (or pub(crate)), keep only the fns pub, adjust the three rustdoc intra-links. Feature-gated off by default, so low-risk now, harder later.

### [§B4] crates/vmem/src/lib.rs:1496 — panic-capable `.expect` reachable from `Drop for Reservation` (miri path)

`Layout::from_size_align(reservation_len, align).expect("release: invalid layout")` — called from Drop (lib.rs:433), so a second panic while unwinding aborts the process. Only trigger: `Reservation::from_raw_parts` (lib.rs:407) handed a non-power-of-two align or overflowing len — a contract it documents but never checks, deferring the failure to the destructor.

**Fix:** validate the layout eagerly in `from_raw_parts` (where a panic is a plain contract error), and in the miri `release_reservation` use a non-panicking fallback (`if let Ok(layout) = … { dealloc } else { debug_assert!(false) }`).

### [§B13] crates/vmem/src/fault_injection.rs:81 — check-then-act decrement on FAIL_NEXT

`let next = FAIL_NEXT.load(Relaxed); if next > 0 { FAIL_NEXT.store(next - 1, Relaxed); … }` — separate load and store, so two concurrent committing threads can both observe `next==1` and both fail (or lose a decrement), breaking the documented "fail exactly the next N calls" determinism. Covered by the same unenforced owner-only contract as the medium §B13 finding.

**Fix:** one-liner — `FAIL_NEXT.fetch_update(Relaxed, Relaxed, |n| n.checked_sub(1))` (or compare_exchange loop) so the decrement is a single atomic RMW.

### [§B26] crates/vmem/src/error.rs:100 — `unwrap_or(0)` makes "no OS code available" indistinguishable from genuine code 0

`std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32` — `os_code()` then reports `Some(0)`, indistinguishable from a real ERROR_SUCCESS/errno-0; a consumer branching on the code sees "the OS refused, cause: success". Compounds the §B25a finding: the stale-errno paths and the test fault injectors (mock.rs:150, lib.rs:715) are the realistic producers of this code-0 error.

**Fix:** preserve the absence — store `Option<u32>` (or NonZeroU32-with-sentinel) so `os_code()` returns None when `raw_os_error()` was None; Display says "unknown OS error" instead of "code 0".

### [§C2] crates/vmem/src/lib.rs:1451 — `let _ =` syscall-result discards without the required "why" comment

`let _ = munmap(addr as *mut c_void, len);` (same at lib.rs:1460 for madvise). §C2 BANNED: silent `let _ =` without a comment stating why discarding is intentional — the adjacent SAFETY comments prove pointer validity, not why the error return is ignored; a failed munmap in the release path silently leaks the reservation.

**Fix:** add a one-line discard-justification at each site (e.g. "// errors ignored: munmap failure in the infallible release path can only leak, never corrupt"), matching the compliant sibling `libc_madvise_hugepage` at lib.rs:1467.

### [§C7] crates/vmem/src/lib.rs:96 — crate-wide `allow(dead_code)` for the fault-injection-without-lazy-commit combo

`#![cfg_attr(all(feature = "fault-injection", not(feature = "lazy-commit")), allow(dead_code))]` recreates exactly the blanket suppression the crate itself narrowed away for `mock` (task #646/F8, lib.rs:77-90): in that feature combo, any genuinely-dead item anywhere in the crate goes unreported.

**Fix:** move the cfg_attr onto the `mod fault_injection` declaration (or per-item inside fault_injection.rs, matching the established per-item pattern), not a crate-level inner attribute.

### [§D3] crates/vmem/src/lib.rs:441 — the `Send` claim on `Reservation` is exercised by no test

`unsafe impl Send for Reservation {}` — the one concurrency claim the crate makes (reserve on one thread, drop/release on another) is untested; the whole suite is single-threaded. Low risk (plain ownership-transfer argument).

**Fix:** one small test — reserve on the main thread, move the Reservation into `std::thread::spawn`, write through it, drop it there.

### [§F1] crates/vmem/src/lib.rs:1407-1414 — local mmap prototype fixes `off_t` at i64 unconditionally

`fn mmap(..., offset: i64)` — on 32-bit Unix targets glibc/musl's `mmap` symbol takes a 32-bit off_t (the 64-bit variant is `mmap64`); an ABI-shape divergence, currently benign only because offset is always literal 0 and last-argument on every supported convention.

**Fix:** declare offset with a per-target off_t-sized alias (i32 on 32-bit gnu/musl, i64 elsewhere) or bind `mmap64`; note the reasoning at the extern block.

### [§F4] crates/vmem/src/lib.rs:379-383,407 — `from_raw_parts` documented as `into_parts`' inverse, but the composition is unformable and untested

`into_parts` returns only `(reservation, reservation_len, align)`, discarding base and len, so `from_raw_parts` (5 args) cannot be fed from its output — the stated law "inverse of into_parts" cannot even be formed; grep confirms zero uses of `from_raw_parts` in tests/ (§F4 BANNED: shipping one half with no round-trip of the composition).

**Fix:** reword the doc to "adopts a foreign reservation into the RAII lifecycle", and add a round-trip test building a Reservation from a real `reserve_aligned`'s known fields, releasing via Drop and via `into_parts`+`release` (fully assertable from the mock call log).

---

## Post-flight summary

Aggregated 🔴 inventory across all agents (occurrences with file:line + status; "none" where a 🔴 item had zero occurrences):

- **§B21 tokio::spawn with dropped JoinHandle** — none (crate-wide N/A, confirmed by two agents: no async runtime, no spawn/JoinHandle tokens; `[dependencies]` empty).
- **§B22 impl Drop doing async work** — none. The crate's sole `impl Drop` (Reservation, src/lib.rs:427) is one synchronous `release_reservation` syscall with a SAFETY comment; no `.await`/block_on/spawn anywhere (confirmed by two agents).
- **§B15b Pin::new_unchecked** — none (crate-wide N/A, two agents: no Pin/Poll/Waker/manual Future machinery).
- **§B13 Relaxed-publish data race** — src/lib.rs:233/:246 (PAGE_SIZE_CACHE): justified, value-carrying atomic, benign idempotent race; src/lib.rs:974/:983, :1239/:1255, :187/:195/:203/:214-216 (bench-internals counters): justified, standalone documented diagnostics; **src/fault_injection.rs:69-70 vs :86-88: VIOLATED in shape** (payload-then-flag Relaxed publish; doc-justified by an unenforced owner-only contract) — filed as the medium §B13 finding; src/fault_injection.rs:58/:81-83: ordering justified under the same contract, but the load+store decrement is a 🟡 check-then-act — filed as the info §B13 finding.
- **§B14 unbounded_channel / unbounded FuturesUnordered** — none (crate-wide N/A: no channels, no FuturesUnordered/JoinSet, no async code).
- **§B5 unsafe without SAFETY** — ~55 unsafe sites inventoried across src/lib.rs (:254, :267, :407, :433, :502/:513, :543/:556/:577/:590, :614-:641, :681-:718, :888, :896/:948/:1210/:1257, :923-:979, :988/:1267/:1492, :999-:1040, :1129/:1137/:1143, :1173-:1225, :1240-:1261, :1291-:1519, :1421-:1471, :1486): all justified with per-site SAFETY proofs — **except src/lib.rs:1279, VIOLATED** (`base.add(start)` with no preceding `// SAFETY:`) — filed as the medium §B5 finding. Test-file unsafe call sites (tests/smoke.rs, tests/mock.rs, tests/lazy_commit.rs, tests/fault_injection.rs): justified, SAFETY-comment counts match unsafe-site counts 1:1 (8/3/14/11). transmute / mem::uninitialized / mem::zeroed: none.
- **§B18 unsafe impl Send** — src/lib.rs:441: justified (SAFETY block cites exclusive-ownership invariant; `Sync` deliberately not implemented, rationale documented at lib.rs:289-291).
- **§B18a raw-pointer wrapper variance/PhantomData** — src/lib.rs:292 (Reservation, 2× NonNull<u8>, no PhantomData): justified/N-A (non-generic type, variance trivially fixed; owns untyped bytes).
- **§B25 extern blocks / from_raw families** — src/lib.rs:1064 (`extern "system"` kernel32) and src/lib.rs:1406 (`extern "C"` libc): justified (sanctioned zero-dep OS seam, `#[repr(C)]` SYSTEM_INFO mirror at :1077, no exported extern fns/#[no_mangle], real FFI isolated behind `#[cfg(miri)]`). Box::from_raw / Vec::from_raw_parts / slice::from_raw_parts: none (the only `*_from_raw_parts` is Reservation's own inventoried unsafe constructor).
- **§B25a errno-read discipline** — **src/lib.rs:488/:795/:844: VIOLATED** (read after intervening cleanup FFI) — filed as the medium §B25a finding; src/lib.rs:1027 and src/error.rs:69/:100/:105: justified (immediate capture); src/lib.rs:715 and src/mock.rs:150/:163: justified (deliberate documented simulation in test-only features); cross-thread FFI contract: N/A/justified (kernel syscalls are per-process thread-safe; fault_injection.rs:26-30 documents its threading contract; mock.rs is thread_local).
- **§A1 slopsquatting / unpinned git-[patch] / build.rs network** — none (three entries, all N/A: `[dependencies]` empty by design; no `[patch]` table or git sources in crate or workspace root; no build.rs exists).
- **§C1 blanket impl in a published pub API** — none (zero occurrences: no generic impls, no public traits crate-wide).
- **§F1/§F2 spec/doc divergence affecting wire format, security guarantee, or persisted data** — boundary case at src/lib.rs:948/:1210 vs README.md:88-92 (false provenance claim in the README's safety-guarantee section): evaluated as soundness-doc mismatch, not wire/persisted/attacker-visible → downgraded 🟡, filed as the medium §F2 finding; otherwise N/A (no wire format, no persisted data).
- **§F3 leaked/unclosed boundary resource an untrusted peer can hold open** — src/lib.rs:1266-1271 (hugetlb munmap EINVAL leak): leak confirmed but only local-caller-reachable via an opt-in feature, not peer-extendable → downgraded 🟡, folded into the medium §F1 hugetlb finding; otherwise N/A (no sockets/streams/peers; all native error paths release the reservation before returning None — verified at lib.rs:943, :979, :1205, :1251).

Modules declaring no 🔴 items (empty inventories by construction): data-and-types, drop-and-raii, testing, security (both its 🔴 categories §B12/§B24 confirmed at zero occurrences), lifetimes-and-api (§C1 zero).
