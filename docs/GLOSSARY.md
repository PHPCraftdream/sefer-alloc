# Identifier glossary

Comments and docs across this codebase reference several parallel ID systems —
invariant labels, phase codes, perf-arc milestones, optimisation candidates,
and task numbers. This table decodes them so a reader on docs.rs (who does not
have `CHANGELOG.md` or the `docs/checkpoints/` snapshots, both excluded from the
crates.io tarball) can follow a comment like `Э9 (P7.1, task #160)` without a
dead reference.

Each row is: ID family → one-line meaning → where to read more.

| ID family | Meaning | Read more |
|---|---|---|
| **I1–I6** | Region/Handle-face invariants (I1 resolution, I2 tombstone, I3 no-ABA, I4 accounting, I5 drop-once, I6 compaction). | [INVARIANTS.md](INVARIANTS.md) |
| **M1–M11** | Malloc-face (allocator) invariants: M1 validity, M2 no double-free/UAF, M3 no-overlap, M4 alignment/size fidelity, M5 reentrancy-freedom, M6 OS-return (decommit), M7 owner routing, M8 generational coherence, M9–M11 cross-thread reclamation/epoch obligations. | [INVARIANTS.md](INVARIANTS.md), [PHASE35_DECOMMIT_DESIGN.md](PHASE35_DECOMMIT_DESIGN.md) |
| **Phase 8–78** | The allocator build-out phases (Phase 8 segment substrate, 10 cross-thread free, 11 `GlobalAlloc` face, 35 decommit, 78 NUMA). Numbers are chronological milestones, not a version. | [ALLOC_PLAN.md](ALLOC_PLAN.md), [PLAN.md](PLAN.md) |
| **3b-I / 3b-II** | The two legacy concurrent-tier sub-phases: 3b-I (`arc-swap` RCU, zero-`unsafe`) and 3b-II (`crossbeam-epoch` per-slot atomics, the confined `hand` organ). | [PLAN.md](PLAN.md), `src/concurrent/` |
| **P0–P8** | The 0.3.x FASTBIN (per-thread tcache magazine) design sweep sub-phases (P0 baseline … P6 CAP sweep … P7/P8 batched carve). Dotted forms like **P6.1 / P7.1** are refinements within a P-phase. | [FASTBIN_DESIGN.md](FASTBIN_DESIGN.md) |
| **Ф0–Ф6** | PERF-3 run-encoded-freelist arc phases (Cyrillic "Ф" = phase): Ф1 storage/layout (`RunStack`, feature `alloc-runfreelist`), Ф2/Ф3 encode/drain, Ф5 go/no-go cost-benefit gate. | [design/RUN_ENCODED_FREELIST_PLAN.md](design/RUN_ENCODED_FREELIST_PLAN.md) |
| **PERF-3 / PERF-4** | Named performance arcs: PERF-3 = run-encoded freelist (the Ф-series); PERF-4 = decommit-churn / hysteresis before OS release (tasks #216/#217). | [design/RUN_ENCODED_FREELIST_PLAN.md](design/RUN_ENCODED_FREELIST_PLAN.md), `CHANGELOG.md` |
| **Э1–Э11** | ALLOC_BENCH perf-arc milestones (Cyrillic "Э"): e.g. Э1 bump-direct/batched carve, Э6 magazine oracle, Э9 the P7.1 refinement. Track individual measured wins in the single-thread bench arc. | [ALLOC_BENCH.md](ALLOC_BENCH.md) |
| **OPT-A…H** | The eight flamegraph-derived optimisation candidates (OPT-A skip re-stamp, OPT-B O(1) `contains_base` hash, OPT-C lazy stamp, OPT-E large cache, …), each with estimated/actual impact. | [PROFILE_FLAMEGRAPHS.md](PROFILE_FLAMEGRAPHS.md), [ALLOC_BENCH.md](ALLOC_BENCH.md) |
| **X2 / X7** | Cross-thread-hardening items; **X7** is the per-granule generational ring (the documented accepted counter-wrap exception on the free face). | [DURABILITY.md](DURABILITY.md), [FASTBIN_DESIGN.md](FASTBIN_DESIGN.md) |
| **W2–W7** | 0.3.0 work items (the "W" release-hardening series): e.g. W3 = the `alloc-stats` per-hit-counter gate, W4 = batched `carve_batch`. | `CHANGELOG.md` |
| **A1 / A2 / C2 / R1 / R2 / H1 / MUST-N** | 0.3.0 correctness/robustness work items in `HeapCore` realloc & cross-thread reclaim: A1/A2 reclaim-on-growth, C2 in-place regression fix, MUST-1 realloc-growth reclaim obligation, H1 (task #167) the opt-in `hardened` interior-pointer guard. | `src/registry/heap_core.rs`, `CHANGELOG.md` |
| **SEC-N** | Security-review findings (see `docs/security/`). | `docs/security/`, `CHANGELOG.md` |
| **task #NNN** | A session task id (#103–#217 and up). The authoritative log is `CHANGELOG.md`; task ids also appear in commit messages. Not resolvable from the published tarball alone — this is expected (the tracker is dev-only). | `CHANGELOG.md`, git history |

Note: `CHANGELOG.md`, `scripts/`, `.github/`, and `docs/checkpoints/` are
excluded from the published crates.io tarball (see `Cargo.toml` `exclude`), so a
docs.rs reader will not have them. This glossary is the in-tarball fallback for
decoding the ID references that remain in source comments.
