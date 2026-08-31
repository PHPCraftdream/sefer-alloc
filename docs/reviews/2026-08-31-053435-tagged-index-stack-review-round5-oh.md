# `tagged-index-stack` — independent publish-readiness review, round 5

- **Reviewer:** Claude (Opus 5), adversarial pass. Source read first and findings formed
  independently; the round-1..4 review docs were read afterwards, to check overlap and to see
  what a prior round's *remediation* left half-done.
- **Date:** 2026-08-31 05:34:35 +0200 (CEST)
- **Revision reviewed:** `d944b89662d101bc7773870845207dd605dbff0a` (landing SHA on `main`;
  working tree clean w.r.t. `crates/tagged-index-stack/`, `.github/workflows/ci.yml`,
  `scripts/loom.mjs`)
- **Scope:** `crates/tagged-index-stack/**` (src, tests, benches, README, CHANGELOG,
  Cargo.toml), the `.github/workflows/ci.yml` rows for this crate, `scripts/loom.mjs`,
  `docs/correctness-open-items/TRACKED_ci_gate_coverage.md` item 25, and the in-tree
  consumers (`src/registry/heap_registry.rs`, `src/registry/bootstrap.rs`).
- **Verification actually performed** (every number below came from running something):
  - `cargo test -p tagged-index-stack --no-fail-fast` — **27 tests green** (17 `stack_unit`,
    5 `proptest_pack_unpack`, 2 `regression_counter_wrap`, 2 `custom_links_impl`,
    1 `readme_example`; `loom_aba` correctly compiles to 0 tests; 0 doctests).
  - `cargo test -p tagged-index-stack --release --no-fail-fast` — **26 tests green** (the
    `#[cfg(debug_assertions)]`-gated `pop_debug_assert_fires_on_invalid_next_from_backing`
    correctly disappears; this is the gate `d944b89` added, and it works).
  - `RUSTFLAGS="--cfg loom" cargo test --release -p tagged-index-stack --features loom
    --test loom_aba` — **10 tests green in 0.15 s**, all three `#[should_panic]`
    counterfactuals included.
  - `cargo clippy -p tagged-index-stack --all-targets -- -D warnings` — clean.
  - `RUSTDOCFLAGS="-D warnings" cargo doc -p tagged-index-stack --no-deps` — clean.
  - `cargo package --list -p tagged-index-stack` → **16 files** (15 last round + the new
    `tests/custom_links_impl.rs`), no strays.
  - `cargo bench -p tagged-index-stack --bench tagged_index_stack_bench` — real numbers,
    quoted in P3-1 below. `bench-iters.txt` verified byte-identical afterwards (no
    JIT-calibration rewrite).
  - **Three out-of-tree scratch experiments** (a standalone copy of the crate outside the
    workspace; nothing in the repo was modified, and the scratch tree was deleted
    afterwards):
    1. a per-test instrumentation probe measuring exactly which loom tests increment
       `POP_RETRY_COUNT` / `PUSH_RETRY_COUNT` — proves P2-1's mechanism;
    2. a neutered-model counterfactual reproducing P2-1's *effect* (a model in which
       `push`'s retry arm is structurally unreachable still reports `ok. 10 passed`);
    3. a controlled A/B contention probe isolating `ArrayLinks` cache-line spacing —
       the measured basis for P3-1.
- **Machine / toolchain for every measured number:** `rustc 1.97.0 (2d8144b78 2026-07-07)`,
  `cargo 1.97.0`, host `x86_64-pc-windows-msvc`, 11th Gen Intel Core i7-11800H (8 cores /
  16 threads, 2.30 GHz base), Windows 10 Pro 19045. Same machine as rounds 3 and 4.
- **No code was changed.** This is a read-only review.

---

## Overall verdict: **CONDITIONAL-GO**

**The shipping algorithm remains correct.** I re-derived the load-bearing claims rather than
inheriting round 4's verdict:

- **Tag monotonicity along the head's modification order** holds: every successful `push` does
  `wrapping_add(1)`, every successful `pop` preserves the tag including across the
  drain-to-empty transition, so `(X, t)` can only recur after a full `2^TAG_BITS` wrap whose
  last push re-installs `X`.
- **`pop`'s `Acquire`-without-`Release` success ordering is sound** for the reason the `head`
  field's `INVARIANT` block states, and the premise still holds: I re-grepped for a plain
  `store` to `head` and found none — `new` is initialization, `raw_head`/`is_empty` only load,
  and all three writers (push's CAS, pop's CAS, the loom-only `cas_head_for_test`) are RMWs.
- **`push`'s `Relaxed` CAS-failure ordering is sound** — push uses the failure read's index
  half as a *value* to store and its tag half as a *number* to bump; it never dereferences a
  link through it.
- **`pop_pop_conservation`'s derivation checks out against the real `pop`.** I traced it:
  with exactly 2 seeded indices, exactly 2 poppers and no third actor, the head cannot become
  empty until after the second successful CAS, and that CAS belongs to one of the two poppers
  — so "both return `Some`, partitioning `{0,1}`" really is the only reachable shape, not an
  over-strong assertion. The test is not vacuous.
