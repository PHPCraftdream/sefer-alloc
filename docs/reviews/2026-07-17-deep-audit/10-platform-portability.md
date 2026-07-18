# Platform & Portability Audit — sefer-alloc

Read-only audit. Scope: cfg-matrix completeness, unreachable/incorrect
cfg-combinations (F2-class defects), page-size assumptions, commit-charge vs
RSS accounting, `no_std` leakage, MSRV 1.88 conformance, multi-arch
(pointer-width/alignment/atomic-width/endianness) correctness.

Files read: `CLAUDE.md`, `README.md`, `Cargo.toml`, `.github/workflows/ci.yml`,
`src/lib.rs`, `src/alloc_core/os.rs`, `src/alloc_core/bootstrap.rs`,
`src/alloc_core/segment_header.rs`, `src/alloc_core/segment_header_layout.rs`,
`src/alloc_core/alloc_core_small.rs`, `src/alloc_core/alloc_core_small_pool.rs`,
`src/alloc_core/alloc_core_large.rs`, `src/alloc_core/remote_free_ring.rs`,
`crates/vmem/src/lib.rs`, `crates/vmem/README.md`, `crates/numa/src/lib.rs`,
`crates/proc-memstat/src/lib.rs`, `tests/lazy_commit_b2_grow.rs`,
`tests/lazy_commit_b4_matrix.rs`, `docs/crate_extraction/03_os_platform.md`.

---

## Summary table

