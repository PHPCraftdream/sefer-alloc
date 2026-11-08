# J13b judge — preparation only; runtime admission disabled

Artifact routing is accepted on `be5fd14b` by CI `42517707507`; the static reference is accepted on `ac8906cb` by CI `42526023632` after personal source/log review. The owning-buffer/realloc component below is a new candidate with native and Miri acceptance pending. Neither receipt nor candidate is a performance measurement or complete runtime-judge acceptance.

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

Manifest fields are exactly `schema`, `run_id`, `artifacts`, and `arms`. `artifacts` maps three symbolic identifiers to objects with exactly `path`, `variant`, `source_commit` (40 lowercase hex), `source_sha256`, `binary_sha256` (64 lowercase hex), `rustc`, `features`, `original_source_sha256`, and `overlay_sha256`. Paths resolve relative to the manifest directory. Non-overlay provenance fields are null; the static-control artifact requires original-source and overlay hashes, a distinct resulting-source hash, and the same original candidate source/commit/toolchain/features. `arms` maps every registered arm to its artifact identifier. Source hashes must be captured by the reviewed artifact-building flow before measurement; accepting a well-formed manifest is not proof that an arbitrary binary was built from its claimed source, nor that the static reference behaves correctly. The accepted manifest-only step delivered no static-reference behavior or measurement artifact; the separately accepted static reference is described below.

The runner checks each regular non-symlink executable's SHA256 before and after every child. It passes artifact metadata (not paths) via `SEFER_J13B_ARTIFACT`; every emitted window must match the dispatched run/artifact/source/binary/toolchain/variant identity. These are integrity checks on frozen trusted artifacts, not protection against an adversary concurrently swapping executables. Mock transport tests inject explicit verification seams; separate artifact tests retain real file hashing and rejecting mutations.

Raw child stdout is persisted (written and fsynced) before post-child hash/error/status checks and before decoding. Afterwards: strict UTF-8, trailing newline, window set equal to the canonical cell windows, identity equal to the dispatched tuple, one shared `invocation_id` (not reused across children). All windows are staged and validated only at the end. The driver remains blocked: the native child is unavailable and runtime report admission remains categorically disabled. Manifest/routing changes do not authorize a campaign. Neither driver nor tests were executed locally.

## Static control-matched reference — accepted on `ac8906cb`

Exact-source CI `42526023632` completed SUCCESS: 39 Node tests, 13 Rust workload tests and one explicitly activated static native test passed (zero ignored). Original production hashes were verified after execution. The two-file overlay digest is `f6a4237c4ebc38f35165ec4ab7076d24e35a956c4f409e1bcbf88c0eac001b77`. Expected unused-learner warnings in the no-learning copy and existing dependency warnings remain visible; the result is not warning-free.

`scripts/j13b_static_reference.mjs` pins the complete SHA256 of the reviewed fresh-advice and runtime files before changing either one in a disposable CI source copy. It enables the existing control branch for Always (preserving Auto selection), leaves fixed-one non-control advice on the existing Always path, and removes only estimator admission after successful observation validation and diagnostic accounting. It changes no production file, public knob, header field, sampling premise, lifecycle or release rule. An Auto floor of one or ordinary Always without controls would not implement this reference.

The transformation receipt identifies original/result hashes per file and an exact two-file overlay digest. That digest is **not** a complete source-tree or binary identity; the eventual artifact-building flow still owes those identities. The destination must be strictly below the caller's `.rush`, not the production checkout or shared root, with private regular source files (no symlinks or hardlinks). Windows cross-drive paths are rejected. A preexisting receipt fails before any source write. The filesystem premise is a trusted, non-concurrently-mutated disposable CI copy, not an adversarial sandbox. Failed copies are discarded, not resumed as valid artifacts.

Six separate Node tests check source pins, exact transformation boundaries, digest reproducibility, path confinement, source-link rejection, receipt refusal and production-source preservation. `tests/j13b_static_reference.rs` is ignored in ordinary source builds because it intentionally requires the altered benchmark-only reference. The dedicated Linux CI step creates one ephemeral source copy, applies the overlay, explicitly runs the exact ignored oracle, requires one nonzero PASS, verifies original production hashes, and removes the copy (including build output) on exit. No local source copy, worktree, branch, test execution or campaign is created by this step.

The native oracle installs SeferAlloc, resolves Always/minimum/budget, holds two complete fresh cohorts, and verifies two real clean sparse owner samples and fourteen full-span advice operations while all e/m/target/applied/dropped rows stay unchanged. The payload size is derived from the runtime page size so header plus payload occupies exactly an 8 MiB span. Removing selection, sampling, fixed-one advice or the admission bypass makes the oracle fail. This proves neither complete per-window accounting nor full runtime report admission, both still outstanding.