- **`push_push_conservation`'s conservation assertion is correct**, and its model genuinely
  forces a push retry (the second CAS's snapshot is the initial empty word).
- **The T10 gating is right, and the release-profile trap round 4 hit really is closed.**
  `[profile.release]` at the repo root sets only `lto`/`codegen-units` — no
  `debug-assertions` override — so `debug_assert!` is compiled out under
  `cargo test --release`, exactly as `stack_unit.rs`'s new `#[cfg(debug_assertions)]` comment
  claims. Verified by running both profiles: 17 vs 16 tests, both green. The neighbouring
  `array_links_load_next_panics_on_index_out_of_range` correctly does *not* carry that gate —
  slice bounds checks are not debug-only — and it runs and passes in release.

What holds this back from an unconditional GO is **one P2, and it is the same defect class
round 4 raised as its own P2-1, re-opened by round 4's own remediation**: the activation
oracle added in `5362a1d` reads a counter that `6c07cc6`'s `MODEL_LOCK` does not actually
make exclusive. I proved it twice — once by measuring the leak (27 increments from an
unlocked test per exploration), once by reproducing the effect end-to-end (a deliberately
neutered model reports `ok. 10 passed` in 3 of 5 default-parallel runs while failing
deterministically under `--test-threads=1`).

Beyond that, the P3 block is again dominated by **cross-file drift introduced by round 4's own
fixes** — which is precisely the class the brief asked me to look for, and it is present in
four independent instances (P3-2, P3-3, P3-4, P3-5). Two of them are literally the same
sentence/count round 4 fixed and then re-broke three commits later in the same wave.

**Perf: one actionable, measured finding this round** (P3-1) — the first in five rounds. It is
not in the stack's CAS loops (those are as tight as a single-word Treiber head gets); it is in
`ArrayLinks<N>`'s layout, and it is worth **1.43–1.45×** of multi-threaded throughput on this
machine, reproducibly, with everything else held constant.

---

## P2 — should fix before publish

### P2-1. The `PUSH_RETRY_COUNT` activation oracle added in round 4 is not exclusive: one unlocked test drives the real `push` under contention and leaks 27 increments into it per exploration

**Location:** `crates/tagged-index-stack/tests/loom_aba.rs:73-86` (`MODEL_LOCK` and its
doc), `:725-729` (`counterfactual_relaxed_cas_failure_corrupts_free_list` — **no lock**),
`:621-710` (`run_cas_retry`, whose thread B calls the REAL `push`), `:755-801`
(`push_push_conservation` and its oracle), `crates/tagged-index-stack/src/lib.rs:1093-1132`
(`PUSH_RETRY_COUNT` / `push_retry_count_for_test` and their published claim).

Round 4's P2-1 diagnosed exactly this shape for `POP_RETRY_COUNT` and the fix (`6c07cc6`)
introduced `MODEL_LOCK` with this stated scope (`:73-74`):

> "Serializes every test in this file that drives the REAL `pop` under contention."

Then, three commits later, `5362a1d` added a **second** counter — `PUSH_RETRY_COUNT` — and a
second oracle in `push_push_conservation`. The lock's membership rule was never widened from
"drives the real `pop`" to "drives the real `push` **or** `pop`", and one test falls exactly
in the gap.

**Classified by hand, all ten models** (which real operations each drives, and whether it
takes the lock):

| # | test | drives real `push`/`pop`? | `MODEL_LOCK`? |
|---|---|---|---|
| 1 | `aba_repush_keeps_free_list_conservation` | B: real pop + push | yes (`:112`) |
| 2 | `counterfactual_untagged_head_lets_aba_corrupt_free_list` | no (local `UntaggedStack`) | no — correct |
| 3 | `tagged_stack_survives_the_same_resurrection_pattern` | B: real pop + push | yes (`:316`) |
| 4 | `pop_empty_transition_preserves_tag` | B: real pop + push | **no** |
| 5 | `counterfactual_empty_transition_tag_reset_lets_aba_recur` | B: real push | **no** |
| 6 | `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type` | real pop ‖ real push | yes (`:567`) |
| 7 | `cas_retry_path_must_acquire_with_concurrent_push` | B: real push | yes (`:718`) |
| 8 | `counterfactual_relaxed_cas_failure_corrupts_free_list` | B: real push | **no** |
| 9 | `push_push_conservation` | two real pushes | yes (`:757`) |
| 10 | `pop_pop_conservation` | two real pops | yes (`:832`) |

**Measured, not argued.** Out-of-tree scratch copy, per-test instrumentation of both counters,
`--test-threads=1` so each figure is that test's own:

```
PROBE run_h2(true):   pop_delta=0  push_delta=0     (model panicked=false)
PROBE run_h2(false):  pop_delta=0  push_delta=0     (model panicked=true)
PROBE untagged:       pop_delta=0  push_delta=0     (model panicked=false)
PROBE(Acquire)  run_cas_retry(Ordering::Acquire):  PUSH_RETRY_COUNT delta = 81
PROBE(Relaxed)  run_cas_retry(Ordering::Relaxed):  PUSH_RETRY_COUNT delta = 27
```

So rows 4 and 5 contribute zero — but only because `run_h2`'s two-flag rendezvous happens to
make thread B's real `pop`/`push` uncontended (A performs no head modification between
`a_loaded.store(1)` and `b_done`). That is an unstated, unenforced property of that harness,
not a rule anything checks; any future edit to the rendezvous silently re-arms them.

