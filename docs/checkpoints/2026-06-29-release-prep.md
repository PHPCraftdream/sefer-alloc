# Checkpoint — 2026-06-29 release-prep

## Session summary

Continuation of `2026-06-29-fastbin-complete` (the previous arc closed the
fastbin/tcache project). This session is the **first-release readiness arc** for
sefer-alloc 0.1.0 — driving the crate from "fastbin complete" toward
publishable-to-crates.io quality.

The arc had **four substantive phases**, each with zero-trust review:

**Phase 1 — release prep hygiene (commits `d29e05a`, `0555796`).** Found three
publication blockers via systematic audit: (a) residual Cyrillic «СУБД» in
`Cargo.toml` + 2 code-comment files missed by the prior translation pass
(683d62f); (b) `[Unreleased]` CHANGELOG section was not empty (workspace
extraction notes belonged in 0.1.0); (c) workspace publish order — sefer-alloc
depends on path-crates (`sefer-region`, `aligned-vmem`, `numa-shim`,
`malloc-bench-rs`) that are not yet on crates.io. Fixed (a) and (b) in code; (c)
is a process step the user must execute. Also wrote `docs/INTEGRATION.md`
consolidating attachment + the three operational knobs (size, period, trigger)
plus README pointer.

**Phase 2 — env-var config → const-builder API (commit `c4be627`).** User
pushed back on env-var reads in a library as anti-pattern. Designed a
`LargeCacheConfig` const-fn builder accepting all 5 knobs (budget / headroom /
decay-interval / decay-rate / mode) via chained const-fn setters, with
`SeferMalloc::with_config(CONFIG)` as the entry point. Delegated to `@sh`
agent. Result: 13 files modified + 2 new (`large_cache_config.rs`,
`docs/INTEGRATION.md` already existed); env-var path completely removed (5
parse functions + `read_env_var_raw` + `read_env_budget_raw` deleted); plumbing
through `SeferMalloc` → `tls_heap::current_for_alloc_with_config` →
`HeapRegistry::claim_with_config` → `HeapCore::new_with_config` →
`AllocCore::new_with_config`. Tests rewritten to use direct config. Zero-trust
review caught one new clippy (missing `Default` impl) — fixed inline.

**Phase 3 — test coverage gaps (commit `6ebb0f4`).** Honest audit found 7
coverage gaps. Delegated to `@o46l` agent. Result: 2 new test files (9 tests
total). Critical was Gap 1 — `SeferMalloc::with_config` end-to-end via real
`#[global_allocator]` install — proving the full plumbing chain end-to-end.
Gap 7 (slot re-claim preserves first-claim config) intentionally skipped — not
observable from integration tests without new internal seams.

**Phase 4 — binary size investigation (UNCOMMITTED, ready for commit).** User
reported +22MB binary growth with sefer in their `he_repl` project. Reproduced:
1.17MB without sefer → 23.6MB with sefer = +22.4MB. First hypothesis (`#[inline(always)]`
campaign causing per-callsite duplication) was REFUTED: switched all 4 `GlobalAlloc`
methods + `current_heap` to `#[inline(never)]` — zero change in binary size.
Real diagnosis via `cargo-bloat` + `objdump -h`: `.text` = only 676 KiB, `.data`
= **22.4 MiB**. The 22 MB is in **`.data`**, not code. Root cause: `static
REGISTRY: Registry = Registry::new_zeroed()` in `src/registry/bootstrap.rs` —
`Registry { slots: [HeapSlot; 4096], ... }` where each `HeapSlot` embeds
`UnsafeCell<MaybeUninit<HeapCore>>` (~5 KiB per slot). Because
`HeapSlot::new_uninit()` sets `next_free: AtomicU32::new(NEXT_FREE_TAIL)` where
`NEXT_FREE_TAIL = u32::MAX` (non-zero), the loader CANNOT put the array in
`.bss` (zero-init virtual section), forcing the full 22 MB into `.data` on disk.

