# `tagged-index-stack` — independent publish-readiness review, round 9

- **Reviewer:** Claude (Opus 5), adversarial pass. `src/lib.rs`, all seven `tests/*.rs`,
  `benches/tagged_index_stack_bench.rs`, `examples/backoff_per_call_latency.rs`, `README.md`,
  `CHANGELOG.md`, `Cargo.toml`, `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` + its committed CSV and
  all three raw logs, `scripts/tis_backoff_cap_sweep_derive_report_data.mjs`, and the `ci.yml`
  rows covering the crate were read first, and every finding below was formed by running
  something or by reading the current files — not inherited from a prior report. The round-8
  report was read only for its *format* and to know which claims are new.
- **Date:** 2026-08-31 13:00:17 +0200 (CEST)
- **Revision reviewed:** `a1f9dc51bfbffeed57229f6f46a5e199d289b9ec` (`main`).
  `git status --porcelain -- crates/tagged-index-stack docs/perf scripts .github` is empty —
  the reviewed tree is clean, and it is still clean at the end of this review.
  Crate source identity: `sha256(crates/tagged-index-stack/src/lib.rs) =`
  `5627ed352504893443421911950be27be270bf73e276fd82bf8e46e33f8a1247`.
- **Scope:** `crates/tagged-index-stack/**`, `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` (+
  `_summary.csv` and all three `_raw_tis_backoff_*.log`),
  `scripts/tis_backoff_cap_sweep_derive_report_data.mjs`, `.github/workflows/ci.yml`'s `msrv` /
  `test workspace members` / `loom-alloc-global` rows, and the in-workspace consumers
  (`src/registry/heap_registry.rs`, `src/registry/bootstrap.rs`).
- **Machine / toolchain for every measured number:** `rustc 1.97.0 (2d8144b78 2026-07-07)`,
  host `x86_64-pc-windows-msvc`, 11th Gen Intel Core i7-11800H (8 cores / 16 logical),
  Windows 10 Pro 19045. Shared dev host, no core pinning.
- **No file in the repository was modified.** Read-only review. The two instrumented A/B probes
  behind **P3-4** and the `#[track_caller]` result ran in a throwaway copy of the crate OUTSIDE
  the repository (`D:/dev/rust/.scratch-tis-r9`, deleted afterwards); the in-repo bench was
  deliberately NOT run, so the tracked root `bench-iters.txt` was never touched. The
  `--write` idempotency test in **P2-1** likewise ran against a copy of the artifacts outside
  the repo, so the committed CSV was never rewritten.

## Verification actually performed

Every number below came from running something, not from reading.

| Check | Result |
| --- | --- |
| `cargo test -p tagged-index-stack --no-fail-fast` | **29 green** (18 `stack_unit`, 5 `proptest_pack_unpack`, 2 `regression_counter_wrap`, 2 `custom_links_impl`, 1 `readme_example`, 1 `threaded_conservation`; `loom_aba` correctly 0; 0 doctests) |
| `cargo test -p tagged-index-stack --release --no-fail-fast` | **29 green**; `threaded_conservation` 0.10 s release / 0.26 s debug |
| `RUSTFLAGS="--cfg loom" cargo test --release -p tagged-index-stack --features loom --test loom_aba` | **11 green in 0.16 s**, including round-8's new `pop_pop_single_element_loser_sees_empty_actual` and all three `#[should_panic]` counterfactuals |
| `cargo clippy -p tagged-index-stack --all-targets -- -D warnings` | clean |
| `RUSTFLAGS="--cfg loom" cargo clippy -p tagged-index-stack --features loom --all-targets -- -D warnings` | clean |
| `cargo fmt -p tagged-index-stack --check`, `RUSTDOCFLAGS="-D warnings" cargo doc -p tagged-index-stack --no-deps` | clean |
| `node scripts/tis_backoff_cap_sweep_derive_report_data.mjs` | **ALL 161 ASSERTIONS PASSED**, exit 0 — the brief's (a) holds for the verify path |
| `node scripts/tis_backoff_cap_sweep_derive_report_data.mjs --write` (out of repo) | **exit 1, uncaught `TypeError: Cannot read properties of undefined (reading 'run')` at line 492** — **P2-1** |
| `awk -F, '{print NF}' TIS_BACKOFF_CAP_SWEEP_GATE_summary.csv \| sort \| uniq -c` | **53 rows × 20 fields, 18 rows × 23 fields** — **P2-1** |
| `csv.DictReader` over the same file | latency rows read `pop_p999_ms = 0.000`, `pop_max_ms = 0.000`, `pop_over_100ms = 10.828`, `wall_ms = 4`, plus 3 unnamed extras — **P2-1** |
| §3.4's medians/ratios re-derived from `_raw_tis_backoff_per_call_latency.log` by hand | **every cell in §3.4's table reproduces exactly** (medians 4.813/0.159, 54.464/2.031, 160.092/42.335; walls 14.6/61.0, 321.9/1560.3, 867.5/3509.6) |
| the SAME log's 16-thread `pop_over_1ms` column | cap 6 `{285, 266, 249}` vs cap 0 `{553, 661, 650}` — **cap 0 has 2.2-2.6× MORE pops over 1 ms**, the cell §3.4 does not cite — **P2-2** |
| `grep -rl "starv" <rendered rustdoc>` and `grep -n "starv" README.md` | **no match in either** — **P2-3** |
| `cargo package -p tagged-index-stack --no-verify --allow-dirty --list` | **18 files** (17 + the new example), no strays; **no `docs/perf/` content** — **P2-3 / P4-8** |
| `git cat-file -e 842c998:crates/tagged-index-stack/examples/backoff_per_call_latency.rs` | **does not exist at the commit §3.4 cites as its source identity** — **P3-3** |
| §2's three raw-log byte counts (23,342 / 8,564 / 6,365) | **exact** |
| crate-root doc's "53.89 ns/pair … 20 samples spanning 51.41-64.72" vs `_raw_..._run1.log` | **exact** (20 `churn` rows, first 53.89, min 51.41, max 64.72) — round-8 P4-3 is properly closed |
| out-of-tree A/B, retry counters ON vs stripped, 8 threads, 9 interleaved reps/arm | 27.36 M vs 27.97 M ops/sec (+2.2 % for the counter-free build) — **but the single-threaded control, where the counters provably never execute, shows the same-direction +1.6 %**, so the gap is build layout, not the counters — **P3-4** |
| out-of-tree A/B, `#[track_caller]` on `pop` present vs removed, 9 reps/arm | 51.73 vs 51.45 ns/pair, inside a 50.27-54.11 ns spread — **no resolvable cost** (see "Checked and clean") |