Row 8 is not benign: `run_cas_retry(Relaxed)` — the body of the **unlocked**
`counterfactual_relaxed_cas_failure_corrupts_free_list` — adds **27** to `PUSH_RETRY_COUNT`
per exploration, and `push_push_conservation` reads a raw before/after delta on that same
process-global static (`src/lib.rs:1106`, a real `core::sync::atomic::AtomicUsize`).

**Failure scenario, executed rather than argued.** Same scratch copy. One change to
`push_push_conservation`'s *model*: join thread A before spawning thread B, so the two pushes
never race and **`push`'s CAS-retry branch is structurally unreachable in that model**. The
free-list conservation assertion still holds (`[0, 1]` either way), so the only thing that
should catch this is the oracle. Then a second change, to the unlocked counterfactual only:
loop its model for 400 ms instead of ~4 ms — standing in for a counterfactual model that
simply takes longer, which nothing structural forbids.

```
=== DEFAULT PARALLEL (what CI and README both run), 5 repeats ===
test result: FAILED. 9 passed; 1 failed; ...
test result: ok. 10 passed; 0 failed; ...      <-- oracle MASKED
test result: FAILED. 9 passed; 1 failed; ...
test result: ok. 10 passed; 0 failed; ...      <-- oracle MASKED
test result: ok. 10 passed; 0 failed; ...      <-- oracle MASKED

=== SERIAL (--test-threads=1), identical tree ===
test result: FAILED. 9 passed; 1 failed; ...   <-- oracle fires, deterministically
```

Three of five default-parallel runs report a fully green suite for a model whose oracle
should have failed. `.github/workflows/ci.yml:2470` and `README.md:158` both run the
default-parallel form.

Without the 400 ms widening the masking does **not** reproduce on today's tree — the unlocked
counterfactual sorts fourth alphabetically and finishes long before `push_push_conservation`
(ninth) can win the lock. **That is the point:** the oracle's soundness currently rests on
libtest's alphabetical dispatch order and on how long each model happens to take, i.e. on the
*names of the tests* and their runtime — not on anything enforced. Round 4 rejected exactly
this reasoning for `POP_RETRY_COUNT` ("A delta measured over a wall-clock window is not
exclusive to its holder"), and the same sentence applies unchanged here.

**Two secondary defects in the same block, both in shipped source:**

1. **`MODEL_LOCK`'s stated poisoning rationale is false.** `:83-85`: "`unwrap_or_else(|e|
   e.into_inner())`, not `.unwrap()`: three tests below are `#[should_panic]` and would
   otherwise poison this mutex for every test that acquires it afterward." **None of the three
   `#[should_panic]` tests acquires `MODEL_LOCK`** (rows 2, 5, 8 above), so none of them can
   poison it. The `into_inner` call is still worth keeping — a *failing* locked test poisons
   it for real — but the reason written next to it names three tests that structurally cannot
   be the cause. This is a copy of round 4's suggested fix text applied to a lock whose
   membership then came out narrower than the suggestion assumed, and it is the clearest
   symptom that the membership rule was never re-derived.
2. **`MODEL_LOCK`'s scope sentence is now under-specified** (`:73-74`, quoted above): it names
   only `pop`, and its worked example names only
   `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type`. The file has carried a
   `push` counter and a second oracle since `5362a1d`.

**Fix.** Restate the membership rule as "every test that drives the REAL `push` or `pop`" and
take the lock in rows 4, 5 and 8 (rows 4/5 for future-proofing — they are zero today only by
an unstated harness property; row 8 for the measured 27). The suite runs in 0.15 s, so full
serialization of 9 of 10 models costs nothing. Correct the poisoning rationale to name the
real hazard (a *failing* locked model, e.g. `push_push_conservation` itself). Optionally add
a `debug_assert!`-style structural guard: have the oracle also assert the lock is held, so a
future test that reads a counter without the lock fails loudly instead of silently.

---

## P3 — worth fixing, not blocking

### P3-1. `ArrayLinks<N>`'s dense layout false-shares under contention — measured 1.43–1.45× throughput, undocumented, and it silently confounds the crate's own `contention/churn` bench row

