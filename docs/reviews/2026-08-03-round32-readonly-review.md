# Read-only review: Round 32 (tasks #486–#505)

Date: 2026-08-03

Reviewed range: `d29a82e..ab6e0d6` (39 commits), i.e. from task #486's
`fix(alloc-core): bind ReservedSmallSegment to its owning AllocCore` through
the round's closing `docs: update CHANGELOG.md for Round 32` commit.
`HEAD` at review time = `ab6e0d65c3fa446c0cf7953ef721e5ca3b344109`.

Review mode: read-only with respect to repository content — the only file
written is this report. Unlike the two prior review docs in this directory,
this review **did** execute verification commands (build, test, clippy, fmt,
loom, and two of the round's own derive scripts); every such command and its
output is quoted inline so the claims below are reproducible without redoing
the work. One derive script (`scripts/r497_dualbitmap_summary.mjs`) rewrites
its own committed CSV as a side effect; `git status`/`git diff` were checked
afterwards and the regenerated file is byte-identical to the committed one
(this is itself evidence, see §1). No allocator code, test, doc, or artifact
was modified.

---

## Executive verdict

**Round 32 is a genuine runtime round, unlike Rounds 27–31, and its
evidence discipline is the strongest this corpus has produced.** Seven
commits change shipping or opt-in code (`5d72bc6`, `cd5c634`, `eb2463a`,
`74345b8`, `5289c66`, `d38bf73`, `e88390b`), and each carries a gate report,
committed raw logs, a checked derive script, and a summary CSV. The
three specific self-flagged items the round's own retrospective asked to be
re-checked independently all survive scrutiny on their central claims.

The round's most valuable outputs are, in my judgement:

1. the **honest REJECT** of F1b (`2dfeaa3`) — a fully implemented,
   correctness-verified change discarded on measured cost with a literal
   zero-`src/` diff. Verified: **CONFIRMED**;
2. the **self-correction of R32-10** (`2c825b2`) — a task's own report
   caught claiming a measurement was impossible when it was not, corrected
   append-only the same day. Verified: **CONFIRMED on arithmetic**, with two
   provenance caveats;
3. the **P0 owner-binding fix** (`d29a82e`), whose counterfactual test is
   genuinely non-vacuous and whose `owner_id` design (process-global
   monotonic counter, not `&self` address) is correct for the stated threat.

Against that, this review raises **nine findings**, of which the two most
important are:

- **P1 — `main` is currently RED on two of CI's five clippy rows.** This is
  inherited from Round 31, was honestly discovered and filed by task #498,
  and was then carried unfixed through seven more tasks and the round's own
  closing commit.
- **P2 — the round's single most-scrutinised task (#502, `RemoteFreeRing`
  shadow head) has a soundness argument whose write-site enumeration is
  falsified by the very commit that publishes it**, and a
  `#[should_panic]` loom "counterfactual" that panics deterministically on
  every interleaving, so it does not demonstrate the property its own doc
  comment claims it demonstrates. The shipped code is, as far as I can
  determine, still correct — but two of its three stated proofs are weaker
  than advertised.

### Did the allocator get faster for a default `production` user?

Yes, marginally and for the first time in several rounds — but the honest
magnitude is small. `production`'s feature composition in `Cargo.toml` is
unchanged; the default-path wins are `-120 Ir` on `realloc_grow` (R32-3),
`-32 Ir` per large-cache hit (R32-7), `-5.0 Ir` per large-cache admission
(R32-12), and a real cross-thread `push` win of `-30%…-36%` ns/push in the
favorable regime (R32-11, the only wall-clock-significant one). R32-10's
`OWN_CACHE_SIZE` change ships a mechanism improvement with, by its own
report's words, "no latency win claimed" — see finding **F4**.

---

## Local verification actually run

All commands run at `HEAD` = `ab6e0d6`, on Windows 10 Pro,
Intel Core i7-11800H.

| command | result |
|---|---|
| `cargo fmt --check` | **PASS** (exit 0, no output) |
| `cargo test --features production` | **PASS** — every test-result block `ok`, `0 failed`; final block `Doc-tests sefer_alloc … 0 tests` |
| `RUSTFLAGS="--cfg loom" cargo test --features alloc-core,alloc-xthread --test loom_remote_ring` | **PASS** — `8 passed; 0 failed`, finished in 0.32 s (see finding **F2** on what that runtime implies) |
| `cargo clippy --features production --all-targets -- -D warnings` | **FAIL** — see finding **F1** |
| `node scripts/verify-gate-report.mjs` | `PASS WITH 55 WARNINGS (d=31, e=9, identity=15) (99 report(s) scanned)` |
| `node scripts/verify-commit-prefixes.mjs` | `PASS (with warnings)` — 38 commits linted, 4 "direction 2" warnings, all four inspected and legitimate (see §5) |
| `node scripts/r497_dualbitmap_summary.mjs` | **PASS**, regenerated CSV byte-identical to the committed one (`git diff` empty) |

---

## 1. Check-item 1 — task #497 (`2dfeaa3`), F1b dual-bitmap REJECT

**Verdict: CONFIRMED, on both halves of the claim.**

*Zero `src/` diff.* Verified directly:

```
$ git show 2dfeaa3 --name-only --format="" | grep -c "^src/"
0
```

The commit touches exactly six files: `docs/perf/OPEN_ITEMS.md`,
`docs/perf/R32_6_DUAL_BITMAP_GATE.md`,
`docs/perf/R32_6_DUAL_BITMAP_GATE_summary.csv`, two `_raw_r497_*.log`
files, and `scripts/r497_dualbitmap_summary.mjs`. 1,963 insertions,
0 deletions, no `src/`.