---

## Overall verdict: **CONDITIONAL-GO**

**The shipping algorithm is still correct, and round 8's two code changes are both sound.** I
re-derived this rather than inheriting it:

- **The new loom model is real and its strong claim holds.** `pop_pop_single_element_loser_sees_empty_actual`
  (`tests/loom_aba.rs:1055-1109`) seeds exactly one index through the real `push`, races two real
  `pop`s, and the only head transition the model admits is `(0, t) -> (empty, t)` — the sole
  writer inside the concurrent window is the winning `pop`. So every `POP_RETRY_COUNT` increment
  its oracle asserts is provably a CAS failure against an *empty* `actual`, which is exactly the
  `is_empty(actual) == true` skip-backoff arm round-8 P3-4 found uncovered. I traced the other
  candidate models to confirm the "no other shipped model or test reaches it" claim: 
  `pop_pop_conservation` (2 elements) always fails with the *remaining* element as `actual`;
  `threaded_conservation.rs` and `examples/backoff_per_call_latency.rs` both prefill 64 with at
  most `threads` in flight, so the stack provably never empties. The claim is accurate.
- **Making `POP_RETRY_COUNT`/`PUSH_RETRY_COUNT` unconditional adds no race surface.** They are
  real `core` `AtomicUsize`, `Relaxed`, written only on the `Err(actual)` arm, read by nothing in
  the algorithm; `push`/`pop` outcomes are unchanged. The out-of-tree A/B above could not resolve
  a cost above the build-layout noise floor.
- **Group3's `bootstrap.rs` shim comment is accurate** against the shim code I read line by line
  (no backoff, no push bounds guard, no pop rule-4 guard — all three confirmed absent, all three
  correctly argued irrelevant), and the new MSRV rows do compile what they claim: `cargo test -p
  tagged-index-stack --no-run` builds the seven `tests/` targets *and* the new `examples/` target
  on 1.88, and `cargo bench -p tagged-index-stack --no-run` is genuinely the only thing in that
  job that type-checks the bench (the crate sets `test = false` on it).

**What holds this back from an unconditional GO is three P2s, and all three are about round 8's
own remediation of round 8's two P2s.** This is the fourth consecutive occurrence of the
campaign's meta-pattern:

- **P2-1** — the machine-readable artifact added to close round-8 P3-3 is **structurally
  malformed** (18 rows carry 23 fields against a 20-column header, so every latency value is
  read under the wrong header name) **and cannot be regenerated by its own generator** (`--write`
  crashes on it). The report even cites one of those columns by name, and the value under that
  name is not the value cited.
- **P2-2** — §3.4, added to close round-8 P2-2, **selects the tail-mass cells that support its
  thesis and omits the one in the same committed log that contradicts it**: at 16 threads the
  shipped cap 6 has 2.2-2.6× *fewer* pops over 1 ms than cap 0. The sentence "Tail mass tells the
  same story per call-count" is false against §3.4's own data, and the incomplete framing
  propagated verbatim into `CHANGELOG.md` and `BACKOFF_SPIN_CAP`'s doc comment. This is round-8
  P2-1's defect class (a conclusion contradicted by the report's own committed evidence) in a new
  location.
- **P2-3** — the "lock-free but not starvation-free" warning, the whole user-facing half of
  round-8 P2-2's fix, **reaches no consumer of the published crate**: it lives on a *private*
  `const`, which rustdoc does not render (verified: `starv` appears nowhere in the generated
  HTML), it was never added to `README.md` as that finding's fix asked, and the report it points
  at is not in the `cargo package` tarball.

None of the three requires changing `BACKOFF_SPIN_CAP` or any shipping code. All three require
the published artifacts to be what they claim to be.

---

## Findings

### P2-1 — the summary CSV's 18 latency rows are structurally malformed (23 fields against a 20-column header), so every latency value reads under the wrong header name; and `--write`, the only path that produces the file, crashes on the file it produced

**Files:** `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE_summary.csv` (rows 54-71);
`scripts/tis_backoff_cap_sweep_derive_report_data.mjs:518` (the row template),
`:485-527` (the `--write` block), `:171-201` (the verify block);
`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md:155-164` (§2's description of the file) and `:360-362`
(the prose that cites a column by name).

The header declares **20** columns:

```text
run,cap,threads,rep,bench,total_ops_per_sec,per_thread_max,per_thread_min,per_thread_mean,
max_over_min,min_over_mean,pop_p50_ms,pop_p90_ms,pop_p99_ms,pop_p999_ms,pop_max_ms,
pop_over_1ms,pop_over_10ms,pop_over_100ms,wall_ms
```

The 52 sweep rows carry 20 fields. The 18 `run=3` latency rows carry **23** — the writer emits
**nine** empty fields where the header has only **six** throughput columns:

```text
3,6,8,1,pop_latency,,,,,,,,,,0.000,0.000,0.000,0.001,41.496,86,34,0,333.9
                    ^^^^^^^^^ 9 empties, not 6
```

Measured: `awk -F, '{print NF}' … | sort | uniq -c` → `53 20` / `18 23`. Every latency value is
therefore shifted **three columns right**. Reading the file the way a summary CSV exists to be
read — by header name — gives, for row 54:

```text
pop_p50_ms='', pop_p90_ms='', pop_p99_ms='',
pop_p999_ms='0.000', pop_max_ms='0.000', pop_over_1ms='0.000',
pop_over_10ms='0.000', pop_over_100ms='10.828', wall_ms='4',
None: ['1','0','14.6']
```

(verbatim `csv.DictReader` output). `pandas.read_csv` does not silently misassign — it raises
`ParserError: Error tokenizing data … expected 20 fields, saw 23`.

The report's own prose walks straight into this. `TIS_BACKOFF_CAP_SWEEP_GATE.md:360-361` states
"99.9 % of cap-6 pops at 8 threads completed within ~1 microsecond (`pop_p999_ms` = 0.001 **in
the CSV rows**)". The column named `pop_p999_ms` in those rows holds `0.000`; `0.001` is the
value three columns further right. The claim is true of the JSON log and false of the artifact it
cites.

The script does not catch this because it addresses the same wrong offsets it wrote:
`:175` checks `vals.slice(5, 14)` are empty and `:198` reads the nine values from
`vals.slice(14, 23)`. Its own assertion message is the tell — it says "the 6 throughput fields
and 4 spacers are empty" (that is 10) while checking 9 fields (see **P4-6**).