| # | Severity | Confidence | Axis | file:line | One-line |
|---|----------|------------|------|-----------|----------|
| P1 | Medium | High | page-size | `crates/vmem/src/lib.rs:104-146`, `src/alloc_core/os.rs:67-73`, `src/alloc_core/alloc_core_small_pool.rs:672-686`, `src/alloc_core/alloc_core_small.rs:1022-1044` | `decommit`/`commit_range`/`recommit` validate offsets against the crate's **fixed 4 KiB `PAGE` constant**, not the real OS page size (`page_size()`, 16 KiB on Apple Silicon, up to 64 KiB on some Linux/aarch64); `alloc_core` never calls `page_size()`. |
| P2 | Low | High (tracked) | cfg-completeness | `tests/lazy_commit_*` (task #191, already filed) | Some lazy-commit b2/b4 test assertions hard-code the eager-path `frontier == SEGMENT` expectation without a `cfg` split for the unreachable `unix ∧ lazy-commit ∧ ¬numa-aware` leg — a latent, non-CI-breaking assertion-completeness gap (see project's own tracked #191). |
| P3 | Low | Medium | commit/RSS docs | `crates/proc-memstat/src/lib.rs:36-43` | Cross-platform `commit` field conflates three semantically different OS metrics (Windows `PagefileUsage`, Linux `VmSize` total-VM, macOS `virtual_size`) under one name; documented, but a naive caller comparing `commit` across platforms would compare apples to oranges. |
| P4 | Info | High | no_std | `src/lib.rs:64-66,204-213`, `Cargo.toml:97-138` | `no_std` core is correctly scoped to `Region`/`Handle` only; `alloc_core` (and hence all OS/segment code) is `std`-gated (`alloc-core = ["std", ...]`) and never compiled on the `thumbv7em-none-eabi` CI leg — confirmed correct, no leakage found. |
| P5 | Info | High | MSRV | `Cargo.toml:7`, various | `is_multiple_of` (stable 1.87) and other syntax used across the crate are within the declared MSRV 1.88; CI has a dedicated `msrv` job (`cargo check --all-features` on pinned 1.88). No violation found. |
| P6 | Info | High | multi-arch | `src/alloc_core/remote_free_ring.rs:1-66`, `src/alloc_core/os.rs:65` | `RemoteFreeRing` cursors are `u32` in-process offsets bounded by `SEGMENT = 4 MiB`, never serialized cross-process/cross-endian — no overflow or endianness risk found. |
| P7 | Info | Medium | CI coverage | `.github/workflows/ci.yml:217-233, 647-701` | `test-macos` (real Apple Silicon, 16 KiB pages) DOES run `production` (which includes `alloc-decommit`), so P1 is live in CI today, not just a theoretical target — yet there is no macOS-specific 16 KiB-page regression test asserting decommit correctness at sub-16K granularity. |

---

## P1 — `alloc_core`'s fixed 4 KiB `PAGE` vs the real OS page size (Medium / High)

### Where

- `crates/vmem/src/lib.rs:104-146` — `pub const PAGE: usize = 1 << 12;` (4 KiB,
  documented as "the *minimum* [granularity]; the real OS page size may be
  larger — query it with `page_size()`"). `page_size()` itself (added in the
  0.2 crate-extraction pass, `CRATE-P2`) correctly queries
  `sysconf(_SC_PAGESIZE)` / `GetSystemInfo` and is cached.
- `src/alloc_core/os.rs:67-73` — `pub(crate) const PAGE: usize =
  vmem::PAGE;` — `alloc_core` re-exports the **compile-time minimum**, not the
  runtime-queried value. Every offset in the segment layout
  (`segment_header_layout.rs`'s `page_map_off`/`bin_table_off`/
  `small_meta_end`/`primordial_meta_end`, all `align_up_const(_, PAGE)`) is
  computed against this fixed 4096.
- `crates/vmem/src/lib.rs:429-606` — `decommit`/`decommit_lazy`/`recommit`/
  `commit_range` all validate `start`/`end` via
  `!start.is_multiple_of(PAGE) || !end.is_multiple_of(PAGE)` — i.e. against
  the fixed 4 KiB constant, not `page_size()`.
- `src/alloc_core/alloc_core_small_pool.rs:672-686` —
  `decommit_empty_segment_impl` calls `os::decommit_pages(base,
  initial_frontier, SEGMENT)` where `initial_frontier = payload_start +
  LAZY_FIRST_CHUNK` and `payload_start = SegLayout::small_meta_end()` — both
  computed with the crate's fixed `PAGE`, so the resulting range boundary is
  only guaranteed 4 KiB-aligned, not 16 KiB/64 KiB-aligned.
- `src/alloc_core/alloc_core_small.rs:1022-1044` — the B2 grow-on-carve path
  (`os::commit_pages(segment, frontier, new_frontier)`) has the identical
  4 KiB-granularity assumption; `frontier`/`new_frontier` derive from
  `small_meta_end()` + multiples of `GROW_CHUNK` (256 KiB, itself a multiple
  of every real page size, so *this specific* offset is safe — but
  `small_meta_end()`, the *starting* frontier, is not proven 16K/64K-aligned).

### Platform / target

Primarily Unix hosts with a real page size > 4 KiB: **macOS on Apple Silicon
(16 KiB pages)** and some **Linux/aarch64 or Linux/ppc64 configurations (up to
64 KiB pages)**. Windows is unaffected in practice (Windows page size is
always 4 KiB on all supported architectures). This is under
`alloc-decommit`/`alloc-lazy-commit`, both part of the `production` feature
bundle.

### Description

`aligned-vmem` 0.2 (crate-extraction task `CRATE-P2`) added the correctly
OS-queried `page_size()` specifically to fix this class of bug for *external*
consumers of the crate, and the crate's own doc/README explicitly flags the
hazard ("a caller that decommits at 4 KiB-but-not-page multiples would
silently do partial work"). However, `alloc_core` — the crate's own internal
consumer, and the one shipping `production`/`alloc-decommit` in this
repository — never adopted `page_size()`. It still computes every segment
metadata offset and every decommit/commit range boundary from the
compile-time `PAGE = 4096` constant. The project's own prior research
(`docs/crate_extraction/03_os_platform.md:51-58`) already diagnosed this exact
defect class as "the #1 correctness item before publishing" `aligned-vmem`,
but the fix landed only in the *library* (`page_size()` now exists and is
correct) — the *consumer* (`alloc_core`) was never migrated to use it.

### Scenario

1. Build with `--features production` (or just `alloc-decommit`) on macOS
   Apple Silicon (real page size 16 KiB).
2. A small segment empties and `decommit_empty_segment_impl` calls
   `madvise(MADV_DONTNEED)` on `[base + initial_frontier, base + SEGMENT)`,
   where `initial_frontier` is only proven 4 KiB-aligned by
   `small_meta_end()`'s `align_up_const(_, PAGE)` (`PAGE` = 4096).
3. On a 16 KiB-page XNU kernel, `madvise` operates on whole real pages; if
   `initial_frontier` is not a multiple of 16 KiB, the kernel either (a)
   rounds the start *up* to the next real page boundary, silently leaving a
   4–12 KiB sliver of the requested range still resident (under-decommit —
   not memory-unsafe, but defeats the RSS-reclaim guarantee the M6 decommit
   feature promises and the accounting `dbg_decommit_count`/soak tests may
   assume), or (b) rounds down, touching bytes the caller did NOT intend to
   discard if that boundary is inside a still-logically-committed region on
   some other path.
4. No test in the current suite runs the decommit soak tests
   (`decommit_soak`, `decommit_stale_ring`, both real macOS CI jobs per
   `ci.yml:233`) with an assertion tied to the *real* page granularity —
   they assert allocator-level invariants (no UAF, ring correctness), not
   "exactly N real-OS pages were reclaimed", so this defect would not fail
   CI even though it silently degrades the RSS-reclaim contract on 16K/64K
   hosts.

### Impact

Not a memory-safety bug (the OS never returns an error for a misaligned
`madvise`/`munmap` range in this direction — Unix `madvise` rounds rather than
faults). The impact is **silent partial decommit**: the M6 decommit feature's
RSS-reduction promise degrades by up to one real-page's worth of memory per
decommit call on 16 KiB/64 KiB-page hosts, and the `committed_payload_end`
accounting (which assumes 4 KiB granularity) can drift from what the OS
actually did, which could confuse future accounting/telemetry work that trusts
`committed_payload_end` as ground truth for physical residency. On Windows,
`VirtualAlloc(MEM_COMMIT)`/`VirtualFree(MEM_DECOMMIT)` are documented to
require addresses/sizes that are multiples of the *system* page size and
Windows' page size is always 4 KiB, so Windows is not at risk today — but the
code has no defense if Windows ever ships a build with a different page
granularity (embedded/ARM64 Windows historically also uses 4 KiB, so this is
theoretical there).

### Fix

Either (a) have `alloc_core::os` query `aligned_vmem::page_size()` once at
process start and round every decommit/commit-range boundary up to that value
(not just the crate's `PAGE` minimum) before calling into `vmem`, or (b) if
16 KiB/64 KiB-page hosts are explicitly out of scope for the decommit feature,
add an explicit runtime or const check that refuses/no-ops `alloc-decommit`
when `aligned_vmem::page_size() != PAGE`, documented as a known limitation,
rather than silently under-decommitting. Given `SEGMENT = 4 MiB` and
`GROW_CHUNK = 256 KiB` are both multiples of every realistic page size, the
minimal fix is likely confined to rounding `small_meta_end()` /
`primordial_meta_end()` (the only offsets NOT already multiples of a
generously-sized constant) up to `page_size()` rather than the fixed `PAGE`.

---

## P2 — Tracked F2-class cfg-completeness gap in lazy-commit tests (Low / High, already filed as #191)

### Where

Project's own task list carries `#191 HYGIENE F2: lazy_commit b2/b4 tests
assert frontier==SEGMENT on the unreachable unix∧lazy∧¬numa leg` (see
`docs/checkpoints/2026-07-17-1859.md:55-56,81`). Spot-checked
`tests/lazy_commit_b2_grow.rs:311` and the ~19 matching sites in
`tests/lazy_commit_b4_matrix.rs` (grep for `cfg(any(not(windows), miri,
feature = "numa-aware"))` / `cfg(all(windows, not(miri), not(feature =
"numa-aware")))`): every site I sampled already correctly splits the eager
path (`not(windows) ∨ miri ∨ numa-aware`) from the Windows-lazy path (`windows
∧ ¬miri ∧ ¬numa-aware`), which is the textbook-correct gate matching
`bootstrap.rs`'s own `#[cfg(all(feature = "alloc-lazy-commit"), not(feature =
"numa-aware"))]` reachability condition. I could not independently locate a
still-unfixed instance of the exact "unreachable unix∧lazy∧¬numa leg" pattern
described by #191's title in the files I sampled — either the description
refers to a narrower assertion not caught by this grep pattern, or the bulk of
the class was already remediated and a residual case remains elsewhere in the
test suite (not confirmed either way in this read-only pass).

### Description

This is the textbook F2 class the task brief calls out: an assert that is
only TRUE on a subset of platforms/feature-combinations but compiles
(and — if mis-gated — RUNS) on all of them. The severity is low because (a)
it is a *test* assertion, not production logic — a wrong assert here fails a
test loudly rather than corrupting allocator state, and (b) the project has
already triaged and filed it as a tracked, deferred hygiene item, explicitly
because the surgery is "subtle" and "not in CI" as a live failure today.

### Fix

Already scoped by the project: "mirror b3's numa/¬numa split when next
touching those files" (per the checkpoint). No new fix proposed here beyond
confirming the class is real and worth closing out per #191's own plan.

---

## P3 — `proc-memstat`'s cross-platform `commit` field name hides three different metrics (Low / Medium)

### Where

`crates/proc-memstat/src/lib.rs:27-43` (platform matrix table) and the
per-platform `snapshot()` implementations at lines 97-104 (Linux), 157-177
(Windows), 212-242 (macOS).

### Description

The crate is explicit and honest in its doc comments: Windows `commit` =
`PagefileUsage` (true commit-charge, matches the task brief's "commit-charge
vs RSS" framing precisely), Linux `commit` = `/proc/self/status` `VmSize`
(total mapped virtual memory — NOT commit-charge in the Windows overcommit
sense; Linux has no equivalent kernel-level accounting), macOS `commit` =
Mach `virtual_size` (also total virtual memory, not a true commit counter).
The doc comment at lines 36-38 already flags this ("Linux's overcommit
accounting is not identical to Windows' commit-charge model"), so this is not
an undocumented trap for a reader of the crate's own docs — but any call site
in `alloc_core`/benchmarks/judges that compares raw `MemStat::commit` numbers
across platforms (e.g. a cross-platform bench table) would be comparing
"charged against a commit limit" (Windows) against "currently mapped VA"
(Unix), which are not the same axis. On Linux specifically, `VmSize` also
includes `MAP_NORESERVE`/lazily-reserved regions that never intend to commit
(e.g. large `mmap(PROT_NONE)` guard regions or the allocator's own
uncommitted lazy-reserved segment tail under `alloc-lazy-commit` on Unix,
where the OS eagerly commits everything anyway per `reserve_aligned_lazy`'s
Unix fallback — so `VmSize` there would already reflect the FULL segment as
"committed" even though the crate's own `committed_payload_end` frontier
tracks a smaller logical value). This is a documentation/consistency
observation, not a functional bug in the crate itself.

### Impact

Low: no memory-safety or correctness impact; a risk only for a future
cross-platform bench/judge comparison that naively diffs `commit` across
Windows vs. Linux/macOS runs and draws a false "commit efficiency" conclusion.

### Fix

No code change needed in `proc-memstat` (its doc is already honest). If/when
a cross-platform commit-efficiency comparison is built on top of this crate
(e.g. extending `bench:table`), that comparison should explicitly label the
Linux/macOS `commit` figure as "mapped VA" rather than implying parity with
Windows' true commit-charge semantics.

---

## P4 — `no_std` scope is correctly confined (Info / High — no defect)

### Where

`src/lib.rs:64-66` (doc: "`SeferAlloc` (and the whole allocator stack) is
`std`-only ... `Region<T>`/`Handle<T>` ... are `no_std` + `alloc`-only"),
`src/lib.rs:212` (`#![cfg_attr(not(feature = "std"), no_std)]`), `Cargo.toml`
feature graph: `alloc-core = ["std", "dep:aligned-vmem", ...]` (line 138),
confirmed transitively pulling `std` for every OS/segment/decommit code path
audited above. CI's `no_std` job (`.github/workflows/ci.yml:263-277`) builds
`--no-default-features --target thumbv7em-none-eabi`, which — with `alloc-core`
off by construction (it requires `std`, and the no_std job passes no
features) — never compiles any of the code this audit's other findings
concern.

