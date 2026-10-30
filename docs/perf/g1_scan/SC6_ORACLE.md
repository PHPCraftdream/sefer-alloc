# SC-6 — how often A-exact would notify: counting oracle

`bench`, measurement only. The question comes from the owner: "on which
benchmarks does the +24 Ir notification budget matter?". It is answered
before any A-exact code exists.

## Oracle

The oracle is compiled only under `--cfg g1_ingress_oracle` + `bench-internals`
(commit `c4dbbd0d` + `67cb28cf` on branch `g1-sc6-p0`; production code is
untouched). It has two halves:

- **Owner.** Every refill miss (`AllocCore::find_segment_with_free`) bumps
  `G1_OWNER_EPOCH[owner & 1023]`.
- **Producer.** Every successful small remote publication
  (`RoutePin::publish_small_borrowed`) counts one publication. It counts a
  notification when the owner's epoch moved since this route's last counted
  notification (`SmallSidecar::oracle_seen`, born `u64::MAX`, so a fresh
  route notifies once).

A-exact re-arms a segment only when the owner processes that segment's
notification on a miss. The oracle re-arms every segment of the heap on every
miss, so its notification count is an upper bound on A-exact's.

## Workload

`examples/ph6b_mt_ab.rs` runs the installed `#[global_allocator]`:

- larson and mstress, T = 1, 2, 4, 8;
- every 16th free goes cross-thread;
- two runs, `production internals bench-internals`, release, Windows,
  i7-11800H.

Raw output: `docs/perf/g1_scan/sc6_oracle/ph6b_r{1,2}.txt`.

## Result (derived by `node scripts/g1_sc6_oracle_report.mjs docs/perf/g1_scan/sc6_oracle docs/perf/g1_scan/SC6_ORACLE_summary.csv`)

The projection uses the spec budgets: +6 Ir for the hot check on every
remote publication, and +24 Ir per notification.

| Run | Workload | T | ops | remote publications | notifications (upper bound) | notifications / publications | projected Ir/op (6·pub + 24·notify) / ops | of which notify |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| 1 | larson | 1 | 801536 | 4 | 1 | 25.00 % (1/4) | 0.0001 | 0.0000 |
| 1 | mstress | 1 | 399360 | 4 | 1 | 25.00 % (1/4) | 0.0001 | 0.0001 |
| 1 | larson | 2 | 1596063 | 44387 | 401 | 0.90 % (401/44387) | 0.1729 | 0.0060 |
| 1 | mstress | 2 | 797472 | 24486 | 74 | 0.30 % (74/24486) | 0.1865 | 0.0022 |
| 1 | larson | 4 | 3201829 | 98790 | 898 | 0.91 % (898/98790) | 0.1919 | 0.0067 |
| 1 | mstress | 4 | 1591543 | 45461 | 135 | 0.30 % (135/45461) | 0.1734 | 0.0020 |
| 1 | larson | 8 | 6400524 | 194345 | 1752 | 0.90 % (1752/194345) | 0.1888 | 0.0066 |
| 1 | mstress | 8 | 3189003 | 97030 | 313 | 0.32 % (313/97030) | 0.1849 | 0.0024 |
| 2 | larson | 1 | 801536 | 4 | 1 | 25.00 % (1/4) | 0.0001 | 0.0000 |
| 2 | mstress | 1 | 399360 | 4 | 1 | 25.00 % (1/4) | 0.0001 | 0.0001 |
| 2 | larson | 2 | 1602231 | 50754 | 447 | 0.88 % (447/50754) | 0.1968 | 0.0067 |
| 2 | mstress | 2 | 797056 | 24056 | 74 | 0.31 % (74/24056) | 0.1833 | 0.0022 |
| 2 | larson | 4 | 3203699 | 100720 | 971 | 0.96 % (971/100720) | 0.1959 | 0.0073 |
| 2 | mstress | 4 | 1595413 | 49456 | 174 | 0.35 % (174/49456) | 0.1886 | 0.0026 |
| 2 | larson | 8 | 6405949 | 199945 | 1745 | 0.87 % (1745/199945) | 0.1938 | 0.0065 |
| 2 | mstress | 8 | 3183599 | 91452 | 259 | 0.28 % (259/91452) | 0.1743 | 0.0020 |

## Reading

With threads, the upper bound puts notifications at 0.28–0.96 % of remote
publications. The +24 notification budget therefore adds at most 0.0073
Ir per operation. The hot check (+6 per remote publication, about 3 % of
operations here) adds 0.17–0.20 Ir per operation. On these benchmarks the
notification budget is negligible; the hot check is the cost that matters,
and Stage 0 measures it directly against its existing +6 / +10 thresholds.
With one thread there are only 4 publications per run (thread start and
teardown), so A-exact costs nothing measurable there.

This is a counting result, not a time result. Native MT time is still the
deciding judge (Stage 0 / Stage 1 kill rules).