**Location:** `crates/tagged-index-stack/src/lib.rs:546-553` (`ArrayLinks`'s type doc — no
layout note), `:616-629` (the layout note `TaggedIndexStack` *does* carry),
`crates/tagged-index-stack/benches/tagged_index_stack_bench.rs:232-255`
(`contention/churn`'s prefill and its comment).

`ArrayLinks<N>` is `next: [AtomicU32; N]` — 4 bytes per index, so **16 consecutive indices
share one 64-byte cache line**. Every `push` writes a link (`store_next`, `Release`) and every
`pop` reads one (`load_next`, `Acquire`), so under multi-threaded churn the link array is a
second contended surface alongside the head — and unlike the head, it is contended *by
accident of index numbering* rather than by design.

`TaggedIndexStack`'s own type doc has a careful "Layout note — no cache-line isolation"
section about the HEAD false-sharing with an adjacent atomic. `ArrayLinks` — the backing this
crate ships for standalone use, and the one that demonstrably costs more — has no note at all.

**Measured, controlled A/B** (out-of-tree probe, 8 threads, 1 s per arm, 3 reps; identical
workload, identical op accounting, identical `ArrayLinks<1024>` backing — the *only*
difference is the spacing of the 64 prefilled indices):

```
threads=8
rep1: A contiguous(stride 1) = 3520990 ops/s | B spread(stride 16) = 5030679 ops/s | B/A = 1.43x
rep2: A contiguous(stride 1) = 3501135 ops/s | B spread(stride 16) = 5015503 ops/s | B/A = 1.43x
rep3: A contiguous(stride 1) = 3495654 ops/s | B spread(stride 16) = 5078229 ops/s | B/A = 1.45x
```

Arm A is what the shipped bench does (`for i in 0..prefill_count { push(i) }`, indices 0..63 =
256 bytes = 4 cache lines shared by 8 threads). Arm B is the same 64 indices at stride 16
(one index per line). Both arms touch the same single 4 KiB page, so this is a cache-line
effect, not TLB or page behaviour.

**The shipped bench corroborates it independently, and this is what makes it a bench-methodology
finding too.** Same run, same machine:

```
  push_pop/single_thread   18925790 iters   980.326 ms   51.80 ns/op
  pop/empty_fast_path     233827422 iters  1005.095 ms    4.30 ns/op
  churn                    18835603 iters   992.555 ms   52.70 ns/op

contention/push_pop: 5715354 ops/sec total (8 threads, 1.001 sec measured)
contention/churn:    3425014 ops/sec total (8 threads, 1.001 sec measured, prefill=64)
```

The two contention rows differ by **1.67×**, and nothing in the bench, its comments, or the
crate docs explains why. My controlled A/B attributes **1.43×** of that gap to link-cell
spacing alone: `contention/push_pop` seeds `thread_id * LINKS_SIZE / num_threads`
(`bench:177`) = indices 0, 32, 64, … — stride 32, i.e. one index per *two* cache lines, which
is my arm B; `contention/churn` prefills `0..64` contiguously, which is my arm A. The numbers
line up (5.72M ≈ arm B's 5.03M; 3.43M ≈ arm A's 3.50M). So `contention/churn`'s row is not
measuring "throughput under contention with an always-nonempty stack" as its comment says —
it is measuring that *plus* ~30 % of link-array false sharing that the comment does not
mention, and the two contention rows are not comparable to each other.

**Why this matters beyond the bench.** The crate's README sells slot-resident links as the
production shape, and a slot-resident link automatically gets whatever spacing the slot has
(the in-tree consumer's `RegistryLinks` puts `next_free: AtomicU32` inside a much larger slot
struct, so it is already spaced). The crate's *own* standalone backing is therefore the worst
case for the exact workload the crate exists to serve, and a consumer who reaches for
`ArrayLinks<N>` because the README offers it gets the slow layout with no warning.

**Fix — cheap, three parts, none of them an API change.**
1. One paragraph on `ArrayLinks` mirroring `TaggedIndexStack`'s existing layout note: 16
   indices per cache line; if indices are handed to different threads, expect false sharing;
   the fix is at the caller (`#[repr(align(64))]` newtype per link, or slot-resident links),
   not blanket padding inside the crate, which would multiply `ArrayLinks<N>`'s footprint 16×
   for every single-threaded user.
2. One sentence in `contention/churn`'s comment naming the confound and stating that the row
   is a *lower* bound on head-CAS throughput.
3. If a number is published, cite the measured ratio and the machine, per the standard round 4
   already applied to the `churn` figure.

Deliberately **not** proposed: changing `ArrayLinks`'s layout. Padding is the wrong default for
a `no_std` primitive whose whole pitch is "don't pay for a second array".

### P3-2. "One model runs end-to-end through the shipped `push`/`pop`" is false in four files — round 4's own T9 added two more end-to-end models and updated none of them; and the CHANGELOG still carries the over-attribution round 4's P3-3 fixed in the other two files

**Location:** `crates/tagged-index-stack/src/lib.rs:157-160`,
`crates/tagged-index-stack/README.md:149-153`,
`crates/tagged-index-stack/CHANGELOG.md:176-178`,
`crates/tagged-index-stack/tests/loom_aba.rs:7-8`.

There are now **three** models that run end-to-end through the shipped `push`/`pop` with no
hand-inlining at all:

- `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type` (`:555`) — real `pop` ‖ real `push`;
- `push_push_conservation` (`:756`) — two real `push`es;
- `pop_pop_conservation` (`:831`) — two real `pop`s.

The last two were added by `5362a1d`, three commits after `a9a184c` rewrote this very sentence
for round 4's P3-3. All four copies still say "one".

`CHANGELOG.md:176-178` is worse on a second axis: it is the **third copy of the sentence round
4's P3-3 fixed, and it was never touched**:

> "One model runs end-to-end through the shipped `push`/`pop`; **the rest** drive the real head
> atomic and the real packing through `cas_head_for_test` so an interleaving can be pinned."

`src/lib.rs` and `README.md` both now read "**most of** the rest … the one exception is the
untagged-ABA counterfactual"; the CHANGELOG still asserts that
`counterfactual_untagged_head_lets_aba_corrupt_free_list` drives the real head atomic. It
drives a locally-defined `UntaggedStack { head: AtomicU32, next: [AtomicU32; N] }` and touches
neither `TaggedIndexStack` nor `TaggedIndex` nor `cas_head_for_test`. Round 4's P3-3 fix named
"both files"; there were three.

`README.md` is the crates.io landing page and this passage is the crate's headline selling
point, so both errors are visible to a reader who never opens rustdoc.

**Fix.** One number and one clause, in four places — or, better, make `tests/loom_aba.rs`'s
module doc the single source and have the other three say "see `tests/loom_aba.rs`'s module
doc for the per-model breakdown", so the count lives in exactly one file that a test edit is
already touching.

### P3-3. Round 4's P3-5 fix (link storage must be a DEDICATED cell) landed in one of the four places that make the inviting claim — including neither of the two a reader actually lands on first

**Location:** `crates/tagged-index-stack/src/lib.rs:498-527` (the fix, on the `Links` trait),
against `crates/tagged-index-stack/README.md:31-37`,
`crates/tagged-index-stack/src/lib.rs:36-43` (the crate-root `# Links — slot-resident OR
owned` section), and `crates/tagged-index-stack/CHANGELOG.md:84-89`.

Round 4's P3-5 was: the README markets the exact layout rule 4 outlaws (the classic
"the link IS the free block's first bytes" idiom), and `pop`'s new `debug_assert!` turns that
layout into debug-build panics on a benign, by-design race. `ad0028c` wrote an excellent
30-line "Storage requirement: a DEDICATED cell, never payload-aliased" section — on the
`Links` trait doc, and nowhere else. Grepped: `dedicated` / `payload` / `payload-aliased`
appear **zero times** in `README.md` and zero times in `CHANGELOG.md`, and the crate-root
`# Links` section has no pointer to the trait's section.

