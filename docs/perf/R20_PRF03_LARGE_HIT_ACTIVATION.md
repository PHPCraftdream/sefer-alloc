# bench: measurement-only, no runtime change

## Point of entry

All traffic runs through a real `#[global_allocator] SeferAlloc` installed in
the harness binary `examples/r20_prf03_large_hit_activation.rs` (the crate's
standard example pattern, as in `examples/global_allocator.rs`). No allocator
is swapped, no code path is stubbed: every allocation below is served by the
same `SeferAlloc` that production links against. The probe counters behind
the numbers are compiled in only under `bench-internals` + `internals` and
are read via `sefer_alloc::alloc_core::r20_prf03_large_probe_snapshot()`
(Relaxed loads over monotonic counters); they mutate nothing.

## Measurement identity

From the raw log `docs/perf/_raw_r20_prf03_large_hit_activation.log`:

- base-commit: `2f207d638e116072226b8b63f5780c2bd72c92a9`
- patch-sha: `c972761cf0acd157a255ae3bec59424250c0776e`
  (`git diff | git hash-object --stdin` — identifies the unstaged working
  tree; the probe files themselves are untracked, see the status snapshot)
- status-digest: `git status --porcelain` snapshot recorded verbatim in the
  raw log header (M Cargo.toml; M alloc_core_large.rs, large/mod.rs,
  alloc_core/mod.rs; untracked probe module, example, test, table script,
  and this log).

Toolchain: rustc 1.97.0 (2d8144b78 2026-07-07), 2026-10-12, one Windows host.

## What was measured

Counter semantics (per workload, emitted as deltas of the monotonic probe
counters):

- `large_requests`: cache-eligible `alloc_large` entries — Large-path
  allocations (size > SMALL_MAX ≈ 252 KiB) with alignment < 4 MiB (SEGMENT).
- `hit`: the request was served by claiming a best-fit slot from the Large
  cache (freed segment reuse), rather than the slow dedicated-segment path.
- `changed_payload_hits`: a hit where the slot's previous header
  `payload_offset` differed from the new `hdr_aligned` — i.e. the hit needed
  a payload-offset adjustment, which is exactly the path PRF-03's retained
  route (R3, ≈ +119 Ir on a ~1287 Ir changed-payload pair, ≈ 9% of the pair)
  was proposed to fix.

Conservation holds and is asserted by
`scripts/r20_prf03_large_hit_activation_table.mjs`:
`large_requests === hits + misses` and `changed_payload_hits <= hits` on
every line.

## Workload table

| workload | what it models | why representative | representativeness limits |
|---|---|---|---|
| bench_table_replay_model | `npm run bench:table` sized scenarios (16 B..1024 B) | the only in-repo real workload shape we replay | entirely small-class; 0 Large requests is itself the finding, not a miss |
| growing_vec_u8 (360 reqs) | log/video-buffer `Vec` doubling growth, 64 KiB..64 MiB | realloc growth chains are the canonical way buffers drift through Large spans | synthetic; real loggers interleave many small writes and rarely hit 64 MiB |
| file_read_whole (600 reqs) | whole-file read buffers of a few varied sizes, freed and repeated | same-size churn (hits) plus size jumps (best-fit) in one shape | real file sizes are far more varied; sizes here are fixed round MiB values |
| serialize_reuse (400 reqs) | per-request serialization buffer, mostly same size, every 8th larger | per-request buffer alloc/free is the dominant Large-cache consumer in server shapes | no payload reuse across cycles; sizes fixed |
| fixed_size_pool (800 reqs) | connection/object pool of fixed-size 2 MiB buffers churning | same-size free-then-alloc, the canonical cache-hit shape | idealized pool: no fragmentation, no size drift |
| alternating_align (120 reqs) | align 16/4096/64 KiB alternation on one 2 MiB size — **deliberately adversarial changed-payload upper bound** | 64 KiB is the only alignment in the sweep that exceeds the page size, so it is the only arm that can activate the changed-payload path | adversarial by construction; excluded from the "typical" judgment and reported separately |

All workloads are synthetic models; no real application traces were used.
The bench:table scenarios are small-class and issue 0 Large requests —
that is a real finding about the in-repo bench, not missing data.

