# R19 PRF04 C1 — bench / measurement-only baseline gate

**Verdict: STOP — material retention >=64 MiB; no V4 implemented or authorized by this report.**
The five-sample median idle commit-charge difference, W128 minus W1 at K=32,
is **5374828544 bytes (5125.8359375 MiB)**, above the preregistered 67108864-byte
floor. This is A-only materiality evidence, not B benefit, GO, or a speedup claim.

## Attempts and immutable identity

Run01 exited 1 after 10m8s: the first production build reached the runner's
600-second spawnSync deadline (`cargo ETIMEDOUT`), before any sample or binary
identity was captured. Retained: `R19_PRF04_A_run01/source.patch`, 35119 bytes,
SHA-256 `02dbf06725f87a88a189113b71eda8302501cbe60c9c4b3307c6bde0d7867e40`.
The run01 binary-copy directory was empty. This was an artificial build deadline,
not an observed allocator error. No samples were duplicated.

The orchestrator explicitly authorized run02 and the sole runner source change:
child-command timeout 600000 -> 1800000 ms. Read-only tasklist/process inspection
found no leftover PRF04 release-build command; unrelated processes were neither
killed nor modified. Run02 command (from this isolated root):

```text
node scripts/r19_prf04_thread_wave.mjs --run docs/perf/R19_PRF04_A_run02
```

Run02 exited 0 in 6m16s, sequentially built both modes, and completed exactly
**ten primary plus two diagnostic samples**, with no extra measurements.
Immutable patch and identity existed before samples; source-drift checks passed.
No measured source was modified after capture; only this report was updated.

- Base SHA: `bfcbbfee3049b3359e3021436e21b6db719d5ddf`.
- Patch SHA-256: `9a4254be65521c89e62caf2db8360de7bbabcd0daf0e1b0c341b96b6b7c1166b`.
- Production binary SHA-256: `f7d89e48cc38b034a7bbff3ff2701d306627100b9c36684d3730810f66bc8301`.
- Diagnostic binary SHA-256: `29cc25379ef5a15012f047d490c170273f6f7917d4b298ce39feb7c58b850f2d`.
- Patch and both copied binaries rehashed after completion: match identity.
- Windows (`win32`) `10.0.19045`, x64, Intel Core i7-11800H @ 2.30GHz.
- rustc 1.97.0 (`2d8144b78`), Cargo 1.97.0, `x86_64-pc-windows-msvc`, release,
  locked/default-features; production features `production`, diagnostic features
  `production bench-internals alloc-stats internals`. Resolved features in identity.

## Raw and derived evidence

All citations below are relative to this report's directory:

- `R19_PRF04_A_run02/identity.json` — metadata captured before measurement.
- `R19_PRF04_A_run02/source.patch` — includes tracked edits and untracked source.
- `R19_PRF04_A_run02/samples.json.gz` — all twelve raw process samples; gzip of the exact bytes the runner wrote (299128 bytes uncompressed, over the 200 KiB raw-artifact ceiling). `gunzip` it before `--aggregate`.
- `R19_PRF04_A_run02/primary_{1..5}_w{1,128}.json` — primary raw samples.
- `R19_PRF04_A_run02/diagnostic_w1.json`, `diagnostic_w128.json` — activation only.
- `R19_PRF04_A_run02/R19_PRF04_THREAD_WAVE_summary.csv`.
- `R19_PRF04_A_run02/R19_PRF04_THREAD_WAVE_TABLES.md` — checked generated tables,
  medians, latency descriptions and count arithmetic; authoritative numeric table.

The single checked runner generated CSV and Markdown from raw data. Independent
read-only arithmetic asserted `5420085248 - 45256704 = 5374828544` and comparison
with the 67108864-byte floor. No handwritten replacement of the generated tables.
Binary copies remain exclusively in the ignored target run directory; no binaries
in docs artifacts. JSON, patch and tables contain no private absolute paths.

## Oracle receipts and regime

Real `#[global_allocator] SeferAlloc`, K=32, W=1/128 spawn+native join waves.
Workers allocate and free before common barriers; no worker can publish a TLS
exit hint before every worker has completed its claim/workload. Main remains LIVE.

All primary samples: count_before=1; W1 delta=32/raw=33; W128 delta=3969/raw=3970.
Thus **3969 is worker high-water growth, plus the main heap**, not process raw count.
All samples assert zero config conflicts and maintenance_running=false.

Diagnostic source order is [hint, bump, cap scan, recovery]:
W1 [0,32,0,0]; W128 [127,3969,0,0]. Successful lease sums equal 32 and 4096.
Resolved-default results [matches,mismatches] are [32,0] and [4096,0], actual
LIVE-policy comparisons under each newly acquired lease. Diagnostic conflict
counts are zero, maintenance false. Existing routed negative-directory scan
deltas are **0 in both diagnostics**, with alloc-stats increments enabled.
Primary routed-scan values are null rather than falsely measured zero.
Diagnostic timings are absent and excluded from all timing evidence.

## Same-A variation, without extra trials

Ranges below are min..max from the **same five primary samples in each cell**,
computed read-only from samples.json (gunzipped), not separate paired A/A or additional runs:

- W1 commit_charge: 45228032..45379584 bytes; RSS: 38174720..38555648 bytes;
  full wall: 75906100..159582400 ns.
- W128 commit_charge: 5420044288..5420138496 bytes; RSS:
  766955520..4269608960 bytes; full wall: 19428953600..23657136800 ns.

Relative spread is explicitly `(max-min)/median`: W1 commit 0.334872%
(151552/45256704), W128 commit 0.001738%
(94208/5420085248); W1 wall 71.503416%
(83676300/117024200), W128 wall 20.501298%
(4228183200/20623978100). RSS spread W128 is 187.848726%
(3502653440/1864613888). These noisy RSS/wall axes cannot support performance
promotion or serve as a calibrated paired A/A control. Commit materiality is
well above its floor; RSS is not substituted for commit.

First-workload allocation timing in generated tables is descriptive only: std
thread setup may bind before the closure. The first-TLS-bind/KILL-3 aspect remains
halted, as preregistered. Barrier/idle maxima are sampled, not continuous OS peaks.
No conclusion about per-heap isolated retention or other churn regimes follows.

## Validation and scope

Local narrow claim regressions: 8 passed; synthetic aggregation self-test,
Node syntax/hook-safety tripwire, rustfmt and diff checks passed. Count-expression
mutant 31 -> 30 caused expected self-test failure (32 != 31); restored script
matched its outside-repository backup. No mutant remained in captured source.

Orchestrator-supplied receipts, not extra runs by this measurement runner:
independent narrow tests 8 passed/0 failed; diagnostic all-targets clippy passed;
production all-targets clippy failed existing negative probe
`sol_f1_dbg_carve_batch_negative_probe` E0599, left untouched.

C1 adds **one example, zero new Rust test targets**; JavaScript synthetic
self-test supplies aggregation rejection controls. Production policy/composition
unchanged. No V4, other-worktree file reads, git writes, index/README/architecture/
changelog/manifest/lockfile edits. STOP here; later C2 requires separate scope and
the full preregistered benefit/kill gates, not automatic promotion from materiality.

## Post-hoc corroboration (not a sample)

After run02 the reviewer re-ran the saved, hash-matched production binary once per cell, outside the
runner, to check the headline is not an artifact of the run: W128 commit_idle 5420097536 bytes
(run02 range 5420044288..5420138496), W1 commit_idle 45289472 bytes (run02 range 45228032..45379584),
count_after 3970 / 33, config conflicts 0. These two runs are not among the ten primary samples and do
not enter any median.