All three unfixed sites carry the sentence that does the inviting, verbatim or near:

> "so a production allocator keeps its links **slot-resident** (an `AtomicU32` field inside a
> slot it already owns) rather than paying for a second array"

The rule is real and consumer-visible: an implementor who overlays the link on the payload
gets a `debug_assert!` firing on every ordinary benign interleaving in debug builds, and in
release gets `pack`'s silent truncation — to a live index (double-issue) or to the empty
sentinel (whole-chain leak), both spelled out in `push`'s rule 4. The `Links` trait doc is
the right *primary* home for it, but the README is what a crates.io visitor reads and the
crate-root doc is the rustdoc landing page.

Same class as P3-2: a fix applied to a subset of the copies of a duplicated fact.

**Fix.** Two sentences in README §"Slot-resident OR owned links" and one cross-reference from
the crate-root `# Links` section to the trait's "Storage requirement". A one-line CHANGELOG
addition to the `Links` bullet would also cover the second gap in the same file — the
CHANGELOG never mentions `pop`'s rule-4 `debug_assert!` at all, which is a consumer-visible
debug-build panic this release introduces.

### P3-4. `ci.yml` says the clippy row covers "all five `tests/` files"; there are six — the identical hardcoded-count defect round 4 fixed (four→five) three commits before it re-broke it

**Location:** `.github/workflows/ci.yml:1959-1963`.

```yaml
      # all five `tests/` files (`stack_unit.rs`,
      # `regression_counter_wrap.rs`, `proptest_pack_unpack.rs`,
      # `readme_example.rs`, and `loom_aba.rs`, ...
```