Independently, `--write` — line 20 of the script's own usage block, and the only mechanism that
produces the CSV — **crashes against the committed CSV**, because its pre-check loop
(`:488-510`) feeds every row including the 18 `run=3` rows into `findRec`, which returns
`undefined` for them, and then dereferences `rec.run`:

```text
$ node scripts/tis_backoff_cap_sweep_derive_report_data.mjs --write
…
TypeError: Cannot read properties of undefined (reading 'run')
    at …/tis_backoff_cap_sweep_derive_report_data.mjs:492:18
$ echo $?
1
```

(run against a copy of the artifacts outside the repo, so nothing was rewritten).

**Failure scenario:** the round-8 remediation's stated purpose was to make the report's numbers
"grep/diff-able across rounds" by one checked script. A future round runs `--write` to refresh
the CSV after a re-measurement and gets a stack trace; it then hand-edits the file, which is
exactly the hand-transcription CLAUDE.md's derived-tables rule forbids. Or a downstream script
reads the CSV by header, silently gets `pop_max_ms = 0.000` for every latency row, and concludes
the backoff has no tail at all — the precise opposite of what §3.4 found.

**Fix:** emit six empty fields (not nine) for the latency rows so all 71 rows are 20 columns
wide; correct the verify block's slice offsets and its assertion message to match; and make the
`--write` pre-check skip `bench === 'pop_latency'` rows (or rebuild from `sweepRows` rather than
`csvRows`) so the script is idempotent against its own output. Re-check §3.4's `pop_p999_ms`
sentence after the columns line up.

---

### P2-2 — §3.4's tail-mass evidence is selective: at 16 threads the shipped cap 6 has 2.2-2.6× FEWER pops over 1 ms than cap 0, which is in the committed log, is not cited, and falsifies the section's own "tells the same story" sentence — and the incomplete framing is what propagated to `CHANGELOG.md` and the doc comment

