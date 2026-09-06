# TIS link ordering / weak CAS gate — current state (2026-09-06)

## Verdict

Production code is unchanged: link cells retain `Acquire`/`Release` ordering,
the pop success CAS retains `Acquire`, and all CAS operations remain strong.
There is **no current wall-clock evidence or verdict**. The link-ordering
candidate still needs native ARM timing. `pop_success_relaxed` is a resolved
codegen NULL (item 63): it is byte-identical to `base` for every probed
function on x86 and AArch64 default/`+lse`, so production keeps `Acquire` for
proof clarity with no current codegen cost.

`store_elided` is an instrument-ready timing candidate. `cas_weak` and
`pop_success_relaxed` are already static NULL controls and are excluded from
timing; production CAS is unchanged on that NULL evidence. Only link-ordering
and store-elision await native ARM timing.

## Accepted measurement identity

The authoritative codegen artifacts use source HEAD
`59bfa6a2720a27c3b32b12b0d935e0996a8290dc`, source-input digest
`c4183d967e0f610aeb343c09ac3fb0ec43e1e6f1c17d2dc1757a350954290a98`, and
`rustc 1.97.0` / `LLVM 22.1.6`. The runner/oracle updates are in `e1a1817` and
`59bfa6a`; the recorded codegen artifacts are in `05be781`. Both current
codegen legs are source-input-identical to that HEAD and use the exact
`release-thin-lto-1cgu-no-incremental` profile.

## Codegen matrix and observations

The exact probe Cartesian matrix is:

- `x86_64-unknown-linux-gnu` × `{default}` × four functions × five variants =
  **20 rows**;
- `aarch64-unknown-linux-gnu` × `{default, lse}` × four functions × five
  variants = **40 rows**.

The functions are `load_next`, `store_next`, `push_index_impl`, and
`pop_index_impl`. The variants are `base`, `links_relaxed`, `cas_weak`,
`pop_success_relaxed`, and `store_elided`.

- `cas_weak` is byte-identical to strong CAS for every function, target, and
  AArch64 feature set.
- `pop_success_relaxed` is byte-identical to `base` for every function, target,
  and AArch64 feature set. This is item 63's NULL closure; production remains
  `Acquire` for the explicit publication proof.
- `links_relaxed` is byte-identical on x86. On AArch64 it removes the link
  `ldar`/`stlr` operations while preserving the head `Acquire`; native ARM
  timing is still required.
- `store_elided` changes only `push_index_impl`: x86 instruction count
  `62 → 71` (+14.5%), AArch64 default `79 → 77` (−2.5%), and AArch64 `+lse`
  `72 → 69` (−4.2%). The relevant static CAS/`stlr` site count is `2 → 1`;
  that is a static assembly count, **not runtime frequency**.
- The `store_elided` safety basis is unchanged ownership: the unpublished index
  remains exclusively owned; elision skips only an unchanged `next_link` store,
  while the first link and every different link are always stored.

## Runner contract

A valid future wall-clock leg requires:

- the exact 3 timed variants `{base, links_relaxed, store_elided}`; the two
  codegen-negative controls remain excluded from timing;
- the deterministic tag-only activation oracle: exact push retries `= 1`, pop
  retries `= 0`, and `store_next` calls `base/links_relaxed/store_elided`
  `= 3/3/2`;
- production-only timing rows on the honest target host, with at least 6
  samples and a sample count that is a multiple of 3;
- within each evidence bundle/run, all freshly generated codegen and timing legs
  must share one source HEAD, source-input digest, exact toolchain, effective
  environment, and exact thin-LTO/1CGU/no-incremental profile; the current
  bundle contains codegen only;
- canonical remap-path flags only (with the separate activation cfg in the
  oracle build): no inherited behavioral flags are permitted; and
- raw-log↔CSV linkage, with smoke output in a separate log and never used as
  evidence or a verdict.

## Next trigger

After the commits are available to CI, run an explicit workflow dispatch of
`tis-weak-memory-wallclock-gate` on native arm64. It must generate fresh x86
and AArch64 codegen, time the three variants above, and write the summary from
one checkout/toolchain. Until that run produces valid evidence, the current
verdict remains **OPEN / no wall-clock evidence**.

## Current artifacts

- `TIS_LINK_ORDERING_WEAK_CAS_GATE_codegen_x86_64-unknown-linux-gnu.csv`
- `TIS_LINK_ORDERING_WEAK_CAS_GATE_codegen_aarch64-unknown-linux-gnu.csv`
- `_raw_tis_p3_ab_x86_64-unknown-linux-gnu_codegen.log`
- `_raw_tis_p3_ab_aarch64-unknown-linux-gnu_codegen.log`
- `_raw_tis_p3_ab_x86_64-unknown-linux-gnu_codegen.s.all`
- `_raw_tis_p3_ab_aarch64-unknown-linux-gnu_codegen.s.all`

No wall-clock CSV or summary is current.

## Reproduction commands

```text
node crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs --mode codegen --target x86_64-unknown-linux-gnu
node crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs --mode codegen --target aarch64-unknown-linux-gnu
node crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs --mode summary
```

The summary command is expected to reject the current checkout until a fresh
native-arm64 wall-clock leg is present.
