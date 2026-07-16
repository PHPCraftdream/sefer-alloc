# Crate-extraction research — lane 3: OS / platform / memory abstraction

Research agent report (read-only survey, 2026-07). Question: beyond the
already-extracted `crates/vmem` (`aligned-vmem`) and `crates/numa`
(`numa-shim`), what other OS/platform/memory abstractions are worth extracting —
or how should `vmem` itself be broadened — for testability and community use?

Surveyed sources: `crates/vmem/src/lib.rs`, `crates/numa/src/lib.rs`,
`src/alloc_core/os.rs`, `src/alloc_core/numa.rs`, `src/alloc_core/segment_header.rs`,
`src/alloc_core/segment_layout.rs`, `examples/first_alloc_process.rs`,
`scripts/first-alloc-bench.mjs`, `src/registry/*` (sidecar pattern), both
crate `Cargo.toml`s and their `tests/`.

---

## Headline answer first

**The single highest-value move in this lane is not a new crate — it is
EXTEND + PUBLISH `aligned-vmem`.** It is already crate-shaped (own
`Cargo.toml` with description/keywords/categories/docs.rs links, own
`tests/smoke.rs` + `tests/lazy_commit.rs`, zero dependencies, miri fallback,
`#![deny(missing_docs)]`), version `0.1.0`, unpublished. Its "why not
`region`/`memmap2`/`mmap-rs`" pitch in the module docs is genuinely correct:
no existing popular crate does *aligned anonymous* reservation +
decommit/recommit + incremental commit as a small no-deps tool. Everything
below in candidates 1–4 is best delivered as `aligned-vmem` 0.2 features, not
as separate crates. Candidate 5 (the memory-probe) is the one real *new*
crate in this lane.

---

## Candidate 1 — `aligned-vmem` itself: publish + broaden (EXTEND, not new)

1. **What / where.** `crates/vmem/src/lib.rs` (~1080 lines, one file, zero
   deps). API today: `reserve_aligned`, `Reservation` (RAII +
   `into_parts`/`from_raw_parts`), `release`, `decommit`, `recommit`, and —
   behind the `alloc-lazy-commit` feature (R7-B0) — `reserve_aligned_lazy` +
   `commit_range`. Three platform apertures: Windows
   (`VirtualAlloc`/`VirtualFree`, locally-declared FFI, no `windows-sys`),
   Unix (`mmap`/`munmap`/`madvise`, no `libc` crate), miri (`std::alloc`).

2. **Coupling.** Essentially zero. The only allocator-flavoured residue is
   naming/doc references to sefer-alloc and the deprecated `is_empty`
   carcass. `src/alloc_core/os.rs` consumes it purely through the public API
   (thin `Segment` newtype), proving the seam is already clean.