*Regression magnitude.* The report's §3.3 table
(`docs/perf/R32_6_DUAL_BITMAP_GATE.md:174-181`) is fully reproducible from
the two committed raw logs. `docs/perf/_raw_r497_dualbitmap_before_production.log:682-689`
gives `small_churn_16b 8,810`, `aligned_churn_640b_a128 8,746`,
`cold_alloc_free_256x16b 50,968`, `recycle_alloc_free_256x16b 99,185`,
`churn_256b 8,810`, and `large_alloc_free_cycle B=4,080` (line 700);
`docs/perf/_raw_r497_dualbitmap_after_production.log:682-689` gives
`9,064 / 8,935 / 51,867 / 101,296 / 9,064` with the same `4,080` bootstrap
proxy. Deltas +254 / +189 / +899 / +2,111 / +254 / **0**. Against a ±10 raw-Ir
churn kill gate that is 18.9×–25.4× — the report's "20-25×" is accurate
(it quotes the three plain-churn benches only, which is the correct
population for that gate).

*Two independent corroborations I checked that the report also claims:*
mimalloc's rows are byte-identical between the two logs
(`mimalloc_small_churn_16b 16,629`, `mimalloc_churn_256b 16,130`,
`mimalloc_cold_alloc_free_256x16b 32,325`,
`mimalloc_recycle_alloc_free_256x16b 53,020` in both), and the
bootstrap-proxy delta is exactly 0. Both hold.

*Derive-script integrity.* Re-running `scripts/r497_dualbitmap_summary.mjs`
regenerates `docs/perf/R32_6_DUAL_BITMAP_GATE_summary.csv` byte-identically
(`git diff` on that path is empty afterwards), and the script's three
assertions (bootstrap delta == 0; every bitmap-touching bench regressed;
`small_churn_16b` and `churn_256b` move by the identical delta) all pass and
are real `throw`s, not `console.log`s
(`scripts/r497_dualbitmap_summary.mjs`, the three `if (...) throw new Error`
blocks). This is the CLAUDE.md derived-not-hand-typed rule working exactly as
intended, and it is the only artifact in the round I could fully round-trip.

*Minor:* §3.4 point 1 reports an isolation-run figure (`small_churn_16b`
8,936) with no committed log, taking the exemption route in §3.5. The
exemption note is explicit and the verdict does not rest on that number, so
this is within the rule as written. Noted, not a finding.

---

## 2. Check-item 2 — task #501 addendum (`2c825b2`), R32-10 §5.2 decomposition

**Verdict: CONFIRMED on the arithmetic and on the script's assertions;
PARTIAL on provenance.**

*The numbers are real.* Each of the three cited raw logs contains the exact
`Instructions:` values the CSV and prose use. Spot-checked at both ends of
the range:

```
docs/perf/_raw_r32_10_killgate_cache4_nocounter.log:5      Instructions: 8810     (small_churn_16b, base)
docs/perf/_raw_r32_10_killgate_cache16_nocounter.log:5     Instructions: 8846     (isolate)
docs/perf/_raw_r32_10_killgate_cache16_withcounter.log:5   Instructions: 9037     (head)
docs/perf/_raw_r32_10_killgate_cache4_nocounter.log:29     Instructions: 99185    (recycle_alloc_free_256x16b, base)
docs/perf/_raw_r32_10_killgate_cache16_nocounter.log:29    Instructions: 99228    (isolate)
docs/perf/_raw_r32_10_killgate_cache16_withcounter.log:29  Instructions: 100763   (head)
```

`Δcache-size-alone` = 36/36/36/43/36 (range **36–43**, as claimed);
`Δcounter-alone` = 191/191/543/1,535/191 (range **191–1,535**, as claimed);
`Δtotal` = 227/227/579/1,578/227 (range **+227 … +1,578**, as claimed). The
"84–97%" counter share is 191/227 = 84.1% and 1,535/1,578 = 97.3% — accurate.

*The script asserts what the prose claims.* I read
`scripts/r32_10_killgate_addendum_summary.mjs` line by line. It contains
three real `throw`-guarded assertions matching the three prose headlines:
(1) the cache-size-alone delta is constant across the flat benches to within
5 Ir **and** `|Δ| ≤ 100` on every bench (the "one-time bootstrap, not per-op"
claim); (2) the counter-alone delta on both 256-iteration benches exceeds
2× the flat-bench maximum (the "scales with call count" claim); (3) the two
components sum to the total on every bench. All three are genuinely
enforced, not printed. This is the strongest per-claim assertion coverage in
the round.

*Is the decomposition itself questionable?* I do not think so, for a reason
independent of the report's own argument: the `Δcache-size-alone` column is
**36 for `cold_alloc_free_256x16b` (a 256-iteration bench) and 36 for
`small_churn_16b` (a single-unit bench)**. A per-operation cost cannot
produce identical absolute deltas across benches differing by ~256× in op
count; a one-time construction cost can. That is a stronger falsification
than the "within 7 Ir" spread the report leads with, and it holds.

### F5 [P3] — the `isolate` arm's provenance cross-reference points at a note about a different arm

§5.2 describes its `isolate` arm as the landing commit "with the two
`fetch_add` calls … TEMPORARILY commented out as a scratch, uncommitted
edit — … no commit exists for this arm; **see the provenance note in §8
below**"
(`docs/perf/R32_10_OWN_CACHE_TIER1_THRASH_GATE.md`, §5.2 "Method", arm 2).

§8's immutable-source-identity caveat, however, is about a **different**
uncommitted edit — the §4.1 `OWN_CACHE_SIZE = 4` "before" arm: *"the 'before'
(`OWN_CACHE_SIZE=4`) measurement was taken by TEMPORARILY editing the
constant in the working tree…"*. `git show 2c825b2 -- docs/perf/R32_10_OWN_CACHE_TIER1_THRASH_GATE.md`
confirms §8 was **not amended** by the addendum commit (the diff has exactly
two hunks: the §5.2 insertion and a §7 strikethrough).

