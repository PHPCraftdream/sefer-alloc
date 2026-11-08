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

The command takes `<artifacts.json> <raw.jsonl>`, not a single executable for all arms. `scripts/j13b_artifacts.mjs` validates an immutable `j13b.artifacts.v1` manifest before opening the exclusive raw output or spawning a child. Input is bounded to 64 KiB and decoded as strict UTF-8. Exactly three artifacts are required: candidate, historical baseline, and a separate static-control reference. Arms `d/n/b/a/ab` share the candidate; `base/base2/mi` share the identical baseline executable; `f` requires a distinct static-control executable hash. PR #224 remains OPEN at head `e94a1da6c87db4af713890b61888a07a95bdd5cc`, not a merged shipping baseline.

Manifest fields are exactly `schema`, `run_id`, `artifacts`, and `arms`. `artifacts` maps three symbolic identifiers to objects with exactly `path`, `variant`, `source_commit` (40 lowercase hex), `source_sha256`, `binary_sha256` (64 lowercase hex), `rustc`, `features`, `original_source_sha256`, and `overlay_sha256`. Paths resolve relative to the manifest directory. Non-overlay provenance fields are null; the static-control artifact requires original-source and overlay hashes, a distinct resulting-source hash, and the same original candidate source/commit/toolchain/features. `arms` maps every registered arm to its artifact identifier. Source hashes must be captured by the reviewed artifact-building flow before measurement; accepting a well-formed manifest is not proof that an arbitrary binary was built from its claimed source, nor that the static reference behaves correctly. No real static-reference overlay or measurement artifact is delivered by this manifest-only step.

The runner checks each regular non-symlink executable's SHA256 before and after every child. It passes artifact metadata (not paths) via `SEFER_J13B_ARTIFACT`; every emitted window must match the dispatched run/artifact/source/binary/toolchain/variant identity. These are integrity checks on frozen trusted artifacts, not protection against an adversary concurrently swapping executables. Mock transport tests inject explicit verification seams; separate artifact tests retain real file hashing and rejecting mutations.

Raw child stdout is persisted (written and fsynced) before post-child hash/error/status checks and before decoding. Afterwards: strict UTF-8, trailing newline, window set equal to the canonical cell windows, identity equal to the dispatched tuple, one shared `invocation_id` (not reused across children). All windows are staged and validated only at the end. The driver remains blocked: the native child is unavailable and runtime report admission remains categorically disabled. Manifest/routing changes do not authorize a campaign. Neither driver nor tests were executed locally.

## Rust side

- `examples/_shared/j13b_workload.rs`: exact-cell planner over safe `Vec<MaybeUninit<u8>>` (no `unsafe` reads); one shared touch kernel (`touch_slots`) used for both the `MaybeUninit<u8>` buffers and the inspectable `u8` test slots. **Oracle limitation:** tests verify written offsets on `u8` slots; the `MaybeUninit` contents are not inspected.
- A→B ownership substrate: A allocates, B touches and drops; the report carries audit events and join evidence (`JoinOutcome` for both threads, both always joined, also on panic/error, undropped-buffer count must be 0). It is one direction only, does not call SeferAlloc and shows no migration performance.
- Training controller: counts real class observations, 96 valid target, 1536 eligible cap, explicit `insufficient_signal`, and `InvalidShare` is reported rather than dropped.
- `tests/j13b_judge_workload.rs` covers these; `tests/j13b_schema.test.mjs` covers the schema. **Neither was run locally.**

## CI wiring

Job `j13b-judge-preparation` in `.github/workflows/ci.yml` runs `node --test tests/j13b_schema.test.mjs tests/j13b_artifacts.test.mjs` and `cargo test --test j13b_judge_workload --features production`. It runs on PR/push events only when changed paths match the J13b file patterns (`scripts/j13b_*.mjs`, `examples/_shared/*`, `examples/g1_j13b_*.rs`, `tests/j13b_*`, `Cargo.toml`, `ci.yml`) and always on schedule/workflow_dispatch. It is not the full CI. No CI evidence exists yet (pending). Earlier `CI42176174292` and PRs #224/#225 are historical background only.

## Remaining integration obligations

1. SeferAlloc adapter and real `GlobalAlloc` shims.
2. Requested/resolved tuple readback, tuple conflict delta, mechanism counters. **Allocator-side hooks now exist** (task #47: `AllocCore::dbg_thp_tuple`, `dbg_thp_tuple_conflicts`, `AllocStats::thp_tuple_conflicts`, plus ledger/advised-byte counters under `bench-internals`), CI pending; the judge still refuses every runtime arm and does not consume them yet.
3. VM advice and residency (range advise / mincore) wiring. (Allocator-side range advice for fresh Large mappings is now wired by task #47, default Off; judge-side residency/mincore sampling is not.)
4. Host and artifact (source/binary) identity capture.
5. Per-window runtime rows (sparse/dense training/trained, mixed subwindows) from real events.
6. Reviewed runtime report admission.
7. Owner-authorized campaign execution.

No raw runtime JSONL, identity hashes, report, measurements or verdict exist.