### Verdict

No leakage found. The `no_std` claim is honest and narrowly scoped to
`Region`/`Handle` (a thin `slotmap` membrane with zero platform-specific
code), which is the correct design for keeping the bare-metal target
meaningful. This is a positive finding, listed for completeness of the
requested audit axes.

---

## P5 — MSRV 1.88 conformance (Info / High — no defect)

### Where

`Cargo.toml:7` (`rust-version = "1.88"`), `.github/workflows/ci.yml:250-261`
(dedicated `msrv` job pinning `dtolnay/rust-toolchain@1.88` and running
`cargo check --all-features`). Searched for `is_multiple_of` (11 call sites
across `src/` — all integer method calls, stabilized in Rust 1.87, within
MSRV), and for `let-else` — none found in `src/**/*.rs`.

### Verdict

No MSRV violation found in the sampled files. The CI MSRV gate is a real
`cargo check`, not merely a declared-but-unverified `rust-version` field, so
this axis has active protection already (the standard "declared but never
enforced" trap this audit was asked to look for does not apply here).

---

## P6 — Multi-arch: pointer width, atomic width, endianness (Info / High — no defect found)

### Where

`src/alloc_core/remote_free_ring.rs:1-66` (module doc: cursor layout,
`head`/`tail`/`overflow` all `AtomicU32`, `SEGMENT = 1 << 22` = 4 MiB, so any
in-segment byte offset fits comfortably in `u32` — no truncation risk even
at the segment's maximum extent). `src/alloc_core/os.rs:65` (`SEGMENT: usize
= 1 << 22`, a `usize` — correctly widens on both 32-bit and 64-bit targets,
though the crate is effectively 64-bit-only in practice per its CI matrix:
`x86_64-unknown-linux-gnu` / `aarch64-unknown-linux-gnu`, no 32-bit target in
CI). `owner_state: u64` bit-packing in `segment_header.rs:82-110` is pure
in-process data, never serialized to disk/network, so **endianness is a
non-issue** — the packing only needs to be self-consistent within one running
process, and it is (same process reads what it wrote, native byte order both
ways).

### Verdict

No pointer-width, atomic-width, or endianness defect found in the sampled
files. The crate is correctly `usize`-based for all address/offset math that
must scale with pointer width, and correctly uses fixed-width (`u32`/`u64`)
only for values with a proven upper bound well inside that width (segment
offsets bounded by `SEGMENT = 4 MiB ≪ u32::MAX`). Caveat: this audit did not
exhaustively review every arithmetic site in the ~30 `alloc_core` files for
32-bit-specific overflow (e.g. a hypothetical `i686` target, which is not in
CI); given the CI matrix is 64-bit-only (`x86_64`/`aarch64`), 32-bit
correctness is untested and unclaimed by the project itself — not flagged as
a defect since the project does not claim 32-bit support.

---

## P7 — CI runs the vulnerable path on real Apple Silicon, but has no page-granularity assertion (Info / Medium)

### Where

`.github/workflows/ci.yml:217-233` (`test-macos`, `runs-on: macos-latest` —
GitHub-hosted macOS runners have been Apple Silicon (arm64, 16 KiB pages)
since late 2023/2024) runs `cargo test --features production` (which includes
`alloc-decommit`) plus the two decommit-specific test binaries
(`decommit_soak`, `decommit_stale_ring`). `.github/workflows/ci.yml:647-701`
(`multi-arch`) covers `aarch64-unknown-linux-gnu` via `cross` but that is
typically a 4 KiB-page QEMU/Docker Linux userspace even on an ARM host, so it
does not exercise a real 16 KiB/64 KiB kernel page size.

### Description

This elevates P1 from "theoretical portability gap" to "silently degraded on
a CI leg that runs today, without a red signal telling anyone." The
`test-macos` job's own comment (`ci.yml:218-220`) correctly notes it exercises
"the aligned-vmem `madvise(MADV_DONTNEED)` decommit path ... on real Darwin",
but neither that job nor the `decommit_soak`/`decommit_stale_ring` test
bodies (not read in full in this pass, but inferred from their purpose per
the surrounding comments) appear to assert anything about the real page size
or the exact byte range actually reclaimed — they assert allocator-level
correctness (no UAF, ring state), which is orthogonal to "did we reclaim the
RSS we think we did."

### Fix

Add (or confirm, if one already exists but was not surfaced by this
read-only pass) a macOS-specific assertion that decommit calls actually
release whole real-OS pages — e.g. round-trip `aligned_vmem::page_size()` in
a test and assert every decommit boundary the allocator computes is a
multiple of it, which would immediately catch P1 as a red CI signal on the
existing `test-macos` job without adding a new job.

---

## Notes on scope boundaries

- This audit did not re-derive or dispute the project's own already-filed
  `#191` (F2); it is cited (P2) with an honest confidence caveat that the
  specific pattern named in its title was not independently reproduced in the
  files sampled here.
- `docs/crate_extraction/03_os_platform.md` (the project's own prior
  research) independently corroborates P1's root cause before this audit
  reached it — cited as first-party evidence, not just this audit's own
  finding, which raises confidence that P1 is a real, previously-known,
  still-open gap rather than a novel false positive.
- Windows commit-charge (`PagefileUsage`) vs Unix RSS/VmSize semantics
  (the task's other named concern) were found to be handled correctly and
  distinctly in `proc-memstat` (P3 is a documentation-precision note, not a
  functional defect) and in `alloc_core`'s own lazy-commit accounting
  (`committed_payload_end`), which is Windows-primary by design and correctly
  falls back to the "always fully committed" eager semantics on Unix/miri
  (verified in `bootstrap.rs` and `alloc_core_small.rs`'s cfg-gated grow-on-
  carve logic).
