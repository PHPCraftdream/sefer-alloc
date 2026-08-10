# bench-scale-tool at the root `sefer-alloc` crate — design note

Status: **design note only — awaiting explicit sign-off before task #763
(implementation) proceeds.** Closes task #662's investigation scope.

## What this is NOT proposing

- **Not** removing, rewriting, or reducing any of the 25 existing
  `[[bench]]` entries (`Cargo.toml`) under criterion/iai. Those remain
  the project's canonical GO/NO-GO evidence — the entire "Phased
  delivery" section of `CLAUDE.md` (raw-log policy, summary-CSV policy,
  immutable-source-identity, per-round manifests, the path-activation-
  oracle and layer-correctness rules) governs them and is unaffected by
  this proposal.
- **Not** a 1:1 port of all 25 benches to bench-scale-tool. bench-scale-tool's
  own README explicitly scopes itself as a "pulse" tool, not a
  replacement for Criterion's statistical rigor ("For publication-grade
  numbers or subtle-regression hunting, reach for Criterion or a real
  profiler") — porting everything would just duplicate the canonical
  suite under a weaker measurement model for no benefit.
- **Not** a new per-PR CI gate.

## What this IS proposing

**One additive bench binary**, `benches/bench_scale_pulse.rs` (`harness =
false`, matching `benches/perf_gate_iai.rs`'s existing non-criterion-harness
pattern), covering a SMALL, curated subset of the hottest, most-frequently-
re-measured workloads already tracked as this project's own reference set —
not a new workload taxonomy. Candidate subset, pulled directly from
`docs/perf/IAI_BASELINE.md`'s own "Current reference for new work" table
(the workloads every perf-gate report already diffs against):

| Workload | Existing iai/criterion counterpart |
|---|---|
| small alloc/free churn | `small_churn_16b` |
| aligned churn | `aligned_churn_640b_a128` |
| large alloc/free cycle | `large_alloc_free_cycle` |
| realloc grow | `realloc_grow` |
| cold alloc/free (2 sizes) | `cold_alloc_free_256x16b`/`64b` |
| recycle alloc/free (2 sizes) | `recycle_alloc_free_256x16b`/`64b` |

Eight workloads, all already named entities in this project's own
canonical reference table — no new workload design needed, only wiring
the same allocation shapes through `bench_scale_tool::Harness` instead of
(or alongside) criterion/iai. This directly answers the risk the task
description names ("materially riskier/larger than the sub-crate tasks"):
the risk is contained by reusing already-vetted, already-understood
workload shapes rather than inventing new ones.

## Why add it alongside criterion/iai, not instead of

- **Different feedback latency.** Criterion's `production` sample runs
  and `npm run iai`'s WSL/valgrind round-trip are the rigorous,
  slower-cadence judge this project already trusts for GO/NO-GO
  decisions. bench-scale-tool's calibrate-once-then-fixed-N model
  (confirmed against its actual 0.1.0 source in this session's earlier
  work on `crates/region`, not assumed from its doc comments) is
  designed for a FASTER, lower-fidelity "did I just make this
  obviously worse" pulse check — the kind of thing a developer runs
  between edits, not the kind of thing that generates a citable
  `docs/perf/*_GATE.md` report. Neither replaces the other; they answer
  different questions at different points in the loop.
- **Zero governance overlap.** Because bench-scale-tool numbers are
  explicitly NOT proposed as gate-report evidence (see "What CI cadence"
  below — no CI job cites its output as a verdict basis), none of
  CLAUDE.md's raw-log/summary-CSV/immutable-source-identity rules apply
  to it. This keeps the addition genuinely low-governance-weight, matching
  its "pulse" scope.

## Compatibility with this project's "fast profile" bench convention

CLAUDE.md's own convention: "Benchmarks (criterion): fast profile —
`sample_size(10)` + short `warm_up_time`/`measurement_time` — the entire
suite in a few seconds." bench-scale-tool's calibrate-once-then-run-fixed-N
model is a genuinely DIFFERENT philosophy: no time cap, the iteration
count `N` is pinned per-workload in a manifest file and reused across runs
until explicitly recalibrated. This is not directly compatible with the
"fast profile, always" convention — but it does not need to be, because
this proposal does NOT add `bench_scale_pulse.rs` to the fast per-commit
loop `npm run check` already runs. It is an opt-in, separately-invoked
binary (`cargo bench -p sefer-alloc --bench bench_scale_pulse`), same as
`region_bench.rs` already is for `sefer-region` — not part of the fast
cycle, so the "entire suite in a few seconds" bar does not apply to it any
more than it applies to `npm run iai` today (iai is also excluded from the
fast loop for the same reason: different cadence, different tool, same
codebase).

A root-level `bench-iters.txt` manifest is needed (matching the
established pattern from `crates/region/benches/bench-iters.txt`, added
under task #656/#792). Unlike `sefer-region`'s crate-local manifest — which
exists specifically because a STANDALONE-extracted `sefer-region` tarball
has no ancestor `[workspace]`-declaring `Cargo.toml` to walk up to — the
root `sefer-alloc` crate IS the workspace root itself, so `Harness`'s
default ancestor-walk resolution (confirmed in `crates/region/src/
region_bench.rs`'s own comment, verified against bench-scale-tool 0.1.0's
real source: walks up from `CARGO_MANIFEST_DIR` for the nearest
`[workspace]`-declaring `Cargo.toml`) finds the root `Cargo.toml`
immediately with zero special-casing needed. The manifest still needs to
be committed and versioned (same self-healing-JIT-calibration caveat F14
already documented for sefer-region applies here too, at a lower stakes
level since the root crate is never independently `cargo package`d/
published outside the workspace the way `sefer-region` is).

## Proposed CI cadence: NOT per-PR, matching the existing scheduled-gate precedent

This project already has a precedent for expensive-but-valuable checks
going to a scheduled cadence instead of every PR: `cargo-hack`'s
feature-powerset job and `numa-real-kernel` both run weekly +
`workflow_dispatch`, explicitly to avoid taxing every push with a check
whose value is real but not urgent enough to justify per-commit cost
(see CLAUDE.md's own `cargo-hack` rationale: "~300 extra check
invocations... too much to add to the per-PR path without materially
slowing every PR"). bench-scale-tool's pulse numbers are lower-stakes than
`cargo-hack`'s compile-matrix coverage (nothing breaks silently if a pulse
check is stale for a week — it's advisory, not a correctness gate), so the
same reasoning applies at least as strongly. **Proposed: weekly +
`workflow_dispatch`, reporting only (no pass/fail threshold, no gate)** —
a human-readable pulse number in the job log, not a citable artifact.

## Open question for sign-off

The exact 8-workload subset above is a starting proposal, not fixed —
narrower (e.g. just `small_churn_16b` + `realloc_grow`, the two most
frequently cited in recent perf-gate reports) or broader (adding a
promotion-boundary or xthread-cross-thread workload) are both reasonable
alternate scopes. This note stops here per task #662's own instruction
("STOP after the design note — surface it to the user... before #763
proceeds") — awaiting explicit sign-off on the workload subset and CI
cadence before any implementation work (#763) begins.
