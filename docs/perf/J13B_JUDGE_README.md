# J13b judge — preparation only; runtime admission disabled

Preparation state on a dirty worktree. Nothing here is a measurement, a verdict, or CI evidence.

## Design

- **Per-child isolation:** one child process per `cell × arm × block × repeat` (12 blocks × 2 repeats, 9 arms). Each child emits one JSONL record per window of its cell, all sharing one `invocation_id`.
- **Canonical matrix:** `scripts/j13b_schema.mjs` `CELL_MATRIX` / `CELL_WINDOWS`. Simple cells have the single window `initial`.
  - `phased` (16 MiB, count 1): `initial`, `sparse-training`, `sparse-trained`, `dense-training`, `dense-trained`.
  - `mixed`: dense 16 MiB ×8 (bucket `[8,32)`) and sparse 64 MiB ×8 at 2% touch (bucket `[32,128)`); windows `dense-training`, `sparse-training`, `dense-only`, `sparse-only`, `co-resident`.
- `grow-1g` keeps the historical single 1 GiB target with a 1 MiB start as plan metadata; no realloc loop is executed.

## Schema `j13b.raw.v4` (training/trained windows)

- Training windows (adaptive arms `a`, `ab`): status `complete` requires ≥96 valid observations; `insufficient_signal` requires exactly 1536 eligible and valid <96; `pending` requires eligible <1536 and valid <96. Counters must be ordered valid ≤ observed ≤ issued ≤ eligible; `attained_share` in [0,1] is required; class and bucket must match the window.
- Trained windows (`*-trained`, `*-only`, `co-resident`) must carry `training_refs` naming their training window(s) and `attained_shares` per referenced bucket.
- Non-adaptive arms and adaptive `initial` windows require `not_applicable`.
- INCONCLUSIVE (`pending`, `insufficient_signal`, or a trained window whose referenced training is not `complete`) propagates to any pair that includes the arm, suppresses that pair's CI, and is distinct from "A/A unreadable" (A/A failure).
- Runtime JSONL parsing is categorically refused (`validateRuntimeRow` throws; no reviewed adapter exists). Only `evidence_class: synthetic`, `status: fixture` rows validate.

## Fixture-only report pipeline

`node scripts/j13b_report.mjs --fixtures raw.jsonl out.md out.csv` derives a report from synthetic fixture rows only. Output is labelled SYNTHETIC / not measurements; files are written exclusively (`wx`), the Markdown is removed if the CSV write fails; input ≤32 MiB, strict UTF-8. Without `--fixtures` no report is emitted. **This pipeline was NOT executed locally.**

## Driver (`scripts/j13b_run.mjs`)

Raw child stdout is persisted (written and fsynced) before child error/status checks and before decoding. Afterwards: strict UTF-8, trailing newline, window set equal to the canonical cell windows, identity equal to the dispatched tuple, one shared `invocation_id` (not reused across children). All windows are staged and validated only at the end. The driver is blocked, because the native child is unavailable: `g1_j13b_judge` emits `status`/`workload_executed:false` records (schema v4), which the driver refuses as "child unavailable". Not executed locally.

## Rust side

- `examples/_shared/j13b_workload.rs`: exact-cell planner over safe `Vec<MaybeUninit<u8>>` (no `unsafe` reads); one shared touch kernel (`touch_slots`) used for both the `MaybeUninit<u8>` buffers and the inspectable `u8` test slots. **Oracle limitation:** tests verify written offsets on `u8` slots; the `MaybeUninit` contents are not inspected.
- A→B ownership substrate: A allocates, B touches and drops; the report carries audit events and join evidence (`JoinOutcome` for both threads, both always joined, also on panic/error, undropped-buffer count must be 0). It is one direction only, does not call SeferAlloc and shows no migration performance.
- Training controller: counts real class observations, 96 valid target, 1536 eligible cap, explicit `insufficient_signal`, and `InvalidShare` is reported rather than dropped.
- `tests/j13b_judge_workload.rs` covers these; `tests/j13b_schema.test.mjs` covers the schema. **Neither was run locally.**

## CI wiring

Job `j13b-judge-preparation` in `.github/workflows/ci.yml` runs `node --test tests/j13b_schema.test.mjs` and `cargo test --test j13b_judge_workload --features production`. It runs on PR/push events only when changed paths match the J13b file patterns (`scripts/j13b_*.mjs`, `examples/_shared/*`, `examples/g1_j13b_*.rs`, `tests/j13b_*`, `Cargo.toml`, `ci.yml`) and always on schedule/workflow_dispatch. It is not the full CI. No CI evidence exists yet (pending). Earlier `CI42176174292` and PRs #224/#225 are historical background only.

## Remaining integration obligations

1. SeferAlloc adapter and real `GlobalAlloc` shims.
2. Requested/resolved tuple readback, tuple conflict delta, mechanism counters.
3. VM advice and residency (range advise / mincore) wiring.
4. Host and artifact (source/binary) identity capture.
5. Per-window runtime rows (sparse/dense training/trained, mixed subwindows) from real events.
6. Reviewed runtime report admission.
7. Owner-authorized campaign execution.

No raw runtime JSONL, identity hashes, report, measurements or verdict exist.
