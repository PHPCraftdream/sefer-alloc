# numa-shim — publication-readiness audit, run 20

**Author:** fm (Fable 5, effort=medium)
**Timestamp:** 2026-08-25 12:29:31 (+02:00)
**Revision reviewed:** `04713d97c65ac9f1a2e4bbf9418238281744fcb8` (local `main`; `origin/main` is `774716e`, 8 commits behind)
**Diff-review range:** `4a03fd37c921ee2fed5537f2caf36f4eec78d372..04713d9` (everything since item 115's twenty-third review)
**Mode:** read-only; no sub-agents; no cargo invocation of any kind (no test/build/check/clippy/miri/bench/publish). Evidence is source reading plus read-only `git log`/`git diff`; the one CI receipt cited below is quoted from the repository's own committed record (task #1352's caveat rewrite), not re-fetched.

## Verdict

**CONDITIONAL GO** — the second consecutive non-NO-GO of this campaign, converging with run 19 (item 115). On the runtime code: **GO** — a full fresh read of `src/lib.rs` (all five `mod platform` blocks, the mock, the cpumap/eintr/linux oracle modules, both FFI seams) and all ten test files found **no P0, no P1, no memory-safety/UB/provenance/ownership/leak/double-free defect, and no doc-vs-behavior lie**. The remaining conditions are release-process steps, all already tracked on items 103/106/115's cards — none is a code change.

## Part 1 — diff since the last review (`4a03fd3..04713d9`): confirmed doc/CI-comment-only

Five commits, four files touched (`.github/workflows/ci.yml`, `docs/CORRECTNESS_OPEN_ITEMS.md`, `docs/correctness-open-items/TRACKED_publish_readiness.md`, the run-19 report file):