## Large threshold note

The Large path engages for size > SMALL_MAX ≈ 252 KiB (geometric series
top); the first Large size is 253 KiB. The `size_sweep` lines in the raw log
re-verify the boundary empirically: 258048 B → 0 large_requests, 259072 B →
1. Alignment 4096 does NOT change the payload offset on any host with page
≥ 4 KiB (`align_up(hdr, align.max(page))` already covers it); 64 KiB does.

## Preregistered decision rule

Stated before the numbers. Rationale: R3 (retained-route fix) measured
≈ +119 Ir on a ~1287 Ir changed-payload pair, i.e. ≈ 9% of that pair. A
changed-payload frequency below 1% of requests therefore bounds the total
recoverable win at ≪ 1% of Large-request cost — below measurement noise.

- If on EVERY non-adversarial workload changed-payload hits ≤ 1% of that
  workload's `large_requests` AND ≤ 5% of its `hits` → recommend **CLOSE
  PRF-03** (variant c: archival reference, hook re-evaluation).
- If ANY non-adversarial workload shows changed-payload hits ≥ 5% of its
  `large_requests` → recommend **REDESIGN** (descriptor of reservation
  separate from the issue snapshot, without rekey).
- Otherwise → **INSUFFICIENT DATA**, and we state what is missing.

## Numbers

From the scaled run (TOTAL 2282 large_requests; raw log and CSV
`docs/perf/R20_PRF03_LARGE_HIT_ACTIVATION_summary.csv`; percentages:
hit-rate = hits / large_requests; changed % = changed_payload_hits over the
denominator named per column):

| workload | requests | hits | misses | changed | hit % (h/r) | changed % (c/h) | changed % (c/r) |
|---|---|---|---|---|---|---|---|
| bench_table_replay_model | 0 | 0 | 0 | 0 | n/a | n/a | n/a |
| growing_vec_u8 | 360 | 354 | 6 | 0 | 98.33 | 0.00 | 0.00 |
| file_read_whole | 600 | 600 | 0 | 0 | 100.00 | 0.00 | 0.00 |
| serialize_reuse | 400 | 400 | 0 | 0 | 100.00 | 0.00 | 0.00 |
| fixed_size_pool | 800 | 794 | 6 | 0 | 99.25 | 0.00 | 0.00 |
| alternating_align (adversarial) | 120 | 120 | 0 | 79 | 100.00 | 65.83 | 65.83 |
| **TOTAL** | 2282 | 2270 | 12 | 80 | 99.47 | 3.52 | 3.51 |

## Verdict

Applying the preregistered rule: on every non-adversarial workload the
changed-payload count is 0 — that is 0/360 requests and 0/354 hits for
growing_vec_u8, 0/600 requests and 0/600 hits for file_read_whole, 0/400
requests and 0/400 hits for serialize_reuse, and 0/800 requests and 0/794
hits for fixed_size_pool — so each workload's changed-payload hits are
0.00% of its `large_requests` (≤ 1%) and 0.00% of its `hits` (≤ 5%). The
only nonzero changed-payload count (79/120 requests = 65.83%) comes
exclusively from `alternating_align`, which is deliberately adversarial by
construction and excluded from the "typical" judgment per the preregistration.
No workload falls into the "otherwise" band. **Verdict: CLOSE PRF-03**
(variant c: archival reference, hook re-evaluation). Even the adversarial
upper bound, folded into TOTAL, is 80/2282 = 3.51% of requests — and its
R3 win would still be ≈ 9% of the changed-payload pair's cost, i.e. noise
against Large-request cost.

## Limitations

- Single-threaded workloads only; the Large cache's cross-thread behavior
  (steal/rebalance) is not exercised.
- One Windows host, one toolchain (rustc 1.97.0), release build only.
- All workloads are synthetic models; no real application traces were run.
- The probe itself exists only under `bench-internals` + `internals` and is
  compiled out of production builds; counter reads are measurement-only.
- The in-repo bench:table scenarios issue 0 Large requests, so no real
  in-repo workload shape contributes Large-path data.
