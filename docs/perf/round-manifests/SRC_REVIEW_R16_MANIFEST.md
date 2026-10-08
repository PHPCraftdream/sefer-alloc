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

## §5. Follow-up: fixes, perf items 83/84 and index close-out (2026-10-08)

**Scope:** the follow-up work after the landing SHA of the review round (`fe171d9e74d7617476e4097d9f9ee9de251a02d0`, CI green there). The table is derived from:

```text
git log --reverse --format="%H|%cI|%s" fe171d9e74d7617476e4097d9f9ee9de251a02d0..9831adccf0e75b1335dc840d2f556b823ddfe49a
```

The close-out commit that records this section and the index moves is intentionally outside that range, avoiding self-reference. The range was pushed on the owner's command; its CI outcome is recorded in §6.

| # | SHA | Commit time (`%cI`) | Prefix (R30-12) | Category |
|---|---|---|---|---|
| 1 | `486f5ace76502e096fc8a46a7ba583f09779a210` | `2026-10-08T08:22:00+02:00` | `fix` | `EpochRegion::drop` isolates destructor panics per slot (`experimental` tier; correctness item 172) + regression test |
| 2 | `32cacc97c09ed541694733dc1ca03c8f7bafad3c` | `2026-10-08T08:22:00+02:00` | `fix` | `Drop for Segment` counts its release (correctness item 173) + regression test |
| 3 | `222e913ddb67062c39e74358d33652cc11c98871` | `2026-10-08T08:26:52+02:00` | `fix` | compile-time `SMALL_CLASS_COUNT <= 64` assertion (correctness item 176); no runtime code |
| 4 | `9df9f6b8c5bf09248bdfd73edecba256fb9c923b` | `2026-10-08T08:26:52+02:00` | `fix` | two release `.expect`s become aborts, no-panic contract text, lexical release-panic scanner in `no_panic_doc_accuracy` (item 174); later superseded by the mask commit |
| 5 | `6f568c2500e5f046cf121c08140ac5d068a85b20` | `2026-10-08T08:31:30+02:00` | `test` | r14 compile-fail harness gated on host == target (item 175); test-only |
| 6 | `265a571abc67323792334dc4dc5452fc59328d19` | `2026-10-08T08:32:36+02:00` | `docs` | test-file count sync |
| 7 | `c8b9344aca6eaf8b2f4f4171699e828aafc4ce62` | `2026-10-08T09:00:36+02:00` | `fix` | stale `src/` comment wording replaced, `allow(dead_code)` on `registry::segment_route` removed (item 154); no behaviour change (`fix` because the prefix verifier rejects `docs` with a non-comment `src` line) |
| 8 | `87f486a29ef8faad85345a4def61f092187a4f7d` | `2026-10-08T10:04:10+02:00` | `bench` | flush-all iai pair, overflow activation receipt, perf 84 gate tooling and report; no `src/` change |
| 9 | `b707d196d942e1c475ee9d21d48f762d003c3910` | `2026-10-08T10:04:11+02:00` | `perf(runtime)` | segment-mask root resolution in the overflow-flush and `flush_all_tcache` loops (perf item 84, GO); always-on `production` path |
| 10 | `549d07b648ad48ee8e9d929bfec40157626a80b8` | `2026-10-08T10:04:55+02:00` | `docs` | test-file count sync |
| 11 | `896d32a1e0a5121bf85217e47556d6a5bf5cf1ab` | `2026-10-08T10:16:30+02:00` | `checkpoint` | session checkpoint file under `docs/checkpoints/`; no code |
| 12 | `edf882cf0edf678544e1536dd1e98bea42b253f0` | `2026-10-08T13:10:50+02:00` | `bench` | Large shrink-in-place gate (perf item 83): iai rows, activation test, RSS probe, checked-table tooling, raw logs; NO-GO, `git diff -- src` empty |
| 13 | `9831adccf0e75b1335dc840d2f556b823ddfe49a` | `2026-10-08T13:12:20+02:00` | `docs` | test-file and example count sync |

### §5.1 Default-feature and measurement impact

- **Net `production` composition: unchanged.** `Cargo.toml` and `Cargo.lock` have no diff over the range; no feature, default, dependency version or `.github/` file changed. 22 `src/` files changed: the 17 files of the comment/`allow` commit `c8b9344a` (it includes `os.rs`, where `32cacc97` also added the `Drop`), plus `epoch_region.rs` (`486f5ace`, `experimental` tier), `size_classes.rs` (`222e913d`, compile-time assertion), `sefer_alloc/mod.rs` (no-panic contract text), `dealloc_own_base.rs` and `tcache_flush.rs` (`9df9f6b8`, then `b707d196`).
- **One `perf(runtime)` commit: `b707d196` (perf item 84, GO).** It changes an always-on `production` hot path (magazine overflow-flush and `flush_all_tcache`). Measured with deterministic callgrind Ir only on `HeapCore` (`production bench-internals internals`): overflow event -146 Ir (n17, 65270/65416), two events -292 Ir (n32, dose-response exact), flush-all of 16 blocks -260 Ir (53949/54209), every control within T = 12 Ir, independently re-measured by the integrator. **No wall-clock and no RSS claim.**
- **No other speedup is claimed.** Perf item 83 (Large shrink in place) is a pre-registered NO-GO with no runtime change (`edf882cf`); its 8 -> 6 MiB -694401 Ir and RSS 0.63x are facts about a reverted candidate, not about what ships.
- Raw perf logs committed in the range: **14 files, 159525 bytes** (perf 84: 5 iai + 3 mutant logs; perf 83: 5 iai excerpts + 1 RSS log), all under the 200 KiB per-file ceiling. Companion artifacts: perf 84 `_summary.csv` + `_identity.json`; perf 83 `_summary.csv` + gzipped `_identity.json.gz` (224311 B, tier 2 option b).