`tests/custom_links_impl.rs` was added by `6576b6f` (round 4's own P4-7 fix), which did not
touch `ci.yml`. Round 4's P4-2 fixed this same comment from "four" to "five" two commits
earlier, in `7d07725`. The row itself (`cargo clippy -p tagged-index-stack --all-targets`)
genuinely covers all six — only the comment's count and list are wrong, which is exactly what
made it a P4 last round and exactly what makes it a *pattern* this round.

`CLAUDE.md`'s own no-hardcoded-counts convention (task #776/F10) is cited elsewhere in this
same workflow file (`:1813-1818`). Applying it here means dropping the enumeration entirely
("`--all-targets` covers every `tests/` file plus `benches/tagged_index_stack_bench.rs`"),
not incrementing "five" to "six" and waiting for round 6.

**Related, same file, lower value:** `.github/workflows/ci.yml:1734` still says "12 of
the crate's 16 tests … had never run in CI". That sentence is *historically* true (it
describes the state at task #639) and the past tense is correct, but nothing marks it as
historical, and the crate now has 27 tests (debug) / 26 (release). Worth one word ("at the
time") if that block is touched for the count fix above.

### P3-5. `tests/loom_aba.rs`'s own "Properties asserted" index stops at (e); the file has a `(f)` section

**Location:** `crates/tagged-index-stack/tests/loom_aba.rs:21-50` (the module doc's
enumerated `(a)`…`(e)`) against `:731-736` (the `// (f) push‖push and pop‖pop` section
banner added by `5362a1d`).

The module doc is this file's index — it is the thing that made round 4's own coverage
classification possible ("classified by which real operations race, the eight models are …").
Its enumeration is now one letter short of the file's own section banners, so the two newest
models are the only ones with no entry in the list a reader consults first. Same commit, same
cause as P3-2: models added, index not updated.

**Fix.** Add `(f)`, two sentences, mirroring the section banner already in the file.

### P3-6. `pop_pop_conservation` has no activation oracle although its own doc derives the exact fact an oracle would assert, `POP_RETRY_COUNT` already exists, and the test already holds `MODEL_LOCK`

**Location:** `crates/tagged-index-stack/tests/loom_aba.rs:803-874`.

The test's doc comment traces the mechanism explicitly (`:815-825`):

> "The loser's CAS **fails once**, retries against the now-single-element head, reads that
> element's link (`TAIL`), and its retry CAS succeeds uncontested"

— which is a `POP_RETRY_COUNT` increment, stated in prose and asserted nowhere. Its sibling
`push_push_conservation` asserts precisely this shape for the push counter, with the reason
spelled out (`:749-754`): "a green run with zero retries would only prove two independent
pushes can succeed when they never collide". The identical argument applies verbatim to two
concurrent pops, and `pop_pop_conservation`'s assertions (`both Some`, `[0, 1]`, drained
afterwards) would all still hold on a fully-sequential exploration that never enters the retry
arm.

Loom's exploration is exhaustive here so the retry *will* be visited today; the oracle's job
is to keep that true after the model is edited — which is the same reason round 4 asked for
the push oracle in the first place. Cost: three lines, and the test already takes the lock, so
(after P2-1) the delta would be sound from birth.

### P3-7. ~29 % of the published `CHANGELOG.md` is review-process narrative — internal task IDs, review-round labels, a commit SHA, and paths that are not in the tarball — and round 4 added the single largest instance in the same wave that removed four small ones

**Location:** `crates/tagged-index-stack/CHANGELOG.md:106-129` (24 lines) and `:130-162`
(33 lines), out of 194 total.

Both bullets are `### Added` entries for things that were **not** added:

- `:106-129` — "Two speculative perf changes considered and deliberately NOT landed". Cites
  `docs/reviews/2026-08-30-132847-tagged-index-stack-claude.md`,
  `docs/reviews/2026-08-30-2243-tagged-index-stack-review-round3-oh.md` and "**P3-5**".
- `:130-162` — "A crate-doc 'one Invariants section' consolidation was considered a second
  time and declined a second time". Cites "Round 3 (`ab4497f`)", "Round 4's independent
  review", `.github/workflows/ci.yml`, `scripts/loom.mjs`, and "the `size-classes` sibling
  crate's precedent (**tasks #1638/#1589/#1545**)".

`cargo package --list` confirms none of `docs/reviews/`, `.github/`, or `scripts/` ships in
the tarball, and the task numbers are meaningless outside this repo. The *decisions* are worth
recording; the record's natural home is a commit body or `docs/`, not the file a crates.io
consumer opens to learn what 0.1.0 contains.

The procedural point: `6576b6f` (round 4's P4-3/P4-4 fix) stripped review-ID citations from
`Cargo.toml`, `tests/stack_unit.rs`, and `benches/tagged_index_stack_bench.rs` — and
`7c76c6e`, two commits earlier in the same wave, wrote the 33-line decline bullet into the
CHANGELOG. The crate's largest concentration of review archaeology was created by the round
that cleaned up its smallest ones. This is the exact defect `size-classes` closed at tasks
#1544 ("first-release CHANGELOG cites internal task numbers") and #1607.

**Fix.** Compress both bullets to their durable technical content (one sentence each: "push's
initial `Acquire` load and both `compare_exchange` calls were evaluated for `Relaxed` /
`_weak` and left unchanged — no AArch64 measurement harness exists to justify the change"; and
delete the consolidation bullet outright, keeping the reasoning in `7c76c6e`'s commit body,
where it already is), and drop every review path, round label and task ID. Move both to a
`### Notes` section if they are kept at all — they are not `Added`.

---

## P4 — minor / cosmetic

**P4-1. Both new `#[should_panic]` tests carry no `expected =` string, and one mis-cites the
pattern it claims to copy.** `tests/stack_unit.rs:276-281` and `:316-324`. The file's own
established pattern is `width_16_push_rejects_index_mask_itself` (`:239-270`), which uses
`catch_unwind` + an explicit message assertion *specifically* so that, in its own words, "an
unrelated out-of-bounds panic (e.g. from `ArrayLinks`) cannot satisfy this test". The doc on
`array_links_load_next_panics_on_index_out_of_range` says "Same `#[should_panic]` pattern as
`width_16_push_rejects_index_mask_itself` above" — that test is not `#[should_panic]` at all.
Neither new test is vacuous today — I traced
`pop_debug_assert_fires_on_invalid_next_from_backing` by hand through the shipped `pop` with
the `debug_assert!` deleted: `load_next` returns `0xFFFF`, which is not `TAIL`, so
`pack(0xFFFF, tag)` reads as the empty sentinel, the CAS succeeds, and `pop` returns
`Some(0)` without panicking, so `#[should_panic]` would fail. But a bare `#[should_panic]`
on a test whose body constructs a stack and pushes will accept any panic from any of those
steps. Add `expected = "index out of bounds"` / `expected = "neither TAIL"`,
and fix the cross-reference.

**P4-2. `ArrayLinks::store_next`'s documented `index >= N` panic still has no test.** Round
4's P4-9 named both `load_next` and `store_next` (`src/lib.rs:594-603` documents both); only
`load_next` got one. A `TaggedIndexStack::<16>` over an `ArrayLinks<4>` pushing index 5 —
which is exactly the worked example in `push`'s `# Panics` section — hits it in one line.

**P4-3. `tests/readme_example.rs`'s header is inaccurate twice.** `:1-13`. It says it "Mirrors
README.md's `## Example` section **verbatim**" but adds a third assertion the README does not
have (harmless and arguably better, but then it is a superset, not a mirror); and its
inventory of the crate's one-file-per-concern test suite (`stack_unit.rs`,
`proptest_pack_unpack.rs`, `regression_counter_wrap.rs`, `loom_aba.rs`) omits
`custom_links_impl.rs`, added in the same round. Third instance of the `custom_links_impl.rs`
enumeration drift (with P3-4 and the ci.yml row).

**P4-4. The retry counters' rustdoc still warns about the contamination `MODEL_LOCK` now
exists to prevent, without mentioning `MODEL_LOCK`.** `src/lib.rs:1082-1085` and `:1122-1126`:
"The count is process-global and cumulative — … and (under the default multi-threaded test
harness) across concurrently running test functions." Accurate about the *counter*, but it is
the only contract a reader of `push_retry_count_for_test` gets, and it now understates what
the suite guarantees (and, per P2-1, overstates it in one direction too: for
`PUSH_RETRY_COUNT` the warning is currently the accurate description). Once P2-1 is fixed,
one clause naming `MODEL_LOCK` as the suite's own mitigation closes it.

