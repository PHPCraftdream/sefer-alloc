# R6-T12 owner-drain performance preregistration

## Status and identity

This is a preregistration, not a result. No performance evidence has been
collected and there is no GO claim. Baseline identity is `HEAD` at immutable
SHA `d3941723`. Candidate identity is the immutable SHA to be supplied by the
orchestrator before measurement; do not substitute a mutable branch name.

## Production expectation and method

The production expectation is reduced owner-side drain work from hoisting
segment context once per scan, retiring accepted live credits in one
`sub_live(k)`, and replacing per-record division with a reciprocal multiply.
This is an expectation only; benefit must be measured.

Run matched A/B measurements at the same `HeapCore` layer, with the same
production `internals bench-internals` configuration, toolchain, runner,
protocol, and workload state for both immutable revisions. Measure these six
requested cells:

- `r19_prf01_publication`
- `seg_cycle_decommit_256k`
- `multiseg_cold_256k`
- `growth16_n`
- `cold_alloc_only_256x16b` (control)
- `dealloc_free_only_16b` (control)

For `growth16_n`, include the corresponding `_2n` and `_4n` sizes and inspect
the growth slope, not just one point. Report per-cell A/B measurements and
uncertainty; do not infer benefit from source structure or a single aggregate.

## Acceptance

Control-cell regressions must be no greater than 0.5%. Any claimed benefit on
the requested production cells must be present in measured evidence and
reported with uncertainty. Regressions, inconclusive results, or absent
measurements do not pass. Evidence is deferred to CI / the designated bench
runner; this document records no result and authorizes no GO decision.

## Correctness and protocol boundaries

Existing exact-count correctness oracles remain unchanged. The new divisibility
test is a conformance test: it agrees with modulo and also passes the old
modulo implementation; a rounded-down reciprocal mutant should fail at true
multiples that its quotient misses. It is not an optimization-benefit oracle.
The live-count batching test is likewise an equivalence oracle and passes the
old sequential decrement implementation; extra or missing decrements fail its
first post-drain live-count assertion. The synthetic magazine-rejection case
remains covered by the existing `r6_terminal_owner_drain` seam; no unsafe
publication from a locally freed allocation is introduced.

No publication protocol change is proposed, and no Loom addition is proposed.
During the drain, `RouteScan::next_cut` / `cut.pop` inspect and consume the
producer's descriptor-only records; observer callbacks record numeric counts
only. Reservation finalization occurs after the batched credit retirement.
The owner lease excludes owner-side readers during the scan, so the
intermediate live-count values from the batch are not observed by owner-side
readers. Existing producer/descriptor synchronization remains unchanged.

No changes to `OPEN_ITEMS` are made by this preregistration.