### §5.2 Final verdicts

| Item | Final verdict | Evidence / limit |
|---|---|---|
| R15-01 / correctness item 172 | **CLOSED** (`486f5ace`) | regression test; old loop red with `later_live_drops=0` |
| R15-02 / correctness item 173 | **CLOSED** (`32cacc97`) | regression test; red without the `Drop` increment (`reserved_delta=1 released_delta=0`) |
| R16-01 / correctness item 174 | **CLOSED** (`9df9f6b8`, then `b707d196`) | the release `.expect`s no longer exist; scanner pins the allowlist |
| R16-02 / correctness item 175 | **CLOSED** after two CI iterations (`6f568c25` rejected, then `eda25f97` + `b5247602`, §6) | first gate turned CI red; final gate green on macOS arm64 and cross aarch64 |
| R16-03 / correctness item 176 | **CLOSED** (`222e913d`) | compile-time assertion |
| Correctness item 154 (R15-03) | **OPEN, partial progress** (`c8b9344a`) | listed examples fixed; structural prose debt and the parent `allow(dead_code, unused_imports)` near `src/lib.rs:482` remain |
| Perf item 84 | **GO, SHIPPED** (`b707d196`) | `docs/perf/R16_PERF84_FLUSH_ROOT_MASK_GATE.md`; closure narrative in `OPEN_ITEMS_ARCHIVE.md` |
| Perf item 83 | **NO-GO (pre-registered gate), `[L]` revisit item** (`edf882cf`) | `docs/perf/R16_PERF83_LARGE_SHRINK_INPLACE_GATE.md`; `realloc_grow` control moved -14 Ir vs T = 12; revisit needs an owner decision on the observable shrink behaviour and a fresh pre-registration |

### §5.3 Verification and limits

- Each code change was reviewed line by line by the integrator and its tests rerun natively; where a runtime test applies it was checked red against the old code (items 172, 173) or against a mutant (item 175's `pub fn prepare`, perf 84 and perf 83 mutants, restored by copy-back and verified). Perf 84 and perf 83 numbers were re-measured independently with a forced rebuild (a first perf 84 re-measurement was discarded because cargo had not rebuilt after a copy; the second matched).
- Linux `clippy -D warnings` over `--all-features`, `production,internals` and `production,bench-internals,internals,alloc-stats` (all targets) found one `needless_late_init` in the perf 83 RSS probe, invisible to the Windows clippy; it was fixed before the perf 83 commit (report §7 item 5).
- `node scripts/verify-commit-prefixes.mjs` PASS over the range plus this close-out; `no_stale_doc_references` 32/32 after the count sync.
- Correctness census after the close-out: ACTIVE **7** numbered cards (items 1, 2, 11, 13, 62, 162, 163); `TRACKED_*.md` **139** numbered records and **139** lookup rows to `TRACKED_*.md` (items 175 and 176 left the tier at the close-out).
- Not run locally: Miri, Loom, Kani, TSan, MSRV, full `npm run check`, benchmarks beyond the deterministic iai rows, wall-clock, non-Windows/non-WSL hosts for the new tests (CI was the confirmation, see §6).

## §6. CI outcome and the two fixes after the push (2026-10-08)

**Honest record:** the push of `9c846e81` landed with CI **red**: job `test (aarch64-unknown-linux-gnu)` (`cross test`) failed 3 of 4 tests of `r14_sidecar_owner_capability_negative` with E0461 (run `37770561319`; 49 other jobs and Kani green). Cause: the first host == target gate (`6f568c25`, §5) trusted `CACHEDIR.TAG` as the target-root marker, but cargo writes that file into every `<root>/<triple>` directory. This was the residual flagged for item 175 (CI is the only confirmation), and it materialised. Local checks had run on a host == target layout only.

The follow-up range is derived from:

```text
git log --reverse --format="%H|%cI|%s" 9c846e812c8baf467c5929a681c4f8e737e17b5f..b524760284c4cae0720aa28e35145f6896f40455
```

The commit that records this section is outside that range.

| # | SHA | Commit time (`%cI`) | Prefix (R30-12) | Category |
|---|---|---|---|---|
| 1 | `eda25f972c8eff91082fa1a0b99f88c061269663` | `2026-10-08T14:26:01+02:00` | `test` | compiler-verdict gate (E0461) replaces the layout/marker gate of `6f568c25`; fixes the red `cross test` aarch64 job |
| 2 | `b524760284c4cae0720aa28e35145f6896f40455` | `2026-10-08T15:15:55+02:00` | `test` | source-freshness filter for stale `libsefer_alloc-*.rlib` candidates; fixes a local failure in long-lived target directories |

- **Net `production` impact: none** (test-only commits; `src/`, `Cargo.*` and `.github/` unchanged). No `perf(runtime)` or `perf(opt-in)` commit in this range, no measured Ir/wall-clock/RSS claim, raw perf logs committed: **0 files**.
- **CI verdicts on the landing SHAs:** `eda25f97` run `37776932309` success (50 jobs success, 4 scheduled jobs skipped) with Kani `37776932388` success; `b5247602` run `37783258647` success (same counts) with Kani `37783258732` success. In both, `test macos (production)` (native arm64) and `test (aarch64-unknown-linux-gnu)` execute the harness (`running 4 tests` / 4 passed at `eda25f97`, `running 5 tests` / 5 passed at `b5247602`).
- **Process note:** the rush reviewer FAILed the first resumed session because the orchestrator delegated work to a worker contrary to the instruction, and the second because it had no edit tools without a worker; both were resolved by allowing exactly one sequential worker, with the diff, reproductions and counterfactuals re-run independently by the integrator.
