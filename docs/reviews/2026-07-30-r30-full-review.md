# Full independent review: Round 30

Date: 2026-07-30

Reviewed range: `b5ee62d..c141d76` (18 commits — R30-1 … R30-14 plus two
SHA-fill-in follow-ups and one checkpoint commit), 72 files, +22,263 / −509.

Review mode: **not paper-only.** Every commit message and every line of the
`src/`, `tests/`, `benches/`, `examples/`, `scripts/` and `.github/` diff was
read. Builds, lints and tests were actually executed on the current `HEAD`
(commands and outcomes in §1). Numeric claims in the three new gate reports
and in `CHANGELOG.md` were independently recomputed from the round's own
committed raw logs / summary CSVs with a Python re-derivation, not
eyeballed. R30-10's file-count survey was independently reproduced. The
tripwire's `cfg` parser was extracted verbatim into a standalone `rustc`
probe outside the repo to test an adversarial input.

No repository file was modified except this report. No `git` command that
mutates the working tree, index, refs or remotes was run (per the
shared-workspace rule), which is why the R30-1 counterfactual was verified
**statically** (§3.1) rather than by reverting the fix.

---

## 0. Executive verdict

Round 30 is a genuinely strong correctness-and-process round. It found and
fixed a real soundness hazard (R30-1), then — instead of declaring victory —
honestly corrected the previous round's overclaim that the same bug class was
"closed for good" and rebuilt the tripwire to be shape-independent (R30-2).
Its measurement work is the most self-critical this project has produced: the
new path-activation oracle caught a real design bug **inside R30-3's own
development** before any number shipped, and the round wrote three new
CLAUDE.md rules (R30-8/R30-9/R30-12) that generalise its own lessons. Every
build/lint/test command I ran on `HEAD` is green — a real improvement over
Round 29, which shipped a red `npm run check` and a red CI job.

**There are no P0 findings.** No soundness regression, no build break, and —
the check CLAUDE.md singles out as highest-priority — **no new safe `pub fn`
that accepts a raw pointer and touches allocator metadata was introduced**
(§2).

However, four P1-level defects survive, and three of them are the *exact*
defect class this round's own new rules were written to prevent:

- **P1-1 — R30-3's report and `CHANGELOG.md` state a "direction-consistent
  regression on 48/48 cells" that the round's own committed CSV refutes**
  (5 of 24 recycled cells are ON-*faster*; the stated `+2.1%..+84.1%` range
  is the eager-arm-only range, true range `−32.1%..+136.8%`). This is
  R30-9's own rule violated by a report R30-9 cites approvingly.
- **P1-2 — R30-7's path-activation oracle is single-armed, and the missing
  arm contradicts the report's framing.** The treatment arm's
  `decommit_calls_total` is **identical (40)** to the control's in all 40
  launches; the mechanism the profile is supposed to eliminate was not
  eliminated, so the null result is not "swamped by noise" — the two arms
  are mechanistically indistinguishable. R30-8 (landed two commits later)
  demands *per-arm* mechanism evidence for exactly this reason.
- **P1-3 — R30-7's same-vs-same control does not characterise the real
  comparison's noise** (`sd` 43.4 ms vs 279.0 ms, 6.4×; control mean 171 ms
  vs 695 ms), yet the report calls it "the SAME noise-band shape"; and the
  report never states its minimum detectable effect (~19% of the mean), so
  its null cannot distinguish "no win" from "a win up to ~19%".
- **P1-4 — two of the three new gate reports do not satisfy the
  pre-existing R29-6 immutable-source-identity rule.** R30-3 defers its
  identity to `CHANGELOG.md`, which records only the *base* SHA; R30-7
  cites "base SHA + uncommitted working tree", the precise pattern R29-6
  forbids. R30-6 does it correctly (and needed a follow-up commit to do so).

Eleven P2 findings follow. None of them threatens correctness; several are
one-line fixes.

---

## 1. Build / lint / test health on `HEAD` (actually run)

| command | result |
|---|---|
| `cargo check` (default features) | **exit 0** |
| `cargo clippy --features production --all-targets -- -D warnings` | **exit 0**, no warnings |
| `cargo clippy --all-features --all-targets -- -D warnings` | **exit 0** |
| `cargo test --features "production bench-internals alloc-stats" --no-fail-fast` | **exit 0** |
| `node scripts/run-check-matrix.mjs --kind check --kind test` | **exit 0** (`check-perf-gate-iai-default` green) |
| `node scripts/verify-perf-gate-stubs.mjs` | **PASS** — 79 group members, 2 opt-in features, every one stubbed |

Targeted runs of the round's own new/changed tests:

```
tests/dbg_hook_safety_tripwire.rs        4 passed  (R30-2)
tests/r30_1_decomp_full_cycle_cursor_safety.rs  2 passed  (R30-1)
tests/profile.rs                         4 passed  (R30-7)
tests/no_stale_doc_references.rs         9 passed  (R30-14 + pre-existing)
tests/r14_4_promotion_free_correctness.rs
   --features "production medium-classes"  3 passed  (incl. per-base leak proof)
   --features "hardened medium-classes"    2 passed  (per-base proof correctly absent)
```

The `hardened medium-classes` / `production medium-classes` split reproduces
R30-11's documented coverage claim **exactly** — the per-base leak proof
compiles in the first combination and not the second, as its module doc
states.

`Cargo.toml` still reads `version = "0.3.0"`; the round's only `Cargo.toml`
additions are eight `[[bench]]`/`[[example]]` registrations. **No version
bump, no dependency bump.** No `TODO`/`FIXME`/`todo!()`/`unimplemented!()`/
"placeholder" was added anywhere in `src/`, `tests/`, `benches/`,
`examples/` or `scripts/` (grepped over the full round diff).

---

## 2. The highest-priority check: new raw-pointer-taking safe `pub fn`s

CLAUDE.md's zero-trust bullet names this as the single most important
adversarial check, and it is the exact class R30-1 fixed. **Result: clean.**

Every new `pub` item added to `src/` this round, enumerated from the full
diff:

| new item | signature | pointer? | metadata write? | gate |
|---|---|---|---|---|
| `alloc_core::profile::Profile` | enum, `#[non_exhaustive]` | no | no | `alloc-decommit` |
| `LargeCacheConfig::for_profile` | `const fn(Profile) -> Self` | no | no | (inherits file) |
| `SeferAlloc::with_profile` | `const fn(Profile) -> Self` | no | no | `alloc-decommit` |
| `HeapCore::dbg_large_cache_hits` | `fn(&self) -> u64` | no | no (read-only) | `alloc-decommit` (see P2-2) |
| `AllocCore::reserve_small_segment_impl` | `fn(&mut self) -> Option<*mut u8>` | returns ptr | yes | **`pub(super)`**, not crate-public |

`reserve_small_segment_impl` returns a raw pointer but is `pub(super)` —
invisible outside `alloc_core`, so it is not in the R25-1 class and is not
in the tripwire's scan surface (which matches `pub fn`/`pub unsafe fn` only).
The `bench-internals` gate correctly stays on the `dbg_decomp_*` callers.

R30-1's `dbg_decomp_release` additionally gained a `debug_assert!(base !=
self.small_cur)` defence-in-depth guard — good practice, correctly documented
as non-load-bearing.

`Profile` and the two `const fn` constructors are held to production-code
standards and meet them: `#[must_use]`, `#[non_exhaustive]` (correctly
justified against `LargeCacheMode`'s precedent), no `unsafe`, no allocation,
`const`-evaluable so they work in a `#[global_allocator]` static. The
illustrative examples in all three new doc comments use ` ```text ` fences —
**the "no doctests" rule is respected** (3 fences opened, all ` ```text `,
verified by grep over the diff).

---

## 3. Test non-vacuity (spot-checked, highest-stakes first)

### 3.1 `tests/r30_1_decomp_full_cycle_cursor_safety.rs` — non-vacuous (verified statically)

I did not revert the fix (shared-workspace git safety), so I verified the
counterfactual by reading the two code paths:

- Pre-fix, `dbg_decomp_full_cycle` called `reserve_small_segment()`, whose
  final statement was `self.small_cur = base` (the line the R30-1 diff moves
  out into the wrapper, `src/alloc_core/alloc_core_small.rs:2265` in the
  pre-image).
- `release_or_pool_empty_segment` (`src/alloc_core/alloc_core_small_pool.rs:333`)
  pools only `if self.pooled_count < self.pool_cap`; otherwise it falls
  through to a genuine release + `table.recycle`.
- The test's pre-fill loop runs `pool_cap + 2` cycles and then
  **asserts `dbg_pooled_count() == pool_cap`** — so the following 8 cycles
  provably take the release branch.
- `alloc_small_with_virgin`'s step 1 is
  `self.pop_free(self.small_cur, class_idx, block_size)`
  (`alloc_core_small.rs:278`), read unconditionally. The ordinary
  `ac.alloc(Layout(256, 8))` the test performs next therefore dereferences
  the cursor.

The dangling read is real and the test drives it. The commit message's
claim of an observed `STATUS_ACCESS_VIOLATION` is consistent with this path.
The test also writes/reads back a 0xAB pattern and then allocates 4096 more
blocks — it is not a bare "did not crash" assertion.

### 3.2 `tests/dbg_hook_safety_tripwire.rs` (R30-2) — non-vacuous, one latent gap