So the §5.2 `isolate` arm has neither of CLAUDE.md's R29-6 forms (temp commit
SHA / `git write-tree` / patch hash / binary hash) **nor** an exemption note
of its own; it borrows one written for a different arm. Since R29-6 is
explicitly forward-looking and this is a brand-new report section, this is a
real, if small, rule miss. Cheap fix: one sentence in §8 covering the
counter-disable scratch edit, or a `git write-tree` captured at the time.

### F6 [P3] — the derive scripts are not idempotent against their own committed artifacts

`scripts/r32_10_killgate_addendum_summary.mjs` hardcodes
`const landingCommit = 'UNFILLED_PLACEHOLDER_40_HEX';` and writes it into
every row. The committed CSV holds the real 40-hex SHA only because the
follow-up commit `c9a3570` filled it by hand. Re-running the "checked derive
script" therefore **destroys** the landing-commit field of the committed
artifact.

This is the round-wide pattern (`a3e3e18`, `62e217f`, `03a6c55`, `48fed64`,
`a632dd4`, `ce3f44d`, `e784dbc`, `f126de1`, `a01445e`, `9dc36d3` are all
"fill the placeholder" follow-ups), so it is a convention, not an accident.
But it means the CLAUDE.md rule "the summary CSV … [is] DERIVED from that raw
data by one checked script" is only true modulo one hand-edited column, and
a future reviewer cannot mechanically re-derive-and-diff a CSV to verify it.
I deliberately did **not** run this script, to avoid dirtying the tree.
Contrast `scripts/r497_dualbitmap_summary.mjs`, which has no
`landing_commit` column and is therefore perfectly round-trippable — that is
the shape worth generalising (emit the SHA from `git rev-parse` at derive
time, or leave it out of the derived file entirely).

---

## 3. Check-item 3 — task #502 (`d38bf73`), `RemoteFreeRing` shadow head

**Verdict: PARTIAL.** The shipped code appears correct and the central
inequality holds under multi-producer races. Two of the three supporting
proofs are weaker than the commit message and report claim, and one
bounded-staleness assumption is unstated.

### 3.1 What holds

The core claim — *a stale `cached_head` can only ever make the ring look
MORE full, so the fast path never accepts a push the real check would
reject* — is correct under concurrent multi-producer interleavings, and I
was unable to construct a counterexample within the intended operating
range. The reasoning that convinces me is not quite the report's:

- `cached_head` is written at exactly one non-test site,
  `full_check`'s refresh (`src/alloc_core/remote_free_ring.rs:965`), always
  with a value read from `head` two instructions earlier (`:964`).
- `cached_head` is therefore **not monotonic** — the classic
  P1-reads-100 / P2-reads-200 / P2-stores-200 / P1-stores-100 interleaving
  moves it backwards. The report does not mention this, and it does not need
  to: backwards motion is the *safe* direction (it inflates apparent
  occupancy).
- Every value `cached_head` ever holds was a real past value of `head`, and
  `head` advances monotonically under `drain` (`:1131`, `h` derived only by
  `wrapping_add(1)` from the previously stored value at `:1097`). Hence
  apparent occupancy `t.wrapping_sub(ch)` ≥ true occupancy `t.wrapping_sub(h)`,
  and the fast path at `:951` can only under-estimate free room.
- `Err` is returned only from `:967`, after a fresh `Acquire` load at `:964`
  — byte-identical to the pre-F10 branch. Confirmed by reading
  `full_check`/`push`/`try_push_uncounted` (`:949-1064`); the CAS-reserve and
  `Release` publish sequences are untouched.

The two positive loom models (`RingModelShadow`, `RingModelShadow1`,
`tests/loom_remote_ring.rs:614+`) faithfully mirror `full_check`'s fast/slow
split, and `RingModelShadow1`'s `CAP = 1` construction genuinely forces the
slow path for the second producer. Both pass. I re-ran the suite myself:
`8 passed; 0 failed`.

### F2 [P2] — the `#[should_panic]` loom counterfactual is deterministic, so it does not prove what it claims

`tests/loom_remote_ring.rs:903` (`counterfactual_shadow_trusts_stale_cache_spuriously_overflows`)
documents itself as *"loom finds the interleaving where the broken variant
overflows despite the ring having room"*, and the commit message
(`d38bf73`) cites it as *"a `#[should_panic]` counterfactual proving the
'always re-derive on the slow path' design is load-bearing"*. It does not do
either. Trace the state:

- The prefill at `:916` (`ring.push(999)` on the `CAP = 1` model) leaves
  `tail = 1`, `head = 0`, `cached_head = 0` (the slow path refreshed it).
- The consumer thread calls `drain`, which touches `head`, the slot and
  nothing else — it never writes `tail` or `cached_head`.
- The producer thread reads `t = tail` (only possible value: `1`) and
  `ch = cached_head` (`:932`; only possible value: `0`), computes
  `1.wrapping_sub(0) = 1`, which is not `< 1`, and returns `false`.

`would_admit` is therefore `false` in **every** execution loom can schedule,
so `assert!(would_admit, "spuriously overflowed…")` fires unconditionally.
Two consequences:

1. The test is interleaving-independent — it would panic identically with no
   concurrency at all, so the loom harness contributes nothing here. (The
   suite's 0.32 s total runtime is consistent with this: loom stops at the
   first failing execution.)
2. Its oracle is invalid in the other direction too. In the interleavings
   where the producer's reads happen *before* the drain completes, the ring
   genuinely **is** full and rejecting is the *correct* answer — yet the test
   still reports it as "spuriously overflowed". A `#[should_panic]` test
   whose assertion is false even when the behaviour under test is right is
   not a counterfactual; it is a tautology.

To be a real counterfactual, the broken variant's check must be reachable
*after* an observed `head` advance (e.g. join the drain thread first, or
have the producer spin until `head_relaxed()` moves), so that the panic
distinguishes stale-shadow rejection from legitimate rejection.