Intermediate experiment: reducing `MAX_HEAPS` from 4096 → 256 dropped binary
to 2.48 MB. User correctly pushed back — 2 MB still cosmically large for
systems programming. Final fix: **lazy-allocate the Registry** via
`aligned-vmem::reserve_aligned` on first call to `ensure()`. The
`static REGISTRY: Registry` becomes `static REGISTRY_PTR: AtomicPtr<Registry>`
(8 bytes of `.data`). First caller wins a CAS (`null → SENTINEL_INITIALIZING →
real_ptr`), allocates `size_of::<Registry>()` bytes via direct OS syscall
(VirtualAlloc / mmap, M5-clean — no `std::alloc`), initialises the struct in
place via `addr_of_mut!` writes (OS-zeroed pages + 2 non-zero field fixups:
`next_free` per slot + `free_slots`), then `Release`-publishes the pointer
and `mem::forget`s the reservation (process-lifetime leak, intentional).
Lost-race threads spin on Acquire load.

Delegated to `@sh` agent. Result: 3 files modified (`bootstrap.rs` rewritten,
`heap_slot.rs` got `#[allow(dead_code)]` on `new_uninit`, `tests/registry_basic.rs`
rewrote `bootstrap_is_idempotent` to assert pointer identity + count preservation).

**Final he_repl size: 1,201,152 bytes (1.15 MB), .data = 1024 bytes.**
Baseline without sefer is 1.17 MB. Sefer overhead now **+34 KB** instead of
+22 MB. All 178+ tests passing. Counterfactual on the rewritten idempotency
test is strong (re-init would reset count to 0 AND change the pointer).

Working tree at session close: **3 files modified** for the lazy-alloc refactor,
zero-trust review complete, **awaiting user "сделай коммит"**. 7 commits ahead
of origin (4 from prior arc + 3 this arc: `d29e05a`, `0555796`, `c4be627`,
`6ebb0f4`).

## Active goal

none (no `/goal` Stop hook)

## TaskList

(empty — checked TaskList; nothing tracked)

## Decisions

- **Env vars REMOVED entirely** (not gated, not deprecated). User: «пользователь
  уже откуда хочет — оттуда и прокинет». Const-builder is the single config path.
- **`SeferMalloc::new()` preserved** as alias for `with_config(LargeCacheConfig::DEFAULT)`.
  Behaviour byte-identical to pre-refactor when no env vars set.
- **Workspace publish order — process step.** sefer-alloc cannot be published
  before sub-crates (aligned-vmem, sefer-region, numa-shim, malloc-bench-rs)
  hit crates.io. Documented in initial audit, not code-fixable.
- **Lazy-allocate Registry** chosen over MAX_HEAPS reduction. The 4096 → 256
  intermediate experiment dropped binary to 2.48 MB but user correctly noted
  2 MB is still cosmic for systems programming. Lazy-alloc gets it to 1.2 MB
  (≈ baseline) AND keeps MAX_HEAPS=4096 runtime capacity intact.
- **`SeferMalloc` rename to `SeferAlloc` — RAISED, DEFERRED.** User noted the
  inconsistency with "100% Rust no C/C++ deps" branding (malloc-suffix is
  libc convention). Discussion stayed exploratory; no code change. Standing
  proposal: rename via @sh in fazes before crates.io publish.

## Open questions

- **When to commit Phase 4 (lazy-alloc).** User instructed «делай, потом
  прогони все нужные тесты». Tests are done. Awaiting explicit «сделай коммит»
  per project convention.
- **`SeferMalloc` → `SeferAlloc` rename** — user wants to think, no decision.
- **Loom test for the new bootstrap CAS** — pattern is textbook lazy-init
  (null → SENTINEL → real_ptr), no existing loom harness covers it. Defer
  as a follow-up phase. NOT a blocker for 0.1.0.

## Repo state

```
 M src/registry/bootstrap.rs
 M src/registry/heap_slot.rs
 M tests/registry_basic.rs
```

```
6ebb0f4 test(alloc-decommit): close coverage gaps for LargeCacheConfig API
c4be627 feat(alloc-decommit): replace env-var config with LargeCacheConfig const builder
0555796 docs(changelog): fold workspace-extraction notes into [0.1.0]
d29e05a docs: drop residual Cyrillic "СУБД" → "DBMS" in code comments
683d62f docs: translate all Russian-content markdown files to English
```

(7 commits ahead of `origin/main`: above 4 from this arc, plus `e9f4716`,
`5f10134`, `cd619ad`, `683d62f` from the prior fastbin arc. Push not done —
awaiting explicit user request.)