`widened_scanner_catches_r30_1_shape_zero_arg_mutator` feeds `scan_file` an
in-memory fixture with the pre-fix shape, asserts it is found / classified
safe / classified ungated / not allowlisted, **and** separately asserts the
fixture text contains neither `*mut` nor `*const` — that last assertion is
what makes it a real counterfactual against the R29-9 scanner rather than a
tautology. `cfg_parser_rejects_negated_and_optional_or_bench_internals`
covers both target shapes plus a `target_os = "bench-internals"` adversarial
leaf. Both are genuine. The `has_bench_internals_cfg` gap is P2-1 below.

### 3.3 `tests/r14_4_promotion_free_correctness.rs` (R30-11) — sound restructure

The split is faithful: the cumulative `released_total <= reserved_total`
assertion moved to `..._no_double_release` with its own message explicitly
disclaiming leak-detection power, and the per-base
`dbg_contains_base`/`dbg_live_count_for` proof moved to a
`#[cfg(all(alloc-decommit, alloc-xthread))]` sibling. The shared setup helper
deliberately stops before the free so each test owns its own ordering — the
right call, since the per-base test needs a `live_count` baseline taken
between trim and free. Verified by running both feature combinations (§1).
Nothing was weakened: the per-base assertion body is byte-equivalent to the
pre-split block.

### 3.4 `tests/profile.rs` (R30-7) — non-vacuous

It reads the **resolved** config back through `dbg_pool_cap()` /
`dbg_decay_config()` rather than trusting the builder, and
`no_profile_reintroduces_the_r27_1_noop_trap` is a real regression guard: if
a future edit raised `pool_segments` without `pool_byte_cap`, the
`min(pool_segments, pool_byte_cap / SEGMENT)` clamp would return 4 and the
`Throughput` assertion would fail. That is exactly the R27-1 trap.

---

## 4. P1 findings

### P1-1 — R30-3's "48/48 direction-consistent regression" is refuted by R30-3's own committed data

