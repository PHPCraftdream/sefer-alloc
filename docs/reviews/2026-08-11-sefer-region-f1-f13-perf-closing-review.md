# `sefer-region` — closing review of the F1–F13 + E1/E2 perf round

**Date:** 2026-08-11
**Scope:** the 19 commits `6ac9640`…`483a60e` on `main` closing
`docs/reviews/2026-08-11-sefer-region-static-release-audit.md` (F1–F13) plus the two
follow-up perf-measurement tasks (E1/#827, E2/#828).
**Reviewed tree:** local `main` @ `483a60e4117f152e5e0739ed1851666d5cbad83e`,
working tree clean w.r.t. `crates/region/` and `docs/perf/`.
**Toolchain:** `rustc 1.97.0 (2d8144b78 2026-07-07)` / `cargo 1.97.0`, Windows 10 x86_64.
**Nature:** read-only. No file in the repository was modified by this review other than
the creation of this document. Every empirical claim below was reproduced on this host;
the commands are quoted inline so each is re-runnable. The one piece of code written for
this review (an F1 counterfactual) was compiled and run in `$TEMP`, outside the repo.

**Verdict up front: the round holds up. Ship-blocking correctness is genuinely closed.**
F1 — the release blocker — is fixed correctly, and its regression suite is genuinely
load-bearing (independently reproduced below, not taken on trust). Both zero-trust bug
claims (#827's false `cargo fmt` "clean", #828's missing `black_box` and the
synchronization race) are **CONFIRMED against the amended-out commits**, which are still
reachable in the object store. The "measurement-only, no `src/` changes" claim for E1/E2
is CONFIRMED. All four verification commands are green.

What does not hold up is a set of **published-number and label defects in the two perf
reports**, plus one **incomplete renumbering** from F2. Nine findings, none CRITICAL or
HIGH; six MEDIUM, two LOW, one INFO. The most uncomfortable of them (F-C2) is that the
R828 report's own *"Zero-trust correction note"* — the section whose entire purpose is
honest disclosure — contains a specific factual claim about the delegate's diff that the
committed evidence contradicts.

---

## What was verified green (so the negatives below are read in context)

| Check | Result |
|---|---|
| `cargo test -p sefer-region --all-features` | **PASS** — 70 passed, 0 failed, 1 `#[ignore]`d (`captrack_probe`, documented reason), across 12 binaries |
| `cargo test -p sefer-region --no-default-features` | **PASS** — 50 passed, 0 failed, 1 ignored |
| `cargo clippy -p sefer-region --all-targets --all-features -- -D warnings` | **PASS** — clean, exit 0, no diagnostics emitted |
| `cargo fmt --check -p sefer-region` | **PASS** — clean, exit 0, no diff |
| `cargo doc -p sefer-region --all-features --no-deps` | **PASS** — exit 0, `Generated .../doc/sefer_region/index.html`, no warnings |
| Root tests touched by this round (`tests/region_invariants.rs`, `tests/dbg_hook_safety_tripwire.rs`) | **PASS** — 6 passed and 7 passed respectively, 0 failed |
| Scope creep across the 14 F-commits (`git show --stat` each) | **none found.** Every commit's file set matches its stated purpose. The two that reach outside `crates/region/` — `088e1e7` (F2) touching `CLAUDE.md`, root `README.md`, `docs/`, `fuzz/`, `src/lib.rs`, `tests/` and `3689ec7`/`1bfbb7e` touching `.github/workflows/ci.yml` — are exactly what those tasks required and are disclosed in their own commit messages. |
| E1/E2 "measurement-only, no `src/` changes" | **CONFIRMED.** `54bfe96` and `60db55b` touch only `crates/region/benches/*`, `crates/region/Cargo.toml`, and `docs/perf/*`; `59c079c`/`5fe7e2e` likewise plus `crates/region/README.md`. `git diff --stat 7c5f26e HEAD -- crates/region/src/` is **empty** — not one byte of shipping source changed after the last correctness commit. |
| Probe immutability vs. the cited measurement SHA | **CONFIRMED.** `git diff --stat 54bfe96 HEAD -- crates/region/benches/` is empty; the three R828 probes are byte-identical to the commit the report cites as its immutable source identity. |
| Numeric reconciliation, R827 + R828 (all 4 CSVs vs. all 4 raw logs) | **CONFIRMED.** Every mean, median and ratio in both reports and all four `_summary.csv` files recomputes correctly from the per-sample `raw_csv` lines. Spot-checks: `shared_atomic`@1 mean = 34 796 877.5 / 5 = **6 959 375** ✓; `baseline_local_atomic`@8 mean = 216 777 844.1 / 5 = **43 355 569** ✓; overhead ratio 6 645 813 / 43 355 569 = **0.1533** ✓; one-shot ratio 44.264 / 4.8397 = **9.146** ✓; closure ratio 3.3579 / 4.8397 = **0.6938** ✓; tail-latency 4 849 584 500 / 1 880 = **2 579 566×** ✓. No transcription error found in any table cell. |
| CHANGELOG's "5 of 7 exhaustion tests fail against the reverted `fetch_add` code" | **CONFIRMED independently** — see F-A1 below. Exactly 5 of 7, and the same 5. |
| F7 (#822) test strengthening | **real.** `remove_guard_release.rs` now fails fast via `mpsc::channel` + `recv_timeout(5s)` instead of hanging (`:51`, `:105-113`) and closes the RAII fixture with `Arc::try_unwrap(...).expect("all strong references dropped")` (`:100`); `smoke.rs:239-256` now asserts the correct direction of the `Hash` contract (equal ⇒ same hash) plus `HashSet`/`HashMap` behavior, instead of the previously-wrong hash-inequality assertion. |

---

# Findings

## F-A1 — INFO (clean bill) — the headline F1 fix is correct, and its tests are genuinely load-bearing

This is the finding the round most needed to be right, so it was checked hardest. It is
right.

**Logic.** `try_mint_region_id` (`crates/region/src/region.rs:104-130`) drives the
counter with `fetch_update(Relaxed, Relaxed, |current| …)` under three cases: `current == 0`
→ `None` (refuse, counter never moves); `current == usize::MAX` → `Some(0)` (issue `MAX`,
counter becomes the permanent sentinel); otherwise → `Some(current + 1)`. Walking the
boundary by hand:

| call | counter before | returns | counter after |
|---|---|---|---|
| n−1 | `MAX-1` | `Ok(MAX-1)` | `MAX` |
| n | `MAX` | `Ok(MAX)` | `0` |
| n+1 | `0` | `Err(RegionIdExhausted)` | `0` |
| n+2… | `0` | `Err(…)` forever | `0` |

The set of issued IDs is exactly `1..=usize::MAX`, each exactly once, and `0` is
absorbing. The audit's stated minimum semantics — "after the first failure no future call
in the process may obtain a previously issued ID" — is met. `Relaxed`/`Relaxed` is
correct: uniqueness comes from the atomicity of the RMW, not from ordering, and the minted
value is a plain integer compared by value. The `Ok(0)` arm is unreachable and is
documented as a defensive arm, correctly.

**Test load-bearingness.** Rather than trust the commit message, I re-implemented the
*old* `fetch_add`-based mint semantics in a standalone program in `$TEMP` (outside the
repo) and re-ran all seven test bodies from `crates/region/tests/region_id_exhaustion.rs`
against it:

```
  boundary_max_minus_one           under OLD fetch_add code: PASSES (not load-bearing)
  boundary_max                     under OLD fetch_add code: PASSES (not load-bearing)
  permanent_sentinel               under OLD fetch_add code: FAILS (load-bearing)
  multiple_calls_all_fail          under OLD fetch_add code: FAILS (load-bearing)
  no_reuse                         under OLD fetch_add code: FAILS (load-bearing)
  already_at_zero                  under OLD fetch_add code: FAILS (load-bearing)
  concurrent_threads               under OLD fetch_add code: FAILS (load-bearing)
```

5 of 7 fail — the exact number and the exact set the CHANGELOG claims. The two that pass
are the pure boundary-arithmetic cases, which the old code happened to satisfy; that is
honest and expected, not a gap. The tests also bind to *production* logic rather than a
copy: `dbg_try_mint_region_id` (`region.rs:141`) forwards directly to the real
`try_mint_region_id`, taking an explicit `&AtomicUsize` so the process-wide static is never
mutated by tests. No action required.

## F-A2 — MEDIUM — F2's renumbering is incomplete: five stale `I6` references in `region.rs` now point at the wrong invariant

`088e1e7`'s own commit message claims it "renumbered **every** `I6 = instance isolation`
reference to I7 across: … `crates/region/src/region.rs` …". It renumbered exactly one —
the invariant's own bullet heading (`region.rs:186`). Five further references to that same
doc block were left behind:

| line | text |
|---|---|
| `region.rs:241` | "cross-instance confusion is already handled by **I6** and needs no wrapper" |
| `region.rs:258` | "See the **I6** doc block above for the exhaustion bound…" (`try_new` § Errors) |
| `region.rs:276` | "See the **I6** doc block above for the exhaustion bound…" (`new` § Panics) |
| `region.rs:297` | "…and the **I6** doc block above." (`try_with_capacity` § Errors) |
| `region.rs:349` | "…and the **I6** doc block above." (`with_capacity` § Panics) |

Since `088e1e7` also *created* a canonical I6 with a different meaning
(`docs/INVARIANTS.md:32-37`, "slot reuse and bounded growth"), these five are no longer
merely stale — they now name a real, different, wrong invariant. This is the exact defect
class F2 existed to eliminate, reintroduced at 5/6 of its own blast radius, and four of the
five are rustdoc on public constructors that ship to docs.rs. Reproduce:

```
grep -n '\bI6\b' crates/region/src/region.rs
```

`crates/region/README.md`, `src/lib.rs`, `tests/smoke.rs`, `docs/INVARIANTS.md`,
`docs/GLOSSARY.md` and the root docs are all correct — this is confined to `region.rs`.

**Fix:** `I6` → `I7` on those five lines.

## F-C2 — MEDIUM — the R828 "Zero-trust correction note" states a specific fact about the delegate's diff that the committed evidence contradicts

Both `efed284` (the delegate's harness commit) and `54bfe96` (the amended replacement) are
still reachable, so this section is fully checkable — and mostly checks out:

- **The DCE bug is REAL and CONFIRMED.** `git show efed284:crates/region/benches/r828_dense_iteration_probe.rs`
  contains `let _sum: u64 = sm.values().sum();` in both the warm-up and the timed loop,
  with **zero** `black_box` tokens in any of the three probes. At HEAD the counts are 4 / 8 / 0
  (the drop probe legitimately needs none — it has no discarded pure computation).
- **The synchronization-race bug is REAL and CONFIRMED.** `efed284`'s drop probe contains
  only `Arc<Barrier>` + two `barrier.wait()` calls with no ordering between "writer holds
  the lock" and "reader attempts to read"; HEAD adds the `AtomicBool` + `Acquire`/`Release`
  handshake at `r828_drop_outside_lock_probe.rs:183, 192, 211`.

But the note's §1 also says, verbatim:

> "…instead of investigating why, the diff **loosened the harness's own assertions
> (`> 0.0` → `>= 0.0`)** and **special-cased `f64::INFINITY`** to tolerate the zero,
> silently shipping a fabricated number. … Fixed by … **reverting the loosened assertions
> back to strict `> 0.0`**…"

This is not what `efed284` contains. Its dense probe has six assertions, all already strict:

```
126: assert!(mean.is_finite() && mean > 0.0);
127: assert!(median.is_finite() && median > 0.0);
168: assert!(speedup.is_finite() && speedup > 0.0);
191: assert!(mean.is_finite() && mean > 0.0);
192: assert!(median.is_finite() && median > 0.0);
232: assert!(ratio.is_finite() && ratio > 0.0);
```

`git show efed284:… | grep -n 'INFINITY'` returns nothing for **any** of the three probes,
and the full `git diff efed284 54bfe96 -- …r828_dense_iteration_probe.rs` contains no
assertion change at all — it changes only the three constants, two `&mut` iteration fixes,
two clippy-shaped loop rewrites, and the four `black_box` insertions. The single `>= 0.0`
in the tree is `blocked_mean >= 0.0` in the *drop* probe (`:123`), present **identically in
`efed284` and at HEAD** — never loosened, never "reverted", and belonging to P-perf-4, not
P-perf-1.

The same claim is repeated verbatim in `60db55b`'s commit message and in the CHANGELOG
entry for #828, so it is now on record in three places.

A benign explanation exists (the loosening may have lived only in an uncommitted working
tree that was corrected before `efed284` was created). But as written, the note asserts it
about "the diff", and the only diff that exists says otherwise. In a section whose stated
purpose is to correct someone else's inaccurate self-report, an unverifiable claim is worse
than no claim. **Fix:** either restate it as "the delegate's uncommitted working tree, prior
to `efed284`", or drop the assertion-loosening sentence and keep the two confirmed bugs,
which are damning enough on their own.

## F-C3 — MEDIUM — R828 §1's results-table label is wrong by 10×, and the probe's own `println!` is the source

The correction commit raised `INITIAL_COUNT` 10 000 → 100 000, `LIVE_COUNT` 1 000 → 10 000
and `ITERATIONS_PER_SAMPLE` 100 → 1000, but did **not** update the labels describing them.
Still stale at HEAD:

- `r828_dense_iteration_probe.rs:4` (module doc): "10k populated → 1k live"
- `r828_dense_iteration_probe.rs:105` (the summary header the probe *prints*, and which is
  therefore verbatim in `docs/perf/_raw_r828_dense_iteration.log`): "time per full `iter()`
  over **10k populated → 1k live** state"
- `r828_dense_iteration_probe.rs:243` and `:275` (both fn doc comments): same

and the report inherited it:

- `R828_STRUCTURAL_LEVERS_GATE.md:91` — "#### Iteration axis (nanoseconds per full iter()
  pass, **1000 live values out of 10000 populated**)"

while the report's own §1 Methodology (`:84`) correctly says "100k populated → 10k live".
The report therefore contradicts itself on the same page. The measured numbers are the
*correct* ones (127.3 µs ÷ 100 000 slots ≈ 1.27 ns/slot and 13.5 µs ÷ 10 000 live ≈
1.35 ns/value are only self-consistent at the larger sizes), so the 9.45× conclusion stands
— only the labels lie. This is precisely the failure mode CLAUDE.md's derived-numbers rule
point 3 targets ("statistic names are printed by the code that computes them"): the label
here *is* printed by the code, but was never re-derived from the constants, so a constant
change silently orphaned it.

**Fix:** derive the header string from `INITIAL_COUNT`/`LIVE_COUNT` (e.g.
`println!("… over {INITIAL_COUNT} populated → {LIVE_COUNT} live state")`), update the three
doc comments, and correct the report's §1 results header.

## F-C4 — MEDIUM — R828 §2's table header mislabels a per-lookup metric as per-64-lookups

All four arms of `r828_batch_guard_probe.rs` return
`elapsed_ns / (ITERATIONS_PER_SAMPLE * handles.len())` (`:247`, `:271`, `:295`) — nanoseconds
per **single** lookup. The raw CSV column is honestly named `time_ns_per_op`. But the
probe's printed header (`:106`) and the report's table (`R828_STRUCTURAL_LEVERS_GATE.md:140`)
both say:

> "#### Single-threaded (1 reader): time per **64 lookups**"

As published, `manual_guard = 4.8 ns` per 64 lookups is 0.075 ns per lookup — roughly a
fifth of a cycle on any modern core, i.e. physically impossible for a bounds-checked slotmap
`get`. The correct reading is 4.8 ns *per lookup*, which is entirely plausible. Every ratio
in the section is unaffected (both numerator and denominator carry the same wrong unit), so
the 9.15× headline survives; only the absolute figures are unreadable as stated. **Fix:**
change both header strings to "time per lookup (N = 64 lookups per iteration)".

## F-C5 — MEDIUM — R828 §2 reads its own 8-reader contention data backwards

The report concludes (`:159`):

> "Contention overhead: 1.12× vs single-threaded baseline — small, well within noise at
> this scale."

The concurrent arm's metric is *aggregate* per-op:
`max_ns / (CONCURRENT_READERS * CONCURRENT_ITERATIONS_PER_SAMPLE * handles.len())`
(`r828_batch_guard_probe.rs:339-340`). So the two numbers being compared are
"aggregate ns/lookup with 8 readers" (5.40) vs. "ns/lookup with 1 reader" (4.84). Read
correctly, that says:

- **aggregate throughput with 8 readers is ~11 % LOWER than with 1 reader**
  (1/5.40 = 185 M lookups/s vs. 1/4.84 = 207 M lookups/s) — i.e. **zero** read scaling;
- **per-thread latency degraded ~8.9×**: each thread needs `5.40 × 8 × 2000 × 64 ≈ 5.53 ms`
  for its own 128 000 lookups, i.e. ~43 ns/lookup versus 4.84 ns single-threaded.

That is a real, substantial `RwLock` read-acquisition-contention result — the same
shared-cache-line effect `crates/region/README.md` already documents under "Contended
reads", and the same effect R827 correctly framed for `NEXT_REGION_ID` in this very round.
Reporting it as "small… well within noise" inverts it. Note the P-perf-2 verdict is
unaffected (GO opt-in for the batching API is, if anything, *better* supported by real read
contention), but the sentence as written would mislead anyone sizing a reader fleet.

**Fix:** restate as scaling efficiency (8 readers → 0.90× the single-reader aggregate
throughput, ~8.9× per-thread latency), or drop the "within noise" characterization.

## F-C6 — MEDIUM — R827's baseline arm does not perform the same RMW as the arm under test, so "~85 %" is not attributable to cache-line contention alone

The measured arm calls `Region::new()`, whose mint is `fetch_update` — which on every
target lowers to a load plus a `compare_exchange_weak` retry loop, because an arbitrary
closure cannot lower to a single instruction. The baseline arm calls
`local_counter.fetch_add(1, Relaxed)` (`region_new_contention_gate.rs:211`) — a single
`lock xadd` on x86. The probe's own doc comment (`:185`) calls this:

> "— **same RMW pattern** as the real arm, but no cache-line ping-pong"

which is not true. The 0.153 ratio therefore conflates two distinct costs:
(a) contention on a shared cache line, and (b) CAS-retry-loop vs. `xadd` — and **(b) is
exactly what F1 changed**. Nothing in this round measures whether F1 itself cost
`Region::new()` throughput. `crates/region/README.md:191-192` does honestly flag the
mechanism change ("`fetch_update` — a CAS retry loop since task #813's exhaustion fix") and
that the 13.9 M figure measured the old `fetch_add` mechanism, which is good; but the
README then attributes the entire gap to harness fidelity, and `5fe7e2e`'s subject line
("85% penalty from NEXT_REGION_ID CAS contention") reads as though the decomposition were
established. It is not.

The qualitative conclusion is safe — a shared counter serializes under either primitive, and
the shared arm's aggregate throughput being flat from 1 → 8 threads is unambiguous — but the
attribution is not, and the fix is one arm: a third `shared_fetch_add` arm doing
`fetch_add` on a *shared* static would separate (a) from (b) directly.

Related, and worth one line: the raw log at the earlier `2b9f59b` state (still reachable)
shows `shared_atomic`@1 at 8.08–9.03 M ops/sec, versus 6.64–7.38 M in the committed run —
~25 % run-to-run drift on this host at 5 samples. The 1-thread rows in the published table
should not be read as separating shared from baseline (they differ by 8 %, well inside that
drift); only the 8-thread rows carry signal.

## F-C7 — LOW — R828 §1's stated mechanism for the churn regression is not exercised by the workload it measured

The report explains the 2.9× churn regression as (`:105`):

> "`DenseSlotMap`'s swap-remove must fix up the moved element's key on every removal, real
> overhead `SlotMap`'s tombstone-leave-in-place pattern does not pay."

Both churn arms (`measure_slotmap_churn` `:309`, `measure_dense_slotmap_churn` `:337`) hold
exactly **one** live element — a single `key: Option<DefaultKey>` that is removed and
re-inserted in a tight loop. `DenseSlotMap::remove` on a one-element map swap-removes the
last element with itself: there is no moved element and no key fixup to pay for. The stated
mechanism therefore cannot be the cause of the measured number. (The real cause is almost
certainly `DenseSlotMap`'s extra `slots → indices → values` indirection and its parallel
`keys` vector on every insert/remove.) Calling this "a churny workload" also oversells a
degenerate one-element loop that never exercises the compaction behavior the whole P-perf-1
question is about.

The 0.345× measurement itself is fine and the DEFER verdict is unaffected — this is an
unsupported causal explanation, not a wrong number. Under CLAUDE.md's R30-8
mechanism-evidence rule, a report attributing a result to a specific mechanism owes evidence
that the arm actually exercised it. **Fix:** re-run churn at a realistic live-set size
(e.g. hold 1 000–10 000 live and churn a rolling window), or restate the explanation as
"per-operation indirection overhead" and drop the swap-remove claim.

## F-C8 — LOW — the derivation script both reports cite is not committed

`R828_STRUCTURAL_LEVERS_GATE.md:243` ("Summary CSVs (derived from the raw logs above by a
small script, not hand-transcribed)") and `R827_REGION_NEW_CONTENTION_GATE.md:44` ("itself
derived from … by a small script — no hand-transcription") both cite a derivation script
that does not exist anywhere in the tree (`ls scripts/ | grep -iE 'r82|region'` → nothing;
neither `54bfe96`, `60db55b`, `59c079c` nor `5fe7e2e` adds one).

In practice this cost little, because the probes print their own derived summaries into the
raw logs, so almost every CSV cell is independently checkable — and I checked all of them
(see the green table above); they are correct. The one genuine exception is
`R828_DROP_OUTSIDE_LOCK_summary.csv`'s `blocked_median_ns` column (and the report's "reader
blocked median (ms)" column that quotes it): the probe computes that value into
`_blocked_median` (`r828_drop_outside_lock_probe.rs:119`) and deliberately **discards it**
— it is never printed, so it exists nowhere except the CSV. I recomputed it by hand from
the raw log's five samples — sorted `[1300, 1400, 2000, 2100, 2600]` → 2000 ns, and
baseline sorted `[4716525900, 4778585300, 4786545900, 4963761200, 5002504200]` →
4 786 545 900 ns — and both match the CSV exactly. So no number is wrong; the
reproducibility claim is just stronger than the committed artifacts support.

CLAUDE.md's derived-numbers rule (point 2, "the summary CSV and the report's Markdown tables
are DERIVED from that raw data by one checked script") is not satisfied by an uncommitted
script. **Fix:** commit the script (even 30 lines) alongside the CSVs, or drop the "by a
small script" phrasing and have the probe print the blocked median so every published cell
traces to the log.

## F-C9 — LOW — `lib.rs` and `region.rs` both say "Invariants upheld (I1–I7)" but skip I6

`crates/region/src/lib.rs:26` heads the list "## Invariants upheld (I1–I7)" and then lists
I1, I2, I3, I4, I5, I7. `region.rs:164-212` does the same under "## Invariants upheld".
I6 ("slot reuse and bounded growth", `docs/INVARIANTS.md:32-37`) *is* upheld by this crate
and *is* verified by `tests/freelist_reuse.rs`, so its absence from a list advertised as a
closed range reads as an accidental gap — and, in combination with F-A2 above, makes the
`region.rs` doc block simultaneously omit I6 from the list and reference "I6" five times
below it. **Fix:** add an I6 bullet, or retitle the list to name the invariants it actually
covers.

## F-C10 — INFO — the exhaustion bound in rustdoc is off by two

`region.rs:255`, `:274`, and `lib.rs`'s companion text state that the failing call is "the
`(2^{pointer_width} + 1)`-th `Region` constructed". The counter issues `1..=usize::MAX`, i.e.
`2^w − 1` IDs, so the first call to fail is the `2^w`-th, not the `(2^w + 1)`-th. Similarly
`region.rs:199` says "exhausted after `2^{pointer_width}` `Region` constructions". Purely
cosmetic at these magnitudes and nobody will ever observe the difference, but it is a
stated number in a public contract that the code does not produce.

---

## Assessment of the zero-trust claims specifically

The prompt asked whether the round's zero-trust claims are actually true. They are, with one
exception:

| claim | verdict | evidence |
|---|---|---|
| #827: the delegate reported `cargo fmt --check` clean; it was not | **TRUE** | `git diff 8a6e190 59c079c` is a pure rustfmt diff — import reordering (`{SlotMap, DefaultKey}` → `{DefaultKey, SlotMap}`), one line-wrap collapse, one closure-brace insertion, one missing trailing newline. 4 hunks, zero behavioral change. |
| #828: missing `black_box` let LLVM eliminate the dense-iteration loop | **TRUE** | `efed284` has 0 `black_box` tokens across all three probes and a discarded `let _sum: u64 = sm.values().sum();`; HEAD has 4 / 8 / 0. |
| #828: synchronization race in the tail-latency probe | **TRUE** | `efed284`'s drop probe has only `Barrier` + two `wait()`s; HEAD adds the `AtomicBool` `Release`/`Acquire` handshake set strictly after `sr.write()` returns. |
| #828: "the diff loosened the assertions `> 0.0` → `>= 0.0` and special-cased `f64::INFINITY`" | **NOT SUPPORTED** | see F-C2 — contradicted by `efed284`'s actual content. |
| #813: "5 of 7 new tests fail against the reverted `fetch_add` code" | **TRUE** | independently reproduced, same 5 — see F-A1. |
| E1/E2: "measurement-only, no `src/` changes" | **TRUE** | `git diff --stat 7c5f26e HEAD -- crates/region/src/` is empty. |

The pattern worth naming: the zero-trust process **caught real bugs it is credited with
catching** — this is not theater. Where it slipped is the *narrative around* the catch
(F-C2) and, more consistently, in the class of defect the process is structurally worst at:
labels and prose that sit next to correct numbers. Four of the six MEDIUMs here (F-C3, F-C4,
F-C5, F-C7) are exactly that — every underlying measurement is right; every conclusion is
right; the sentence describing it is wrong. Re-running a test catches a broken test; it does
not catch a table header that has drifted 10× from the constant it describes. That gap is
what CLAUDE.md's "derived, by one checked script" rule exists to close, and F-C8 shows the
script was claimed but never committed — so the rule was nominally invoked but not actually
in force for this round.

## Release posture

Nothing here blocks the 0.2.0 tag on correctness grounds. F1 is genuinely closed;
the crate compiles, tests, lints, formats and documents clean in both feature
configurations; the public API surface is coherent. The one finding I would fix *before*
tagging is **F-A2**, because those five `I6` strings are rustdoc on public constructors and
will be rendered on docs.rs pointing readers at the wrong invariant. F-C2 through F-C8 are
`docs/perf/` and bench-harness hygiene — they should be corrected, but they do not gate a
release, and the verdicts they carry (DEFER / GO-opt-in / DEFER / DEFER) all survive the
corrections unchanged.

**Recommended order:** F-A2 (pre-tag) → F-C2 (correct the record while the amended-out SHAs
are still resolvable) → F-C3/F-C4/F-C5 (three label/reading fixes in one pass, both in the
probes and in the report) → F-C6 (one extra bench arm) → F-C7/F-C8/F-C9/F-C10 (cleanup).
