# TIS link ordering / weak CAS gate — current state (2026-09-06)

## Verdict

The ordering algorithm remains unchanged in substance: link cells retain `Acquire`/`Release` ordering,
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

The final authoritative codegen artifacts were captured under the current
run-25 remediation source HEAD
`77fafc36234af838749a70c3ae5cb7c2ea877ee6`, tree
`69ed043e90822c4bedda72fd9cf2ab64e3507fa1`, source-input digest
`6295e69d5c983fc429eeaacf6f3accf2d1a6e5403772628096be2e3e9fedd1a6`, and
`rustc 1.97.0` / `LLVM 22.1.6`. The ODB-pinned evidence capture is in
`c00087f` plus its scratch-guard closure `393f81e`; host-receipt and
natural-workload support was implemented in `b01580f`, and the earlier codegen
artifact recording landed in `133d842`—both are intermediate implementation
commits. Current codegen artifact commit is `0203577`. Both recorded codegen legs are
source-input-identical to that captured HEAD and use the exact
`release-thin-lto-1cgu-no-incremental` profile. The current run-25 remediation
closure is represented by `21fc146`, `9215cba`, `1b5ce46`, `b67c550`,
`3509ee6`, and `77fafc3`; these receipts are source-input-identical to the
current run-25 remediation HEAD and do not imply native ARM timing.

The HS P4 source-byte/provenance finding is closed by `c00087f` and `393f81e`:
evidence modes pin HEAD before reading each source input with
`git show <HEAD>:<path>` and retain those ODB bytes. The runner's intended
parent-config closure is narrow: each evidence Cargo build starts from a fresh
cwd outside the checkout ancestry, uses an absolute manifest path, a fresh
`CARGO_HOME`, and an isolated target; a preflight rejects `config`/`config.toml`
anywhere in the cwd ancestor chain or in `CARGO_HOME`. Tracked repository
config is therefore irrelevant to the evidence build. This closes provenance
only; it does not create native ARM timing evidence.

The current receipt is a local Windows codegen context, not an ARM host:
`platform=win32`, `release=10.0.19045`, `arch=x64`, CPU model
`11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz`, logical CPU count `16`;
online topology, physical/package-core topology, GitHub `ImageOS`,
`ImageVersion`, `RUNNER_ARCH`, and scaling-governor policy are explicitly
`unavailable`. Its canonical nonsecret JSON is pinned by SHA-256
`cc1db78be23882bb367b3e2120eefe377396f30046fbcb8f7362240849af72f7` and the
matching base64 bytes
`eyJwbGF0Zm9ybSI6IndpbjMyIiwicmVsZWFzZSI6IjEwLjAuMTkwNDUiLCJhcmNoIjoieDY0IiwiY3B1X21vZGVsX3NldCI6eyJzdGF0dXMiOiJhdmFpbGFibGUiLCJ2YWx1ZSI6WyIxMXRoIEdlbiBJbnRlbChSKSBDb3JlKFRNKSBpNy0xMTgwMEggQCAyLjMwR0h6Il19LCJsb2dpY2FsX2NwdV9jb3VudCI6eyJzdGF0dXMiOiJhdmFpbGFibGUiLCJ2YWx1ZSI6MTZ9LCJvbmxpbmVfdG9wb2xvZ3kiOnsic3RhdHVzIjoidW5hdmFpbGFibGUifSwicGh5c2ljYWxfdG9wb2xvZ3kiOnsic3RhdHVzIjoidW5hdmFpbGFibGUifSwiZ2l0aHViIjp7IkltYWdlT1MiOnsic3RhdHVzIjoidW5hdmFpbGFibGUifSwiSW1hZ2VWZXJzaW9uIjp7InN0YXR1cyI6InVuYXZhaWxhYmxlIn0sIlJVTk5FUl9BUkNIIjp7InN0YXR1cyI6InVuYXZhaWxhYmxlIn19LCJzY2FsaW5nX2dvdmVybm9yIjp7InN0YXR1cyI6InVuYXZhaWxhYmxlIn19`.
The raw logs and every codegen CSV row carry the exact same receipt JSON,
SHA, and base64; codegen timing-before/after fields are empty. No hostname or
dynamic-frequency value is part of the receipt.

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
  `62 → 71` (+14.5%), AArch64 default `78 → 77` (−1.3%), and AArch64 `+lse`
  `71 → 69` (−2.8%). The relevant static CAS/`stlr` site count is `2 → 1`;
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
- after production timing, a separate `cfg(tagged_index_stack_test)`
  `natural_workload` activation run with the same thread count and timing
  window; it is a path/counter oracle, never timing evidence. Its exact
  arithmetic is `natural_push_attempts = ops_total + push_retries`,
  `natural_store_next_calls = attempts` and `natural_store_elisions = 0` for
  `base`/`links_relaxed`, and `natural_store_next_calls < attempts` with
  `natural_store_elisions > 0` for `store_elided`;
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
- the canonical nonsecret host receipt must include OS/kernel release, arch,
  CPU-model set, logical CPU count, Linux package/core topology and online
  mask, GitHub `ImageOS`/`ImageVersion`/`RUNNER_ARCH`, and governor policy;
  unavailable fields stay explicitly unavailable. Its exact SHA-256 and
  base64 bytes must match raw↔CSV, remain stable across fresh legs on the same
  host, and match immediately before and after production timing. Hostname and
  dynamic frequency are excluded.

## Next trigger

After the commits are available to CI, run an explicit workflow dispatch of
`tis-weak-memory-wallclock-gate` on native arm64. It must generate fresh x86
and AArch64 codegen, run the deterministic oracle, time the three production
variants, then run the separate same-threads/same-window natural workload
oracle, and write the summary from one checkout/toolchain. Until that run
produces valid evidence, the current verdict remains **OPEN / no wall-clock
evidence**. The present AArch64 rows are cross-target codegen only; no ARM
host timing or natural ARM workload has been executed.

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
node crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs --mode summary --target aarch64-unknown-linux-gnu
```

The summary command reaches the missing native ARM wall-clock artifact
validation (rather than failing at argument parsing) and is expected to reject
the current checkout until a fresh native-arm64 wall-clock leg is present.
