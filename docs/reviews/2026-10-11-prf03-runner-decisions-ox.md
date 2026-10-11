# PRF-03 gate runner — arbiter (ox) decisions, 2026-10-11

Scope: four design questions raised by the first runner delivery (preflight only, no executor).
Verified by the arbiter by reading the arm-A worktree only; the candidate-B worktree was not
opened. The binding text is Amendment A2 of `docs/perf/R19_PRF03_LARGE_ROUTE_RETAIN_PREREG.md`
(commit `09084fc3`); this file records the reasoning and the runner obligations.

## 1. featureExactReceipt

Decision: a primary-build (no alloc-stats) signature, not a B-side hit counter. Slot occupancy
alone is insufficient: the hit branch of `alloc_large` has two other exits (generation exhaustion
and a failed `register_payload`) that also take a slot, release it and reserve a fresh one; only
the hit exit has the signature (occupancy -1, reserve 0, release 0). `dbg_large_cache_hits` is
alloc-stats-gated and reads 0 in the primary build, so primary records `null`, never 0.
Observers available without alloc-stats: `dbg_large_cache_slot_sizes`, `live_route_census_for_test`,
`dbg_segments_reserved_total` / `dbg_segments_released_total`. The same oracle source is built
per arm as primary and as paired (plus alloc-stats); every structural field must match, paired adds
`tailHitDelta = 1`; a 12 MiB negative control must show the miss signature. The receipt is a file
reviewed personally by the orchestrator (binary hashes, raw outputs, source identity, enumeration of
all cache-arm exits with their signatures).

## 2. routedScans on non-regime cells

Decision: Amendment A2 items 1-2. `routedScans` and `prefill` are required integers only on
R1/R2/R3 rows (from the paired oracle); control, affected and mimalloc rows carry `null`, because
their bodies run only under Callgrind where no observer exists. Rejected: moving ~24 existing cells
into a support module (Ir/baseline drift), class-equivalent replicas (the value would belong to a
different body, i.e. invented), a placeholder 0. `dbg_routed_miss_scans` counts only routed
negative-directory Small-path scans under alloc-stats; sidecar drain/rescue scans
(`r18_sidecar_scan_bench`) are outside the frozen feature set and are reported as unobserved.
Judge change limited to this null/integer split and its selftest mutations.

## 3. Resolved configuration of the native axis

Decision: a twin binary (same source and arm, `production bench-internals internals`) runs
immediately before each native process and reads `dbg_current_large_cache_total_slots` /
`dbg_current_large_cache_budget` on every worker; the native row copies them with provenance
(twin runId, pid, binary hash, receipt hash). There is no public production surface for
budget/slots (`SeferAlloc::stats()` has conflicts, segment totals and claimed high-water only).
Config resolution carries no `bench-internals`/`internals` cfg, so the transfer is valid; building
the native binary with those features instead would change the measured binary. In-process asserts
through public APIs only: config-conflict and fallback-conflict deltas 0, maintenance off,
`heaps_claimed_high_water` delta == N, `segments_reserved/released_total` deltas 0 between the
ready and done barriers (native path-activation oracle: every timed cycle is a cache hit).

## 4. Incarnation delta

Decision: the live-route incarnation proxy is only a consistency check. `incarnationDelta` is
bracketed by two probe registrations on `RouteDirectory::global()` (public `register`, kind Large,
System-owned SEGMENT-aligned span, sentinel owner, dropped at once, outside the tail):
`after.incarnation() - before.incarnation() - 1`, i.e. A = 1, B = 0, arm-neutral and with no B-only
counter. `next_incarnation` is a process-global counter incremented on every `register` attempt,
so any foreign registration breaks A=1 / B=0 and fails loudly instead of passing silently.

## Related gap closed by A2 item 3

Non-regime iai rows cannot read resolved configuration from inside Callgrind; they take it from a
probe process per (source arm x heap-claim path), recorded in `config.resolvedProvenance`, with the
row's own pid parsed from that cell's retained valgrind log (no log means INCOMPLETE). mimalloc
rows record cache budget, slots and maintenance as `null`.