**Where:** `docs/perf/R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE.md` §5.2
("recycled ranges +2.1% to +84.1%, ALWAYS in the 'ON slower' direction";
"directionally consistent across every one of 24 cells and both commit
policies (48 cells total), which plain measurement noise would not
produce"), propagated verbatim into `CHANGELOG.md:24` ("the recycled/hot-churn
family shows a small but direction-consistent regression (ON slower on 48/48
cells)").

**What's wrong:** recomputed from
`docs/perf/R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE_summary.csv` (and
cross-checked directly against `_raw_r30_3_off_lazy.log` /
`_raw_r30_3_on_lazy.log` / `_raw_r30_3_off_eager.log` / `_raw_r30_3_on_eager.log`),
there are **24** OFF-vs-ON recycled cell pairs, not 48, and ON is *not*
always slower:

| policy | size | touch | OFF mean ns/op | ON mean ns/op | Δ |
|---|---|---|---:|---:|---:|
| eager | 16k | notouch | 357.5 | 326.9 | **−8.6%** |
| lazy | 128k | full | 143,275.0 | 97,344.4 | **−32.1%** |
| lazy | 16k | full | 12,262.5 | 11,682.5 | **−4.7%** |
| lazy | 16k | notouch | 318.1 | 308.8 | **−2.9%** |
| lazy | 64k | onebyte | 1,951.2 | 1,930.0 | **−1.1%** |

That is **19/24 ON-slower on the mean** (20/24 on `p50`; the `p50`
exceptions are eager/128k/onebyte, eager/4k/full, lazy/16k/full,
lazy/64k/notouch). The quoted range `+2.1% .. +84.1%` is precisely the
**eager-arm-only** min and max (eager/4k/full and eager/4k/onebyte); the
full range across both policies is **−32.1% .. +136.8%**, which also makes
"small" hard to sustain.

**Why it matters:** the sentence is not decorative — it is the report's
stated evidence for a *structural* dispatch overhead ("which plain
measurement noise would not produce"), which is half of the NO-GO verdict's
justification. The NO-GO itself survives (its primary leg is "no material
win on the virgin side"), but a published claim that the report's own
committed CSV contradicts is exactly the class R30-9 §2/§6 was written to
prevent — and R30-9's own text cites R30-3 as a reference-quality judge.

**Suggested fix:** append-only correction to §5.2 restating the honest
numbers (19/24 mean, 20/24 median, range −32.1%..+136.8%), soften
"structural overhead" to "a majority-direction trend consistent with, but
not proven by, extra dispatch bookkeeping", and correct `CHANGELOG.md`'s
"48/48". Better still, per R30-9 point 6, have the generating binary
`assert!` the consistency claim it prints.

### P1-2 — R30-7's activation oracle is single-armed, and the missing arm contradicts the report

**Where:** `docs/perf/R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE.md`
§0 ("Activation oracle" paragraph) and §3.4 point 2; propagated to
`README.md:171-175` ("even though the underlying pool-overflow mechanism is
proven activated in that workload too").

**What's wrong:** re-parsing `docs/perf/_raw_r30_7_server_shaped_ab.log`
(80 RESULT records: 40 `default` + 40 `throughput`):

```
arm default      n=40  decommit_calls_total distinct: [40]  large_cache_hits: [45]
arm throughput   n=40  decommit_calls_total distinct: [40]  large_cache_hits: [45]
```

The treatment arm's decommit count is **bit-identical to the control's in
every single launch**. R27-4's mechanism — the one `Profile::Throughput`
exists to buy — is "9 → 0 decommit calls/run". Here it is 40 → 40. Whatever
the profile changed in this workload, it did not change the mechanism the
comparison is about.

The report reports only the `default` arm's value and reads it as "the
mechanism the throughput profile targets WAS genuinely activated ... so this
is not a vacuous 'the workload never touched the mechanism' null." That
inference does not follow: the oracle rules out one vacuity mode (workload
never touches the mechanism) while the *other* mode — treatment never
changes the mechanism — is affirmatively demonstrated by the arm the report
does not print. §3.4 point 2 then compounds it, using the **absence** of
between-arm difference as proof the compiled config took effect ("a
mis-wired config would show materially different segment counts between
arms, which the raw log does NOT show").

For completeness, the config almost certainly *did* take effect —
`segments_reserved_total` reaches 62/64 in the throughput arm and never
below 68 in the default arm — but that is a weak, incidental signal, not the
oracle the report claims.

**Why it matters:** §2 offers three noise-based hypotheses for why the win
vanishes and explicitly says none is proven; the simplest explanation
consistent with the report's own data — the two arms did the same amount of
decommit work — is not among them. This is the precise failure R30-8 (commit
`3c414d8`, two commits later) generalises R26-4 to prevent: *per-arm*
mechanism-activation evidence. R30-8's text names R30-3 and R30-6 as the
first compliant applications and is silent about R30-7, its own round-mate.

**Suggested fix:** append a §0.1 to the report printing
`decommit_calls_total` for **both** arms, state plainly that the mechanism
delta is zero at this workload, and add that as hypothesis 0 in §2 (ahead of
the three noise hypotheses). Correct the README sentence to "the mechanism
fires in both arms *identically*, so this workload does not separate them"
rather than "proven activated".

### P1-3 — R30-7's control does not characterise the real comparison's noise; no minimum-detectable-effect is stated

**Where:** same report, §0 ("A same-vs-same honesty control ... shows the
SAME noise-band shape") and §0's table.

**What's wrong:** from the report's own
`R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_summary.csv` and the two raw
logs:

| run | n | mean Δ | **sd** | se | t | mean elapsed |
|---|---:|---:|---:|---:|---:|---:|
| default vs throughput | 20 | −7.44 ms | **279.02 ms** | 62.39 ms | −0.119 | ~695 ms |
| default vs default (control) | 20 | −10.09 ms | **43.41 ms** | 9.71 ms | −1.039 | ~171 ms |

The control's dispersion is **6.4× tighter** and its absolute runtime **~4×
faster** than the comparison it is supposed to validate (per-launch
`elapsed_ns` ranges: 136–1,910 ms for the real comparison vs 115–420 ms for
the control). The two runs were not taken under comparable host load — the
report itself discloses "this host is shared with other concurrent agent
work". So "the SAME noise-band shape" is contradicted by the report's own
committed CSV; only the *sign split* is comparable, and a sign split is a
much weaker statistic than the variance it is standing in for here.

Second, the report never states its power. With `se = 62.39 ms` and
`crit = 2.101`, the smallest detectable difference is `2.101 × 62.39 ≈
131 ms` ≈ **18.8% of the ~695 ms mean**. The 95% CI on the mean Δ is
`[−138.5, +123.6] ms`. So the study can exclude R27-4's ~22% figure only
barely, and cannot exclude a win of, say, 15%. The headline sentence is
worded carefully ("not ... statistically distinguishable"), but §4's
takeaway ("the win is workload-shape-dependent") and README's "Treat the
~22% figure as workload-shape-specific" both read as a substantive
finding, which the power does not support at this strength.

**Suggested fix:** append a note giving the minimum detectable effect and
the CI, state that the control was run in a different load regime and
therefore bounds the harness's *self-consistency* rather than the real
comparison's noise, and either re-run both comparisons back-to-back or label
the null explicitly as underpowered.

### P1-4 — two of three new gate reports miss the R29-6 immutable-source-identity rule

This is a **pre-existing** CLAUDE.md rule (added R29-6/task #437), not one
this round wrote, and R30-9 point 7 restates it in stronger form in this very
round.

| report | landing SHA cited? | what it actually cites |
|---|---|---|
| `R30_6_LARGE_CACHE_HEADROOM_AB_GATE.md` | **yes** — `97c2f07bf5c…` (§ header, filled in by follow-up `1272a52`) | correct, form 1 |
| `R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE.md` | **no** | "base commit `50d5adc…` … that commit's own SHA (recorded in `CHANGELOG.md`'s Round 30 section for this task)" — but `CHANGELOG.md:24` records only the **base** commit `50d5adc…`. `d8f467b` appears nowhere in the report or its CSV. The summary CSV's `commit_sha` column is `50d5adc…`, a commit at which `benches/r30_3_virgin_zero_skip_native_gate.rs` **does not exist** (`git cat-file -e 50d5adc:benches/… ` → not in tree). |
| `R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE.md` | **no** | "`main` @ `1272a522…` + this task's uncommitted working tree" — verbatim the pattern R29-6 forbids. No temp-commit SHA, no `write-tree`, no patch hash, no binary hash. Its CSV records `commit=1272a52…, git_dirty=true`. `b5efe8c` appears nowhere. |

Both reports point at "the commit this lands in" without naming it, which is
resolvable by a human with `git log` but is not a citation. R30-6 needed a
dedicated follow-up commit to close this; R30-3 and R30-7 did not get one.

**Suggested fix:** two one-line follow-up commits, exactly like `1272a52`,
adding `d8f467b…` and `b5efe8c…` to the respective report headers, and
correcting R30-3's summary-CSV `commit_sha` column (or adding a
`landing_commit` column).

---

## 5. P2 findings

### P2-1 — `has_bench_internals_cfg` accepts `#[cfg_attr(...)]` as a gate (latent, same class R30-2 fixed)

`tests/dbg_hook_safety_tripwire.rs:657` matches the 5-byte prefix `#[cfg`,
which also matches `#[cfg_attr(`. The parser then reads `cfg_attr`'s first
argument (its *predicate*) as if it were a `cfg` predicate.

Proven, not inferred — I extracted lines 471–702 verbatim into a standalone
`rustc` binary outside the repo:

```
cfg_attr(feature = "bench-internals", allow(dead_code))            -> true
cfg_attr(all(... feature = "bench-internals"), allow(dead_code))   -> true
cfg(feature = "alloc-decommit")                                    -> false
```

A `#[cfg_attr]` mentioning `bench-internals` on a `dbg_*` hook would be
silently treated as a genuine gate and the hook would drop out of the
at-risk set. **No live instance exists today** (no `cfg_attr` in `src/` or
`crates/` mentions `bench-internals` — grepped), which is exactly the status
R30-2 gave the two shapes it *did* fix. One-line fix: match `#[cfg(`
(including the open paren) instead of `#[cfg`.

### P2-2 — `HeapCore::dbg_large_cache_hits` (new, R30-6) is the only new measurement delegation in its file not `bench-internals`-gated

`src/registry/heap_core_diag.rs:352-357` gates the new hook on
`alloc-decommit` alone, justified as "matching `AllocCore::dbg_large_cache_hits`'s
own gate exactly". `alloc-decommit` is inside `production`
(`Cargo.toml:399`), so this widens a production build's safe public surface.
The same file's four other measurement delegations all chose the opposite:
`dbg_pool_cap` (`:302`), `dbg_segment_state_reconciliation` (`:318`),
`dbg_large_cache_used` (`:334`), `dbg_large_cache_slot_sizes` (`:373`) are
each `all(alloc-decommit, bench-internals)` and each cites
"no production caller → R25-10 sub-rule 2" in its own doc.

CLAUDE.md's benchmark-hook rule 2 says any hook with no production caller
MUST default to `bench-internals`, with `dbg_push_to_ring` as the *only*
sanctioned exception. The hook is read-only (`&self -> u64`, no pointer, no
mutation) so this is not a soundness issue, and the R30-2 tripwire is
satisfied because R30-6 added it to `PURE_OBSERVERS`
(`tests/dbg_hook_safety_tripwire.rs:240`) — but "the delegated method's
pre-existing gate" is precisely the reasoning rule 2 rejects for *new*
hooks. Suggested fix: add `feature = "bench-internals"` to its `cfg` and
adjust the tripwire list accordingly (the R30-6 probe already requires
`bench-internals`, so nothing breaks).

### P2-3 — R30-6's idle-RSS claim cites the wrong column pair and is false as written in all 36 rows

`R30_6_LARGE_CACHE_HEADROOM_AB_GATE.md` §0.1: "RSS at burst2 and RSS after
the 1200 ms idle window are identical to the KiB (see raw log;
`rss_idle_kib - rss_burst2_kib` is 0 or within single-digit KiB noise in
every row)".

Recomputed over all 36 CSV rows in
`_raw_r30_6_large_cache_headroom_ab_gate.log`: `|rss_idle − rss_burst2|` is
**16–28 KiB at 1 thread, 180–212 KiB at 8, 756–848 KiB at 32**, and
`−1,574,932 KiB` in one row. Not a single row satisfies the stated bound.
The comparison is also structurally wrong — `burst2` is measured *after* the
idle window, so a difference is expected. The claim the report actually
wants is `rss_idle − rss_burst1 == 0`, which **is** exactly satisfied in
33/36 rows (and off by 4 KiB in two more). The finding is right; the cited
arithmetic is not.

### P2-4 — R30-6 silently includes an impossible RSS sample

Row `67108864,64,32,2` reports `rss_burst1_kib = 1,580,920` and
`rss_idle_kib = 424` — a 32-thread process cannot drop to 424 KiB RSS across
a 1.2 s sleep. §3 discusses this same cell's *timing* outlier (5.02 s) but
not its impossible memory reading, and the row is neither excluded nor
flagged. Medians protect §0.1's table, so no headline changes; but a
probe that can emit a physically impossible sample deserves a note, and
ideally a sanity assertion in the harness.

### P2-5 — R30-6's headline joins two findings from mutually exclusive regimes

The headline is "64 MiB preserves the FULL measured hit-rate benefit of the
256 MiB default … while RSS retention drops … roughly 7× smaller". Both
halves are individually sourced, but they cannot both apply to one workload:

- The hit-rate parity is measured at 48 MiB/burst, which §1.4/§2 correctly
  explain is **below both** the 64 MiB and 256 MiB targets, so
  `maybe_decay_large_cache`'s early-return fires unconditionally for both
  arms. The 100%-vs-100% result is therefore structurally guaranteed, not an
  empirical discovery, and says nothing about hit rate at 64 MiB in a
  workload whose cache occupancy exceeds 64 MiB.
- The ~7× RSS saving is R29-13's, measured at 272 MiB/burst — i.e. exactly
  the regime where 64 MiB **does** bind and where the hit-rate cost is
  unmeasured. (R29-13's floors are also *post-forced-drain* fixed points;
  R29-13's own verdict is that idle reclaims zero bytes in either config.)

§2's last paragraph is honest about the burst-size choice, so the report
body is defensible. The problem is downstream: `Profile::Balanced`'s and
`Profile::Throughput`'s **shipped doc comments**
(`src/alloc_core/profile.rs:38-45, 68-84`) and `README.md`'s profile table
carry the headline (`"full 100.0 % hit-rate parity … at ~7× less RSS"`)
without the regime caveat. A user reading the API doc has no way to know the
parity was measured where the knob is inert.

### P2-6 — `Profile::Throughput`'s doc omits its own task's deliverable-4 null

`src/alloc_core/profile.rs:68-84` cites "~22% lower elapsed time, 9→0
decommit calls/run on the 1024 B batch-120 churn-with-teardown workload" —
correctly workload-scoped, but with no pointer to
`R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE.md`, produced by the *same
task, in the same commit*, finding the win does not reproduce. `README.md`
does disclose it (lines 166-179); the API doc a caller actually reads at the
call site does not. One sentence + a link closes it.

### P2-7 — R30-5's ci.yml comment asserts an enforcement that does not exist

`.github/workflows/ci.yml`'s `clippy` job header says its 5 rows "are now
GENERATED from `scripts/check-matrix.mjs`'s `PER_PR_ROWS`" and that "if this
job's hand-listed steps ever diverge from the manifest, the `check-matrix`
job runs the manifest's OWN version of each row independently and would still
catch a regression".

Neither holds for clippy: the steps are hand-transcribed (the same comment
concedes this two paragraphs later), and the `check-matrix` job runs
`node scripts/run-check-matrix.mjs --kind check --kind test`, whose filter
**excludes every `clippy` row by construction**
(`run-check-matrix.mjs:60-62`). Nothing in CI or `tests/` asserts that
ci.yml's five clippy steps match the manifest — I grepped for it. The local
`npm run check` *does* generate them from the manifest
(`scripts/check-all.mjs`), so the drift risk is CI-only, but the comment
claims a guarantee the repo does not provide. Also cosmetic: the same
comment says "Deliberately `--kind check` here" while the step passes
`--kind check --kind test`.

### P2-8 — R30-10's "160 total hooks" is the *classified* total, not the total

`docs/design/R30_10_MEASUREMENT_HOOK_ISOLATION_DESIGN.md` §2.1's table
labels `99 + 39 + 22 = 160` as "**Total**", and §2.3 says "all 160 hooks".
Those three lists are, by the tripwire's own construction, only the *safe
and ungated* plus *unsafe* hooks — the **safe, `bench-internals`-gated**
hooks are in none of them. Reproducing the tripwire's scanner in Python over
`src/` + `crates/` gives **179** `pub fn dbg_*`/`pub unsafe fn dbg_*`
definitions: 22 unsafe + ~20 safe-and-gated + ~137 safe-and-ungated.

Independent reproduction of the rest of the survey:

| claim | reproduced |
|---|---|
| 99 `PURE_OBSERVERS` / 39 `SAFE_MUTATORS` / 22 `UNSAFE_HOOKS` | **exact** |
| 18 files define hooks | **exact** |
| 102 test/example/bench files touch `SAFE_MUTATORS ∪ UNSAFE_HOOKS` | **exact** |
| 139 files touch all three buckets | I get **151** |
| ~227 test files | **exact** (227) |

Both discrepancies understate the relocation cost, so the decline decision
is safe *a fortiori*; but "160" and "139" should be corrected, since the
decline explicitly rests on them.

### P2-9 — R30-14's "byte-identical in the archive" claim overstates (substance is fine)

Commit `4c52c26`'s message says of the item-13 compaction: "Verified before
compacting: every fact removed is already byte-identical in
`OPEN_ITEMS_ARCHIVE.md` section A13 (grep-checked against the archive text
directly)." Diffing the 16 removed lines against the archive, **none is
byte-identical** anywhere (the removed bullets are R27-era current-state
summaries; A13's narrative predates them).

The *substance* does hold — I checked each load-bearing fact and all are
present in the archive and/or the rewritten card: `t=8.114`,
`~+8 MiB/heap`, `~255 MiB`, `R27_3_POOL_RETENTION_GATE`,
`R27_4_REAL_DEFAULT_AB_GATE`, `R24_11_TEARDOWN_RESIDUAL_ROOTCAUSE`, the
refuted `34%` RSS claim, `REOPENED`, and the `NO-OP` byte-cap note. So this
is a wording defect in a commit message, not lost history. Worth noting only
because "byte-identical, grep-checked" is the kind of claim a future reviewer
would take at face value.

Separately, and to the round's credit: **the four Round-29 gate reports
R30-4 corrected are strictly append-only** — `R29_13` +106/−0, `R29_16`
+82/−0, `R29_3` +147/−0, `R29_5` +54/−0 — and `docs/CORRECTNESS_OPEN_ITEMS.md`
is +266/−0. The only substantive deletions anywhere are in
`docs/perf/OPEN_ITEMS.md`'s current-state cards (by design — that file is a
live index, and CLAUDE.md's convention for it is "move closed items to the
Recently-resolved trail", which R30-14 followed) and
`docs/FEATURE_PROMOTION_STATUS.md`'s survey table (+46/−16, a status table).

### P2-10 — R30-6's summary CSV has a prose placeholder where its commit SHA belongs

`docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE_summary.csv:2` reads
`# commit_sha=<see report header, this file is committed alongside the same
commit that lands the measured tree>`. The whole point of the summary-CSV
rule (CLAUDE.md, R14-10) is that a script can track a metric across rounds
without parsing prose; a prose placeholder in the SHA field defeats it. The
report header *does* now carry `97c2f07…` (follow-up `1272a52`), so this is
a one-token fix.

### P2-11 — small inaccuracies worth a sweep

- `R30_7_…_GATE.md` §0 and §6 say "20 pairs, **40** process launches"; the
  raw log contains **80** launches per comparison (40 per arm). Its sibling
  `R30_6_…` §1.6 states this correctly ("20 pairs = 80 process launches").
- `scripts/check-matrix.mjs:52-56` types `@property {string} [target]` while
  every row uses an object `{flag, name}` (`:107`). Non-functional, but the
  file is billed as the single source of truth.
- `docs/design/R30_10_…md` §2.1's `awk` reproduction commands are accurate
  and did reproduce — good practice worth keeping.

---

## 6. Round-internal rule self-consistency (R30-8 / R30-9 / R30-12)

The prompt asks whether the round's own late tasks obey the rules the round
itself wrote. Checked commit-by-commit against `git log --oneline`:

- **R30-12 (`3f7db16`) commit-prefix taxonomy.** Applies to commits landing
  *after* it. Those are `df666b5` **`docs(config)`** (R30-13 — a pure doc
  comment on `SeferAlloc::new()` plus README; no code changed → correct per
  the taxonomy), `4c52c26` **`docs(perf)`** (docs + one test file → not a
  `perf(...)` prefix, so not in scope of the restriction; acceptable), and
  `c141d76` **`docs:`**. **Compliant.**
  For the record, the round's two earlier `perf(...)` commits —
  `97c2f07 perf(large-cache):` (measurement-only; would be `bench` under the
  new taxonomy) and `b5efe8c perf(profiles):` (opt-in code; would be
  `perf(opt-in)`) — predate the rule, which is explicitly non-retroactive.
  The irony that R30-12's own motivating example is reproduced twice inside
  its own round is worth a line in the rule's text.
- **R30-8 (`3c414d8`) path-activation oracle.** R30-3 (`benches/r30_3_…rs`,
  `MIN_ACTIVATION_PCT = 95`, hard-asserted) and R30-6 (§1.3, `admissions_ok`
  **and** `hits_ok`, all 36 arms, `oracle_pass = 1` in every CSV row —
  independently verified from the raw log) both genuinely carry one, as the
  rule claims. **R30-7 does not carry a per-arm one** — see P1-2. R30-7
  landed two commits before R30-8, so it is not formally in scope; but
  R30-8's own text enumerates the compliant reports and does not mention its
  round-mate at all, which reads as an oversight rather than a deliberate
  exemption.
- **R30-9 (`374c6d1`) derived-not-transcribed tables.** Landed after R30-3
  and R30-6, so neither is formally in scope. R30-3's summary CSV *is*
  genuinely derived (its rows are byte-identical to the harness's own
  `R30_3_ROW` stdout lines — I diffed them), which is the good half of the
  rule; but its §5.2 prose was hand-generalised from the eager arm and got
  the claim wrong (P1-1) — the exact defect R30-9 point 6 targets, occurring
  in the same round the rule was written. R30-6's §0.2 latency table matches
  its raw log's four summary blocks **exactly** (`t = −0.503 / −0.144 /
  0.173 / −1.134`, sign splits `12/8, 7/13, 10/10, 14/6`) — verified
  line-for-line.

---

## 7. What the round got right (recorded so the findings above are read in proportion)

- **R30-1** is a correct, minimal, well-scoped soundness fix. The
  wrapper/impl split is the right shape (production callers keep the
  cursor-publishing wrapper; only the measurement hooks bypass it), the doc
  comments explain the hazard at both call sites, and the `debug_assert` in
  `dbg_decomp_release` is honest defence-in-depth rather than theatre.
- **R30-2** is the rare case of a project correcting its own "this closes
  the class for good" claim without being forced to. The shape-independent
  rewrite is a real generalisation (the scanner no longer inspects signature
  text at all), the `cfg` grammar is parsed structurally, `any(...)` is
  deliberately refused even in the degenerate all-branches case, and the
  non-vacuity test proves the *old* scanner would have missed the fixture.
- **R30-3**'s oracle caught a genuine bug in its own first attempt (6.25%
  activation from `carve_block_with_refill`'s 31-block refill) *before* a
  number shipped. That is the entire value proposition of the technique,
  demonstrated in-round.
- **R30-4** re-verified 8 prior review findings and was willing to REFUTE one
  of them with an argued mechanism, while adding the methodological caveat
  that made the refutation honest rather than defensive — and did all of it
  append-only.
- **R30-5** closes both of Round 29's real escape routes with actually-running
  checks (I ran both and they pass), and `verify-perf-gate-stubs.mjs` is a
  genuinely mechanical form of a rule that was previously prose.
- **R30-6** is, methodologically, the best gate report in the round: all four
  R26-4 config-identity pieces present per row, a two-condition oracle
  hard-asserted on all 36 arms, subprocess isolation that is structural
  rather than asserted, and §0.2's table reproduces from its raw log
  exactly. Its problems are framing, not method.
- **R30-11** does something unusual and valuable: it renames a test to
  *narrow* what it claims, and documents a feature-combination coverage gap
  rather than papering over it. Both documented combinations behave exactly
  as its module doc says (verified by running them).
- **R30-14**'s new
  `every_undecided_feature_has_exactly_one_owner_with_a_next_trigger` test
  turns the R18-8/R22-3 "silently dropped follow-up" class into a mechanical
  check, and the task found and closed three genuinely zero-owner features
  in the process.

---

## 8. Overall verdict

**Round 30 is safe to consider done as committed — nothing in it needs to be
reverted, and nothing blocks further work on top of it — but four P1 items
should be closed before its measurement conclusions are cited anywhere
downstream.** The shipping allocator is sound: there are no P0 findings, no
new safe raw-pointer hook, no version bump, no scope creep, and every build,
lint and test command I ran on `HEAD` is green, including the two feature
combinations Round 29 shipped red. The soundness fix at the round's centre is
correct and its counterfactual is real. What does not hold up is a subset of
the round's *reporting*: R30-3 published a "48/48 direction-consistent
regression" that its own committed CSV refutes (19/24, with five ON-faster
cells and a range twice as wide as stated); R30-7 built a single-armed
activation oracle whose missing arm shows the treatment changed the
mechanism by exactly zero, called a 6.4×-tighter control "the same noise
band", and drew a workload-shape conclusion from a comparison underpowered
below ~19%; and R30-3 and R30-7 both fail the pre-existing R29-6
immutable-source-identity rule that R30-6 satisfied in the same round. The
uncomfortable pattern is that three of those four are the exact defect
classes R30-8 and R30-9 were written — days apart, in this same round — to
prevent, which suggests the new rules need to be applied retroactively to the
round that authored them, not merely "going forward". All four are closable
with append-only corrections and two one-line SHA follow-ups, in the same
style the round already used for `1272a52` and `9335979`; none requires a
re-measurement except, optionally, a back-to-back re-run of R30-7's two
comparisons under comparable host load. Until then, `Profile::Throughput`'s
and `Profile::Balanced`'s shipped doc comments should be treated as the
round's most externally-visible overclaims (P2-5, P2-6), since they are what
a downstream user reads.
