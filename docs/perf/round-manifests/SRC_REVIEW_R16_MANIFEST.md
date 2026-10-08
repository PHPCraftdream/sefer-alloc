# Source review round 16 manifest — commits, impact & verdicts

**Scope:** the round-16 work commit after base `6a0d47f62b14eb724e027ab37054ad16037aa185` (the last R15 closing manifest). The work-commit row is derived from:

```text
git log --reverse --format="%H|%cI|%s" 6a0d47f62b14eb724e027ab37054ad16037aa185..6b27cd561611a200b127b41b81303386e43c04a2
```

This closing manifest is intentionally outside that range, avoiding self-reference. The round's later commit, which appends the independent verification of round 15 to `docs/reviews/2026-10-07-src-review-xs-round-15.md`, lands after this manifest and is docs-only. It is described in that report section, not in this table.

## §1. Work commits

| # | SHA | Commit time (`%cI`) | Prefix | Subject | Category |
|---|---|---|---|---|---|
| 1 | `6b27cd561611a200b127b41b81303386e43c04a2` | `2026-10-08T07:05:58+02:00` | `docs` | `record src review round 16` | oxx review report, three P4 findings (correctness items 174–176), item 22 closure, item 154 evidence, `[A]` census correction, perf hypotheses 83/84, changelog; no runtime source change |

## §2. Default-feature and measurement impact

- **Net `production` impact: none — review only.** `production` composition, defaults, dependency versions and `Cargo.lock` are unchanged; no `src/`, `tests/`, `Cargo.*`, `scripts/` or `.github/` file changed. This round has no `perf(runtime)` or `perf(opt-in)` commit.
- No wall-clock, Ir, RSS, promotion or GO/NO-GO result is claimed. Perf items 83/84 are unmeasured hypotheses. The witness's "Large shrink moved" output is a structural observation, not a performance number.
- Raw perf logs committed: **0 files, 0 bytes**. No performance report or summary CSV is owed, because no verdict rests on a measured performance number.

## §3. Findings and final verdicts

| Item | Final verdict | Evidence / limit |
|---|---|---|
| R16-01 / correctness item 174 | **OPEN, P4** | Source-confirmed path and `git log -S` origin (`2da9a941`); unreachable without prior metadata corruption |
| R16-02 / correctness item 175 | **OPEN, P4** | CI logs: macOS arm64 job ran the R14-01 harness 3/3 at `714ea5b7`, 0 at `6a0d47f6` |
| R16-03 / correctness item 176 | **OPEN, P4** | Latent; maximum class count is 58 today |
| Correctness item 22 | **CLOSED/SUPERSEDED** | `RemoteFreeRing`/`DrainHeadPublish` absent from `src/`; closure record in `docs/correctness-open-items/RESOLVED.md` |
| Correctness items 172/173 (R15-01/02) | **OPEN, unchanged** | Re-confirmed by a temporary R16 witness: panic tail 1/0 drops; reserved +1 / released 0 |
| Correctness item 154 | **OPEN, evidence only** | New prose examples; trigger unchanged |
| Perf items 83/84 | **OPEN `[L]`, hypotheses** | Not measured |
| All other items in both indexes | **LEAVE** | Report §5 lists every ID |

## §4. Final verification

- Temporary witness (`tests/zz_r16_review_witness.rs`, removed before commit): **4 passed** under `--features "production internals bench-internals experimental"`. Its source is in the report's appendix A.
- `r14_sidecar_owner_capability_negative`, `r14_shard_late_tls_teardown`, `r14_shard_prune_work_bound`: **9 passed** on the unmodified tree. Three counterfactual mutants each produced the expected failures and were restored with `git checkout --`.
- `cargo test --locked --features "production internals alloc-stats bench-internals batch-api" --test no_stale_doc_references -- --test-threads=1`: **32 passed** after the index edits.
- `node scripts/verify-commit-prefixes.mjs 6a0d47f62b14eb724e027ab37054ad16037aa185..HEAD`: **PASS**. `git diff --cached --check`: clean.
- Correctness census after the round: ACTIVE **10** numbered cards; `TRACKED_*.md` **141** numbered records and **141** lookup rows to `TRACKED_*.md`.
- Not run: Miri, Loom, Kani, TSan, release profile, MSRV, full `npm run check`, clippy/rustfmt/rustdoc, feature powerset, benchmarks, NUMA, non-Windows hosts. Nothing was pushed.
