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
  It exists because `snapshot()`'s all-zero fallback is indistinguishable from
  a genuinely tiny process, so a before/after pair whose second read failed
  reads as a complete release of memory. `SnapshotError` (`Unsupported` / `Os`
  / `Malformed`, `#[non_exhaustive]`) says which happened.
- **`MemStat`** — `rss: u64` plus `virtual_size`, `commit_charge` and
  `peak_rss`, each an `Option<u64>` that is **absent rather than substituted**
  where the platform API does not provide that quantity. Commit charge and
  virtual size are different axes and are never conflated: Windows reports
  commit charge (`PagefileUsage`) and no virtual size; macOS reports virtual
  size and no commit charge; Linux reports both.
- **`MemStat::charged_or_reserved_bytes()`** — the documented compatibility
  accessor for callers that want "whichever of the two this platform has",
  `commit_charge` first, then `virtual_size`, then 0. Its rustdoc states
  outright that the reading is platform-dependent, because the whole point of
  splitting the two axes was to stop that dependence from being silent.
- **Three platform backends, no C dependencies and no crate dependencies at
  all**: Linux parses `/proc/self/status`, Windows calls
  `K32GetProcessMemoryInfo`, macOS calls `task_info` with
  `MACH_TASK_BASIC_INFO` — each through locally-declared FFI.
- **Its own three-OS CI job** (`proc-memstat gates` in
  `.github/workflows/ci.yml`): fmt, clippy `-D warnings`, tests, rustdoc
  `-D warnings`, a link-smoke consumer built **outside** the workspace that
  actually links and calls `snapshot()`, and a packaging dry-run. Before this,
  nothing in CI built or tested this crate on any platform — which matters
  more than usual for a crate that is almost entirely platform FFI.

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

### Measured, decided, not changed

- **Fusing the Linux reader's three `/proc/self/status` scans into one:
  NO-GO.** The review asked for a measurement before optimizing. On the real
  1510-byte file the fused reader is 2.62x faster than three separate scans
  (543.2 ns → 207.5 ns) — but one whole `snapshot()` costs 17 125.3 ns,
  because the open/read/close round trip dominates, so the saving is 335.8 ns
  out of 17 125.3 ns, i.e. **2.0% of a call**. The backend keeps its three
  per-field reads. Evidence:
  `docs/perf/_raw_proc_memstat_p4_2_scan_cost.log` and
  `docs/perf/PROC_MEMSTAT_P4_2_SCAN_COST_summary.csv` in the repository;
  reproduce with
  `cargo run --release -p proc-memstat --example status_scan_cost`.

### Testing

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