3. **Extraction effort + API broadening for community use.** Effort to
   *publish as-is*: near zero (README exists, tests exist). Effort to make it
   a genuinely competitive community crate (a 0.2): small-to-moderate. The
   concrete gaps a general audience will hit:
   - **`page_size()` is hardcoded 4 KiB.** Fine for x86_64/aarch64-4k, wrong
     on aarch64-16k (Apple Silicon macOS pages are 16 KiB!) and 64k-page
     Linux. The function-shaped constant was explicitly designed for this
     upgrade ("exposed as a function so a future version can query the OS
     without a breaking change") — do it: `sysconf(_SC_PAGESIZE)` /
     `GetSystemInfo`. This is the #1 correctness item before publishing;
     today `decommit` offsets that are 4 KiB-but-not-16 KiB multiples
     silently do partial work on macOS.
   - **Fallible API / error cause.** Everything returns `Option`. A
     community crate wants `Result<_, VmemError>` carrying
     `errno`/`GetLastError` (at least behind a feature) — "why did reserve
     fail" is the first question a user asks. Can be added non-breakingly as
     `try_reserve_aligned`.
   - **Ungate `commit_range`/`reserve_aligned_lazy`.** For an external
     audience, incremental commit is the *selling point*, not an experiment.
     Rename the feature to something crate-local (`lazy-commit`) or make it
     default; the "allocator's own feature bundle" gating convention
     (`alloc-*`) is sefer-internal vocabulary leaking into the crate.
   - **Decommit policy knob.** Unix path is hardwired `MADV_DONTNEED`; a
     `decommit_lazy` (Linux `MADV_FREE`, macOS `MADV_FREE_REUSABLE`) variant
     is what server allocator/arena authors actually want (cheaper reclaim,
     kernel takes pages under pressure). The honest XNU platform note already
     in the file becomes an API choice instead of a caveat.
   - **Huge/large pages.** `reserve_aligned_huge` (Linux `MAP_HUGETLB` /
     transparent-huge-page `madvise(MADV_HUGEPAGE)`, Windows
     `MEM_LARGE_PAGES`) — natural fit for the existing "aligned span for an
     allocator segment" story; can be a feature so the zero-deps core stays
     tiny.
   - **Protection changes** (`mprotect`/`VirtualProtect` guard pages) —
     *optional*; this starts to overlap `region`'s territory, so only add if
     a consumer (e.g. sefer's hardened mode) actually wants guard pages.

4. **Testability gain.** Publishing forces the crate to be tested on its own
   CI matrix rather than through the allocator; the real page-size fix makes
   the macOS/aarch64 CI legs meaningful. See also candidate 2 (mock layer)
   which multiplies this.

5. **Community value.** High and honest. The niche — *aligned anonymous* VM
   with commit-charge control, zero deps, miri-clean — is real and unoccupied
   (`memmap2` = file mappings; `region` = protection; `mmap-rs` = heavyweight
   general wrapper). Every arena, GC, custom allocator, and DB buffer-pool
   project reinvents exactly this over-reserve+trim + decommit code.

6. **Crate name / API sketch.** Keep `aligned-vmem`.
   `let r = aligned_vmem::reserve_aligned_lazy(4<<20, 4<<20, 64<<10)?;
   unsafe { commit_range(r.as_ptr(), 64<<10, 128<<10) };` — the sketch is the
   existing API; the 0.2 work is page-size honesty + error type + ungating.

---

## Candidate 2 — vmem test double: recording mock + fault injection (EXTEND vmem)

1. **What / where.** Two existing patterns that belong together inside
   `aligned-vmem` as a `mock`/`test-hooks` feature:
   - `crates/numa/src/lib.rs` `mod mock` (feature `mock`): a thread-local
     recording mock that replaces platform syscalls, letting any target
     (macOS, miri) assert the wrapping logic. Proven pattern, already tested
     (`crates/numa/tests/mock_dispatch.rs`).
   - `src/alloc_core/os.rs` `COMMIT_FAIL_ARMED` (R7-B2): a fault-injection
     atomic that makes the next `commit_pages` fail without touching the OS,
     used for commit-charge-exhaustion (OOM-path) tests. Today it lives in
     the *allocator*, above the vmem seam.

2. **Coupling.** The numa mock: none (it is already in the extracted crate).
   `COMMIT_FAIL_ARMED`: coupled only by placement — its logic ("fail the
   N-th commit") knows nothing about the allocator and would serve any vmem
   consumer testing its own OOM handling.

3. **Effort + API shape.** Small. A `mock` feature in `aligned-vmem`
   mirroring numa-shim's: thread-local call log (`Reserve{size,align}`,
   `Decommit{start,end}`, `Commit{..}`, `Release`), scripted failures
   (`mock::fail_next_commit(n)`, `mock::fail_next_reserve(n)`), backed by
   `std::alloc` like the miri path. sefer's `os.rs` then deletes
   `COMMIT_FAIL_ARMED` and arms the vmem hook instead. The R7-B4 plan item
   ("formalize into a richer fail-N-th-commit framework") lands here, once,
   for everyone.

4. **Testability gain.** Large — this is *the* testability item in this
   lane. Commit-failure paths are currently testable only via the ad-hoc
   atomic and only inside sefer; with the hook in vmem, sefer's tests get a
   supported API, vmem gets its own decommit/recommit/lazy-commit contract
   tests on every target (including miri, where the real syscalls don't
   exist), and every downstream consumer can test their OOM handling without
   actually exhausting commit charge.

5. **Community value.** Medium-high as a multiplier on candidate 1: "you can
   deterministically test your OOM path" is a differentiator no competing VM
   crate offers.

6. **Name / sketch.** Feature `mock` on `aligned-vmem`.
   `aligned_vmem::mock::fail_next_commit(1); assert!(!unsafe { commit_range(p, 0, PAGE) });
   assert_eq!(mock::drain(), vec![Call::CommitRange{..}]);`

---

## Candidate 3 — process memory probe: RSS + commit charge + peak RSS (NEW crate)

1. **What / where.** `examples/first_alloc_process.rs` lines 132–317:
   `rss_kib()`, `commit_kib()`, `peak_rss_kib()` — Linux (`/proc/self/statm`
   fields 1 and 0) and Windows (`K32GetProcessMemoryInfo`:
   `WorkingSetSize`, `PagefileUsage`, `PeakWorkingSetSize`) with a
   locally-declared `PROCESS_MEMORY_COUNTERS` FFI struct. The struct+extern
   block is **copy-pasted three times** in the one example — the code itself
   is begging to be a library.

2. **Coupling.** Zero. Pure "read my own process's memory counters" — knows
   nothing about allocators. The only reason it lives in an example is that
   the library is `#![forbid(unsafe_code)]` and examples may hold `unsafe`.

3. **Effort + API shape.** Low (a weekend crate). One file, three platform
   apertures (same shim pattern as vmem/numa: linux / windows / stub),
   dedup the FFI struct, add the documented-but-unwired Linux `VmHWM` peak
   read from `/proc/self/status`, and a macOS aperture
   (`task_info(MACH_TASK_BASIC_INFO)` — currently an honest `0`). API:
   a single snapshot call so RSS and commit come from the same instant
   (the example already needs exactly this for its apples-to-apples note).

4. **Testability gain.** Direct: the probe gets its own unit tests
   (monotonicity: touch 1 MiB → RSS grows; commit grows on reserve+commit
   without touch — which is precisely the RSS-vs-commit distinction the
   R6-OPT-A1 work needed a judge for). sefer's example shrinks to the
   scenario logic; the counters stop being triplicated untested FFI.

5. **Community value.** High. The standard answers today are `sysinfo`
   (heavy, whole-system, many deps) or hand-rolled `/proc` parsing. A
   zero-dep "my own process, RSS + **commit charge** + peak, one struct"
   crate fills a real gap — commit charge in particular is almost never
   surfaced by existing crates, and it is the metric that catches
   `MEM_COMMIT`-heavy designs that RSS hides (the exact ~125 MiB-vs-0.1 MiB
   gap this repo's own judge exists to see). Useful to anyone writing memory
   benchmarks, allocator CI gates, or leak canaries.

6. **Name / sketch.** `proc-memstat` (or `own-rss`).
   `let m = proc_memstat::snapshot(); // MemStat { rss, commit, peak_rss: Option<u64> } — bytes, same-instant`

---

## Candidate 4 — leaked zero-initialized VM sidecar (small EXTEND of vmem)

1. **What / where.** A pattern used three times: `src/registry/registry_chunk.rs`
   + `src/registry/heap_overflow.rs` (via `bootstrap.rs`), and
   `src/alloc_core/os.rs::reserve_directory_sidecar` (R7-A1). Shape: round
   `size_of::<T>()` up to PAGE, `reserve_aligned(size, PAGE)`, rely on
   OS-zeroed pages as the all-zero valid initial state (explicit
   `write_bytes(0)` under miri because `std::alloc` doesn't zero),
   `core::mem::forget` the reservation (process-lifetime), deref through one
   documented membrane function.

2. **Coupling.** The *pattern* is clean; the three in-tree instances each add
   their own ownership discipline (owner-only vs cross-thread CAS) which must
   NOT be extracted — that's the consumer's concern.

3. **Effort + API shape.** Very small: one function in `aligned-vmem` (or a
   20-line `vmem-static` micro-crate, but that's over-fragmentation):
   `pub fn leak_zeroed_pages(size: usize) -> Option<NonNull<u8>>` — reserve,
   guarantee zeroed on every backend (including the miri fallback — folding
   the `#[cfg(miri)] write_bytes` fix in once, instead of per-call-site),
   leak. Possibly a typed sugar `leak_zeroed::<T>() -> Option<*mut T>` with a
   `T: FromZeroes`-style documented contract (without the zerocopy dep —
   keep it an unsafe-contract doc, matching the crate's style).

4. **Testability gain.** Modest but real: the "is it actually zeroed under
   miri" invariant is currently re-proven at each of three call sites; one
   vmem-level test covers it forever.

5. **Community value.** Medium. Anyone building a `GlobalAlloc`, a signal
   handler, or pre-main machinery needs "static-lifetime zeroed memory that
   does not route through the very allocator I'm implementing". Niche but
   the exact audience that already wants `aligned-vmem`.

6. **Name / sketch.** Inside `aligned-vmem`:
   `let p = aligned_vmem::leak_zeroed_pages(size_of::<Big>().next_multiple_of(PAGE))?;`

---

## Candidate 5 — fresh-process first-touch benchmark harness (borderline)

1. **What / where.** The *methodology* of `scripts/first-alloc-bench.mjs` +
   `examples/first_alloc_process.rs`: per-sample fresh process (because
   first-touch/bootstrap cost is paid once per process and is invisible to
   Criterion/iai in-process iteration), machine-parseable `RESULT key=value`
   stdout protocol, min/median/max aggregation across N process launches.

2. **Coupling.** The runner is ~80% generic (build binary, run N times, parse
   `RESULT` lines, aggregate); ~20% sefer-specific (feature set, headline
   metric names, prose). The example is fully sefer-specific.

3. **Effort + shape.** To generalize honestly it should become a Rust crate
   (`#[first_touch_bench]`-less, plain: a lib that a user's tiny bin calls to
   emit `RESULT` lines from candidate 3's probe, plus a runner —
   `cargo-firsttouch` — that spawns it N times and aggregates). Moderate
   effort, and the value only materializes *after* candidate 3 exists (the
   probe is the reusable half).

4. **Testability gain.** For sefer: none beyond today. This is an export of
   methodology, not a de-risking of in-tree code.

5. **Community value.** Medium: "bench your library's per-process
   startup/first-touch RSS and commit" is a genuinely under-served
   measurement (every criterion-style harness gets it wrong for the stated
   reason), but the audience is narrower and the JS runner doesn't translate
   directly. Ship candidate 3 first; revisit this only if the probe crate
   gets traction. **Verdict: not now.**

---

## Honestly NOT extractable (allocator-specific)

- **`Segment` / `segment_base_of` / SEGMENT constant** (`src/alloc_core/os.rs`,
  `segment_layout.rs`): the newtype is 60 lines over `Reservation` plus
  sefer's diagnostic counters (`SEGMENTS_RESERVED_TOTAL`); `segment_base_of`
  is one mask. There is no crate here — vmem already *is* the extraction, and
  the residue is exactly the allocator-policy glue that should stay.
- **`SegmentHeader` / `PageMap` / `BinTable`** (`segment_header.rs`): the
  page-map is self-hosted metadata welded to sefer's size-class table, owner
  packing, generation protocol, and the `node` seam. A general "page-granular
  region tracker" reconstructed from it would share no code with it. Skip.
- **The directory/registry sidecars' concurrency protocols**
  (`heap_overflow.rs` CAS handoff, owner-only directory): the *reservation*
  pattern extracts (candidate 4); the protocols are the allocator.
- **The platform-shim cfg pattern itself** (`mod platform` per
  linux/windows/macos/miri/fallback, locally-declared FFI, per-site SAFETY
  wrappers): it is a *convention*, consistently used across vmem/numa/os.rs —
  worth a page in vmem's README as "how to add a platform", not a crate.
- **`src/alloc_core/numa.rs`**: already a 90-line compat shim over the
  extracted `numa-shim`; nothing left to extract. (`numa-shim` itself, like
  vmem, is publish-ready and unpublished — same "just publish it" note
  applies, though that is lane-adjacent.)

---

## Ranked shortlist

| # | Item | Kind | Effort | Testability gain | Community value |
|---|------|------|--------|------------------|-----------------|
| 1 | **`aligned-vmem` 0.2: publish + real `page_size()` + fallible API + ungate lazy-commit + `MADV_FREE` variant (+ optional huge pages)** | EXTEND + publish | S–M | High (own CI, macOS 16k pages become correct) | **High — unoccupied niche** |
| 2 | **vmem `mock` feature: recording mock + fail-N-th-commit fault injection** (absorbs `COMMIT_FAIL_ARMED`, mirrors numa-shim's proven mock) | EXTEND vmem | S | **Highest in lane** | Medium-high (differentiator) |
| 3 | **`proc-memstat`: same-instant RSS + commit-charge + peak-RSS self-probe** (dedups the example's triplicated FFI) | NEW crate | S | Medium (probe gets tested; example shrinks) | High (commit charge is unserved) |
| 4 | **`leak_zeroed_pages` in vmem** (registry/directory sidecar pattern, incl. the miri-zeroing fix, once) | EXTEND vmem | XS | Modest | Medium (GlobalAlloc authors) |
| 5 | First-touch process-per-sample bench harness | NEW (later) | M | None in-tree | Medium — defer until #3 exists |

**Bottom line:** one new crate (`proc-memstat`), everything else lands as
`aligned-vmem` 0.2 — extend and publish the existing crate rather than
fragmenting the seam into more micro-crates. `numa-shim` needs no new
extraction, only (eventually) publication.
