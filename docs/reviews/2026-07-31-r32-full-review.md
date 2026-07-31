# Round 32 — independent full review (2026-07-31)

**Reviewer:** independent read-only review agent (second set of eyes; not the
orchestrating session that implemented and signed off on Round 32, and not
either of the delegated sub-agent transports it used — `@sh` for 9 tasks,
`crush run` for the last 2).
**Scope:** `9cebfca..846e64f`, **13 commits**, 11 tasks. Every commit message
and every diff hunk read in full — no sampling.
**Method:** CLAUDE.md's own zero-trust discipline applied literally — diffs
read line by line; the round's one measured result recomputed **from the
committed raw artifacts**, not from the report's own tables; the README
`unsafe`-inventory counts re-grepped from source; the new CLAUDE.md rule's
cited arithmetic re-derived from `src/`; both new scripts actually executed;
`npm run check` actually run to completion on `HEAD`; and the round's
non-vacuity narratives cross-checked against `git reflog` rather than taken
on the commit messages' word.

---

## 0. Executive summary

| Tier | Count | Verdict |
|---|---:|---|
| **P0** (soundness / correctness / build break) | **0** | Explicitly: none found. |
| **P1** (real but non-critical: misleading claim, vacuous evidence, user-facing doc contradiction) | **3** | All three attach to R31-10, the round's only runtime-changing task. |
| **P2** (process / polish) | **11** | |

**Every measured number this round published is correct.** I independently
recomputed all of R31-10's headline figures straight from
`docs/perf/R31_10_TRIM_CURRENT_THREAD_RSS_GATE_summary.csv` (medians, the
128.0 MiB RSS win, the 144.3 MiB commit win, both per-arm burst figures) and
**every one reproduced exactly**. Its mechanism oracle is genuinely sound —
not decoration. Its immutable source identity resolves and I verified
end-to-end that the measured tree differs from the landed commit only by
`cargo fmt` reflows. No `production` composition change, no version bump, no
`Cargo.lock` drift, no TODO/placeholder introduced, and no new safe `pub fn`
that takes a raw pointer and touches allocator metadata.