## Owning buffers and actual page geometry — candidate, acceptance pending

`examples/_shared/j13b_owned_block.rs` owns a nonzero requested byte allocation through a borrowed `GlobalAlloc + Sync` allocator. It validates requested-size/Layout bounds before calls, retains the original NonNull provenance, updates pointer/layout together only after successful realloc, and frees the exact current layout once. Its exclusive `MaybeUninit<u8>` view assumes no initialization and cannot outlive the owner borrow. The unique byte owner has a documented conditional Send implementation, no Sync/Clone/raw-pointer getter, and no pointer/integer round-trip. The allocator reference and explicit Drop encode lifetime/drop checking; no additional marker is needed for a generic borrowed payload because there is no such payload type. These are internal judge interfaces, not a new allocator API.

The single shape kernel now has `touch_slots_with_page`, which validates host-page geometry and checks the two-page edge bound without overflow. Legacy preparation entrypoints preserve their original geometry. Six bounded actual-source fixture tests inspect initialized owned payload bytes and cover bounds/null, grow/shrink/failed-realloc prefix retention, both allocation/free thread directions, panic cleanup with an observed join, and invalid page inputs. A separately isolated Linux test binary installs SeferAlloc and exercises owner-return and foreign-free control paths through the same owner: only owner samples may train; remote reclaim remains UNKNOWN and repeated trim cannot train again. The complete worker-pool/campaign and opposite installed worker allocation direction remain later obligations.

CI explicitly requires seven native tests and six System-backed tests under each strict-provenance Stacked Borrows and Tree Borrows configuration. The unsafe owner helper itself selects the existing seam group; ordinary safe examples remain fast-only. These Miri cases verify the owner wrapper, not the entire SeferAlloc metadata lifecycle; correctness item271 remains CONDITIONAL. No local test/model/workload execution or new branch/worktree is used.

## Rust side

- `examples/_shared/j13b_workload.rs`: exact-cell planner over safe `Vec<MaybeUninit<u8>>` (no `unsafe` reads); one shared touch kernel (`touch_slots`) used for both the `MaybeUninit<u8>` buffers and the inspectable `u8` test slots. Legacy preparation tests inspect offsets on `u8` slots; the new owning-buffer fixtures also inspect explicitly initialized `MaybeUninit` storage. Neither is a full runtime-judge acceptance.
- A→B ownership substrate: A allocates, B touches and drops; the report carries audit events and join evidence (`JoinOutcome` for both threads, both always joined, also on panic/error, undropped-buffer count must be 0). It is one direction only, does not call SeferAlloc and shows no migration performance.
- Training controller: counts real class observations, 96 valid target, 1536 eligible cap, explicit `insufficient_signal`, and `InvalidShare` is reported rather than dropped.
- `tests/j13b_judge_workload.rs` covers these; `tests/j13b_schema.test.mjs` covers the schema. **Neither was run locally.**

## CI wiring

Job `j13b-judge-preparation` in `.github/workflows/ci.yml` runs all three Node files (`j13b_schema`, `j13b_artifacts`, `j13b_static_reference`), the existing 13-test Rust workload suite, the six-test owned-buffer suite and isolated one-test installed boundary, and the dedicated overlaid static-reference oracle. It runs on PR/push events only when changed paths match the J13b file patterns (`scripts/j13b_*.mjs`, `examples/_shared/*`, `examples/g1_j13b_*.rs`, `tests/j13b_*`, `Cargo.toml`, `ci.yml`) and always on schedule/workflow_dispatch. No full-CI activation or campaign dispatch is added. Artifact-routing receipt `42517707507` applies to `be5fd14b`, and static-reference receipt `42526023632` applies to `ac8906cb`; neither accepts the new owned-buffer candidate. Its six-case SB/TB Miri executions are additionally required in `miri-core`.

## Remaining integration obligations

1. SeferAlloc adapter and real `GlobalAlloc` shims.
2. Requested/resolved tuple readback, tuple conflict delta, mechanism counters. **Allocator-side hooks now exist** (task #47: `AllocCore::dbg_thp_tuple`, `dbg_thp_tuple_conflicts`, `AllocStats::thp_tuple_conflicts`, plus ledger/advised-byte counters under `bench-internals`), CI pending; the judge still refuses every runtime arm and does not consume them yet.
3. VM advice and residency (range advise / mincore) wiring. (Allocator-side range advice for fresh Large mappings is now wired by task #47, default Off; judge-side residency/mincore sampling is not.)
4. Host and artifact (source/binary) identity capture.
5. Per-window runtime rows (sparse/dense training/trained, mixed subwindows) from real events.
6. Reviewed runtime report admission.
7. Owner-authorized campaign execution.

No raw runtime JSONL, identity hashes, report, measurements or verdict exist.
