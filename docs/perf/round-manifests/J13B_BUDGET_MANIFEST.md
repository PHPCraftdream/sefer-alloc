# J13b budget/config runtime (task #47) — delivery manifest

**Current state (2026-11-10):** PR #226 VM foundation and PR #225 runtime are merged in current main; PR #227 repairs landing history and PR #228 closes that incident (`bad95849`). Task #49 Auto learning is accepted on exact `6c2ba419`, CI `42512485612`; the text below remains the dated Task47 delivery receipt. Defaults remain Off; no J13b measurement or campaign GO. Historical J13 campaign-2 RSS NO-GO and owner acceptance remain preserved in immutable `7ae557aa:docs/perf/J13_OWNER_DECISION.md`; old misses are not an automatic veto for review of materially new problems in current range Always. The owner-approved limited Always-vs-Off first-PR delivery direction is registered in `docs/perf/J13_LIMITED_STUDY_PREREG.md`; it does not authorize execution or replace the broader plan. The remaining product question is disposition of materially new regressions; no new numeric gate is implied. Header size is 144 bytes; Ir cost remains unknown (item 264).

Status: foundation IMPLEMENTATION VERIFIED by exact CI `42444595011` on `f7925dc899707281af2d7acf59cc4705503144a4`; not a performance GO. Class: `perf(opt-in)` (defaults unchanged).

Parent inspected actual-source Loom job `128621483903`: all three tuple-publication and both ledger cases passed. Matrix job `128621483811` executed the pre-syscall charge-return and failed-release charge-retention tests (one actual pass each; zero-test feature rows excluded), plus 12 selector and 14 planner tests in six rows. The overall run, including aarch64 and Miri, completed SUCCESS. The completed observation worktree and branch were removed only after byte-preserving source snapshots and personal comparison; this closes tasks #47/#55, not runtime sampling/learning #48/#49 or the performance campaign.

Delivery is stacked: PR #226 preserves the verified VM foundation at `6c62ff0f`; PR #225 targets that SHA-pinned base branch and evaluates the runtime delta separately. The first combined-range run `42443060521` on `73af9b1e` rejected stale `R-Ph3b` evidence and failed zero-cost because earlier foundation Cargo/build-graph changes prevented the declared-runtime exception. No gate is weakened: the declared-runtime classification applies only to the properly separated delta. The CI pull-request trigger additionally names the exact verified-base branch; without that narrow entry the workflow's `main`-only filter would silently exclude the stacked PR. Job selection, zero-cost policy and full-tier activation remain unchanged. `R-Ph3b` is now CONDITIONAL for the new header layout, with its historical receipt untouched; correctness item 271 requires fresh kind-aware tests and discriminating-mutant evidence before current-source PASS can return.

Personal review found that the legacy infallible VM release discards native failure, so returning from it is not proof of unmapping. The checked-release prerequisite is now delivered and verified by CI `42426350786`; the independent cross-abort verification repair is verified by CI `42439762856` on `6c62ff0f`. The dependent runtime funnel uses checked release for charged mappings: only confirmed success returns charge, while error or unknown outcome retains it without retry. The runtime source and its failure-retention oracle received their own successful CI `42444595011`; this acceptance does not rely merely on prerequisite success.

## Commit identity

The commit list is not typed here because this manifest is written before the delivery commits exist. Derive it after they land with:

```sh
git log --reverse --format="%h%x09%s" 39f228d8..HEAD
```

(base `39f228d8`, `bench(thp): prepare fail-closed J13b judge contracts and CI tests`; exclude this manifest-only commit to avoid a self-reference). No SHA is invented in this file.

The delivery commit combines the opt-in runtime, its tests, standalone observation primitives, semantic catalog/fixtures, models and documentation under `perf(opt-in)`; its body explicitly states that no speedup is measured or claimed. Earlier prerequisite commits retain their own `feat`, `fix`, `test`, `docs` and checkpoint subjects. Derive their exact identities from the range command rather than assuming one commit per file category.

## Scope and verdict