- `1df1b2c` — adds the run-19 report (new file only).
- `b760406` — files item 115's card + one lookup-table row. Docs only.
- `82bb2ab` (task #1351, item 115's P2-1) — synchronizes item 114's contradicting Status/"remain"/Next-trigger lines in place. Docs only; I re-read the card and confirmed it now reads consistently (Status: CLOSED, all six findings mapped to landed commits `68b04cd`/`96d6884`/`fcad8b3`/`44264ac`).
- `bda0c87` (task #1352, item 115's P2-2) — rewrites the `#1099/I4` mixed-separator `$RUNNER_TEMP` caveat in `ci.yml` from "UNVERIFIED" to VERIFIED, citing CI run `32828560947` / job `97741909197` (green on landing SHA `774716e`). **Mechanically verified comment-only:** filtering the ci.yml diff to changed non-comment lines yields zero lines — every `+`/`-` line is a `#` comment. No `run:` block, env, or trigger changed.
- `04713d9` — refreshes item 115's own Status to CLOSED. Docs only.

**Nothing in this range touches runtime behavior, test logic, or CI execution semantics.** The claim in the task prompt is accurate.

## Part 2 — whole-crate fresh review

What I checked and found sound (selected, not exhaustive):

- **Linux FFI (`platform`, `mbind_preferred_linux`, `libc_mbind`):** `maxnode = 65` correctly compensates the kernel's `get_nodes()` decrement quirk; nodemask is a live stack `u64` for the syscall's duration; errno is captured before any cleanup FFI at every failure site (open/read/mbind); the mbind-failure path drops the reservation and returns the pre-captured error — no half-bound reservation can escape. `read_cpumap_into` closes its fd exactly once on every path (overflow, read error, EOF), and the EINTR streak resets on progress (bounds consecutive interruptions only, as documented).
- **Topology cache:** allocation-free `OnceLock<ReverseIndex>` initializer (heap-reentrancy hazard structurally removed, task #777); init-before-`sched_getcpu` ordering (task #1331) is in place in both `current_node_impl` and `current_node_resolution_impl`; `O_CLOEXEC` per-arch split including the sparc `0x400000` value (task #1345) matches its own documentation.
- **Windows FFI (`reserve_aligned_numa`):** reserve-then-commit with `node` on the (load-bearing) `MEM_RESERVE` call; strict-provenance `.addr()`/`.with_addr()`; checked alignment arithmetic with release-on-overflow; unconditional (not debug-only) commit-base contract check (task #1304); commit-failure and mismatch paths release `raw` before returning, and the `from_raw_parts` SAFETY argument (`base + size <= raw + over`) is arithmetically correct. `ProcessorNumber` layout pinned by const asserts.
- **cpumap parser / ReverseIndex:** single interpreter (`parse_each_set_cpu`), fail-closed on every malformed input (empty token, bad digit, >8-digit token), single linear pass via `rsplit`; `index_node`'s validate-then-write two-pass prevents partial commit; first-mapping-wins matches the ascending caller. Test suite covers word-order boundaries, capacity boundaries, malformed input, and probe/index non-divergence.
- **Mock:** record-before-validate, two-stage policy simulation with record-after-drop ordering as the release proof, reentrancy-safe `try_with`/`try_borrow_mut` discipline, capped log. `tests/mock_dispatch.rs` genuinely asserts the sequences (equality on the log, not vacuous `is_ok`).
- **Docs vs behavior:** README platform table, crate-doc matrix, `Cargo.toml` policy comments, and the four `tests/readme_examples.rs` compiled copies of the public snippets are mutually consistent at this revision; the post-#1346 `.ok().or_else(...)` best-effort idiom is in all three places (README, `NodeId::new` doc, `reserve_preferred_on_node` doc) and in the compiled oracles. The CHANGELOG's two formerly-broken historical snippets now read `.ok().or_else(...)` (verified at lines 112/120).
- **CI coverage:** numa-shim has test rows on real Linux/Windows/macOS/macOS-miri, mock rows with green-and-dead sentinels, clippy in three configurations (all-features, default, mock), rustdoc in three configurations (all-features, docs.rs-derived set, default — the task #1142 rule satisfied), an MSRV mock-arm compile row, and the `NUMA_SHIM_REQUIRE_ORACLE=1` row with per-test-name sentinels including `readme_vmem_integration_example_compiles_and_runs`.

### Findings

No P0. No P1. No P2.

- **P3-1 (NEW, doc-precision only, fail-closed direction) — `read_cpumap_into`'s doc understates its rejection boundary by one byte.** `crates/numa-shim/src/lib.rs` (~line 1623): the doc says `None` is returned "if the file is wider than `out`", but the implementation also rejects a file EXACTLY `out.len()` (4096) bytes long: after `total` reaches `out.len()`, the loop's `total >= out.len()` guard fires before the EOF-proving zero-byte read can ever be issued, so a complete exactly-4096-byte file is indistinguishable from a truncated one and is (correctly, fail-closed) rejected. Unreachable in practice — the complete cpumap text for `MAX_INDEXED_CPUS` is ~2304 bytes and a 4096-byte file already implies a CPU-ID space beyond every supported kernel's `NR_CPUS` — so this is a one-word doc fix ("wider than" → "as wide as or wider than"), not a behavior change. Post-release cleanup candidate.
- **P3-2 (restated, already tracked — not new) — the CHANGELOG's Phase-1 caveat cites the historical 31/0 count while the current suite yields 33.** `crates/numa-shim/CHANGELOG.md` line ~21 cites "31/0 at `c427dd6`" — accurate as a citation of that SHA's run, and the caveat itself says "the final pre-tag re-run is still owed". Item 103's U1/U5/U6 already fold the caveat/waiver text update into #1262's final-step commit alongside the pre-tag Phase-1 re-run (which item 104 execution-confirmed yields 33/33). Nothing to do now; just do not tag without that #1262 final step.
- **Out of scope per the brief, acknowledged for completeness:** (a) version deliberately still `0.1.0` (owner-scheduled bump); (b) `tests/readme_examples.rs` is a manual transcription with a self-documented drift-guard limitation (item 115's P3, deferred post-release).

### Release-gate conditions (the CONDITIONAL in this GO)

1. **Push the 8-commit unpushed wave (`774716e..04713d9`) with explicit owner authorization and confirm CI green on the landing SHA read from the remote.** This gives task #1299's CI rows — the Windows/macOS-miri mock sentinels, the five T9 root-crate greps, and the T12 MSRV mock-arm row — their FIRST execution. The `#1099/I4` mixed-path pattern they share is now verified by identity (run `32828560947`, job `97741909197`, green on `774716e`), and any residual failure mode is loud, not silently green; but "confirmed green on the landing SHA" is this repo's own standing rule and has not happened for these rows yet.
2. **The owner-scheduled release scope**, unchanged from items 103/106: version bump; consolidate `## Unreleased` under a dated heading; #1262's final step (pre-tag Phase-1 re-run recorded as a dated report + raw log, with the CHANGELOG caveat/waiver text updated in the same commit — covers P3-2 above); one post-bump `cargo publish --dry-run -p numa-shim` (U7 — the T7 package-gates gap is mitigated by the manual dry-runs items 103/104 already executed and re-executed, including item 104's tarball build against registry `aligned-vmem 0.2.0`).
3. Then the release-gate closure review, per item 115's own Next-trigger.

## Bottom line

Twenty-fourth independent look at this crate; the code has now been stable across four consecutive reviews with every finding wave shrinking in severity (P1 → P2 → P2-doc → P3-doc). What ships is correct as far as static reading can establish; what remains is process: push, watch CI go green on the landing SHA, execute the already-scripted release steps. One new P3 doc nit filed above; nothing blocking beyond the standing conditions.
