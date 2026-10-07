# Source review round 14 manifest — commits, impact & verdicts

**Scope:** the Round-14 source-review report and its remediation commit, from
`7be5539629df34881840e89ccbc47bbcb43bb1bb` through
`62b16ce9e2bf1c54b79897b7c414782356f81b12`. The table is derived from:

```text
git log --reverse --format="%H|%cI|%s" 7be5539629df34881840e89ccbc47bbcb43bb1bb^..62b16ce9e2bf1c54b79897b7c414782356f81b12
```

The manifest is a separate closing artifact and is intentionally outside that
work-commit range; it is not a numbered review/remediation task. This avoids a
self-referential commit row while preserving an exact, reproducible range for
all work commits in the wave.

## §1. Work commits

| # | SHA | Commit time (`%cI`) | Commit prefix | Subject | Category |
|---|---|---|---|---|---|
| 1 | `7be5539629df34881840e89ccbc47bbcb43bb1bb` | `2026-10-06T22:45:52+02:00` | `docs` | `record src review round 14` | Source-review report and initial disposition; no code change |
| 2 | `62b16ce9e2bf1c54b79897b7c414782356f81b12` | `2026-10-07T07:18:40+02:00` | `perf(opt-in)` | `close src review round 14 findings` | Experimental shard work-bound optimization plus correctness, tests, and documentation |

## §2. Default-feature and measurement impact

- `production` feature composition, defaults, dependency versions, and
  `Cargo.lock` are unchanged. One production-reachable callsite moved to the
  owner-checked `RouteRegistration` API without changing allocator semantics.
- R14-02 changes only the opt-in `experimental` shard router. Its deterministic
  counter proves fewer per-claim checks when there are no token-backing deaths;
  this is not a latency, Ir, RSS, or speedup result.
- No criterion/iai/wall-clock/RSS gate ran. No promotion verdict or production
  performance claim was made.
- Raw perf logs committed: **0 files, 0 bytes**.

## §3. Findings and final verdicts

| Item | Final verdict | Evidence / limit |
|---|---|---|
| R14-01 | **CLOSED** | Sealed `SmallSidecar` owner mutators; public owner API lives on `!Sync` `RouteRegistration`. Three actual-crate compile-fail fixtures pin both private methods and the auto-trait boundary. |
| R14-02 | **CLOSED** | Death-hint-gated pruning; exact per-claim work counter proves zero checks across live-only cold misses and bounded cleanup after deaths. No timing or memory delta claimed. |
| R14-03 | **CLOSED** | Removed Rust tripwire marked historical; live MJS successor verified. Current state-machine document distinguishes physical reservation and route descriptor protocols. |
| R14-04 | **CLOSED** | Late TLS cannot acquire an unrecordable token; deterministic tests cover late insert, refused late bind, both TLS destruction orders, and live-bind release. |
| R14-05 | **CLOSED** | Correctness/perf indexes, tier labels, counts, archive placement, and cited evidence corrected without changing prior verdicts. |
| Miri residual / correctness item 171 | **OPEN, separate pre-existing issue** | Current nightly Miri reproduces Stacked Borrows UB in the existing `crossbeam-epoch 0.9.20` default-collector path via `tests/epoch.rs`; not attributed to R14 and not suppressed. Root-cause/configuration investigation remains open. |
| Accepted P1-box / correctness item 164 | **OPEN, unchanged** | Owner-accepted known defect; no Miri-clean or UB-free claim. |

## §4. Final verification

- `cargo test --locked -j 2 --all-features --tests -- --test-threads=1` — passed
  on a fresh isolated target.
- Feature-precise regression set — **62 passed**: `no_stale_doc_references` (32),
  R13 directory/shard controls (8), R14 tests (9), and R6/R11 route tests (13).
  The closure-pointer guard and all 32 `no_stale_doc_references` tests were rerun
  after correcting the item-170 pointer and passed.
- `node scripts/fmt-check.mjs` — 578 files, two chunks; passed.
- `cargo clippy --locked -j 2 --all-features --all-targets -- -D warnings` — passed.
- Warning-strict rustdoc — production public API and all-features private items;
  both passed.
- `node scripts/verify-dbg-hook-safety.mjs` — passed (141 reviewed safe, 30
  reviewed unsafe, 78 bench-gated safe hooks).
- Production allocator smoke — passed; summed 100,000 integers and stored 10,000
  map entries through `SeferAlloc`.
- Current-nightly Miri also reproduces the separately tracked existing
  `EpochRegion`/Crossbeam failure; see item 171 and the remediation receipt in
  `docs/reviews/2026-10-06-src-review-sol-round-14.md`.