Severity is P2 rather than P1 because the *shipped* implementation is not
affected — the test's failure mode is over-claiming, not masking a bug. But
this is the exact "test vacuity / invalid oracle" class CLAUDE.md's
zero-trust rule targets, in the round's highest-risk task, and it survived
the round's own review.

### F3 [P2] — the soundness argument's write-site enumeration is falsified by the same commit that publishes it

The module doc's monotonicity claim
(`src/alloc_core/remote_free_ring.rs:105-114`) reads:

> "Verified by enumerating every write site: `drain`'s `head.store(h, Release)` …
> **the only OTHER write site**, `dbg_set_cursors`, is `#[doc(hidden)]`
> test-only …"

The gate report repeats it verbatim
(`docs/perf/R32_11_REMOTE_RING_SHADOW_HEAD_GATE.md:45`). Grepping the file
for actual `head` writes:

```
$ grep -n "head().store\|HEAD_OFF) as \*mut u32" src/alloc_core/remote_free_ring.rs
838:  self.head().store(head, Ordering::Release);   # dbg_set_cursors
856:  self.head().store(head, Ordering::Release);   # dbg_advance_head_only  <-- ADDED BY THIS COMMIT
881:  Node::write_u32(Node::offset(ring.base, HEAD_OFF) as *mut u32, 0);  # init_in_place
1131: self.head().store(h, Ordering::Release);      # drain
```

There are **four** write sites, not two. `dbg_advance_head_only` (`:855`)
was introduced by `d38bf73` itself, and the same report acknowledges it
17 sections later (`R32_11_…GATE.md:131`: *"advances ONLY `head`,
deliberately leaving the shadow stale"*) — so the document contradicts its
own §1 within its own text. `init_in_place` (`:881`) is benign (it resets
`head` and `cached_head` together at `:887`), but it is still an
unenumerated write to a value the proof asserts is monotonic.

`dbg_advance_head_only` is the one that matters: it stores an **arbitrary**
`u32` into `head` and deliberately does not touch `cached_head`. Storing a
*lower* value regresses `head` below `cached_head`, producing the
stale-**HIGH** shadow the whole argument declares impossible, which in turn
lets the fast path admit a push into a full ring. Its doc comment
(`:843-857`) documents a quiescent-ring precondition but — unlike
`dbg_set_cursors`, which explicitly requires
`tail.wrapping_sub(head) <= RING_CAP` (`:822`) — says nothing about never
regressing `head`.

Mitigating: the hook is `#[doc(hidden)]`, `alloc-xthread`-gated, and a
`RemoteFreeRing` over a real segment is unreachable from outside the crate
(`at()` is `pub(crate)`; `over_test_buffer` is `pub unsafe fn`). Its only
caller is `tests/remote_ring_shadow_head.rs:288`, which uses
`h.wrapping_add(1)` — an advance. It is correctly enumerated in
`tests/dbg_hook_safety_tripwire.rs:376-379` under `SAFE_MUTATORS` with a
bounded-blast-radius justification. So this is a **documentation-completeness
defect in a formally-stated proof**, not a live soundness hole. It is
reported at P2 because the round's own framing ("formally verified", "the
only OTHER write site") is precisely the kind of claim that must be exactly
true to be worth anything.

### F7 [P3] — the shadow's staleness lag is unbounded, and the wrap argument does not account for that

The module's "Wrap correctness" paragraph
(`src/alloc_core/remote_free_ring.rs:158-168`) argues that `cached_head`
"inherits the exact same `u32` wrapping-counter semantics as `head`/`tail`"
and "adds no new modulus arithmetic". That is true of the *shape* of the
comparison but not of its *precondition*.

The pre-F10 check compared `t` against a `head` value read microseconds
earlier: the lag was bounded by cache-coherence latency. The shadow's lag is
bounded by **nothing** — a producer preempted between the `Acquire` load at
`:964` and the `Relaxed` store at `:965` writes an arbitrarily old value.
"`cached_head <= head`" is only meaningful modulo `2^32`: if the true `head`
advances by exactly `2^32 - k` during that window, the stored value is
modularly `k` **ahead**, and `t.wrapping_sub(ch)` under-reports occupancy by
`k`. At `k = 1` with a genuinely full ring, the fast path at `:951` admits
a push it must not — premature slot reuse.

I want to be clear about the practical weight: this requires a thread to be
descheduled between two adjacent instructions while ~4.29 × 10⁹ drains
complete on that one segment's ring. It is not a bug I expect anyone to hit.
But the module itself already treats a full `u32` cursor wrap as reachable —
`:319-321` calls it *"the ONE genuinely reachable wrap hazard (2^32
cross-thread frees on a single hot, long-lived segment)"* and spends a
compile-time assert plus a dedicated regression suite
(`tests/regression_ring_cursor_wrap.rs`) on it. Given that stance, a proof
that silently assumes "shadow staleness < 2^32" should say so. One sentence
stating the assumption, or refreshing `cached_head` with a
`fetch_max`-style monotone update instead of a plain `store`, closes it.

### 3.2 Measurement quality (R32-11)

Good, and unusually self-critical. The report discloses three separate
false starts, including the genuinely instructive one where the harness's
own path-activation counters (`DBG_RING_PUSH_SHADOW_FAST`/`_SLOW`,
`src/alloc_core/remote_free_ring.rs:275-293`) added a locked RMW per push
and made the fix look like a regression (`t = -13.3`, reproduced 3×). The
resolution — an oracle-bearing build to prove regime activation plus a
`bench-internals`-free build for the cited timings — is the correct one and
directly satisfies CLAUDE.md's R30-8 mechanism-evidence rule without
contaminating the timing axis. Both counters are `bench-internals`-gated,
per the benchmark-hook rule.