**P4-5. `src/lib.rs` is now 74.5 % comment (843 / 1132 lines), up from round 4's 73.6 % (760 /
1033).** Recorded as a series, not re-litigated: round 4's second decline of the consolidation
(`7c76c6e`) is a substantive argument — the drift is cross-*file*, and a single in-crate
Invariants section would not have prevented any of P3-2/P3-3/P3-4/P3-5 — and this round's
findings support it, since all four of those instances are again duplicated facts in
`README.md` / `CHANGELOG.md` / `ci.yml` / `tests/`, not verbosity inside one rustdoc passage.
What *would* have prevented three of them is the single-source-of-truth suggestion in P3-2's
fix. Noting the number so a future round has the trend, and noting that the argument in
`CHANGELOG.md:130-162` should not survive as a CHANGELOG entry (P3-7) even though it is
correct.

**P4-6. `CHANGELOG.md:7` still reads "0.1.0 - Unreleased".** Known release-commit checklist
item; no action expected before the publish commit itself. (Fifth consecutive round.)

---

## Considered adversarially and **rejected** — recorded so they are not re-filed

1. **`push` re-writes the link on every retry iteration; skip the `store_next` when the value
   is unchanged.** Round 4 rejected this on inspection; I re-examined it and the rejection is
   *stronger* than round 4 stated. The retry path is precisely the case where the value
   changed: `push` retries because the head moved, and the head moving is what changes
   `next_link`. The only schedule where the retry's `next_link` matches the previous
   iteration's is one where another thread pushed *and popped back to the same index* inside
   the window — rare, and the saving would be one already-owned-line store against an added
   load-compare-branch. Not worth it, and not worth measuring.
2. **`push`'s initial `head.load(Acquire)` → `Relaxed`, and `compare_exchange_weak` in both
   loops.** Already recorded as explicitly declined in `CHANGELOG.md:106-129`, with the
   correct trigger (an AArch64 measurement harness). Still correct, still no such harness in
   this repo. Not re-filed — but see P3-7 about where that record belongs.
3. **`#[inline]` / `#[track_caller]` on `push`/`pop`.** Round 4 A/B-measured both out-of-tree
   and found them inside noise on x86-64. Not re-measured; nothing changed in those paths
   since.
4. **The duplicated `head & INDEX_MASK` in `push` (`is_empty(head)` recomputes what `unpack`
   already produced) and in `pop`.** Textbook CSE; both are the identical subexpression on the
   same local, in the same basic block. Rejected on inspection, no measurement warranted.
5. **`pop`'s CAS success ordering could be `Relaxed`.** Round 4's analysis stands: LLVM lowers
   a `cmpxchg` from the stronger of the two orderings, so there is no codegen to win on
   x86-64 or LSE AArch64, and it is a load-bearing ordering not worth churning.
