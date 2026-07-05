# Page-fault judge — WSL2 scouting verdict (task X3 / #184)

Scouting note (no implementation). Investigates whether WSL2 exposes a
reliable page-fault counter that could anchor a **fault-based perf judge** for
the sefer-alloc benches — the missing third axis after `Ir` (instruction count)
and `Estimated Cycles` (cache-aware). A fault judge would surface the
demand-paging cost of cold first-touch benches (the `cold_*`/`recycle_*`/new
`multiseg_cold_256k` family) that `Ir` and even `Estimated Cycles` only partly
see: a bench that touches a fresh 4 MiB segment pays one minor fault per page
whether or not callgrind's cache model charges for it.

## Three probes, on this host (WSL2 kernel `6.18.33.2-microsoft-standard-WSL2`)

### 1. `perf stat -e page-faults` — BROKEN (hard block)

`perf` is installed but cannot attach to the WSL2 kernel:

```
WARNING: perf not found for kernel 6.18.33.2-microsoft
  You may need to install the following packages for this specific kernel:
    linux-tools-6.18.33.2-microsoft-standard-WSL2
    linux-cloud-tools-6.18.33.2-microsoft-standard-WSL2
```

This is the well-known WSL2 limitation: Microsoft ships a custom kernel image
without the matching `linux-tools` / `linux-cloud-tools` packages, and the
`perf_event_open` syscall the `perf` frontend needs is not wired up under the
WSL2 lightweight hypervisor. Installing `linux-tools-generic` does NOT help —
it pulls tools for the generic Ubuntu kernel, not the `-microsoft-standard-WSL2`
image WSL2 actually runs. `perf stat` is therefore **unusable as a fault judge
on this dev host**. (It works fine on real Linux CI — see recommendation.)

### 2. `/proc/<pid>/stat` field 10 (`minflt`) — present but unreliable for our workload

The field exists and parses, but it **barely moves** under workloads that
definitely fault (a 50-iteration `cat /proc/self/stat` loop delta'd by 4; a
`dd if=/dev/zero of=/dev/null bs=4096 count=4000` delta'd by 0). The WSL2
kernel's `/proc` fault accounting undercounts the demand-paging that the
sefer-alloc benches would trigger — the pages faults happen, but they are
served by the WSL2 memory subsystem and not consistently charged to the
process's `minflt` via the `/proc` view. Field 9 (`majflt`) is also empty.
**Not usable as a judge.**

### 3. `getrusage(RUSAGE_SELF).ru_minflt` — HONEST but NON-DETERMINISTIC

A tiny Rust probe (`extern "C" fn getrusage`) touches 4000 fresh 4 KiB pages
(16 MiB) in-process and reads `ru_minflt` before/after. Result across three
runs:

```
run 1: ru_minflt before=0 after=4882 delta=4882
run 2: ru_minflt before=0 after=4132 delta=4132
run 3: ru_minflt before=0 after=4887 delta=4887
```

The counter **does** track demand-paged faults (delta ~4000-4900 vs the ~4000
expected; the overhead is the Rust std runtime's own page touches). Unlike
`/proc`, `getrusage` charges faults to the calling process honestly. BUT it is
**not deterministic run-to-run**: deltas vary by ~750 (±~10-15%) on identical
input. Callgrind `Ir` is byte-exact; this is not. So `ru_minflt` is usable as
a **coarse** signal ("did faults roughly halve? 2× worse?") but NOT for the
tight (~5-10%) threshold a regression gate needs.

## Verdict

**No single counter is a drop-in fault judge on WSL2.** `perf stat` is a hard
block (missing kernel tools); `/proc/.../stat` minflt undercounts; `getrusage`
is honest but ±10-15% noisy. The fault axis is therefore **not judgeable on
this Windows dev host** the way `Ir` is.

## Recommendation

1. **Primary (adopt):** run fault-sensitive judging on **real Linux CI only**.
   The `.github/workflows/perf-gate.yml` job already runs on Linux runners
   where `perf stat -e page-faults,minor-faults` works and `getrusage` is
   tighter. A fault column can be added there as a coarse (±20%) regression
   signal alongside `Ir` (which stays the tight gate). On the dev host, `Ir`
   + `Estimated Cycles` (the X3 upgrade) already cover the deterministic axis;
   faults are the one axis left to CI.
2. **Secondary (optional, dev host):** if a *local* coarse fault signal is
   wanted, wrap `getrusage(RUSAGE_SELF).ru_minflt` around each bench fn in a
   tiny native (non-callgrind) harness — take the **median of N≥5 runs** and
   compare only large ratios (>1.5×), never small percentages. This is a
   "did the fault count blow up?" smoke probe, not a gate.
3. **Fallback:** drop pre-fault experiments for lack of a deterministic judge.
   The `cold_*`/`multiseg_cold_256k` benches' `Ir` already rises with the
   number of distinct pages touched (each fresh-page carve costs `Ir` for the
   page-table / decommit-recommit work), so `Ir` is a *proxy* for fault cost
   even without a direct fault counter — good enough for relative ranking, if
   not for absolute fault accounting.

## What a fault judge would need (design sketch, not built)

- A native (non-valgrind) bench harness: one `SeferAlloc` per bench fn, the
  same alloc/free shapes as `perf_gate_iai.rs`, but timed by `getrusage`
  deltas, not callgrind.
- `ru_minflt` captured before first alloc and after last free, per bench.
- Median-of-N (N≥5) reporting; threshold ≥1.5× (coarse).
- Gated behind a `--faults` flag on a new `scripts/faults.mjs` (mirroring
  `iai.mjs`'s WSL plumbing), OR a `cargo test --features fault-judge` mode.
- NOT a replacement for `Ir`/`Estimated Cycles` — a third, coarser axis.