The committed CSV (`docs/perf/R32_11_REMOTE_RING_SHADOW_HEAD_GATE_summary.csv`)
is internally consistent (sign conventions, `t` vs `crit`, sign-test counts)
and its `git_commit` column is the base SHA `c9a3570bfa…` in full 40 hex.
The adversarial arm is honestly reported as 3/5 significant with two runs
attributed to host contention; the `adversarial_trial3_n30_rerun` row's
before-arm mean (5,829 ns/push vs 2,339 in trial 1 — 2.5×) makes that
attribution credible rather than convenient.

---

## 4. Check-item 4 — tasks #498 / #503 vs. the R25-1 hazard pattern

**Verdict: CONFIRMED clean. Neither task introduces the hazard shape.**

I swept the whole round's `src/` diff for the specific signature:

```
$ git diff d29a82e~1..ab6e0d6 -- src/ | grep -E "^\+.*pub (unsafe )?fn .*\*(mut|const)"
+    pub fn dbg_decomp_win_reserve_only() -> Option<(*mut u8, *mut u8, usize)>
+    pub unsafe fn dbg_decomp_win_commit_only(base: *mut u8) -> bool
+    pub unsafe fn dbg_decomp_win_release_only(reservation_ptr: *mut u8, reservation_len: usize)
+pub fn current_for_trim() -> Option<*mut HeapCore>
```

(the three `dbg_decomp_win_*` entries appear twice — once in
`alloc_core_small_pool.rs`, once in the `heap_core_diag.rs` delegation.)

Assessment of each:

- **Task #498 (`eb2463a`)** adds `set_large_align_at` / `set_bump_at` /
  `set_magic_at` (`src/alloc_core/segment_header_views.rs:222-291`). All
  three take `base: *mut u8` and write header metadata — but all three are
  **`pub(crate)`**, not `pub`, so they are not reachable from downstream safe
  code and the R25-1 rule does not engage. Their doc comments each restate
  the UBFIX-6 unregistered-window precondition, and the accompanying
  `const _: () = assert!(size_of::<SegmentHeader>() == 144);`
  (`src/alloc_core/segment_header.rs:1324`) is a genuinely useful addition:
  it converts "which fields does the targeted write need to cover" from a
  comment into a build failure. Good practice, adopted unprompted.
- **Task #503 (`e88390b`)** adds exactly one new public surface,
  `AllocCore::dbg_large_cache_occupied_bits(&self) -> u64`
  (`src/alloc_core/alloc_core_large_cache.rs:707-716`). No pointer, no
  mutation, pure observer. It is gated on `alloc-decommit` (a `production`
  feature) rather than `bench-internals`, which reads at first glance like a
  violation of CLAUDE.md's benchmark-hook rule 2 — but the codified policy in
  `tests/dbg_hook_safety_tripwire.rs` rule (a) explicitly exempts
  allowlisted pure observers, and the hook is correctly listed at
  `tests/dbg_hook_safety_tripwire.rs:227`. Within policy.
- **Task #504 (`f6c3a61`)**: the two pointer-*consuming* hooks are
  `pub unsafe fn` and both are added to `UNSAFE_HOOKS`
  (`tests/dbg_hook_safety_tripwire.rs:458-459, 472-473`); the
  pointer-*producing* one is safe but `bench-internals`-gated
  (`src/alloc_core/alloc_core_small_pool.rs:1121-1123`), satisfying rule (a)
  without needing an allowlist entry. Correct.
- **`current_for_trim`** (`src/global/tls_heap.rs:557`) is a safe `pub fn`
  returning `Option<*mut HeapCore>` in a `pub mod` (`src/global/mod.rs:30`).
  This is the same shape as the pre-existing `current_for_alloc` /
  `current_for_dealloc` (`tls_heap.rs:397, 492`), which already hand out
  heap pointers inside `CurrentHeap`. Not a new class of exposure, and
  returning a raw pointer is not itself unsound. No finding.

Independently, task #503's own correctness argument (only two functions in
the crate ever write a large-cache slot, both updated in lockstep) checks
out against the diff: `large_cache_slot_set` sets the bit in both the base
and extension arms (`alloc_core_large_cache.rs:287, 298`),
`large_cache_slot_take` clears it in both (`:127, :142`), and the clear
happens only *after* `.expect()` proves an entry was present — so the mask
can never be cleared for an already-empty slot. The `u64` width is pinned by
a compile-time assert against `LARGE_CACHE_SLOTS + LARGE_CACHE_EXTENDED_SLOTS`
(`src/alloc_core/alloc_core.rs:106-112`). This is the right shape.

---

## 5. Check-item 5 — commit-prefix taxonomy

**Verdict: CONFIRMED, with one out-of-taxonomy prefix that is nonetheless
defensible.**

`node scripts/verify-commit-prefixes.mjs` reports `PASS (with warnings)`
over all 38 unpushed commits. The four "direction 2" warnings all flag
`bench(…)` commits that touch `src/` or `Cargo.toml`; I inspected each:

