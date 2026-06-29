# Checkpoint — 2026-06-28 numa-complete-perf-investigation

## Session summary

Massive session on `sefer-alloc`. Most of it is closed; one task is in flight (sh-agent in background).

**What was closed (in order):**

- **Large-cache redesign** (`#90/#91/#92/#93/#94/#95/#100`): removed the per-span cap `MAX_CACHED_LARGE_BYTES=64MiB` (was an "artificial disability"), implemented byte-budget admission (default unbounded — client controls), lazy exponential decay 10%/sec toward `live+headroom` (256MiB default), mode-selector stub `LargeCacheMode { Lazy, Background, Both }` via `SEFER_LARGE_CACHE_MODE`. **Two perf fixes**: `Instant::now()` early-exit when `cached <= headroom` (#95 — restored 4 MiB cache hit from 148ns back to 54ns) and admission policy fix when `budget=None` and both slots are occupied (#94 — 64 MiB stopped infinitely cache-missing, 117us → 60ns). M5 reentrancy test was flaky on Linux (process-global counter caught background allocs from std/glibc), fixed with thread-local `Cell<usize>` (#100). Headline perf: 4 MiB **13.7x** / 16 MiB **21.4x** / 64 MiB **33x** faster than mimalloc on cached round-trip.
- **NUMA 4-phase testing stack** (`#96/#97/#98/#99`): #96 mock-shim feature in numa-shim (per-PR coverage on any target); #97 weekly QEMU/numactl Linux job; #98 Hyper-V virtual NUMA dev recipe (`docs/NUMA_WINDOWS_DEV_RECIPE.md`); #99 cloud release gate with AWS/Azure recipes (`docs/NUMA_RELEASE_GATE.md`).
- **#83 Windows VirtualAllocExNuma direct path**: added `pub unsafe fn Reservation::from_raw_parts(base, len, reservation, reservation_len, align) → Self` in aligned-vmem; numa-shim Windows now calls `VirtualAllocExNuma` directly (over-reserve+trim, adopt via from_raw_parts). Local round-trip 4 MiB x 4 MiB x 1024-page fault-in x 8 repeat — passes on real Windows kernel. CI job `numa-shim-windows` on `windows-latest` also passing.
- **CI Windows + macOS coverage**: added `numa-shim-windows` and `numa-shim-macos` jobs (correctness, not perf — single-NUMA OK for memory-safety validation). macOS aarch64 (Apple Silicon) uses 16K pages — had to switch tests to runtime `aligned_vmem::page_size()` instead of the constant `PAGE` (845560f).
- **Stress validation**: soak_xthread 8 threads x 20s — 322M ops, 16M ops/sec, `alloc==free zero leak`. tokio_burn_in no-panic. heap_soak + decommit_soak passing. Full release test suite 53 ok, 0 FAILED.

**What is in flight now:**
- **#101 Flamegraph + small-path optimization** — sh-agent `aa76e29109cd58a7a` in background. Goal: on the single-thread small-class hot path (16-256B) we are 1.8-2.3× slower than mimalloc (~8ns gap per call). The agent will try flamegraph (samply/blondie/WSL2 perf), then 1-2 of the most promising optimizations: `try_with → with` on TLS, more aggressive `#[inline(always)]`, `CurrentHeap` tag enum → raw pointer with NULL sentinel, moving `maybe_decay_large_cache` from the small path to large-only.
- **CI watch background**: `b5caeqvx3` has already completed (included Windows+macOS jobs); on `c85e872` (cargo fmt) should also be green. On `845560f` (macOS fix) CI watch was not yet started — should pass after the macOS test with runtime page_size.

**Hypotheses alive (for #101):**
- Most likely culprit: Rust `try_with` on TLS adds ~3-5ns safety overhead vs raw `with`.
- `CurrentHeap` enum tag dispatch: ~1-2ns overhead vs raw pointer + null check.
- `#[inline]` vs `#[inline(always)]` on HeapCore::alloc / AllocCore::alloc / alloc_small may make a difference (cargo currently inlines on release, but not aggressively enough).
- `maybe_decay_large_cache` hook on EVERY alloc/free even when `cached_bytes == 0` (early-exits via cmp, but cmp + branch is still there) — on the small path this hook should not exist at all.

**Files inspected this session (key):**
- `src/global/sefer_malloc.rs` (132-196): GlobalAlloc impl, dispatch via `current_for_alloc()`
- `src/global/tls_heap.rs` (175-220): `current_for_alloc` (try_with-based) + `CurrentHeap` enum
- `src/alloc_core/alloc_core.rs` (1771-1860 alloc_large, 1930-1948 maybe_decay): hot paths
- `src/alloc_core/numa.rs` (46): bind_segment seam (Windows VirtualAllocExNuma routing)
- `crates/vmem/src/lib.rs` (88-220): Reservation + new from_raw_parts
- `crates/numa/src/lib.rs` (640+): Windows reserve_aligned_numa rewrite
- `tests/alloc_core_reentrancy.rs` (40-65): M5 test, fixed counter to thread-local
- `benches/global_alloc.rs` (38-90): bench harness, OPS=1024 per iter

**Timers active:**
- babysit cron — None (deleted earlier when TaskList emptied; not re-armed for #101)
- CI watches running in background — completed already, results captured

**Repo state:**
- Branch `main`, 16+ commits past origin earlier, all pushed up to `845560f`.
- 1 modification in working tree: `src/alloc_core/alloc_core.rs` — sh-agent #101 in flight modifying for optimization attempts.
- `docs/checkpoints/*` untracked (user territory, never staged).

## Active goal

(none — `/goal` was cleared after large-cache campaign; #101 is single in-flight task)

## TaskList

### in_progress
- #101 Flamegraph small-class hot path + radically accelerate vs mimalloc (target: 1.5-2× speedup)  (blockedBy: —)

### pending
(none)

### recently completed
- #100 Fix M5 reentrancy regression (thread-local counter)
- #99 NUMA Phase 4 — cloud multi-socket VM pre-release recipe
- #98 NUMA Phase 3 — Hyper-V virtual NUMA experiment + dev recipe doc
- #97 NUMA Phase 2 — QEMU `-numa` Linux CI job (weekly schedule)
- #96 NUMA Phase 1 — mock-shim feature in numa-shim + per-target unit tests
- #95 Investigate ~3× perf regression: 4-16 MiB cache hit (45->148 ns)
- #94 Investigate large_alloc_free/64MiB ~140us cache miss
- #93 Performance verify + docs update — closing large-cache redesign goal
- #92 Large-cache Phase 3 — optional background scavenger thread (stub)
- #91 Large-cache Phase 2 — lazy decay (10%/s, configurable)

(deleted earlier in session: #83 + 8 other completed cleanup-deletes)

## Decisions

- **Large-cache: byte-budget admission, no per-span cap.** Per-span cap (`MAX_CACHED_LARGE_BYTES=64MiB`) was "artificial disability" — 30 GB span never cached on 64 GB / 32-core box. Chose process-wide / per-shard byte budget instead (default `None` = unbounded, client controls via `SEFER_LARGE_CACHE_BUDGET`).
- **Lazy decay model: exponential 10%/sec toward live + headroom**, not linear. Self-damping, no oscillation. Headroom default 256 MiB (anti-thrashing pad). All knobs env-overridable.
- **Phase 3 (background thread) = stub.** Real spawn requires Mutex refactor + HeapRegistry::for_each + safe spawn timing + TSan; deferred. Mode-selector and warning are live; full impl in follow-up.
- **M5 test: thread-local counter, not process-global.** Process-global counter caught background allocs from std/glibc/test-harness on OTHER threads (CI flaked while local PASSED on same commit). Thread-local Cell is the correct scope for M5 invariant.
- **#83 fix: add `Reservation::from_raw_parts` to aligned-vmem.** Allows numa-shim to call `VirtualAllocExNuma` directly and adopt the result into RAII handle. Validated by 4 MiB x 1024-page fault-in x 8 repeat test, PASSES on `windows-latest` CI.

## Open questions

- **#101 outcome**: unclear what cardinal speedup is achievable. Hypothesis: 5-8ns saving via TLS `try_with → with` + `inline(always)` is realistic; 1.5-2× speedup requires deeper redesign (perhaps mimalloc-style inline TLS free-list pointer that bypasses HeapCore dispatch entirely). Agent will report honestly.
- **Phase 3 background scavenger thread**: deferred indefinitely. Real implementation needs ~3-day effort + TSan validation. No user request yet to revisit.
- **Hyper-V virtual NUMA recipe (#98)**: draft, verification log is empty. Needs maintainer with Hyper-V Pro host to fill in.

## Repo state

```
M src/alloc_core/alloc_core.rs
?? docs/checkpoints/2026-06-26-1230.md
?? docs/checkpoints/2026-06-28-campaign-complete.md
?? docs/checkpoints/2026-06-28-highload-hardening-tasks.md
?? docs/checkpoints/2026-06-28-large-cache-redesign.md
?? docs/checkpoints/2026-06-28-oss-ready.md
```

(`src/alloc_core/alloc_core.rs` modification = sh-agent #101 in flight. Other untracked = checkpoint files, user's territory.)

```
845560f test(numa-shim): use runtime page_size() — fixes macOS aarch64 (Apple Silicon)
c85e872 chore(numa-shim,vmem): cargo fmt after #83 + Windows/macOS CI add
93e10b7 ci: numa-shim per-PR coverage on real Windows + macOS kernels
2c42873 feat(numa-shim,vmem): Windows VirtualAllocExNuma direct path — close #83
2ad686a docs(numa): cloud multi-socket pre-release gate recipe (#99)
687e7bf docs(numa): Hyper-V virtual NUMA dev recipe — Windows release gate (#98)
1d19a83 ci: numa-real-kernel job — weekly + on-demand env-guarded tests (#97)
c1ecc3f test(alloc-core-reentrancy): switch M5 counter to thread-local (#100)
d1dc9c2 feat(numa-shim): mock-shim feature for per-target CI coverage (#96)
0dfe370 docs(numa): research — testing the multi-socket code path without multi-socket hardware (#89)
```