**Files:** `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md:356-376` (§3.4's prose and "Reading");
`scripts/tis_backoff_cap_sweep_derive_report_data.mjs:470-479` (assertion D4);
`crates/tagged-index-stack/CHANGELOG.md:222-229`; `crates/tagged-index-stack/src/lib.rs:298-310`.
Contradicting evidence: `docs/perf/_raw_tis_backoff_per_call_latency.log`, the
`pop_over_1ms` / `pop_over_10ms` / `pop_p*_ms` fields.

§3.4 writes:

> Tail mass tells the same story per call-count: at `8 x 200,000`, 60-86 pops of 1.6M per rep
> exceeded 1 ms under cap 6, vs 0-8 pops under cap 0; at `16 x 200,000`, 3-4 pops of 3.2M per rep
> exceeded 100 ms under cap 6, vs 0 under cap 0.

Both cited cells are real. But the threshold **changes between the two shapes** — 1 ms at 8
threads, 100 ms at 16 — and the reason is visible in the same log: at 16 threads the 1 ms cell
reverses the sign. Re-read from `_raw_tis_backoff_per_call_latency.log` (three reps each):

| shape / threshold | cap 6 (shipped) | cap 0 | who is worse |
|---|---|---|---|
| 8 × 200k, `> 1 ms` | 86, 66, 60 | 8, 0, 3 | cap 6 (cited) |
| 8 × 200k, `> 10 ms` | 34, 29, 26 | 2, 0, 0 | cap 6 (not cited) |
| **16 × 200k, `> 1 ms`** | **285, 266, 249** | **553, 661, 650** | **cap 0 — 2.2-2.6× worse** |
| 16 × 200k, `> 10 ms` | 178, 131, 169 | 110, 161, 157 | roughly tied |
| 16 × 200k, `> 100 ms` | 4, 3, 3 | 0, 0, 0 | cap 6 (cited) |

So "tells the same story per call-count" is **false** for the 16-thread `>1 ms` row and
unsupported for the 16-thread `>10 ms` row. The story is narrower and more interesting than the
one told: the backoff moves mass *out of* the 1-10 ms band and *into* a handful of >100 ms
outliers.

The percentile columns say the same thing and are never quoted for cap 0 at all. From the same
log:

| shape | cap 6 `p999` | cap 0 `p999` | cap 6 `p50` | cap 0 `p50` |
|---|---|---|---|---|
| 4 × 20k | 0.000-0.001 ms | 0.022-0.037 ms | 0.000 | 0.001 |
| 8 × 200k | 0.001 ms | 0.054-0.057 ms | 0.000 | 0.002 |
| 16 × 200k | 0.001 ms | 0.172-0.182 ms | 0.000 | 0.003-0.004 |

The shipped cap is **1-2 orders of magnitude better at p50, p90, p99 and p99.9 in every shape**,
and better on wall clock by 4.05-4.85×. §3.4 quotes cap 6's `p999` in isolation ("99.9 % … within
~1 microsecond") and never states cap 0's, so a reader cannot see that the only axis on which
cap 0 wins is the extreme maximum. That omission works *against* the shipped default, but it is
the same defect as round-8 P2-1 (a published conclusion that its own committed CSV does not
support), and it is what the two consumer-facing artifacts now repeat:

- `CHANGELOG.md:224-225` — "with 60-86 pops over 1 ms per rep vs 0-8" and "at 16 threads x 200k,
  worst pop 130-173 ms vs 40-46 ms": the same two supporting cells, the contradicting one absent.
- `src/lib.rs:300-310` — "it trades per-call tail latency for aggregate throughput", stated
  unqualified. Against this data the honest statement is narrower: it trades a *small number of
  very large outliers* for better latency at every percentile up to p99.9 **and** better
  throughput.

Assertion D4 (`:470-479`) pins exactly the two supporting comparisons and nothing else, so the
assertion layer added to prevent selective claims was written around the selected ones.

**Failure scenario:** a consumer reads `CHANGELOG.md`'s "lock-free but NOT starvation-free … the
backoff trades per-call tail latency for aggregate throughput" and concludes that disabling the
backoff (a local fork with cap 0, which §1's recipe explicitly invites) would improve their
latency profile. On this crate's own committed data it would make their p50 through p99.9 worse
by 2-180×, their throughput worse by ~4-5×, and — at 16 threads — their count of pops over 1 ms
worse by 2.5×. The published framing points a latency-sensitive reader at the wrong lever.

**Fix (no code change):** state the 16-thread `>1 ms` and `>10 ms` cells alongside the two already
cited; add cap 0's percentile column next to cap 6's; and reword §3.4's "Reading", the CHANGELOG
bullet and `BACKOFF_SPIN_CAP`'s doc to "a small number of very large outliers in exchange for
better throughput *and* better latency at every percentile through p99.9". Extend D4 to assert
all five threshold cells, not the two that agree.

---

### P2-3 — the "lock-free is not starvation-free" warning is invisible to every consumer of the published crate: it lives on a private `const` rustdoc does not render, it was never added to the README, and the report it cites is not in the package

**Files:** `crates/tagged-index-stack/src/lib.rs:311` (`const BACKOFF_SPIN_CAP: u32 = 6;` — no
`pub`) and `:298-310` (the warning paragraph attached to it); `crates/tagged-index-stack/README.md`
(no occurrence of "starv", "tail" or any latency caveat); `crates/tagged-index-stack/src/lib.rs:1-206`
(the crate-root doc — likewise none).

Round-8 P2-2's stated fix was: "add one paragraph to `BACKOFF_SPIN_CAP`'s doc **and to the
README**". Only the first half landed, and the first half is attached to a **private** item.
Verified against the actual rustdoc output of
`RUSTDOCFLAGS="-D warnings" cargo doc -p tagged-index-stack --no-deps`:

```text
$ grep -rl "starv" <target>/doc/tagged_index_stack/
(no match)
```

`cargo doc` without `--document-private-items` — which is what docs.rs builds — does not render a
private `const`. The only trace of the name in the rendered HTML is the bare string
`BACKOFF_SPIN_CAP` inside `pop`'s prose (`struct.TaggedIndexStack.html`), which is not a link and
resolves to nothing a reader can open.

The other two places the paragraph exists are also out of reach:

- `CHANGELOG.md:227-229` carries it, but crates.io renders `README.md`, not `CHANGELOG.md`.
- `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` §3.4 carries the full table — and
  `cargo package -p tagged-index-stack --list` shows **18 files**, none of them under
  `docs/perf/`. A `cargo add tagged-index-stack` consumer has no copy of it at all.

So for the two surfaces a consumer actually meets — the crates.io landing page and docs.rs — the
crate still says only "lock-free", exactly as it did when round 8 raised this.

**Failure scenario:** the README pitches the crate for "slab allocators, object pools,
entity-component stores, id allocators, and connection tables". A consumer recycling a slot on a
request path reads "lock-free", ships it, and meets the 3-4 pops per 3.2 M that exceed 100 ms
(§3.4's own 16-thread measurement) in production with nothing in the documentation having
prepared them. That is precisely the scenario round-8 P2-2 opened, unchanged.

**Fix (no code change):** move the paragraph somewhere rendered — a `# Latency` (or
`# Lock-freedom and starvation`) section on `TaggedIndexStack::pop`/`push` and/or the crate-root
doc, both of which docs.rs renders — and add the same two or three sentences to `README.md`. Keep
the numbers, but state them in the corrected form **P2-2** asks for.

---

### P3-1 — `threaded_conservation.rs`'s new activation oracle asserts `delta > 0`, which does not pin the "`spins` genuinely climbs into its higher range" claim its own doc says it asserts

**File:** `crates/tagged-index-stack/tests/threaded_conservation.rs:10-19` (the claim),
`:114-129` (the assertions).

The module doc now says:

> genuine contention forces **many CAS retries per call**, and `spins` **genuinely climbs into its
> higher range** in practice — something no existing test (loom or otherwise) does. **That claim is
> ASSERTED, not assumed:** … asserts both the pop and push counters advanced after it

What is asserted is `pop_retries_after > pop_retries_before` and the same for push — i.e. **at
least one** retry of each kind across 1.6 M contended iterations. That assertion cannot
distinguish 1 retry from 50,000, and it cannot observe `spins` at all: `spins` is a per-call
local (`src/lib.rs:900`, `:1061`) that nothing outside `push`/`pop` can see. A single retry
anywhere in the run leaves `spins` at 0 for that call and satisfies the oracle.

Concretely: delete the backoff entirely (`for _ in 0..(1u32 << spins.min(BACKOFF_SPIN_CAP))` and
both `spins` sites, in both functions) and this test stays green — retries still occur, both
counters still advance, conservation still holds. The oracle protects the sentence "the retry
*branch* is reachable"; the doc claims it protects "`spins` climbs into its higher range". Those
are different claims and only the first is covered.

This is round-8 P3-1 ("its central purpose claim has no oracle") partially closed while the doc
asserts it fully closed — the file now states its coverage more strongly than the code delivers,
which is a new form of the same problem rather than a residue of the old one.

**Failure scenario:** a future change caps `spins` at 0, resets it per iteration, or moves the
`spins += 1` outside the reachable path. Every test in the crate stays green, the loom suite
(which cannot reach `spins > 1` — the file's own opening paragraph says so) stays green, and the
crate ships with its documented backoff silently inert. Nothing would notice.

**Fix:** either (a) weaken the doc to what the two assertions actually prove ("the retry branch is
reached under real threads", which is a genuine and worthwhile oracle), or (b) strengthen the
oracle — a third counter incremented only when `spins` reaches `BACKOFF_SPIN_CAP`, exposed
through the same `retry_counts_for_test` shape, asserted non-zero — and keep the doc as written.
(b) is the version that would actually catch the failure scenario; (a) is one line.

---

### P3-2 — §3.4 tells the reader to "read the cap6/cap0 RATIOS as the robust part", and its own n=3 data makes the headline ratio range from 1.8× to 100× depending on which reps are paired

**File:** `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md:364-376` (the "Reading" paragraph and its
caveat (2)); evidence: `docs/perf/_raw_tis_backoff_per_call_latency.log`.

The section publishes "the worst single pop multiplied **~27x** (median-to-median, `8 x 200,000`)"
and then closes with:

> (2) 3 reps on a shared dev host — read absolute magnitudes as order-of-magnitude and the
> cap6/cap0 **RATIOS as the robust part**.

The ratios are the *least* robust part of this dataset. The three cap-0 `pop_max_ms` samples at
`8 × 200,000` are `23.567 / 0.596 / 2.031` — a 40× spread within one arm, which the section's own
caveat (1) attributes to scheduler noise. Pairing reps differently against cap 6's
`41.496 / 59.705 / 54.464`:

| shape | min plausible ratio | median-to-median (published) | max plausible ratio |
|---|---:|---:|---:|
| 4 × 20,000 | 1.276 / 0.297 = **4.3×** | 30.3× | 10.828 / 0.159 = **68.1×** |
| 8 × 200,000 | 41.496 / 23.567 = **1.8×** | 26.8× | 59.705 / 0.596 = **100.2×** |
| 16 × 200,000 | 130.425 / 46.301 = **2.8×** | 3.8× | 173.365 / 39.610 = **4.4×** |

Only the 16-thread shape is genuinely stable. And the three published per-shape ratios themselves
(30.3, 26.8, 3.8) span an order of magnitude, so "~27×" is a property of one shape, not of the
mechanism — yet §5 (`:433-435`) restates it as a general statement about the default ("the shipped
cap 6 multiplies the worst single `pop` by ~27x vs cap 0 at 8 threads while making the same
workload ~4.9x faster").

The one figure that *is* robust across all three reps and all three shapes is the wall-clock
speedup (4.18 / 4.85 / 4.05, tight within each arm) — and that is the one the caveat does not
single out.

**Failure scenario:** a future round quotes "~27×" as this crate's characterised backoff tail
penalty and sizes a decision on it, when the same three committed reps also support "1.8×". The
caveat that was written to protect the reader points at the wrong quantity.

**Fix:** replace caveat (2) with the accurate version — the *wall-clock* speedup is the robust
quantity; the worst-pop ratio is a max-of-3-over-max-of-3 statistic whose plausible range spans
1.8×-100× at `8 × 200,000` — and state the per-shape ratio spread inline wherever "~27×" appears
(§3.4 and §5). An in-script assertion of the min/max plausible ratio, next to D2's
median-to-median one, would make the spread visible in the generated output.

---

### P3-3 — §3.4's cited immutable source identity does not contain the probe that produced the measurement

**File:** `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md:343-347`;
`docs/perf/_raw_tis_backoff_per_call_latency.log:4`.

§3.4 states:

> **Source identity:** base commit `842c99805992d362c5d82df59fc646c691598285` (this crate's
> round-8 worktree HEAD — the cap-6 arm was measured on that **CLEAN tree**, R29-6 option 1)

`842c998` resolves (`docs(review): tagged-index-stack independent publish-readiness review, round
8`), but:

```text
$ git cat-file -e 842c998:crates/tagged-index-stack/examples/backoff_per_call_latency.rs
fatal: path '…/examples/backoff_per_call_latency.rs' exists on disk, but not in '842c998'
$ git log --oneline --diff-filter=A -- crates/tagged-index-stack/examples/backoff_per_call_latency.rs
a1f9dc5 bench(tis): fix round-8 review P2-1/P2-2/P3-2/P3-3 in backoff cap-sweep gate
```

The example that produced every number in §3.4 was added by `a1f9dc5` — the commit that landed
this report. So the tree at measurement time was `842c998` **plus at least one untracked new
file**, which is not a clean tree, and R29-6 option 1 (a commit SHA that recovers the measured
source) is not satisfied: `git checkout 842c998 && cargo run --release --example
backoff_per_call_latency` fails, the example does not exist there. The cap-0 arm's patch hash
(`9cf4469a…`, R29-6 option 3) is a hash of `git diff`, which by construction does not include an
untracked file either, so it does not close the gap.

Everything else in the log's provenance block is genuine and I checked it: the captured
`285:const BACKOFF_SPIN_CAP: u32 = 6;` line number matches `git show
842c998:crates/tagged-index-stack/src/lib.rs` exactly (it is 311 at HEAD only because `a1f9dc5`
grew the doc comment), the cap-0 counterpart line is present, and the post-run restore
verification line is there. This is a citation defect, not a fabrication — the measurement *is*
reproducible today, just not from the commit named.

**Failure scenario:** a later round tries to reproduce §3.4 from its stated identity, finds no
probe at `842c998`, and cannot tell whether the example was edited between measurement and commit
or merely added. That is the same "recipe applied to a never-preserved tree" failure mode
CLAUDE.md's R29-6 rule was written after R29-13's broken hash, arriving on the first new artifact
written under it.

**Fix:** cite `a1f9dc5` (the commit that contains both the example and the shipped cap) as the
cap-6 arm's identity — or, if the measured tree differed from `a1f9dc5` in any way, a
`git write-tree` snapshot or a hash over `git diff HEAD` *plus* `git ls-files --others
--exclude-standard`. Drop "CLEAN tree".

---

### P3-4 — round 8 added an always-on hot-path global write and a third `#[doc(hidden)] pub` accessor to a published `no_std` primitive to serve one integration test, with the cost asserted rather than measured and no way for a consumer to opt out

**Files:** `crates/tagged-index-stack/src/lib.rs:958` and `:1116` (the unconditional
`fetch_add`s), `:1255` / `:1298` (the statics), `:1345-1352` (`retry_counts_for_test`);
`crates/tagged-index-stack/Cargo.toml` (no feature that gates any of it).

Before round 8 the two counters and their accessors were `#[cfg(loom)]`. Group2 removed the gate
from the statics and from both `fetch_add` sites, and added an ungated `#[doc(hidden)] pub fn
retry_counts_for_test()`. The result ships in **every** build of the published crate: two
process-global `AtomicUsize` in `.bss`, a `Relaxed` RMW on the CAS-retry arm of both operations,
and a publicly callable accessor — for a crate whose pitch is "allocation-free, `no_std`,
`#![forbid(unsafe_code)]`" minimality and whose own docs twice go out of their way to warn about
adding contended cache lines (`ArrayLinks`'s "Layout note — link-array false sharing",
`TaggedIndexStack`'s "Layout note — no cache-line isolation"). The two statics are declared
adjacently and will in practice share one cache line, so a `push` retry on one core and a `pop`
retry on another false-share by construction — the exact shape those two notes tell callers to
avoid.

The cost is *asserted*, not measured: `:1252-1254` says "Cost is one Relaxed `fetch_add` per lost
CAS, on the retry arm only — the uncontended fast path never touches it." I tried to falsify it.
Out-of-tree copy of the crate, arm A = shipped, arm B = both `fetch_add` lines deleted, separate
source trees and target directories, 9 interleaved reps per arm of a 1-second 8-thread
pop-then-repush workload over `ArrayLinks<256>` prefilled `0..64` (the bench's `contention/churn`
shape), plus a single-threaded control where the retry arm is provably never reached:

| arm | 8-thread mean ops/sec | 8-thread median | single-threaded churn mean |
|---|---:|---:|---:|
| A (shipped, counters on) | 27.36 M | 27.36 M | 54.19 ns/pair |
| B (counters stripped) | 27.97 M (**+2.2 %**) | 27.78 M (+1.6 %) | 53.35 ns/pair (**+1.6 %**) |

**The control kills the result:** the single-threaded arm never executes a `fetch_add` (no CAS
ever fails), yet shows the same direction and magnitude of difference. So the contended gap is
code layout, not the counters, and I cannot demonstrate a runtime cost. The claim in the doc
comment is plausible; it is also still unverified in-repo, and this crate has spent two rounds
establishing that an unmeasured perf rationale is a finding (round-7 P2-1 on `BACKOFF_SPIN_CAP`;
round-7 P3-1 measured the rule-4 guard at ≈ 0 ns before promoting it). The same bar was not
applied here.

The design point stands independently of cost. This repo's own convention (CLAUDE.md's
benchmark-hook rule, point 2) is that a hook with no production caller gates behind a
`bench-internals`-style feature rather than riding along in the default build; here there is no
gate at all.

**Failure scenario:** a downstream `no_std` consumer on a 32-bit target links two words of `.bss`
and a public accessor it can neither use nor remove; a downstream consumer with several
`TaggedIndexStack` instances has all of them writing the same two globals on every lost CAS. Both
are small — but neither was measured, neither is opt-out-able, and both exist solely so one
integration test can read a counter.

**Fix:** gate the statics, the `fetch_add`s and all three accessors behind a
`test-internals`/`bench-internals` Cargo feature (`tests/threaded_conservation.rs` then declares
`required-features`, exactly as this workspace does elsewhere), or — if keeping them
unconditional — commit the A/B that justifies it, and separate the two statics by
`#[repr(align(64))]` wrappers so the false-sharing note the crate gives its callers also applies
to the crate itself.

---

### P4 findings

**P4-1 — `model()`'s doc says "three tests", `model_with_oracle`'s says "four".**
`tests/loom_aba.rs:147` — "The **three** tests whose activation-oracle snapshot/assert window must
span the entire `check()` call … use [`model_with_oracle`] instead" — versus `:162-166`, "Variant
of [`model`] for the **four** tests …", which then names all four. Group2 added the fourth model
and updated one of the two paragraphs. Per this repo's own no-hardcoded-counts convention (task
#776/F10), the fix is to drop the number from both, not to bump the stale one.

**P4-2 — `bootstrap.rs`'s "THREE deliberate divergences" is now four.**
`src/registry/bootstrap.rs:465-483` enumerates three ways the loom shim differs from the shipped
type. Since `11b6833` there is a fourth: the shipped `push`/`pop` increment
`PUSH_RETRY_COUNT`/`POP_RETRY_COUNT` on the retry arm and the shim does not. It carries no
protocol content (a real `core` atomic loom does not model), so the shim is still correct — but
the comment was rewritten *in this same round* specifically to stop claiming an exhaustiveness it
did not have, and a later commit in the same round invalidated the new count. `0fd3a86` is the
oldest round-8 commit and `11b6833` is later, so the drift was introduced after the fix.

**P4-3 — three places still call the retry counters "loom-only", which they no longer are.**
`src/lib.rs:1239` and `:1282` both open "**loom-test-only** activation counter for …" and then,
ten lines later, say "Compiled in EVERY build — the retry-arm increments are unconditional". The
two sentences contradict each other inside one doc comment. `README.md:206-207` likewise says
`pop_retry_count_for_test`/`push_retry_count_for_test` "read the **loom-only** retry-activation
counters" — the *accessors* are loom-only, the counters are not. A reader grepping for
"loom-test-only" concludes the statics are absent from a normal build; they are not.

**P4-4 — `CHANGELOG.md`'s `### Added` inventory omits `retry_counts_for_test`.**
The list explicitly enumerates `raw_head()` (`:150-153`) and `TaggedIndex::empty()` (`:42-46`)
as `#[doc(hidden)]` items shipping in 0.1.0, and this is a first release whose own header says
"Everything below is new in this version". `retry_counts_for_test` — a third `#[doc(hidden)] pub`
item, added in the same round — appears nowhere in `### Added`; it is mentioned only in passing
inside a `### Performance` bullet's prose. `README.md` was corrected for exactly this by `fed6f3f`;
the CHANGELOG was not.

**P4-5 — the gate report's §7 verification receipt is stale and hardcoded.**
`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md:471` records
`… --test loom_aba`: **10/10 green**". The suite has eleven models (measured: 11 passed in
0.16 s) since `11b6833`, which landed *before* `a1f9dc5` last edited this report. The same
section's round-8 addendum (`:479-481`) correctly refuses to hardcode the script's assertion
count for precisely this reason — the loom line should get the same treatment.

**P4-6 — the derive script's assertion message does not describe what it checks.**
`scripts/tis_backoff_cap_sweep_derive_report_data.mjs:174-177`: the message reads "the **6**
throughput fields and **4** spacers are empty" (10 fields) while the code checks
`vals.slice(5, 14)` (9 fields). Neither number matches the header, which has 6 throughput columns
and no spacers at all. This mismatch is the visible tell for **P2-1** and would have been the
cheapest place to catch it.

**P4-7 — several published prose ranges are hand-transcribed and unasserted by the script that asserts everything else.**
D2/D3 pin the medians and the median-to-median ratios; D4 pins only two *orderings*. The ranges
the report and both consumer artifacts actually publish — "60-86" / "0-8" / "3-4"
(`TIS_BACKOFF_CAP_SWEEP_GATE.md:357-359`, `CHANGELOG.md:224-225`) and "41-60 ms" / "0.6-24 ms"
(`src/lib.rs:305-307`) — are not. They happen to be correct against the log today (I checked all
five). CLAUDE.md's derived-tables rule point 6 exists so that stays true after the next
re-measurement.

**P4-8 — every `docs/perf/` citation in the published crate is a dead end for a crates.io consumer.**
`src/lib.rs:148-152`, `:274`, `:310` and `README.md:117-118` cite
`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` and `docs/perf/_raw_tis_backoff_cap_sweep_run1.log` as
committed receipts. `cargo package --list` contains neither (18 files, none under `docs/`), and
docs.rs renders the rustdoc, not the repository. The citations are honest and resolvable from
GitHub — the `repository` field is set — but the doc text presents them as if they sit next to the
source. One clause ("in this crate's repository, `<url>/blob/main/docs/perf/…`") would close it.

**P4-9 — `readme_example.rs`'s enumeration of the crate's test files is stale.**
`tests/readme_example.rs:12-14` lists "`stack_unit.rs`, `proptest_pack_unpack.rs`,
`regression_counter_wrap.rs`, `custom_links_impl.rs`, `loom_aba.rs`" as the whole suite;
`threaded_conservation.rs` (round 7) is missing. Same hardcoded-inventory class as P4-1/P4-2/P4-5,
in a fourth file.

**P4-10 — two micro-nits.** (a) `benches/tagged_index_stack_bench.rs:268-272` and `:373-377` print
`min/mean = {:.2}x`; `min/mean` is a *share* of a fair split (`1.0` = fair), not a multiplier, and
the report's own §3.2 legend describes it that way — the `x` suffix invites reading `0.38` as
"0.38 times worse". (b) `tests/regression_counter_wrap.rs:62` sweeps tags
`[0u64, 1, 42, (1 << TAG_BITS) - 1, 0]` — the trailing `0` repeats the first element and adds no
coverage; if the intent was "wrap back to 0", the value that demonstrates it is
`(1 << TAG_BITS)` packed through `push`'s `wrapping_add`, not a literal 0.

---

## The three questions in the brief, answered directly

### (a) Does Group1's rewritten cap-sweep report hold up?

**The correction it was written to make is genuine and complete; the new material it added is
not.** Verified independently:

- **The round-8 P2-1 fix is real.** §3.2 now tabulates all five measured caps, both metrics, and
  states cap 6 as mid-curve. Assertion B7 mechanically pins "cap 6 is the strict `min/mean`
  maximum in 0 of 8 arms" — I re-derived the same 8 arms from the CSV and got the same answer.
  The false superlative is gone from the report, `CHANGELOG.md` and `src/lib.rs`.
- **The round-8 P3-2 fix is real.** §3.1's heading now names the `4/churn` cap-10 exception and
  the honest `−0.4 % to +58.4 %` span, with the `n=1` caveat stated inline; A1/A2/A3 pin all
  sixteen cells.
- **The round-8 P3-3 fix is real for the verify path.** `node
  scripts/tis_backoff_cap_sweep_derive_report_data.mjs` runs clean, **ALL 161 ASSERTIONS PASSED**,
  exit 0, on the committed artifacts. It cross-checks all 52 sweep rows cell-for-cell against
  both raw logs, and §3.2's and §3.3's tables really are its output (same columns, same
  precision, same ordering — I diffed them by eye against the script's stdout). Every byte count
  in §2 (23,342 / 8,564 / 6,365) is exact; the base SHA `47c81e90…` resolves.
- **What does not hold up:** the CSV the script maintains is malformed and the script's own
  `--write` path crashes on it (**P2-1**); §3.4's tail-mass evidence is selective and one of its
  sentences is false against its own log (**P2-2**); §3.4's robustness caveat points at the least
  robust statistic (**P3-2**); §3.4's source identity does not contain the probe (**P3-3**);
  §7's loom receipt is stale (**P4-5**).

**§3.4's new example** (`examples/backoff_per_call_latency.rs`) is a good, honest harness: it uses
the public API only, mirrors the committed test's and bench's contention discipline exactly, keeps
both `Instant` reads outside the timed `pop` and identical in both arms, states in its own module
doc that `TIS_CAP_LABEL` is informational and that the *resolved*-cap evidence is the captured
`const` line (the R26-4 requirement, correctly applied), and admits the two-clock-read inflation.
Its numbers reproduce exactly: I re-derived every cell of §3.4's table by hand from the JSON lines
in `_raw_tis_backoff_per_call_latency.log` and all twelve medians and both maxima match. The
problem is not the harness — it is which cells of its output the prose chose to quote.

### (b) Does Group2's loom model and the newly-unconditional retry counters hold up?

**The model holds up completely. The counters hold up on correctness and cost, but not on
convention, and their documentation is now self-contradictory.**

- **The model:** derived above under the verdict — the single-element seed makes
  `(0, t) -> (empty, t)` the only admitted head transition, so every `POP_RETRY_COUNT` increment
  the oracle asserts is provably an empty-`actual` retry. I traced the three other candidate paths
  (`pop_pop_conservation`, `threaded_conservation.rs`, the new example) and confirmed none reaches
  that arm, so "a path no other shipped model or test reaches" is accurate. `model_with_oracle`
  holds `MODEL_LOCK` across snapshot → `check()` → snapshot → verify, so the delta is exclusive.
  11/11 loom models green in 0.16 s. No finding.
- **No new race surface:** the counters are `Relaxed`, on the `Err` arm only, read by nothing in
  the algorithm; `push`/`pop` outcomes and orderings are byte-identical. Under `--cfg loom` they
  are deliberately *real* `core` atomics so they survive loom's re-runs, which is correct and
  documented.
- **No measurable overhead I could demonstrate** — 9 interleaved reps/arm at 8 threads showed the
  counter-free build +2.2 %, but the single-threaded control where the counters provably never
  execute showed the same-direction +1.6 %, so the difference is build layout (**P3-4** has the
  table). The remaining objections are conventional (no feature gate, unconditional public
  surface, unmeasured in-repo) and documentary (**P4-3**).
- **`retry_counts_for_test`'s doc-hidden rationale** correctly points at `raw_head`'s canonical
  paragraph (the crate's established single-site convention, from round 7), correctly explains why
  `threaded_conservation.rs` cannot use the `#[cfg(loom)]` twins, and correctly notes the
  process-global cumulative semantics and why this test's window is exclusive (one `#[test]` per
  binary). The tuple order `(pop, push)` matches its call site. Its **use** in
  `threaded_conservation.rs` is where the gap is: the assertion is weaker than the claim the
  module doc says it pins (**P3-1**).

### (c) Is Group3's `bootstrap.rs` comment accurate, and does the new MSRV row compile what it claims?

**Yes to both, with one count that has since drifted.**

- I read the shim's `push` and `pop` line by line against the shipped ones. All three enumerated
  divergences are exactly right and exactly where the comment says: no spin between retries (the
  `Err(actual) => head = actual` arms are bare), no `index < INDEX_MASK` guard in `push`, no
  rule-4 guard on `load_next`'s result in `pop`. The head protocol itself — Acquire load,
  `store_next` inside `push` only, `Release`/`Relaxed` push CAS, `Acquire`/`Acquire` pop CAS, H-2
  running-tag preservation on the drain — is replicated faithfully. The "irrelevant to the shim's
  purpose" argument for each is sound: `spin_loop()` touches no atomic loom models, and the
  registry pushes only indices `< MAX_HEAPS (4096) < INDEX_MASK (65535)` written solely by the
  crate's own `push`, so both guards are structurally unsatisfiable there. The only defect is the
  hardcoded "THREE" (**P4-2**).
- The MSRV rows do compile what they claim. `cargo test -p tagged-index-stack --no-run` builds all
  seven `tests/` targets and — because `cargo test`'s default target set includes examples —
  the new `examples/backoff_per_call_latency.rs` as well, so the round-8 example is MSRV-covered
  without a separate row. `cargo bench -p tagged-index-stack --no-run` is genuinely the only
  thing in that job that type-checks the bench, since the manifest sets `test = false` on it and
  `cargo test`'s default target set excludes bench targets — the comment's stated reasoning is
  correct on both halves. The "no `--all-features` here" justification is also correct: the only
  feature is the implicit `loom` optional-dependency feature and the job sets no `--cfg loom`.

---

## Checked and clean (no finding)

- **`#[track_caller]` on `pop` costs nothing measurable.** Round-8 P4-5 asked to "add it (and note
  the cost)"; the attribute was added and no cost note was written. I measured it: out-of-tree
  A/B, arm A shipped vs arm C with the attribute removed from `pop`, 9 interleaved reps/arm of a
  20 M-iteration single-threaded churn loop — 51.73 vs 51.45 ns/pair mean, inside a 50.27-54.11 ns
  spread. There is no cost to note, so the missing note is not a finding.
- **The crate-root/README wall-clock citation is exact.** `_raw_tis_backoff_cap_sweep_run1.log`
  carries exactly 20 single-threaded `churn` rows, the first is `53.89 ns/op`, and the span is
  `51.41-64.72 ns/op` — all three as cited (`src/lib.rs:148-154`, `README.md:117-123`). My own
  out-of-tree single-threaded runs landed at 50.27-55.23 ns/pair on the same host, corroborating
  the ~`2 × 10^7` pushes/sec the tag-width derivation rests on. Round-8 P4-3 is properly closed.
- **The `head` field still has no plain `store`.** I re-grepped: `new` initialises, `raw_head` and
  `is_empty` only load, and all three writers (push's `Release` CAS, pop's `Acquire` CAS, the
  loom-only `cas_head_for_test`) are RMWs. The release-sequence premise behind `pop`'s
  `Acquire`-only success ordering holds, and the `INVARIANT` block (`src/lib.rs:721-744`) states
  it correctly.
- **`pop`'s backoff-skip-when-empty is still correct** and is now covered — `head = actual` is
  assigned before the `is_empty(actual)` test, the loop's first statement returns `None`, and no
  outcome changes. Round-8 P3-4's coverage gap is genuinely closed by the new model.
- **`_CHECK_BITS` is still unbypassable** from every public associated item, and
  `try_pack`'s `1u64 << TAG_BITS` still cannot reach the `<< 64` boundary
  (`TAG_BITS ∈ [48, 63]`; `proptest_pack_unpack.rs:87-110` pins the width-1/shift-63 case).
- **`cargo package --list` is clean:** 18 files (17 from round 8 plus the new example), both
  licences, all seven tests, the bench, no scratch, no stray raw logs.
- **`cargo fmt --check`, `clippy --all-targets -D warnings` (plain and `--cfg loom`), and
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` are all clean**, and the crate declares no
  `[package.metadata.docs.rs]`, so the default-feature rustdoc row IS docs.rs's exact
  configuration — CLAUDE.md's docs.rs-feature-set rule has no gap here.
- **§3.2's own honesty nuances are real and correctly asserted.** B5's metric-flip note (`min/mean`
  orders `0 > 4` while `max/min` orders `4 < 0`) and B6's admission that cap 0's `max/min` at
  `16/push_pop` is *worse* than cap 6's are both true against the CSV and both asserted. That
  section is the model the rest of the report should follow.

## Refuted

- **"Making the retry counters unconditional slowed the contended path."** Not supported: the
  8-thread A/B gap (+2.2 % for the counter-free build) is reproduced by a single-threaded control
  in which the counters provably never execute, so it is code layout. See **P3-4**'s table.
- **"`#[track_caller]` on `pop` costs throughput."** Measured, no resolvable difference.
- **"The new loom model's `POP_RETRY_COUNT` delta could come from a non-empty `actual`."** It
  cannot: the only writer inside the model's concurrent window is the winning `pop`, which
  installs `(empty, t)`. `compare_exchange` (not `_weak`) does not fail spuriously.
- **"§3.4's numbers were hand-transcribed."** They were not — I re-derived all twelve medians and
  both maxima from the raw JSON lines and every cell matches. The defect is *selection*
  (**P2-2**), not transcription.

---

## Suggested task queue for round 9

| # | Finding | Task |
|---|---|---|
| 1 | **P2-1** | Emit 6 (not 9) empty fields for the latency rows so all 71 CSV rows are 20 columns; fix the verify block's slice offsets and its message; make `--write` idempotent against its own output; re-check §3.4's `pop_p999_ms` sentence once the columns line up. |
| 2 | **P2-2** | Add the 16-thread `>1 ms` / `>10 ms` cells and cap 0's percentile column to §3.4; reword §3.4's "Reading", `CHANGELOG.md` and `BACKOFF_SPIN_CAP`'s doc to "a few very large outliers in exchange for better throughput AND better latency through p99.9"; extend assertion D4 to all five threshold cells. |
| 3 | **P2-3** | Move the "lock-free is not starvation-free" paragraph onto a rustdoc-rendered surface (`pop`/`push`'s docs and/or the crate root) and add it to `README.md`, in the corrected form from task 2. |
| 4 | **P3-1 + P3-4** | Decide the retry counters' shape: either gate statics + `fetch_add`s + accessors behind a `test-internals` feature, or commit the A/B that justifies keeping them unconditional; and either weaken `threaded_conservation.rs`'s doc to what its `delta > 0` oracle proves or add a spins-at-cap counter that actually pins the claim. |
| 5 | **P3-2 + P3-3** | Replace §3.4's caveat (2) with the accurate robustness statement and state the per-shape ratio spread wherever "~27×" appears; re-cite §3.4's source identity to `a1f9dc5` (or a `write-tree` snapshot) and drop "CLEAN tree". |
| 6 | **P4 bundle** | P4-1 (`model` "three"), P4-2 (shim "THREE"), P4-3 ("loom-only" counters ×3), P4-4 (CHANGELOG `### Added`), P4-5 (§7 "10/10"), P4-6 (script message arithmetic), P4-7 (unasserted prose ranges), P4-8 (`docs/perf` citations from the published crate), P4-9 (`readme_example.rs` file list), P4-10 (bench `x` suffix, duplicated tag literal). Four of these ten are stale hardcoded inventories — consider closing the class rather than the instances. |

**Findings by priority: P0 = 0, P1 = 0, P2 = 3, P3 = 4, P4 = 10.**
