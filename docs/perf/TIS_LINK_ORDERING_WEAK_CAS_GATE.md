# TIS link ordering / weak CAS gate — current state (2026-09-06)

## Verdict

Production code is unchanged: link cells retain `Acquire`/`Release` ordering and
CAS operations remain strong. There is **no current wall-clock evidence or
verdict**. The stale instrumented smoke artifacts were removed; summary is
intentionally fail-closed until a fresh evidence wall-clock leg exists.

No ordering or CAS change is authorized before that result, per the
measurement-first rule.

## Accepted measurement identity

Only the current clean-HEAD codegen legs from commit `4a2a178` are valid. They
must identify source HEAD `bab227f`, source-input digest
`aaf9e3514befb90777800bae7f1009147526d98043a701e4931121af08dbd015`, and
rustc `1.97.0` / LLVM `22.1.6`. All legs must use the same revision, digest, and
toolchain.

## Codegen matrix and observations

The exact probe Cartesian matrix is:

- `x86_64-unknown-linux-gnu` × `{default}` ×
  `{load_next, store_next, push_index_impl, pop_index_impl}` ×
  `{base, links_relaxed, cas_weak}` = 12 rows;
- `aarch64-unknown-linux-gnu` × `{default, lse}` × the same four functions ×
  the same three variants = 24 rows.

The probe must use target-aware instruction families and immediates. CSV is
authoritative. The x86 matrix is identity for every variant. On AArch64,
`links_relaxed` removes the link `ldar`/`stlr` operations while preserving the
head Acquire; `cas_weak` is identity with strong CAS.

## Runner contract

A valid future wall-clock leg requires:

- the exact matrix above, target-aware probes, `source_inputs_at_head=true`,
  and raw-log↔CSV SHA linkage;
- one revision, source-input digest, and toolchain across all legs;
- empty production flags;
- separate cfg activation after warm-up, with both activation counters
  satisfying `push > 0` and `pop > 0`;
- production-only timing rows, with `samples % 3 == 0` and at least 3 samples;
- smoke output in a separate log, never evidence or a verdict.

## Next trigger

After the commits are available to CI, run an explicit workflow dispatch of
`tis-weak-memory-wallclock-gate` on native arm64. It must generate fresh x86
and AArch64 codegen, ARM wall-clock data, and the summary from one
checkout/toolchain. Until that run produces valid evidence, the current
verdict remains **OPEN / no wall-clock evidence**.

## Current artifacts

- `TIS_LINK_ORDERING_WEAK_CAS_GATE_codegen_x86_64-unknown-linux-gnu.csv`
- `TIS_LINK_ORDERING_WEAK_CAS_GATE_codegen_aarch64-unknown-linux-gnu.csv`
- `_raw_tis_p3_ab_x86_64-unknown-linux-gnu_codegen.log`
- `_raw_tis_p3_ab_aarch64-unknown-linux-gnu_codegen.log`
- `_raw_tis_p3_ab_x86_64-unknown-linux-gnu_codegen.s.all`
- `_raw_tis_p3_ab_aarch64-unknown-linux-gnu_codegen.s.all`

## Reproduction commands

```text
node crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs --mode codegen --target x86_64-unknown-linux-gnu
node crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs --mode codegen --target aarch64-unknown-linux-gnu
node crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs --mode summary
```

The summary command is expected to reject the current checkout until a fresh
evidence wall-clock leg is present.