- Implemented: four opt-in `LargeCacheConfig` knobs (`huge_page_mode` with `LargeThpMode {Off, Always, Auto}`, `huge_page_block_min_bytes`, `huge_page_budget_bytes`, `huge_page_gradient`); a process-wide tuple published first-wins-as-a-whole through a write-once cell, only by a config that sets at least one knob, at heap materialisation; `AllocStats::thp_tuple_conflicts`; range advice (2 MiB-floored prefix of the usable span, `Reservation::try_advise_huge_pages_range`) of fresh non-NUMA `align < SEGMENT` Large mappings; a strict mapping-owned budget charge in `SegmentHeader::thp_charge` (header 128 → 136 bytes) returned exactly once after the unmap by `release_large_mapping` at every terminal Large release site.
- `Auto` stores and publishes its tuple but advises nothing: the learned share seam `auto_share` returns zero until scopes #48/#49. Auto learning is NOT implemented.
- The budget bounds advised bytes, not RSS. Linux/Android only; compiled out under `numa-aware`; no advice on other targets.
- Static J13 baseline: PR #224 (whole-mapping 8 MiB hint, OPEN) is ABSENT from this tree; nothing was merged or cherry-picked. `Always` is a new range-advice implementation, not a J13 identity.
- Dated design record: `docs/perf/J13B_BOUNDED_THP_PREREG.md`, "2026-11-06 #47 implementation amendment".
- Declared limitations: `docs/CORRECTNESS_OPEN_ITEMS.md` items 262–267. UNKNOWN until CI: CI host THP mode; iai Ir effect of the 8-byte header growth and of the added Acquire load on the fresh Large path.

## Production default impact

Unchanged. With no THP knob set nothing is published, the tuple reads `(Off, 8 MiB, budget 0 = unlimited, CAUTIOUS)` and no Large mapping is ever advised. `production` feature composition is unchanged; no new feature, dependency, `Cargo.toml` edit or version change. The only always-on effects are one Acquire load per fresh Large reservation and the 8-byte header growth (Ir unmeasured).

## Measurements and artifacts

No wall-clock, Ir or RSS measurement exists and there is no GO/NO-GO. New raw performance logs: **0 files, 0 bytes**. Verdict: **IMPLEMENTATION VERIFIED** on `f7925dc8` / CI `42444595011` (no performance result to classify as GO, NO-GO or NULL).

## Local verification actually done

- `cargo check` and `cargo clippy -- -D warnings` on the touched feature rows, and per-file `rustfmt --check` (as reported by the implementing workers; cargo fmt at the root may hit Windows os error 206).
- Cargo-free scripts: structure limits, debug-hook inventory, CI-table consistency. After the completed catalog edits, the parent personally confirmed `node scripts/ghost_inventory.mjs --check`: 709 sites, 95 files, 25 protocols, 48 scopes, zero gaps/double claims/dead scopes; `verify-loom-target-wiring` confirms 28 root targets. This inventory success is not semantic-lint execution or a concurrency proof.
- **No test, example, benchmark, Miri, Loom, kani or doc build was executed locally.** `tests/no_stale_doc_references.rs` was checked by reading only.

## Tests added (23 ordinary THP root files, none executed locally)

`tests/j13b_thp_advice_classify.rs`, `j13b_thp_advice_extent.rs`, `j13b_thp_always_unlimited.rs`, `j13b_thp_auto_seam.rs`, `j13b_thp_budget_conservation.rs`, `j13b_thp_budget_ledger.rs`, `j13b_thp_budget_ledger_stress.rs`, `j13b_thp_config.rs`, `j13b_thp_default_off_identity.rs`, `j13b_thp_header_charge_carry.rs`, `j13b_thp_injected_pre_syscall.rs`, `j13b_thp_min_zero.rs`, `j13b_thp_numa_off.rs`, `j13b_thp_off_explicit.rs`, `j13b_thp_publish_concurrent.rs`, `j13b_thp_publish_first_wins.rs`, `j13b_thp_publish_same_tuple.rs`, `j13b_thp_realloc_charge.rs`, `j13b_thp_registration_failure.rs`, `j13b_thp_release_failure_retains_charge.rs`, `j13b_thp_remote_free_reclaim.rs`, `j13b_thp_tuple_codec.rs`, `j13b_thp_unpublished_default.rs`. Each scenario that publishes a tuple is its own test binary (process-global state) and self-gates against `numa-aware`. `tests/r6_terminal_header_atomic.rs` header pins were updated to 136/136/136/144. The injected pre-syscall test needs the check-matrix row `test-j13b-thp-injected-pre-syscall` (added in `scripts/check-matrix.mjs`). Test-surface limits: the remote-free test covers only the cache-admits path; a registration failure on the cache-reuse path is not injectable; actual-source Loom models `tests/loom_thp_tuple_cell.rs` and `tests/loom_thp_ledger.rs` are part of this change and wired for CI; execution passed in exact CI `42444595011`; real-code model-mutant rejection remains unverified. Kani arithmetic coverage is not implemented.

The integrated candidate has 515 root Rust test files (488 + 23 ordinary THP tests + 2 Loom targets + 2 observation-core targets); README and `docs/ARCHITECTURE.md` agree. Root Loom count is 28 (26 + 2). The observation core remains unwired in production and is exercised through test-only path imports.

## Permission scope

Push and PR for CI only; no merge to main, no default promotion and no merge of PR #224 authorized by this delivery.