6. **Padding `ArrayLinks`'s cells to a cache line.** This would fix P3-1's *number* and is the
   wrong default: it multiplies the backing's footprint 16× for every single-threaded user of
   a crate whose stated pitch is not paying for a second array. Documenting the effect and
   leaving the choice to the caller is the right resolution — see P3-1's fix.
7. **`counterfactual_untagged_head_lets_aba_corrupt_free_list` not taking `MODEL_LOCK`.**
   Checked and correct as-is: it drives only its local `UntaggedStack`, and the probe measured
   `pop_delta=0 push_delta=0` for it. It should stay unlocked.
8. **Tarball hygiene.** `cargo package --list` → 16 files, no strays; `tests/` and `benches/`
   ship as intended; clippy and `RUSTDOCFLAGS="-D warnings" cargo doc` are both clean. The
   `x86_64-unknown-none` bare-metal row, the `--release` test row, and the crate-scoped
   clippy/doc rows all exist in `ci.yml`. Clean.
9. **`scripts/loom.mjs` (round 4's P2-2).** Re-verified: `loom_aba` now resolves through the
   `crate:` branch with `['-p', crateName, '--features', 'loom']` (`:134-137`), and the
   `alloc-global,alloc-xthread,tagged-index-stack/loom` feature string matches `ci.yml:2636`
   exactly, at all four entries that need it (`:50`, `:66`, `:68`, `:70`). Fixed properly.
10. **`docs/correctness-open-items/TRACKED_ci_gate_coverage.md` item 25 (round 4's P3-8).**
    Re-verified: now `**CLOSED** (2026-08-31, …)` with the narrative moved to `RESOLVED.md`,
    per `CLAUDE.md`'s current-state-index rule. Fixed properly.

---

## What is genuinely good

- **The algorithm survived a fifth independent attack.** I went at tag monotonicity across the
  drain transition, the release-sequence status of every head write, `INDEX_MASK` vs `TAIL` at
  both extreme widths, the `1u64 << 63` shift boundary at width 1, `_CHECK_BITS`'s routing
  from every public associated item, and the stale-snapshot window between `load_next` and the
  CAS. Nothing broke.
- **Round 4's `#[cfg(debug_assertions)]` gate (`d944b89`) is the right fix, correctly
  reasoned.** Its comment names the exact CI row that broke, the exact profile default that
  causes it, and the exact failure mode ("test did not panic as expected") — and it gates the
  helper struct alongside the test so nothing is flagged dead in release. I ran both profiles;
  it behaves exactly as documented.
- **`pop_pop_conservation`'s doc comment is the best test documentation in the crate.** It
  *derives* its assertion from `pop`'s actual loop rather than asserting a plausible-looking
  invariant, states why this case is strictly stronger than the file's other conservation
  oracles (no third actor), and says what a failure would mean ("the traced derivation was
  wrong or the shipped `pop` regressed"). I re-derived it independently and it is correct.
- **The `Links` trait's new "Storage requirement" section (`ad0028c`) is exactly the right
  content** — it names the benign race, explains why the read is safe with dedicated storage
  and not merely "meaningful", and connects it to the `debug_assert!`. Its only problem is
  that it lives in one file (P3-3).
- **`churn`'s prefill fix (`31a12a0`) is real.** The row now genuinely exercises push's
  `cur_idx` branch and pop's `next != TAIL` branch, and the cited `51.56 ns/pair` reproduces
  at `52.70 ns/pair` on the same machine — a 2.2 % spread, and the figure now names machine,
  toolchain and date, which is what round 4's P3-2 asked for.
- **The bench's contention discipline (re-push exactly what you popped; drain-and-assert-empty
  before the phase-2 prefill) has now held through four rounds of edits** without regressing
  the double-push bug round 1 found.

---

## Suggested order of work

1. **P2-1** — widen `MODEL_LOCK`'s membership to "drives the real `push` or `pop`" (rows 4, 5,
   8), correct its poisoning rationale and scope sentence. Re-run the neutered-model
   counterfactual from this report to confirm the oracle fires under the default harness.
2. **P3-6** — add the `POP_RETRY_COUNT` oracle to `pop_pop_conservation`, after P2-1 so it is
   sound from birth. Three lines, same shape as `push_push_conservation`'s.
3. **P3-2 / P3-4 / P3-5** — the three count/enumeration drifts, as one bundle. Prefer removing
   the counts (single-source-of-truth in `tests/loom_aba.rs`'s module doc; "every `tests/`
   file" in `ci.yml`) over incrementing them.
4. **P3-3** — two sentences in `README.md` and one cross-reference in the crate-root doc, plus
   the CHANGELOG line for `pop`'s rule-4 `debug_assert!`.
5. **P3-1** — the `ArrayLinks` layout note and the `contention/churn` confound sentence. This
   is the round's one measured perf finding; the raw A/B is in this report and reproduces in
   about 6 seconds.
6. **P3-7** — compress the two review-narrative CHANGELOG bullets to their technical content
   and strip the task IDs / review paths / round labels.
7. **P4-1 / P4-2 / P4-3 / P4-4** as one bundle; **P4-5 / P4-6** are records, not actions.
