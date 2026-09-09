# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0 - Unreleased

First release. Everything below is new in this version; nothing has shipped
before it.

The crate existed in-tree before this release (extracted 2026-07-17 from six
duplicated FFI copies scattered across this repository's benchmark probes), so
the "Fixed" section below records defects found and closed **during** that
in-tree life and the pre-publication review — not regressions any published
version ever carried.

### Added

- **`snapshot() -> MemStat`** — a process's own memory, in bytes, from one
  read: resident set size, and — where the platform actually provides them —
  virtual size, commit charge, and peak RSS. Best-effort: returns an all-zero
  `MemStat` when no reading is available.
- **`try_snapshot() -> Result<MemStat, SnapshotError>`** — the fallible form.
  It exists because through `snapshot()` a failed read is indistinguishable
  from a genuinely tiny process to a caller reading `rss` alone — `rss` reads
  `0` either way, and the all-zero fallback erases the cause — so a
  before/after pair whose second read failed reads as a complete release of
  memory. `SnapshotError` (`Unsupported` / `Os`
  / `Malformed`, `#[non_exhaustive]`) says which happened.
- **`MemStat`** — `rss: u64` plus `virtual_size`, `commit_charge` and
  `peak_rss`, each an `Option<u64>` that is **absent rather than substituted**
  where the platform API does not provide that quantity. Commit charge and
  virtual size are different axes and are never conflated: Windows reports
  commit charge (`PagefileUsage`) and no virtual size; Linux (`VmSize`) and
  macOS (`virtual_size`) report virtual size and no commit charge.
- **`MemStat::charged_or_reserved_bytes()`** — the documented compatibility
  accessor for callers that want "whichever of the two this platform has",
  `commit_charge` first, then `virtual_size`, then 0. Its rustdoc states
  outright that the reading is platform-dependent, because the whole point of
  splitting the two axes was to stop that dependence from being silent.
  **Only Windows provides commit charge** (`PagefileUsage`); Linux (`VmSize`)
  and macOS (`virtual_size`) provide virtual size and report `commit_charge`
  as `None`. Those `None`s are structural, not unimplemented — neither
  `/proc/self/status` nor `MACH_TASK_BASIC_INFO` has a commit-charge counter,
  and reporting address space under that name would invert the very
  distinction the split exists to draw.
- **Three platform backends with zero CRATE dependencies** — no `sysinfo`, no
  `libc`: Linux parses `/proc/self/status`, Windows calls
  `K32GetProcessMemoryInfo`, macOS calls `task_info` with
  `MACH_TASK_BASIC_INFO`, each through locally-declared FFI. Deliberately NOT
  claimed as "no C libraries": two of the three backends do call native OS
  APIs. What the crate has none of is *crate* dependencies.
- **Its own three-OS CI job** (`proc-memstat gates` in
  `.github/workflows/ci.yml`): fmt, clippy `-D warnings`, tests, rustdoc
  `-D warnings`, a link-smoke consumer built **outside** the workspace that
  actually links and calls `snapshot()`, and a packaging dry-run. Before
  this, no CI ever ran this crate's OWN tests or gates on any
  platform — the crate was already being COMPILED (it is a workspace member
  and a dev-dependency of the root crate, so the workspace-wide
  `cargo clippy --all-targets` rows and the root `cargo test` rows compiled
  it everywhere they ran) — but nothing executed its own test suite, which
  matters more than usual for a crate that is almost entirely platform FFI.

### Fixed

- **macOS: the Mach self-port was bound as a function, which does not exist.**
  `mach_task_self()` is a macro over the global `mach_task_self_`, so the
  binding requested a symbol (`_mach_task_self`) no library exports. Now bound
  as the cached data symbol it is, read through a documented safe helper.
  Verified by symbol probe, not by reasoning: the built rlib requests
  `_mach_task_self_`.
- **Linux: a non-UTF-8 process name blanked every memory field.**
  `/proc/self/status` was read with `read_to_string`, which fails if *any*
  byte in the file is invalid UTF-8 — and the `Name:` line is a byte string
  (`PR_SET_NAME` bounds the name in bytes and can truncate mid-character). The
  caller's `unwrap_or_default()` then turned that failure into an empty file,
  so `snapshot()` silently reported zeros while `VmRSS`/`VmSize`/`VmHWM` were
  ordinary ASCII digits on their own lines all along. The parser now works on
  bytes and touches only the ASCII numeric fields it was asked for.
- **Linux: a live multithreaded process whose main thread had exited read
  as unmeasurable.** `/proc/self` resolves to the thread-group LEADER — the
  main thread — not to the calling thread. A multithreaded process may keep
  running after its main thread exits via `pthread_exit` (the use
  pthread_exit(3) NOTES documents for exactly this purpose): the kernel
  clears the exiting leader's `task->mm` at thread exit while the surviving
  threads keep the shared `mm` alive, and procfs prints the `Vm*` fields
  only for a task whose `mm` is still set. The leader's status file still
  EXISTS — it just carries no `VmRSS`/`VmSize`/`VmHWM` — so the backend
  returned `Malformed` and `snapshot()` fell back to all zeros: a live
  process with real memory read exactly like a complete release. The
  backend now reads the CALLING THREAD's own status,
  `/proc/thread-self/status` (Linux 3.17+; `/proc/self/status` stays as the
  fallback on kernels without it). One read, of one task: all threads share
  one `mm`, so the calling thread's figures ARE the process's, and nothing
  is ever summed across threads. Verified by a native lifecycle scenario
  (`tests/thread_leader_exit.rs`): the process's real main thread exits via
  the thread-exit syscall while a worker keeps running; on the pre-fix
  backend the same scenario returns `Malformed` and fails, with the fix the
  worker gets a real reading.
- **Linux: the kB parser accepted a numeric PREFIX instead of a validated
  integer.** `12oops`, `12.5`, and `1e6` were read as `12`, `12`, and `1` —
  and `12 MB` was silently read as 12 KiB — because the byte parser cut the
  value at the first non-digit and never checked the unit. The accepted
  grammar is now exactly `zero-or-more ASCII whitespace + integer + one-or-more ASCII whitespace + "kB" + trailing whitespace` (real kernels always print
  the tab before the unit; unit REQUIRED and case-sensitive: mainline
  kernels print these lines as a literal TAB after the prefix, the value
  right-aligned to width 8, and " kB" (v6.12 fs/proc/task_mmu.c), so
  requiring the unit rejects nothing a real kernel prints, while treating any other unit as kB is precisely the
  silent misread this forbids). Any deviation is rejected and surfaces as
  `Malformed` through `try_snapshot`. As of review round 3's P4-1 fix that
  "any deviation" holds for the OPTIONAL fields (`VmSize`/`VmHWM`) too: a
  present-but-invalid one (wrong unit, junk, or a value too large to
  represent even in KiB) is `Malformed`, never a silent `None`; only a
  genuinely absent optional field is `None`. (Sol-codex review round 2, P3-1.)
- **The ×1024 scale claim was moved from a live ratio band to an exact
  fixture oracle.** The old oracle compared two LIVE reads in a
  `kb*64..=kb*16384` band, which cannot tell ×1024 from ×2048 or from a
  ×1000 slip (for a 512-multiple kB figure such as 4096, both wrong
  multipliers pass every leg — demonstrated in review round 2's P3-2). The
  production conversion `status bytes -> Result<MemStat, SnapshotError>` was
  therefore extracted into a closed seam (`src/status_convert.rs`: the ONLY
  home of the `* 1024` scale and of the `Os`/`Malformed` classification),
  `#[path]`-included by `tests/status_convert.rs` and held against an
  immutable fixture byte-for-byte — exact bytes for all four fields,
  absent-optional handling, and the boundary `u64::MAX / 1024` KiB figure.
  The live test stays as an availability/smoke band only (the ±25%
  tightening was rightly reverted once already), its known blind spots are
  pinned AS PASSING so the imprecision is testable documentation, and the
  x1000 negative control now uses a page-granular kB figure (12_348,
  printable by a real 4 KiB-page kernel) with a paired positive control,
  replacing the old 12_345 input that no real procfs could print.
- **The UTF-8-regression guard and the error contract are tested THROUGH the
  real backend, not around it.** The parser-fixture test could not catch a
  regression of the production reader to `read_to_string` (every live test
  passes on an ASCII-named binary), and `Ok`/`Os`/`Malformed` were only ever
  constructed as bare enum values. Now: each error variant is produced
  against ONE injected result at the conversion seam — a failed read
  (`InvalidData`, the exact kind `read_to_string` produces on non-UTF-8)
  maps to `Os`, never `Malformed`; malformed content maps to `Malformed`;
  and the causation behind `snapshot()`'s all-zero fallback is proven
  deterministically rather than inferred from two independent live calls
  (Sol-codex round 2, P3-3). Additionally, a fresh-process Linux scenario
  (`tests/non_utf8_name.rs`) sets the calling thread's name to raw bytes
  containing `0xFF` via `prctl(PR_SET_NAME)` — bytes >= 0x80 pass through
  the kernel's `Name:` escaping RAW, so `/proc/thread-self/status` is
  genuinely invalid UTF-8 — and runs the real backend against it; under a
  `read_to_string` regression this test FAILS (verified by temporary
  revert), where every previous live test would have stayed green.
- **`commit` named three different quantities depending on the platform.**
  Split into `virtual_size` and `commit_charge` (see "Added"). **Breaking
  relative to the in-tree API**, and the reason this crate is published as
  0.1.0 with the split already done rather than shipping the ambiguity.
- **Linux: the reader was page-size-dependent.** It derived bytes from
  `/proc/self/statm`, which is expressed in pages, using a hardcoded 4 KiB —
  wrong on kernels with 16 KiB or 64 KiB pages (aarch64, ppc64).
  `/proc/self/status` reports kB regardless of base page size.
- **The "same-instant" snapshot claim was withdrawn.** The mechanism cannot
  keep it: the kernel documents RSS accounting as asynchronous and possibly
  inexact, procfs's own `task_mem` reads the totals in separate operations,
  and a file read is not one syscall. The documented promise is now
  "single-read" — one observation from one source in one go, which narrows the
  window without pretending to close it.
- **The unconditional "a probe must never take the process down" claim was
  qualified** with what it does and does not cover (allocation, reentrancy,
  OOM).
- **Linux: a parseable KiB figure whose ×1024 product overflows `u64` could
  panic (debug) or wrap to a fabricated near-zero byte count (release).**
  The strict parser rejected overflow at the KiB figure itself, but the
  subsequent scaling step was unchecked — `u64::MAX / 1024 + 1` KiB parsed
  fine and then overflowed on the way to bytes. All THREE scaled fields
  (`rss`, `virtual_size`, `peak_rss`) now go through `checked_mul(1024)` and
  report the overflow as `SnapshotError::Malformed`, the same content-defect
  classification as a figure the grammar rejects. Defensive hardening
  (review round 2, P4-1) — not a size any real Linux process reaches;
  fixtures pin the exact boundary (`u64::MAX / 1024` still succeeds, the
  next value up is `Malformed`) and the same check on the optional fields.
  Since review round 3's P4-1 fix, overflow at EITHER stage — the KiB parse
  itself or the ×1024 scale — is classified identically (`Malformed`) on ALL
  fields, closing the gap where a parse-stage overflow on an optional field
  collapsed to `None`.
- **Windows test fixture only: the cleanup `Drop`'s failure diagnostic could
  itself panic.** `tests/monotonicity.rs`'s `CommittedRegion` guard printed
  a `VirtualFree` failure with `eprintln!`, which Rust documents as able to
  panic on a broken stderr — and a panic inside `drop` during an unwind
  aborts the process, hiding the original assertion failure behind a bare
  abort. The diagnostic is now a raw `write_all` with its `Result`
  discarded; the happy-path `VirtualFree` check is unchanged (review round
  2, P4-3).
- **Linux: the error contract was non-monotonic — a MORE-broken status
  could turn an error back into success.** A present-but-invalid OPTIONAL
  field (`VmSize`/`VmHWM` with a wrong unit, junk, or a decimal run too
  large for `u64`) silently became `None`, while a SCALE-stage-overflowing
  one on the same field raised `Malformed` — so a parse-stage overflow
  (strictly more broken) regressed an error into a successful snapshot with
  the field missing. The parser now distinguishes absent from invalid
  internally via a three-state `KibFieldLookup` enum (`Absent`/`Invalid`/
  `Value`) — internal-only, no public API change: `None` now means exactly
  "the key is genuinely absent", and every present-but-invalid shape on any
  field this crate reads is `Malformed`, like the same defect on the
  required `VmRSS` (Sol-codex review round 3, P4-1).
- **Test fixture only: `prctl(PR_SET_NAME)`'s variadic zero arguments are
  now passed at full machine width** (`c_ulong`, not untyped zeros that
  default to `i32` through the variadic ABI), per prctl(2)'s CAVEATS.
  No crash was reproduced on tested hosts — this is a portability-
  correctness fix, not a behavior fix (Sol-codex review round 3, P4-2).

### Documentation

- **The macOS cached Mach self-port's SAFETY proof now names BOTH of
  libSystem's write points** — startup via `libSystem_initializer`, and the
  single-threaded post-fork child re-initialization (`_mach_fork_child()` →
  `mach_init_doit()`, Apple libsyscall `mach_init.c`). The getter was always
  sound — a by-value port-name read cannot race either write; the proof
  simply omitted the fork path. No data race existed or is claimed (review
  round 2, P4-2).
- **The rejected fused reader's contract now states its precondition
  outright** — distinct, non-overlapping prefixes — instead of asserting "a
  line matches at most one prefix" as if the signature guaranteed it. With
  duplicate prefixes a line is attributed to the first matching prefix only;
  that exact behavior is now pinned by a fixture in `tests/status_parse.rs`
  as a documented, discoverable limitation. Production never calls the
  fused reader (NO-GO measurement subject), so no metric was ever affected.
  The fused reader's own loop is unchanged since the measurement, but the
  SHARED field-value tail it calls now additionally validates grammar/unit
  (round-2 P3-1), so the recorded numbers are pinned to commit `3119719`
  and a current-HEAD re-run measures different code (review rounds 2 and
  3, P4-4).
- **The scan-cost measurement's wording now matches what it measured**
  (review round 2, P4-5 — labels only; nothing re-measured): the ~2.0%
  figure is labeled an ESTIMATE of the removable work's share (parse-only
  arms vs a separately measured whole `snapshot()`, not a full backend
  A/B); the summary CSV's `samples` column is renamed `batch_iterations`
  (one timed batch's iteration count, not independent timing samples); the
  example's self-consistency `assert!` is documented as exactly that; and
  the CSV now carries the parse's own share of a call (~3.17%, row
  `parse_share_real`) beside the ~2.0% fusing SAVES, which the concluding
  verdict row no longer conflates.
- **Remaining overclaims narrowed** (review round 2, P4-6): the
  all-zero-fallback wording now says precisely what is indistinguishable —
  a zero `rss` read alone; a normally-succeeding backend fills the `Some`
  fields its platform-matrix row requires even at zero, so the whole
  fallback shape never occurs on success — while keeping the
  erased-error-cause point; the CHANGELOG's CI history now says the crate
  was already COMPILED by workspace-wide CI rows before its own gates
  existed (the absence was its own tests, not compilation); and
  `tests/platform_contract.rs`'s header now says its re-parse is independent
  as a READ/fixture source, not as a parser (same `#[path]`-included
  module), and no longer claims the file moves no process memory at all.
- **The `rss` field's documented exclusions are now stated outright**
  (Sol-codex review round 3, P4-3): it is the platform's own RSS/working-set
  counter, not total physical footprint — the Windows working set contains
  only pageable allocations (nonpageable ones such as AWE and large-page
  allocations are excluded), and Linux `VmRSS` excludes explicit HugeTLB
  pages, reported separately as `HugetlbPages`; transparent huge pages are
  explicitly NOT excluded (they are part of RSS).
- **Grammar and label precision pass** (Sol-codex review round 3, P4-4):
  the kB parser's leading-whitespace grammar corrected to zero-or-more and
  pinned by a fixture; the stale `%5lu` claim replaced with the verified
  v6.12 width-8/tab fact (fs/proc/task_mmu.c); the smoke-band "same file"
  claim corrected to same-shared-`mm`-while-leader-alive; the scan-cost
  example's stale `/proc/self` denominator label corrected; and the
  historical gate figures pinned to commit `3119719` with an explicit
  current-HEAD caveat.

### Measured, decided, not changed

- **Fusing the Linux reader's three `/proc/self/status` scans into one:
  NO-GO.** The review asked for a measurement before optimizing. On the real
  1510-byte file the fused reader is 2.62x faster than three separate scans
  (543.2 ns → 207.5 ns) — but one whole `snapshot()` costs 17 125.3 ns,
  because the open/read/close round trip dominates, so the saving is 335.8 ns
  out of 17 125.3 ns, i.e. **2.0% of a call** (measured at commit
  `3119719`). The backend keeps its three
  per-field reads. Re-running the example on current HEAD is not expected
  to reproduce these exact numbers because the measured code changed after
  the measurement (round-2 P3-1 grammar validation inside the shared tail;
  round-3 P2-1 thread-self acquisition in the whole-call denominator).
  Evidence:
  `docs/perf/_raw_proc_memstat_p4_2_scan_cost.log` and
  `docs/perf/PROC_MEMSTAT_P4_2_SCAN_COST_summary.csv` in the repository;
  reproduce with
  `cargo run --release -p proc-memstat --example status_scan_cost`.

### Testing

- Legacy-kernel SKIP notices now bypass both child and parent libtest capture
  through direct stderr writes. A nested-runner regression test checks the
  final output without `--nocapture`; the temporary result file is removed.
- The non-UTF-8 path fixture now checks an ASCII control first and separates
  `create_new` from data writes. Only known filename-encoding rejections
  (Linux `EINVAL`, macOS `EILSEQ`) may skip the byte-read assertion, with a
  visible notice; other setup errors fail. Error classification has negative
  controls, and partial-file cleanup is attempted after write failures.
- Per-platform **contract oracles** — each backend's field-presence shape is
  asserted against what that platform actually provides, and byte-vs-KiB scale
  is checked three ways, so a reading that is off by a factor of 1024 fails
  instead of passing.
- **Exact-value parser oracles** over synthetic `/proc/self/status` buffers,
  including a deliberately non-UTF-8 fixture and an oracle proving the
  UTF-8 route this parser replaced would still lose every field.
- Scenarios that move process-wide memory counters run in **fresh processes**
  rather than sharing one, because the counters are process-wide: a lock would
  fix the schedule without removing the coupling.