- `f6c3a61` (#504) — `src/` changes are three `bench-internals`-gated hooks
  plus two `*_for_measurement` siblings of production functions; no
  production reservation policy changed. **Legitimate.**
- `2ea920b` (#500) — one `bench-internals` diagnostic in `heap_core_diag.rs`
  plus `Cargo.toml` bench registration. **Legitimate.**
- `c72a27b` (#490) — `Cargo.toml` only (bench registration). **Legitimate.**
- `4f89723` (#488) — `src/global/sefer_alloc.rs` diagnostic accessor plus
  `Cargo.toml`. **Legitimate.**

Spot-checking the other direction (does `perf(runtime)` appear only where
shipping code changed?): all seven `perf(runtime)`/`perf(opt-in)` commits
change `src/` files reachable from `production` or from a named opt-in
feature, and each commit body states which. `cd5c634` and `74345b8` are
correctly `perf(opt-in)` (virgin-zero-skip / non-default `LargeCachePolicy`
variants respectively). No misuse found.

The one deviation is `5df56d3`, **`fix(perf)`** — a prefix not in R30-12's
four-way taxonomy at all, on a commit that does change shipping code
(`src/registry/tcache.rs`, `PerClass` gains `#[repr(C)]`). The report
justifies the choice explicitly
(`docs/perf/R32_5_PERCLASS_REPR_C_LAYOUT_FIX_GATE.md`, §6: *"`fix(perf)`, not
`perf(runtime)`/`perf(opt-in)`: no runtime algorithm or default changed, and
no measurable speedup is claimed"*). That reasoning is honest and arguably
better than mislabelling a 0-Ir layout fix as a perf win — but the taxonomy
as written has no such slot, and the lint does not enforce one. Worth a
one-line CLAUDE.md amendment rather than a fix to the commit.

---

## 6. Check-item 6 — raw-log and summary-CSV policy

**Verdict: PARTIAL. Landing-commit SHAs are fully fixed; two reports break
the CSV naming rule.**

*Landing commits.* I scanned every R31/R32 summary CSV for hex tokens
7–39 characters long (the short-SHA signature):

```
$ for f in docs/perf/R3[12]_*_summary.csv; do
    grep -oE '\b[0-9a-f]{7,39}\b' "$f" | grep -E '[a-f]' | sort -u
  done
```

Zero hits across all 17 files. Every `landing_commit` / `base_commit` /
`git_commit` value is a full 40-hex SHA, and
`grep -rn "TBD\|PLACEHOLDER\|<fill\|TODO" docs/perf/R32_*.md docs/perf/R32_*.csv`
returns nothing. The short-SHA bug is genuinely fixed across the round, not
just in the caught instances. **CONFIRMED.**

*Cited raw logs.* `node scripts/verify-gate-report.mjs` scans 99 reports and
reports `PASS WITH 55 WARNINGS`, with no failures — check (c), "cited raw
logs exist", is green for every R32 report. Spot-verified by hand:
`ls docs/perf/` shows all 24 `_raw_r32_*.log` files plus `r32_11_run.json`
present.

### F8 [P3] — two of this round's reports break the same-base-name summary-CSV rule

CLAUDE.md requires the companion to be
`docs/perf/<REPORT_NAME>_summary.csv`, "same base name as the report it
summarizes". Two Round-32 reports use the task-number naming instead:

| report | committed CSV | expected |
|---|---|---|
| `R32_4_ALLOC_ZEROED_MAGAZINE_HIT_STAMP_REMOVAL_GATE.md` | `R495_STAMP_REMOVAL_GATE_summary.csv` | `R32_4_ALLOC_ZEROED_MAGAZINE_HIT_STAMP_REMOVAL_GATE_summary.csv` |
| `R32_5_PERCLASS_REPR_C_LAYOUT_FIX_GATE.md` | `R496_PERCLASS_REPR_C_LAYOUT_FIX_GATE_summary.csv` | `R32_5_PERCLASS_REPR_C_LAYOUT_FIX_GATE_summary.csv` |

(cited at `R32_4_…GATE.md:152,188` and `R32_5_…GATE.md:144`). Both files
exist and are correctly cited in prose, so `verify-gate-report.mjs` check (a)
passes — the verifier follows the cited path rather than deriving the
expected name, so this class of drift is invisible to it. Low impact
(grep-ability across rounds), trivially fixable, and worth one extra
assertion in the verifier.

---

## 7. Additional findings the round's retrospective did not flag

### F1 [P1] — `main` is red on two of CI's five clippy rows at the round's final commit

At `HEAD` = `ab6e0d6`:

```
$ cargo clippy --features production --all-targets -- -D warnings
error: doc list item without indentation
   --> examples\_shared/r31_3_large_cache_extended_narrow_ab_workload.rs:257:9
    = note: `-D clippy::doc-lazy-continuation` implied by `-D warnings`
error: could not compile `sefer-alloc` (example "r31_3_large_cache_extended_narrow_off") due to 1 previous error
```

`.github/workflows/ci.yml:130` runs exactly
`cargo clippy --all-targets --features "production" -- -D warnings`, and
`:111` runs the `--all-features` variant, which fails the same way. So two
of the five clippy rows in CI's own matrix are currently failing on `main`.

**This is correctly attributed and honestly filed** — task #498 discovered it
during its own verification pass, re-verified in an isolated worktree at the
pre-#498 base `2dfeaa3` that the failure predates the task, and recorded it
as `docs/CORRECTNESS_OPEN_ITEMS.md` item 11 (lines 951–984) along with a
third row (`hardened medium-classes`, `E0601: main function not found` in
`examples/r31_10_trim_cost_gate.rs:326`). The diagnosis in that item is
accurate and the root cause (Round 31 example files) is correct.

What I am flagging is the *disposition*, not the discovery: item 11 was
filed at task #498 and then carried, unfixed, through tasks #499–#505 and
seven further commits including the round's closing CHANGELOG commit. The
fix is one line of indentation in one example doc comment plus one `fn main`.
CLAUDE.md's own "before every push: `npm run check`" rule exists to prevent
exactly a red-CI push, and item 11's own text observes that `npm run check`
"apparently did not [catch it] either, or drifted since" — which is a second,
still-open finding embedded inside the first: **the pre-push gate does not
cover the feature/target combinations CI actually runs.** Both deserve to be
the first task of Round 33.

### F4 [P2] — R32-10 ships a production default change on the weakest latency evidence in the round

`5289c66` changes `OWN_CACHE_SIZE` 4 → 16 — a production default constant,
correctly tagged `perf(runtime)` per the taxonomy. Its own summary CSV
(`docs/perf/R32_10_OWN_CACHE_TIER1_THRASH_GATE_summary.csv`) reports
`ns_per_op_median` for both arms across seven K values:

| K | before (cache=4) | after (cache=16) | Δ |
|---:|---:|---:|---:|
| 4 | 23.990 | 26.099 | **+2.11 (+8.8%)** |
| 8 | 27.309 | 26.382 | −0.93 |
| 16 | 27.734 | 28.930 | +1.20 |
| 24 | 28.718 | 28.428 | −0.29 |
| 32 | 27.762 | 28.323 | +0.56 |
| 48 | 25.731 | 28.165 | **+2.43 (+9.5%)** |
| 64 | 26.554 | 26.145 | −0.41 |

The after arm is slower in 4 of 7 cells, **including K = 4 — the exact arm
where the mechanism win is maximal (0.00% → 99.99% Tier-1 hit rate)**. The
report handles this honestly in prose (§4.1 Headline 4: *"the latency signal
is an HONEST NULL … No latency win is claimed by this report"*) and offers a
plausible mechanism (a ~4 Ir Tier-1-vs-Tier-2 delta is ~1–2 ns, below the
harness's noise).

My concern is that the null is **asserted, not demonstrated**. Grepping the
whole report for any dispersion statistic —
`grep -nE "t=|stddev|confidence|same-vs-same|IQR|p-value"` — returns exactly
one hit, and it is the phrase "run-to-run noise band" itself. There is no
standard deviation, no t-test, no confidence interval, and no same-vs-same
control on the latency axis; the only evidence offered for "this is noise" is
that K=16 also varied (27.7 → 28.9) in an arm where the hit rate did not
change. That is a plausibility argument, not a measurement.

This matters because two *other* tasks in the same round set the bar higher
for exactly this question: R32-11 ran 20-pair A/B with `t` vs `crit` plus
before-vs-before and after-vs-after same-vs-same controls
(`R32_11_…_summary.csv` rows `favorable_before_control`,
`favorable_after_control`, `adversarial_after_control`), and R32-12 reported
`t=0.492 vs crit=2.101` with an explicit same-vs-same control row before
calling its wall-clock result a null. R32-10 changes a `production` default
and the addendum later established it also carries a real (if one-time)
+36–43 Ir bootstrap cost — so this is the one task in the round where a
rigorous latency null was most load-bearing, and it is the one that did not
run one. The correct disposition is not to revert; it is to re-run the
existing harness with the same paired-A/B + control machinery the round
already owns, and record the result.

### F9 [P2] — R32-8 measures the benefit of its own trade and argues the cost

`74345b8` throttles `maybe_decay_large_cache`'s clock read to 1-in-64 once
past the headroom guard
(`src/alloc_core/alloc_core_large_cache.rs`, `DECAY_CLOCK_CHECK_STRIDE`).
I read the implementation carefully and it is correct: the counter resets to
0 on every real clock read, so the stride always counts from the last check;
`last_decay_tick.is_some()` keeps the first-ever priming call unthrottled;
`dbg_force_decay_tick` primes the counter to `STRIDE - 1` so its documented
"every call fires exactly one tick" contract is preserved byte-for-byte. The
"never fires EARLY" claim holds — a delayed clock read cannot fabricate
elapsed time. The §3.1 explanation of why the measured reduction is 128×
rather than the nominal 64× (only the alloc-side call of each cycle sees
`used > headroom` in this single-resident-object workload) is mechanistically
sound and correctly scoped as workload-specific.

The gap is on the cost axis. The trade is: **decay ticks may now fire up to
63 large ops late**, and decay is event-driven only — it runs *only* on a
large alloc/free. The two profiles this change targets, `LowHeadroom` (16
MiB) and `Trimmed64MiB`, exist for exactly one purpose: bounding retained
RSS. A workload that crosses the headroom and then performs fewer than 64
further large ops now retains cached spans that the pre-change code would
have released on the very next op. The report never measures this. It argues
it away qualitatively (*"63 large ops is a small sliver of time on any
workload with meaningful large-op throughput … on a sparse-large-op workload
the guard was already mostly idle-triggered"*,
`docs/perf/R32_8_LARGE_CACHE_DECAY_CLOCK_READ_GATE.md`, §4), which is
reasonable but is precisely the shape CLAUDE.md's own "cost and benefit must
be measured in the SAME workload regime" rule was written against: the
benefit (ns/call saved) is measured in a high-throughput regime, the cost
(retention) is asserted for a low-throughput regime and never instrumented,
and the two are combined into one GO. The instrument to close it already
exists in this repo (R29-13's retention harness / `dbg_large_cache_used`),
so this is a re-run, not new infrastructure.

### F10 [P3] — R32-3 is the round's only shipping change with no gate report, no CSV and no raw log

`5d72bc6` (task #494) is tagged `perf(runtime)`, changes
`src/registry/heap_core_free.rs` on the default `production` path, and bases
its ship decision on measured numbers (`realloc_grow` 492,694 → 492,574 Ir;
four churn benches byte-exact). Those numbers exist **only in the commit
message**. There is no `docs/perf/R32_3_*.md`, no `_summary.csv`, no
`_raw_*.log`, and no derive script — `ls docs/perf/ | grep R32` confirms the
gap between `R32_0_…` and `R32_4_…`.

CLAUDE.md's boundary rule is phrased in terms of "a perf-gate report", so a
commit message arguably falls outside its letter. But the rule's stated test
is *"does the verdict rest on a number obtained by running something"* — and
here it plainly does. R32-4 and R32-5, which are comparable in size and
smaller in measured effect, both received a full report + raw logs + checked
derive script. The inconsistency is the finding; either R32-3 owes the same
artifacts, or the rule should say explicitly that a commit-message-only
measurement is acceptable for a change below some threshold.

### F11 [P2] — Round 32 has no `### Round 32` heading in CHANGELOG.md, and a bolded "Runtime improvements this round: 0" sits directly above eight runtime improvements

`ab6e0d6` is titled "docs: update CHANGELOG.md for Round 32 (tasks
#486-505)", but adds its entries under the existing `### Round 31` heading
(`CHANGELOG.md:10`), whose one-paragraph title text now runs from R31-0
through task #505. `grep -n "^### Round" CHANGELOG.md` confirms no
`### Round 32` heading exists.

The concrete consequence is at `CHANGELOG.md:16-28`:

```
16: **Runtime improvements this round: 0.** This is measurement-only work — …
17:
18: #### Runtime improvements
19:
20-28: [eight bullets, six of them tagged "[runtime improvement …]",
       covering R31-10, R32-3, R32-4, R32-7, R32-8, R32-10, R32-11, R32-12]
```

Line 16 is, in context, a per-task statement about R31-0 (`virgin-zero-skip`
was deliberately not promoted). But CLAUDE.md itself treats the exact string
"**Runtime improvements this round: N**" as a *round-level* honesty signal —
the R30-12 rule quotes it verbatim as the precedent it is generalising, and
every other round in this file uses it that way (lines 65, 100, 131, 146,
170, 192, 217, 239). A reader skimming `CHANGELOG.md` — the reading path
that rule exists to protect — now sees a bolded "Runtime improvements this
round: 0" two lines above a heading listing eight of them. Round 32 is the
first round in nine to actually ship runtime wins, which makes this the
worst possible round for that particular collision.

The individual bullets themselves are accurate and honestly tagged
(R32-10's bullet, `CHANGELOG.md:26`, correctly says "Latency delta an honest
noise-band null"). The defect is structural placement, not content. The
one over-claim I did find is in the *commit message* of `ab6e0d6`, which
groups R32-10 under "Runtime improvements (…, **real measured wins**)" and
cites "hit rate 0%->99.99% at K<=8" as its evidence — a mechanism metric
presented in a bucket labelled for measured wins, for the one task whose
report explicitly declines to claim a win. The durable artifact
(CHANGELOG.md) is honest; the commit subject/body is not, which inverts the
usual direction of this project's reporting-honesty problem.

---

## 8. Things I checked and found sound (no finding)

- **Task #486 (`d29a82e`) P0 fix.** `dbg_decomp_release` is `unsafe fn`
  again with a documented `# Safety` contract, plus a release-surviving
  (non-`debug_assert!`) owner-id check. The `owner_id` source is a
  process-global monotonic `AtomicU64` `fetch_add`
  (`src/alloc_core/alloc_core.rs`, `DBG_RESERVATION_OWNER_ID_COUNTER`), and
  the field's doc comment explicitly rejects the `&self`-address alternative
  for the right reason (an `AllocCore` is `Send` and movable, so addresses
  are reusable). The field is `bench-internals`-gated so it costs zero bytes
  × `MAX_HEAPS = 4096` in a production build. The counterfactual test
  (`tests/r31_15_reserved_small_segment_cross_core_release.rs:90`,
  `#[should_panic(expected = "handle was reserved by a DIFFERENT AllocCore")]`)
  documents its own non-vacuity honestly, including the subtlety that the
  pre-fix counterfactual is behavioural rather than compile-time. This is the
  best-argued fix in the round.
- **Task #503's invariant test** (`tests/large_cache_occupancy_bitmask_invariant.rs`,
  4 tests) compares the mask bit-for-bit against actual slot occupancy at
  every step, and the commit message discloses that the test caught two of
  its own false assumptions before either was mistaken for a bitmask bug —
  falsification-first working as intended.
- **Task #498's `debug_assert_eq!` premise check.** Adding the assertion
  that the four carried-forward fields really are unchanged *before*
  narrowing the write, and keeping it as a permanent pin, is the correct
  order of operations and is the pattern other tasks should copy.
- **`tests/dbg_hook_safety_tripwire.rs`** correctly absorbed every new hook
  this round added (4 `UNSAFE_HOOKS` entries from #504, 1 `SAFE_MUTATORS`
  entry from #502, 1 observer allowlist entry from #503, 1 from #500). The
  shape-independent R30-2 redesign is holding.
- **`verify-gate-report.mjs`'s non-retroactivity scoping** (R32-2, `f3020fd`)
  correctly SKIPs checks (e)/(f) for reports predating their rule commits,
  with the rule commit SHA and date printed in the skip reason. That is the
  right way to encode a non-retroactive rule in a linter.

---

## 9. Recommended dispositions

Ordered by what I would do first.

1. **F1** — fix the two example files (one doc-comment indent, one missing
   `fn main`), get all five CI clippy rows green, and then separately fix the
   `npm run check` coverage gap item 11 itself identifies. This is the only
   finding that leaves `main` in a failing state.
2. **F2** — rewrite `counterfactual_shadow_trusts_stale_cache_spuriously_overflows`
   so the broken check runs strictly after an observed `head` advance, and
   confirm it still panics (and that a *correct* full-check in the same
   position does not).
3. **F3** — correct the write-site enumeration in both
   `src/alloc_core/remote_free_ring.rs`'s module doc and
   `R32_11_…GATE.md` §1 (append-only), and add a "must never regress `head`"
   precondition to `dbg_advance_head_only`'s doc.
4. **F4** — re-run R32-10's latency axis through the round's own paired-A/B +
   same-vs-same machinery and record the result, whichever way it falls.
5. **F9** — measure R32-8's retention cost in the low-large-op-throughput
   regime, using R29-13's existing harness.
6. **F11** — split a `### Round 32` heading out of the Round-31 section, with
   its own accurate "Runtime improvements this round: N" line.
7. **F5 / F6 / F7 / F8 / F10** — small, mechanical, and individually cheap;
   F6 in particular is worth generalising (derive the landing SHA in-script
   from `git rev-parse`, or drop it from derived files) because it currently
   defeats the main mechanical check a future reviewer would want to run.

None of these findings calls for reverting any of the round's seven shipping
changes.