The three P1s share one shape and one origin: **R31-10 is the only task in
the round whose claims outran its evidence.** Two of the three are about the
*fallback-heap* story around the new public API (a test that does not test
what it says, and a rustdoc sentence that is factually wrong about the
allocator's own TLS binding); the third is that the round shipped a new
public API while two committed documents still say it is "not implemented".
None of the three is a data defect, and none affects a `production` build's
runtime behaviour.

The rest of the round is unusually strong: the three new process tools
(`verify-gate-report.mjs`, `verify-commit-prefixes.mjs`,
`tests/ci_clippy_matrix_consistency.rs`) are real, they run clean, and one of
them **caught a live defect on its first use** that two prior commits had
each falsely claimed to have fixed. R31-14a/b's ten review-P2 repairs are
each independently verifiable and each honours the append-only convention.

---

## 1. Build / test / lint health on `HEAD` — actually run

All against `846e64fe38ecc33ee615cf3eb487ce7cdeb3d5de` with a clean working
tree (`git status --porcelain` → `?? .claude/` only).

| Command | Result |
|---|---|
| `npm run check` (full: argv-roundtrip, fmt, clippy ×5, test ×4, perf-gate check, verify-perf-gate-stubs, verify-gate-report, verify-commit-prefixes, iai) | **PASS — `ALL GREEN — safe to push`, exit 0** |
| `npm run iai` tail (inside the above) | **PASS — `79 without regressions; 0 regressed`** |
| `node scripts/verify-gate-report.mjs` | **exit 0** — `ALL GREEN (88 report(s) scanned)` |
| `node scripts/verify-commit-prefixes.mjs` | **exit 0** — 29 commits linted, 0 FAIL, 6 WARN (all direction-2, all benign `Cargo.toml`/diagnostic-accessor cases) |
| `cargo check --features "alloc-global fastbin" --bench global_alloc` | **PASS** (used as evidence for P1-2, below) |

Item 8 of the review brief — **CONFIRMED**. The orchestrating session's
"ALL GREEN" claim holds on the final `HEAD`, verified by me, not accepted.

---

## 2. Scope-creep / unauthorized-change audit — CLEAN

Verified directly:

```
$ git show 9cebfca:Cargo.toml | grep -n "^production = \|^version = "
3:version = "0.3.0"
399:production = ["alloc-global", "alloc-xthread", "alloc-decommit", "fastbin", "alloc-segment-directory", "primordial-lazy-commit", "class-aware-dirty"]
$ grep -n "^production = \|^version = " Cargo.toml      # HEAD
3:version = "0.3.0"
399:production = ["alloc-global", "alloc-xthread", "alloc-decommit", "fastbin", "alloc-segment-directory", "primordial-lazy-commit", "class-aware-dirty"]
```

- **`production` feature list byte-identical**, same line number, before and
  after. R31-10's "no `Cargo.toml` feature-composition change" claim is
  **CONFIRMED**.
- **No version bump** (`0.3.0` both ends).
- **`Cargo.lock` untouched** — `git diff --stat 9cebfca..846e64f -- Cargo.lock`
  is empty.
- `Cargo.toml`'s only two removed lines are stale `Profile::Throughput`
  comment text replaced by R31-14a's own correction; everything else is pure
  addition (one `[[example]]` registration).
- **No TODO/FIXME/XXX/`todo!()`/`unimplemented!()`/HACK** introduced anywhere
  in `src/ examples/ tests/ benches/ crates/ scripts/` across the whole range
  (the only "placeholder"/"UNFILLED" hits are the new lint's own *detector*
  strings and doc text).

---

## 3. The standing adversarial check: new safe `pub fn` + raw pointer + allocator metadata

Three new `pub fn`s landed in `src/` this round:

| New item | Shape | Verdict |
|---|---|---|
| `AllocCore::dbg_decomp_recommit_payload` (`src/alloc_core/alloc_core_small_pool.rs:1179`) | `pub unsafe fn`, `#[cfg(all(alloc-decommit, bench-internals))]`, `#[must_use]`, `# Safety` contract | **COMPLIANT** — matches the sanctioned `dbg_decomp_decommit_payload` pattern exactly |
| `HeapCore::dbg_decomp_recommit_payload` (`src/registry/heap_core_diag.rs:966`) | same, forwarding delegation with forwarded contract | **COMPLIANT** |
| `SeferAlloc::trim_current_thread` (`src/global/sefer_alloc.rs:489`) | safe `pub fn`, **no pointer argument**, `#[cfg(alloc-decommit)]` | **COMPLIANT with rule 1**; deliberately NOT `bench-internals`-gated, justified (a real production caller exists — application code at a phase boundary), so CLAUDE.md's benchmark-hook rule 2 correctly does not apply |

Both new `unsafe fn` hooks are registered in `tests/dbg_hook_safety_tripwire.rs`'s
`UNSAFE_HOOKS` list in the same commit. `ReservedSmallSegment::base` →
`dbg_base` (R31-14b) restores tripwire visibility for a raw-pointer *return*
and adds a redundant per-item `#[cfg]` — the failure that forced it
(`"NEW unaccounted-for SAFE, non-bench-internals-gated hooks: ...::dbg_base"`)
is itself independent evidence the tripwire works end-to-end.

**No R25-1-shaped hole introduced.**

---

## 4. R31-10 — the round's one measured result, recomputed independently

Brief item 3. All four sub-questions answered from the committed artifacts.

### 4.1 (a) The 128.0 MiB win — **CONFIRMED EXACTLY**

Recomputed from `docs/perf/R31_10_TRIM_CURRENT_THREAD_RSS_GATE_summary.csv`
(6 rows, 2 arms × 3 reps), medians taken myself:

| quantity | my recomputation | report's figure |
|---|---:|---:|
| TRIM `rss_idle` median | 3276 KiB = 3.199 MiB | **3.2 MiB** ✓ |
| NO_TRIM `rss_idle` median | 134 368 KiB = 131.219 MiB | **131.2 MiB** ✓ |
| **RSS win** | 131 092 KiB = **128.02 MiB** | **128.0 MiB** ✓ |
| TRIM `commit_idle` median | 1676 KiB = 1.637 MiB | **1.6 MiB** ✓ |
| NO_TRIM `commit_idle` median | 149 416 KiB = 145.914 MiB | **145.9 MiB** ✓ |
| **commit win** | 147 740 KiB = **144.28 MiB** | **144.3 MiB** ✓ |
| TRIM / NO_TRIM `rss_burst1` medians | 131.195 / 131.203 MiB | **131.2 / 131.2** ✓ |

The report's §0.2 arithmetic (`128.0 = 131.2 − 3.2`) is stated with both
operands named, satisfying CLAUDE.md's numerator/denominator rule. Stability
claim ("3276–3280 KiB in every rep", "134 352–134 380 KiB in every rep") also
checks out against the CSV row-for-row.

### 4.2 (b) "Allocator layer under test" — **HONEST**

The report names `SeferAlloc` — direct `GlobalAlloc::alloc`/`dealloc` +
`trim_current_thread()` — and explicitly says it is **NOT** the
`HeapCore`/`HeapRegistry` substrate. I verified this against
`examples/r31_10_trim_rss_gate.rs`: the child arm constructs a `SeferAlloc`
and drives it through the `GlobalAlloc` trait only. This is the layer the
feature actually ships at. **No substrate substitution.**

### 4.3 (c) The `action_released_delta` mechanism oracle — **GENUINELY SOUND**

Not a rubber stamp, and stronger than a bare "the RSS counter moved":

- The delta is read **strictly around the `trim_current_thread()` call**
  (`examples/r31_10_trim_rss_gate.rs:151`/`:158`), not around the whole run.
- It is **hard-asserted per repetition, in the child**
  (`:161-169`), not only on the median in the orchestrator — the report's
  "every TRIM arm hard-asserted" claim is accurate.
- Each (arm, rep) cell is its **own OS subprocess**, so the process-wide
  `segments_released_total` counter is effectively per-run — nothing else in
  the process can move it.
- The **NO_TRIM control reports 0**, and a separate `idle_released_delta` is
  also 0 in both arms — so the report can, and does, distinguish "trim
  released" from "idle released" from "something else released".

This is a correct application of CLAUDE.md's R30-8 rule, and it is
attributive, not merely correlative.

### 4.4 (d) Immutable source identity — **STRONG FORM VERIFIED; weak form not reproducible**

The primary claim (R29-6 form 2, a `git write-tree` tree SHA captured before
the measurement binaries were built) **holds and is verifiable**:

```
$ git cat-file -t 065f0bc5b8d7b720d56a6316ca29dcac78867a0c
tree
$ git diff --stat 065f0bc5b8d7b720d56a6316ca29dcac78867a0c 38fbe8f
 CHANGELOG.md | 4 +-  docs/ARCHITECTURE.md | 2 +-
 docs/perf/R31_10_*.md | 172 +   ..._summary.csv | 17 +   _raw_*.log | 148 +
 examples/r31_10_trim_rss_gate.rs | 25 +-   tests/r31_10_*.rs | 4 +-
```

`src/global/sefer_alloc.rs` and `Cargo.toml` are **byte-identical** between
the measured tree and the landed commit; the 25+4 lines that do differ in the
example and the test are, on inspection, **pure `cargo fmt` reflows with zero
semantic change**. So what was measured is, functionally, exactly what
shipped. This is the best provenance any gate report in this repo has had.

Two defects attach to the *secondary* identity and the *recovery command* —
see P2-2 and P2-3.

---

## 5. Independently re-verified: README `unsafe` inventory (brief item 4)

Re-grepped with CLAUDE.md's own canonical, comment-proof command:

| quantity | my re-grep | README |
|---|---:|---|
| tier-2 item-scoped `#[allow(unsafe_code)]` | **68** | **68** ✓ |
| distinct files holding tier-2 sites | **18** | **18** ✓ |
| tier-1 module-level `#![allow(unsafe_code)]` | **20** | **20** ✓ |
| …of which in `src/` | **13** | **13** ✓ |
| …of which in `crates/` | **7** | **7** ✓ |
| `src/alloc_core/alloc_core_small_pool.rs` row (R31-6 bumped 2→3) | **3** | **3** ✓ |

All aggregate counts and the row R31-6 edited are **correct**. One row it did
**not** edit is now wrong — see **P2-1**.

---

## 6. Independently re-verified: CLAUDE.md's new rule's arithmetic (brief item 6)

`0a34ba1`'s rule cites specific source facts. Re-derived from `src/`:

- `SEGMENT` — `src/alloc_core/os.rs:65`: `pub(crate) const SEGMENT: usize = 1 << 22;`
  = 4 194 304 = **4 MiB** ✓ (exact file and line as cited).
- Rounding — `src/alloc_core/alloc_core_large.rs:190-192`:
  `let n_segments = needed.div_ceil(SEGMENT); n_segments * SEGMENT` ✓ (exact
  lines as cited), with `needed = hdr_aligned + align_up(size, align)` at
  `:153`.
- Worked example — a 6 MiB request: `needed` = one page of header + 6 MiB →
  `div_ceil(4 MiB)` = **2** → usable span **8 MiB**; × 8 objects =
  **64 MiB** ✓, exactly matching R30-6's own committed
  `burst1_used_max_bytes = 67108864` the rule cites.

**The rule's arithmetic is CONFIRMED from source, not merely repeated.** The
commit message's own claim that it re-derived rather than trusted the framing
is borne out.

---

## 7. The two new scripts — actually run (brief item 7)

Both execute cleanly on the current tree and both do real work.

- `node scripts/verify-gate-report.mjs` → **exit 0**, 88 reports scanned. Its
  FAIL-capable checks (a) companion CSV exists, (b) valid 40-hex SHA / no
  placeholder, (c) cited raw logs exist all pass, with a curated,
  individually-justified retroactive-exemption list for pre-R29-6 reports.
- `node scripts/verify-commit-prefixes.mjs` → **exit 0**, 29 commits, 0 FAIL,
  6 WARN — I inspected all 6 and each is the documented benign shape
  (`Cargo.toml` `[[example]]`/`[[bench]]` registration, or a
  `bench-internals`-gated diagnostic accessor).

**The tooling caught a real defect on first use, and I verified the catch.**
`541783b`'s claim that `verify-gate-report.mjs` found
`R31_0_..._summary.csv`'s `landing_commit` column still literally `UNFILLED`
on every row — after **two** prior commits each claimed to have filled it —
holds up: `c7b3eda`'s diff really did touch `commit_sha`, not
`landing_commit`. I also verified the repair diff is *exclusively* that
column (every changed line pairs 1:1 once `UNFILLED` ↔ the SHA is masked out;
zero asymmetric lines), and that `dece4a7025f80…` really is R31-0's landing
commit.

Their WARN-side coverage is weaker than the commit messages imply — see
**P2-4** and **P2-5**.

---

## 8. Append-only convention for report corrections (brief item 5) — **HONORED**

- `R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE.md` gained a dated **§7**
  (R31-14a) and a dated **§8** (R31-14b), each opening with an explicit
  "appended, not a rewrite" statement.
- `R31_3_LARGE_CACHE_EXTENDED_REVERIFICATION_GATE.md` gained a dated
  `**CORRECTED 2026-07-31 …**` block; the CSV note carries an inline
  `CORRECTED 2026-07-31` marker and the `value`/`unit` columns are unchanged.
- `R31_1_LARGE_CACHE_HEADROOM_CROSSING_REGIME_GATE.md` gained a dated §5
  addendum.
- The **one** place prose was edited in place — R31-0 §3 and §5, removing
  four percentages sourced from a never-committed third run — **restates the
  removed figures verbatim inside §8** ("−91%/−97%/−99%/−99%"), so nothing is
  recoverable only from git history. That satisfies the convention's intent.
- Regenerated CSVs (R31-0's summary/retention split, the OFF-arm `NA`
  fix) are derived artifacts, regenerated by the report's own script from
  the **already-committed raw logs** with the script's headline hard-assert
  re-passing — and the change is fully described in the addendum. I verified
  the post-state: the summary CSV is now uniformly 24 fields across all 49
  rows, the retention CSV exists with its own header, and exactly 24 OFF rows
  read `NA,NA,NA`.

Spot-checked arithmetic in the corrections: `410 112 KiB ÷ 1024 = 400.5 MiB`
✓ (P2-8's unit slip is real and correctly fixed); `grep -c '^[0-9]'
R30_6_..._summary.csv` → **12**, not 36 ✓ (P2-7's misattribution is real, and
`scripts/r31_12_repair_r30_6_data.mjs:56`'s own `expected 36 rows in R30-6
raw log` hard-assert confirms where the 36 actually lives).

---

## 9. Non-vacuity of claimed break-it-see-it-fire verifications (brief item 2)

| Claim | Independent corroboration |
|---|---|
| `eb6935b` (R31-5c): "created a scratch commit `perf(docs): fake test commit` → caught as FAIL; a second `perf(runtime):` touching only `docs/` → also FAIL; both removed via `git reset --soft`" | **CORROBORATED BY `git reflog`**, not just asserted: `8eae855 commit: perf(docs): fake test commit` → `reset: moving to HEAD~1` → `3dc528d commit: perf(runtime): fake docs-only test commit` → `reset: moving to HEAD~1`, all between `541783b` and `eb6935b`. Neither scratch commit is in `main`'s history. This is the strongest non-vacuity evidence in the round. |
| `d3fad4a` (R31-11): 4-step break/revert cycle against `tests/ci_clippy_matrix_consistency.rs` | **STRUCTURALLY SOUND.** I read the test: the count assertion (`:240-252`) and the per-position feature-string assertion (`:254-266`) both fail exactly as described for the two described mutations, and a `!manifest_rows.is_empty()` guard (`:219-226`) explicitly forecloses vacuous-pass-on-parser-drift. Its `expected_clippy_invocation` faithfully mirrors `check-matrix.mjs`'s `rowToCargoArgs` for the no-target clippy case (verified line by line against `scripts/check-matrix.mjs:136-170`). |
| `3b4f668` (R31-14b P2-12): "the rename alone surfaced a second gap — the tripwire genuinely FAILED until a per-method `#[cfg]` was added" | **CONSISTENT WITH SOURCE.** `tests/dbg_hook_safety_tripwire.rs`'s scanner matches `pub fn dbg_` by name prefix and reads only the attribute block immediately preceding the item, so `base` was invisible and post-rename `dbg_base` was ungated at item level. The redundant `#[cfg(all(alloc-decommit, bench-internals))]` at `src/alloc_core/reserved_small_segment.rs:128` is present and is exactly what makes it pass. |
| `3b4f668` (R31-14b P2-11): "tightening `AllocCore::dbg_large_cache_hits` would break two real tests" | **CONFIRMED.** `tests/alloc_zeroed_fresh_large_skip.rs` and `tests/regression_large_cache_span_usable_stable.rs` both gate on `alloc-core + alloc-decommit` only and assert on its return value. The "a production caller genuinely exists" justification for keeping it as a sanctioned exception is correct, and the asymmetry is now documented at the call site. |
| `d9a68e1` (R31-13): "no `alloc_batch`/`dealloc_batch` consumer beyond the `SeferAlloc` forwarding layer" | **CONFIRMED** by repo-wide grep: hits only in `src/global/sefer_alloc.rs` and `src/registry/{heap_core_alloc,heap_core_dealloc_batch,heap_core_diag,mod,tcache}.rs`. Zero in `crates/` or `examples/`. |

One claimed non-vacuity does **not** hold — R31-10's AC4. See **P1-1**.

---

## 10. Brief item 10 — the `crush run` "already in the working tree" narrative

**Assessment: benign. No real risk; the phrasing is imprecise, not suspicious.**

- The claim is **not in commit `ba52822`'s message** at all — `ba52822`'s
  message is a clean, self-consistent technical account. The narrative lives
  only in the checkpoint's Decisions section, where the orchestrating session
  itself already flags it as "an ambiguous, unverifiable claim" it declined
  to trust.
- The commit message's own words explain the likely source of the confusion:
  it reports the BEFORE-state reproduction as run "**via isolated worktree**"
  at `HEAD @ 3b4f668`. An agent describing its own scratch worktree as "the
  working tree" is the ordinary reading.
- **Attribution risk is nil, verified.** The fix is a thin wrapper over
  `os::recommit_pages` → `aligned_vmem::recommit`, both of which have been in
  this repo since `fe035d9` / `4a59c2b` (the aligned-vmem extraction, many
  rounds ago). Nothing in the diff originates outside the repository.
- `git worktree list` shows one worktree (`main`); no scratch worktree was
  left behind.
- The diff itself is minimal, correct, and defensible on its own terms
  (Windows `MEM_DECOMMIT` genuinely unmaps; a write without recommit is
  genuinely an access violation; the wrapper is a documented no-op on Unix,
  so Linux numbers are not inflated).

The orchestrating session's handling ("don't trust the narrative, verify the
diff") was the right call and I reach the same conclusion independently.

---

## 11. Findings

### P1 — real defects (3)

#### P1-1 — `AC4` does not test the fallback heap, and the new public API's rustdoc is factually wrong about it

**`src/global/sefer_alloc.rs:472-473`; `tests/r31_10_trim_current_thread_api.rs:258-289`**

The new public API's doc comment states:

> `/// A no-op on the fallback heap (no per-thread heap bound yet, or TLS`
> `/// already torn down) — there is nothing thread-local to trim.`

The parenthetical **"no per-thread heap bound yet"** is wrong. Traced through
the actual TLS resolution:

- `trim_current_thread` → `self.current_heap()` (`:352-355`) →
  `tls_heap::current_for_alloc_with_config`.
- `src/global/tls_heap.rs:521-530`: a **null** `LOCAL` (i.e. a thread that has
  never allocated) matches `Ok(p) if p.is_null() => bind_slow_tagged_with_config(*config)`
  at `:527` — it does **not** fall to `Fallback`.
- `:569-572`: `bind_slow_tagged_with_config` calls
  `HeapRegistry::claim_with_config(config)` and then `finish_bind`.
- `:629-654`: `finish_bind` returns `CurrentHeap::Fallback` **only** on
  registry exhaustion/OOM (`:630-635`) or a failed `GUARD.try_with`
  (`:640-646`). Otherwise it returns `CurrentHeap::Own(heap)` at `:654`.
- `CurrentHeap::Fallback` is otherwise reachable only via the `TORN` marker
  (`:528`) or a TLS-destruction `Err` (`:529`).

Consequences:

1. **`ac4_trim_on_fallback_heap_is_safe_noop` is vacuous with respect to its
   own subject.** Its comment says "This is the FIRST thing the thread does —
   no prior allocation has bound a per-thread heap", but that thread takes the
   `Own` branch: it **claims a fresh registry slot** and trims a brand-new
   empty own heap. The fallback path is never exercised. The test passes, but
   not for the reason it states.
2. **AC4 is recorded as closed on that basis** — in `38fbe8f`'s message
   ("AC4 (fallback-heap no-op) — safe on a fresh thread with no bound heap")
   and verbatim in `CHANGELOG.md:20`.
3. **A minor real behavioural surprise on new public API:** calling
   `trim_current_thread()` first-thing on a thread that never allocates now
   *claims a registry slot* as a side effect. Harmless (the `AbandonGuard` is
   armed, so the slot recycles at thread exit), but it is not the "no-op" the
   doc promises.

**Suggested fix:** correct the rustdoc to say the no-op case is *TLS torn
down / registry exhausted*, and either (a) rewrite AC4 to reach `Fallback`
genuinely — the crate already has `tls_heap::dbg_mark_local_torn_for_test`
(`bench-internals`-gated) for exactly this — or (b) rename the test to what
it actually proves (`trim on a freshly-bound, never-allocated heap is safe`)
and re-scope AC4 in the design doc.

#### P1-2 — `dbg_trim_current_thread` silently became a no-op under `alloc-global + fastbin`, justified by a false claim

**`src/global/sefer_alloc.rs:520-527`; `src/registry/heap_core_ownership.rs:252-265`**

R31-10 rewrote the hook as:

```rust
#[doc(hidden)]
pub fn dbg_trim_current_thread(&self) {
    #[cfg(feature = "alloc-decommit")]
    {
        self.trim_current_thread();
    }
}
```

Before R31-10 the body was **unconditional**: it called
`HeapCore::trim_for_recycle()` regardless of `alloc-decommit`. And
`trim_for_recycle` is **not** empty without `alloc-decommit`:

```rust
pub(crate) fn trim_for_recycle(&mut self) {
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    self.flush_all_tcache();                      // ← still real work
    #[cfg(feature = "alloc-decommit")]
    { self.core.drain_small_pool(); self.core.evict_all(); }
}
```

So in an `alloc-global + fastbin` build **without** `alloc-decommit`, the
hook lost its tcache flush. The commit message's justification — "without
that feature there is nothing to trim and this is a documented no-op" — and
the new doc comment repeating it are **factually incorrect**.

This configuration is real, not hypothetical: `benches/global_alloc.rs` has
`required-features = ["alloc-global"]` (Cargo.toml:906-908) and I verified
`cargo check --features "alloc-global fastbin" --bench global_alloc` builds
clean. That bench documents this hook at `benches/global_alloc.rs:28-34` as
the mechanism that prevents cross-group state contamination and calls it at 7
sites; `benches/heap_lifecycle_teardown.rs` group 2 **times the call itself**
(`:200-212`), which in that config now times an empty function.

Not reachable from `production`, `--all-features`, or `npm run bench:table`
(all include `alloc-decommit`), so no shipped behaviour changed — but a
benchmark harness's stated isolation guarantee silently regressed in a
buildable configuration, on the strength of a claim that does not hold.

**Suggested fix:** gate only the delegation's *inner* call, or keep the hook
unconditional and route to `trim_for_recycle` directly when `alloc-decommit`
is off; and correct the two doc sentences.

#### P1-3 — README and the design doc still say `trim_current_thread()` is "not implemented"

**`README.md:1426`; `docs/design/R30_7_TRIM_SCAVENGE_API_DESIGN.md:1`**

The round shipped its only new public API and its only runtime improvement,
and two committed documents still assert the opposite:

- `README.md:1426` — "`docs/design/R30_7_TRIM_SCAVENGE_API_DESIGN.md` |
  **Design proposal (not implemented)** for an explicit, caller-driven
  `trim_current_thread()` API…"
- The design doc's own title line — "R30-7 — an explicit, caller-driven
  trim/scavenge API: **DESIGN PROPOSAL, not implemented this round**" — with
  **no dated addendum** recording R31-10/task #474's implementation, despite
  `38fbe8f` claiming to implement its §6 acceptance criteria "verbatim".

Compounding it: `README.md:83`'s "**Memory policy — read this before you
deploy `SeferAlloc::new()`**" section — added by R30-13 precisely to surface
the idle-retention implication of the 256 MiB large-cache default — never
mentions the API that now solves exactly that problem. A user reading the
README end-to-end learns about the retention problem and is told the fix is
unimplemented.

This is the one place the round's otherwise-scrupulous append-only discipline
was not applied: R31-14a/b appended dated corrections to three prior reports,
but the report R31-10 itself fulfilled got none.

**Suggested fix:** a dated `**IMPLEMENTED 2026-07-31 (R31-10, task #474)**`
header block on the design doc, the README table row updated, and one
sentence in README §"Memory policy" pointing at `trim_current_thread()`.

---

### P2 — process / polish (11)

**P2-1 — README per-file `unsafe` row for `heap_core_diag.rs` drifted; the tripwire cannot see it.**
`README.md:594` says `src/registry/heap_core_diag.rs | 6`; the real count is
**7** — R31-6 added `dbg_decomp_recommit_payload` at `heap_core_diag.rs:966`
and bumped the aggregate 66→68 and the `alloc_core_small_pool.rs` row 2→3, but
left this row (and its prose enumeration of 6 hooks) untouched. The commit
message's "independently re-grepped" claim covered the **totals only**.
`tests/no_stale_doc_references.rs:352-385` asserts exactly three aggregate
tokens (tier-1 count, tier-2 count, tier-2 file count) and never per-file
rows, so this drift is invisible to CI by construction. (Separately: the
prose at `README.md:606` "All **seven** of that file's `unsafe fn` entries"
was already inconsistent with the table's 6 before this round — it is
accidentally correct again now.)

**P2-2 — the new provenance helper emits an invalid recovery command, and it shipped verbatim into the round's flagship report.**
`scripts/capture-measurement-identity.mjs:86` builds
`` `git show ${treeSha}: -- <path>` ``. I ran it: git **silently ignores** the
`-- <path>` and prints the root tree listing, **exiting 0** — a worse failure
mode than an error, because a future reader gets plausible-looking output that
is not the file. The correct form is `git show <tree>:<path>` (verified
working). This string was copied verbatim into
`docs/perf/R31_10_TRIM_CURRENT_THREAD_RSS_GATE.md:26`. The tool built to fix
unreproducible provenance citations shipped one.

**P2-3 — the helper's two identity forms are computed from different snapshots, and the patch hash does not reproduce.**
`treeSha` comes from `git write-tree` — the **index** (`:77`);
`patchSha256` comes from `git diff HEAD` — the **working tree** (`:94`). If
the index and working tree differ at capture time (routine: some files staged,
others not), the two "identities" in one capture describe different content.
R31-10's published `patch_sha256 = d1aaa9cb34c9…` does not reproduce by the
obvious route (`git diff ba52822 065f0bc5 | sha256sum` → `d66d0171…`), which is
consistent with exactly that. Additionally the helper's own
`patchReproduceCommand` (`:97-98`) instructs `git apply <saved-patch>` — but
the script **never saves the patch**, only hashes and discards it, so the
stated recipe is not executable. Mitigating: the *primary* form (the tree SHA)
is valid and I verified it end-to-end (§4.4), so R31-10's provenance claim
holds on its strong form; the weak form is decoration that cannot be checked.

**P2-4 — `verify-gate-report.mjs`'s headline cross-check SKIPped on the round's own flagship report.**
Check (d) — the prose↔CSV cross-check that was R31-5b's headline feature — is
`SKIP` on `R31_10_TRIM_CURRENT_THREAD_RSS_GATE.md` ("no headline-shaped
number+unit token found near a headline keyword"), and corpus-wide is
SKIP on **66 of 88** reports (9 PASS, 13 WARN). Cause:
`HEADLINE_KEYWORD_RE` (`scripts/verify-gate-report.mjs:486-487`) requires
`headline|hit-rate|retention|MDE|decommit|mechanism delta|activation` on the
**same line** as the number, and R31-10's headline sentences use none of them
(`\bdecommit\b` also does not match "decommitted"). Worse, had it matched it
would likely have fired a **false** WARN: the check is unit-agnostic, and
R31-10's prose is MiB while its CSV is entirely KiB — no CSV token rounds to
128, 3.2, or 131.2. `38fbe8f`'s "passes with zero WARNs" is literally true but
partly because the check that mattered did nothing. Recommend anchoring on
`^#{2,4}.*[Hh]eadline` section scope rather than same-line keywords, and
adding a KiB↔MiB normalisation pass.

**P2-5 — `verify-gate-report.mjs` emits 353 WARN lines and still prints "ALL GREEN".**
Check (e) (allocator layer under test) is WARN on **86 of 88** reports; (f) on
24. A gate that warns on essentially every input on every run trains readers
to ignore it, and a genuinely new WARN on a genuinely new report is
indistinguishable from the 350 pre-existing ones. Checks (b) and (c) already
have a curated retroactive-exemption mechanism; extending it to (e)/(f) — or
scoping the WARN-only checks to reports created after the rule commit, which
the sibling `verify-commit-prefixes.mjs` already does via `merge-base
--is-ancestor` — would make new-report WARNs visible.

**P2-6 — `CHANGELOG.md` covers 1 of the round's 11 tasks, and now contains a stale claim.**
`CHANGELOG.md` is touched by exactly one of the round's 13 commits (`38fbe8f`,
for R31-10). Absent entirely: R31-8's new CLAUDE.md rule, three new
CI/process tools (`verify-gate-report.mjs`, `verify-commit-prefixes.mjs`,
`tests/ci_clippy_matrix_consistency.rs`), R31-6's Windows correctness fix with
its two new `unsafe fn` hooks and README inventory change, R31-13's
reconfirmed batch-API decision, and all ten review-P2 repairs. And the
existing Round-31 bullet at `CHANGELOG.md:31` still asserts "**The other 11
P2s were filed, not fixed**" — no longer true for ten of them after R31-14a/b.
The `#### Measurement, correctness & tooling` bullet-tag convention that
CLAUDE.md's own R30-12 rule leans on as the round-level honesty mechanism was
simply not applied to this round's work.

**P2-7 — `tests/r31_10_trim_current_thread_api.rs` asserts equality on a process-wide counter across a window its sibling tests can perturb.**
`SeferAlloc::stats()` is documented as "a cheap, **process-wide** diagnostic
snapshot" (`src/global/sefer_alloc.rs:362`).
`ac1_trim_empties_pool_and_evicts_large_cache:86-90` asserts
`assert_eq!(released_after_cache, released_before)` across an alloc+dealloc
window. The default libtest harness runs the file's four `#[test]`s
concurrently, and `ac3`'s two threads and `ac4`'s spawned thread each trim
and/or hit `AbandonGuard::drop → trim_for_recycle` on exit — any of which
increments `segments_released_total`. Low probability, but a real flake vector
in a repo that tracks flaky tests as a standing correctness item. (The AC5
probe gets this right: subprocess-per-arm, single-threaded.) Cheap fix: assert
a **delta computed by the same thread around its own trim** rather than
process-wide equality, or serialise the four tests.

**P2-8 — `ba52822`'s subject `fix(examples):` under-declares its diff, and the new lint structurally cannot catch it.**
That commit adds two new `pub unsafe fn` hooks to `src/` and edits README's
`unsafe` inventory, under a subject naming only `examples`.
`scripts/verify-commit-prefixes.mjs`'s direction-2 WARN applies only to
`bench(...)`/`docs(...)` prefixes; everything else lands in the `'other'`
bucket and is explicitly out of scope (`:298`). Consistent with R30-12's
letter (which governs `perf`), but it is the same reader-misleading shape the
rule exists to prevent — a `git log`-skimming reviewer would not know `src/`
changed.

**P2-9 — R31-10 §0.3 mis-attributes the RSS-vs-commit gap to "segment headers" rather than the `SEGMENT` rounding the round itself just codified.**
The report explains the 16.3 MiB gap between the 144.3 MiB commit drop and the
128.0 MiB RSS drop as "~16 MiB of segment overhead (4 MiB segment headers,
metadata pages, guard pages)". The actual mechanism is whole-`SEGMENT`
rounding: `needed = align_up(sizeof(SegmentHeader), PAGE) + align_up(32 MiB, 8)`
= 32 MiB + 4 KiB → `div_ceil(4 MiB)` = **9** segments = **36 MiB** usable span
per object (`src/alloc_core/alloc_core_large.rs:153`, `:190-192`), so
4 × 36 = **144 MiB** is committed and released while only each object's touched
32 MiB payload ever entered the working set. The numbers are right, the named
cause is not — and it is precisely the effect `0a34ba1`, the round's *first*
commit, added to CLAUDE.md. Relatedly, the report labels the workload
"128 MiB/burst" where the span footprint is 144 MiB.

**P2-10 — R31-10's summary-CSV provenance header is hand-typed, not emitted by the probe.**
`examples/r31_10_trim_rss_gate.rs` prints the data rows (which I verified are
byte-identical to the committed CSV's rows 12-17 — the substance complies with
CLAUDE.md's R30-9 point 2), but the `# commit_sha=` / `# tree_sha=` /
`# patch_sha256=` / `# captured_at=` / `# platform=` header block was typed by
hand. That is also exactly where the one unverifiable number sits (P2-3).
`scripts/capture-measurement-identity.mjs --json` already emits these fields
machine-readably; wiring the probe to consume that JSON would close the loop.

**P2-11 — `eb6935b` was committed before its own `npm run check` finished.**
Its message honestly states "the full test x4 + iai tail of `npm run check`
was still completing … at commit time". No Rust source was touched and the
tree is green now (I re-ran the full suite on `HEAD` myself), so no harm
resulted — but it is a literal deviation from CLAUDE.md's "Between phases: run
tests and commit". Noted alongside a related process observation: the same
task created and removed two scratch commits **on `main`** via
`git reset --soft` (visible in `git reflog` — see §9, where this is also the
round's strongest piece of non-vacuity evidence). Nothing was lost and the
graph is linear, but a shared-workspace round should prefer a scratch branch
or worktree for that manoeuvre.

---

## 12. Verification commands and evidence trail

```
git log --oneline 9cebfca..846e64f                       # 13 commits
git show <each of the 13>                                 # every diff read in full
npm run check                                             # ALL GREEN, exit 0
node scripts/verify-gate-report.mjs                       # exit 0, 88 reports
node scripts/verify-commit-prefixes.mjs                   # exit 0, 29 commits, 0 FAIL
cargo check --features "alloc-global fastbin" --bench global_alloc   # P1-2 evidence

grep -rE '^\s*#\[allow\(unsafe_code\)\]'  src/ crates/ | wc -l   # 68  (README: 68 ✓)
grep -rlE '^\s*#\[allow\(unsafe_code\)\]' src/ crates/ | wc -l   # 18  (README: 18 ✓)
grep -rE '^\s*#!\[allow\(unsafe_code\)\]' src/ crates/ | wc -l   # 20  (README: 20 ✓)
grep -nE '^\s*#\[allow\(unsafe_code\)\]' src/registry/heap_core_diag.rs   # 7 (README: 6 ✗ → P2-1)

git cat-file -t 065f0bc5b8d7b720d56a6316ca29dcac78867a0c        # tree  ✓
git diff 065f0bc5b8d7b720d56a6316ca29dcac78867a0c 38fbe8f       # rustfmt-only in src-relevant files ✓
git diff ba52822 065f0bc5 | sha256sum                           # d66d0171… ≠ published d1aaa9cb… → P2-3
git show 065f0bc5…: -- examples/r31_10_trim_rss_gate.rs         # prints ROOT TREE, exit 0 → P2-2

git reflog                                                # corroborates 8eae855 / 3dc528d scratch commits
git show 9cebfca:Cargo.toml | grep '^production = '       # byte-identical to HEAD ✓
git diff --stat 9cebfca..846e64f -- Cargo.lock            # empty ✓
awk -F',' '{c[NF]++}' docs/perf/R31_0_..._summary.csv     # 24 fields × 49 rows ✓
grep -c '^[0-9]' docs/perf/R30_6_..._summary.csv          # 12 ✓ (P2-7 confirmed)
```

Medians and wins recomputed in Python straight from
`R31_10_TRIM_CURRENT_THREAD_RSS_GATE_summary.csv` — see §4.1.

---

## 13. Overall verdict

**0 P0 · 3 P1 · 11 P2.**

**This work is safe to trust and safe to build on.** Nothing shipped a wrong
number; every headline figure I could recompute reproduced exactly; the
allocator's shipped behaviour under `production` is unchanged; the two new
`unsafe fn` hooks are correctly `unsafe`, correctly `bench-internals`-gated,
and correctly registered in the tripwire; and `npm run check` is genuinely
ALL GREEN on `HEAD` when I run it myself. The orchestrating session's claim
to have zero-trust reviewed each commit is, on the evidence, **substantially
borne out** — its independent re-verification of R31-10's measurement, its
refusal to accept the `crush` agent's provenance narrative, and R31-5c's
reflog-corroborated break-it-see-it-fire cycle are all real, checkable work,
not assertions.

The three P1s are concentrated in exactly one place — **R31-10, the only task
that changed shipping surface, and the only one delivered through the
timed-out `crush run` session that had to be finished by hand**. All three
are claim/documentation defects rather than data or soundness defects: an
acceptance criterion whose test does not exercise its stated subject, a
rustdoc sentence contradicted by the crate's own TLS binding code, and a
README/design-doc pair that still calls the shipped API "not implemented".
None blocks anything; all three should be closed before the API is described
to a user or the version is bumped.

The most useful systemic observation: **the round's own new tooling did not
check the round's own new report.** `verify-gate-report.mjs` reported "zero
WARNs" on R31-10 partly because its load-bearing semantic check silently
SKIPped (P2-4), and `capture-measurement-identity.mjs` produced both an
uncheckable secondary hash (P2-3) and an invalid recovery command (P2-2) on
its first real use. That is not a failure of intent — the tools are real and
one of them caught a genuine live defect the same day it landed — but the next
round should treat "the tool passed" as a claim requiring the same zero-trust
posture this project already applies to sub-agents.
