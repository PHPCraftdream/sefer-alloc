# `tagged-index-stack` — independent publish-readiness review, round 6

- **Reviewer:** Claude (Opus 5), adversarial pass. Source and docs read first and every finding
  formed independently; the round-1..5 review docs were deliberately NOT read before forming
  findings, and were consulted afterwards only to (a) check overlap, (b) check whether a prior
  round's *remediation* left something half-done, and (c) avoid re-raising an already-declined
  item without new evidence.
- **Date:** 2026-08-31 07:29:11 +0200 (CEST)
- **Revision reviewed:** `b89796a10b52b80ea541a2b411b3916ee1a83763` (landing SHA on `main`;
  `git status --porcelain -- crates/tagged-index-stack` is empty — the crate tree is clean)
- **Scope:** `crates/tagged-index-stack/**` (`src/lib.rs`, all six `tests/*.rs`,
  `benches/tagged_index_stack_bench.rs`, `README.md`, `CHANGELOG.md`, `Cargo.toml`), the
  `.github/workflows/ci.yml` rows for this crate, `scripts/loom.mjs`, and the in-workspace
  consumers (`src/registry/heap_registry.rs`, `src/registry/bootstrap.rs`) where a shipped doc
  comment names them.
- **Machine / toolchain for every measured number:** `rustc 1.97.0 (2d8144b78 2026-07-07)`,
  `cargo 1.97.0 (c980f4866 2026-06-30)`, host `x86_64-pc-windows-msvc`, 11th Gen Intel Core
  i7-11800H (8 cores / 16 threads, 2.30 GHz base), Windows 10 Pro 19045.
- **No file in the repository was modified.** This is a read-only review. All A/B experiments
  ran in throwaway trees OUTSIDE the repository (`D:/dev/rust/.scratch-tis-r6`,
  `D:/dev/rust/.scratch-tis-consumer`), deleted afterwards; `bench-iters.txt` was md5-verified
  byte-identical before and after the bench run.

## Verification actually performed

Every number below came from running something, not from reading.

| Check | Result |
| --- | --- |
| `cargo test -p tagged-index-stack --no-fail-fast` | **28 green** (18 `stack_unit`, 5 `proptest_pack_unpack`, 2 `regression_counter_wrap`, 2 `custom_links_impl`, 1 `readme_example`; `loom_aba` correctly 0 tests; 0 doctests) |
| `cargo test -p tagged-index-stack --release --no-fail-fast` | **27 green** (the `#[cfg(debug_assertions)]`-gated `pop_debug_assert_fires_on_invalid_next_from_backing` correctly disappears) |
| `RUSTFLAGS="--cfg loom" cargo test --release -p tagged-index-stack --features loom --test loom_aba` | **10 green in 0.16 s**, all three `#[should_panic]` counterfactuals included |
| each of the three activation-oracle tests run ALONE (`-- --exact <name>`) | all 3 green — see §MODEL_LOCK below; this is the direct check that the oracles are not passing on cross-test noise |
| `cargo clippy -p tagged-index-stack --all-targets -- -D warnings` | clean |
| `RUSTDOCFLAGS="-D warnings" cargo doc -p tagged-index-stack --no-deps` | clean |
| `cargo package -p tagged-index-stack --no-verify` | **16 files, 204.5 KiB (61.6 KiB compressed)**, no strays; generated manifest resolves `loom = "0.7"`, `optional = true` correctly |
| downstream lockfile claim (`Cargo.toml`'s "31 locked packages down to 2") | **CONFIRMED** — a fresh scratch consumer with `tagged-index-stack` as its only dependency locks exactly **2** packages |
| `cargo bench -p tagged-index-stack --bench tagged_index_stack_bench` | ran; numbers in §Perf; `bench-iters.txt` md5 `a20401df…` identical before and after |
| out-of-tree A/B: CAS-retry backoff | **P2-1** — 7.9×–9.2× contended throughput, 0 % single-threaded cost; raw numbers in P2-1 |
| out-of-tree probe: `#[track_caller]` effectiveness from a downstream caller | works — panic location resolves to the CALLER's line, not `lib.rs`; nothing in-tree pins it (**P3-6**) |

---

## Overall verdict: **CONDITIONAL-GO**

**The shipping algorithm is correct, and I re-derived the load-bearing claims rather than
inheriting a prior round's verdict:**

- **Tag monotonicity along the head's modification order holds.** Every successful `push` does
  `tag.wrapping_add(1)`; every successful `pop` preserves the observed tag, including across the
  drain-to-empty transition (`src/lib.rs:980-985`). So a `(X, t)` head word can only recur after
  a full `2^TAG_BITS` wrap whose last push re-installs `X`.
- **`pop`'s `Acquire`-success-without-`Release` ordering is sound**, and the premise it rests on
  still holds. The argument is the release-sequence rule (C++20 / Rust: a release sequence
  headed by a release store continues through every subsequent RMW to the same location,
  whatever those RMWs' own orderings). I re-grepped `head` for a plain `store` and found none:
  `new` is initialization, `raw_head`/`is_empty` only load, and all three writers (push's
  `Release` CAS, pop's `Acquire` CAS, the loom-only `cas_head_for_test`) are RMWs. The
  `INVARIANT` block on the private field (`src/lib.rs:656-679`) states exactly this and warns
  against adding a plain store — that comment is correct and load-bearing.
- **`push`'s `Relaxed` CAS-failure ordering is sound.** `push` consumes the failure read's index
  half as a *value* to store into a link and its tag half as a *number* to bump; it never
  dereferences a link through it. `pop` is genuinely asymmetric here (its retry's re-read names
  the index whose link `load_next` consults next) and correctly keeps `Acquire`.
- **`_CHECK_BITS` really is unbypassable.** `unpack`, `empty_index`, `is_empty` and `empty` all
  route through `INDEX_MASK` (whose initializer evaluates it), `pack` and `try_pack` force it
  with an explicit `let () = Self::_CHECK_BITS;`, and `TAG_BITS` evaluates it too. The doc's
  claim at `src/lib.rs:283-292` checks out item by item.
- **`try_pack`'s shift boundary is safe at every legal width.** `TAG_BITS ∈ [48, 63]`, so
  `1u64 << Self::TAG_BITS` never reaches the `<< 64` UB boundary; width 1 (`TAG_BITS == 63`) is
  pinned explicitly by `try_pack_width_1_tag_boundary_at_shift_63`.
- **`pop_pop_conservation`'s derivation is correct.** With exactly 2 seeded indices, exactly 2
  poppers and no third actor, the head cannot become empty until after the second successful
  CAS, and that CAS belongs to one of the two poppers — so "both return `Some`, partitioning
  `{0,1}`" is the only reachable shape, not an over-strong assertion.
- **The `debug_assert!` in `pop` (`src/lib.rs:968-979`) is precise about what it catches.** It
  fires on exactly the `next` values that `pack` would silently truncate (`>= INDEX_MASK`), and
  its two-way failure message correctly distinguishes truncate-to-empty-sentinel from
  truncate-to-a-live-index. It does NOT catch an in-range-but-stale `next` — and it does not
  claim to.

**What holds this back from an unconditional GO is one P2: a measured 7.9×–9.2× contended
throughput headroom that the crate leaves on the table**, in the exact workload its own README
names as the crate's value ("this is a lock-free Treiber stack whose value is concurrent
throughput"). It is not a correctness defect and not semver-relevant — it could ship in 0.1.1 —
but publishing a 0.1.0 whose own contention bench row is ~9× below what a three-line change
reaches, without either landing it or recording the measured decline, is exactly the kind of
thing this review series exists to catch.

**Findings: 0 × P0, 0 × P1, 1 × P2, 7 × P3, 8 × P4.**

---

## MODEL_LOCK re-verification (the round-4 / round-5 recurring defect class)

This was audited as its own task, function by function, because the same defect class was
introduced twice in a row by the SAME file's remediation (round 4 added `MODEL_LOCK` but scoped
its membership to `pop`'s oracle only; round 5 then found `push_push_conservation`'s new
`PUSH_RETRY_COUNT` oracle outside that membership).

**Result: membership is COMPLETE this round. No new hole of that class exists.** Every one of
the file's ten `#[test]` functions was checked individually:

| # | Test (`tests/loom_aba.rs`) | Drives real `push`/`pop`/`cas_head_for_test`? | `MODEL_LOCK`? |
| --- | --- | --- | --- |
| 1 | `aba_repush_keeps_free_list_conservation` (`:129`) | yes — B does real `pop`+`push`; A drives `cas_head_for_test`; final drain does real `pop` | **held** (`:130`) |
| 2 | `counterfactual_untagged_head_lets_aba_corrupt_free_list` (`:243`) | **no** — drives only the file-local `UntaggedStack` (`:195-239`), a bare `loom::AtomicU32` head with no tag; touches no crate type at all except the `TAIL` constant | **not held — correctly exempt** |
| 3 | `tagged_stack_survives_the_same_resurrection_pattern` (`:333`) | yes — B does two real `pop`s + one real `push`; A drives `cas_head_for_test` | **held** (`:334`) |
| 4 | `pop_empty_transition_preserves_tag` (`:533`) | yes — `run_h2(true)`: B does real `pop` + real `push` | **held** (`:534`) |
| 5 | `counterfactual_empty_transition_tag_reset_lets_aba_recur` (`:542`) | yes — `run_h2(false)`: B does real `push`; `bug_pop_drain_to_empty` drives `cas_head_for_test` | **held** (`:543`) |
| 6 | `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type` (`:575`) | yes — end-to-end real `pop` ‖ real `push`; **reads `POP_RETRY_COUNT`** | **held** (`:587`), acquired BEFORE the `retries_before` snapshot (`:588`) |
| 7 | `cas_retry_path_must_acquire_with_concurrent_push` (`:737`) | yes — `run_cas_retry(Acquire)`: B does real `push`, final drain real `pop` | **held** (`:738`) |
| 8 | `counterfactual_relaxed_cas_failure_corrupts_free_list` (`:747`) | yes — `run_cas_retry(Relaxed)`, same shape | **held** (`:748`) |
| 9 | `push_push_conservation` (`:777`) | yes — two real `push`es; **reads `PUSH_RETRY_COUNT`** | **held** (`:778`), before the snapshot (`:779`) |
| 10 | `pop_pop_conservation` (`:860`) | yes — two real `pop`s over a real-`push` seed; **reads `POP_RETRY_COUNT`** | **held** (`:861`), before the snapshot (`:862`) |

Row 2's exemption is genuinely sound, not a loophole: `UntaggedStack::pop`/`push` (`:209-238`)
operate on the struct's own `head: AtomicU32` and `next: [AtomicU32; N]` and never enter
`src/lib.rs`, so neither `POP_RETRY_COUNT` nor `PUSH_RETRY_COUNT` can be incremented by it. It
is also safe for it to run concurrently with a lock-holding test: loom's execution state is
thread-local, so two `Builder::check` explorations on two libtest threads do not interfere.

Three further checks I ran rather than assumed:

1. **Lock-before-snapshot ordering** in all three oracle tests — verified; a snapshot taken
   before the lock would reopen the hole in a different way, and none does that.
2. **Poisoning** — every acquisition uses `.unwrap_or_else(|e| e.into_inner())`, which matters
   because three of the nine lock-holders are `#[should_panic]` and therefore unwind while
   holding the guard. Verified: the suite is green, so no poisoned-mutex cascade.
3. **The oracles are not passing on cross-test noise** — I ran each of the three
   counter-reading tests ALONE (`-- --exact <name>`, 9 filtered out). All three still pass:
   `push_push_conservation` ok, `pop_pop_conservation` ok,
   `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type` ok. Each model therefore
   genuinely drives its own retry branch. This is the direct positive control that the
   `MODEL_LOCK` mechanism is doing what it claims.

What I did NOT find clean is the *prevention*: membership is still enforced only by remembering
to type one line in each new test (**P3-2**), and the file's own prose describing the hazard is
now stale in two places (**P3-3**, **P3-4**). Those are the residue of the class, not a new
instance of it.

---

## Findings

### P2-1 — Neither CAS-retry arm backs off; measured 7.9×–9.2× contended-throughput headroom at zero single-threaded cost

`src/lib.rs:881-894` (push's `Err(actual)` arm) and `src/lib.rs:997-1010` (pop's `Err(actual)`
arm) both do the same thing on a lost CAS: record `head = actual` and immediately re-issue the
`lock cmpxchg`. There is no `core::hint::spin_loop()`, no backoff of any kind, in a primitive
whose bench file's own header says "this is a lock-free Treiber stack whose value is concurrent
throughput".

**Failure scenario (throughput, not correctness):** 4–8 threads churning the same head word
collapse into a coherence-thrash regime where aggregate throughput is *worse than a single
thread's*. The crate's own committed `contention/churn` row shows it: 8 threads reach 5.02 M
ops/sec (= 2.51 M pop+push pairs/sec) while the single-threaded `churn` row does 58.24 ns/pair
= 17.2 M pairs/sec. Eight cores deliver ~15 % of one core.

**Measurement.** Out-of-tree A/B, in `D:/dev/rust/.scratch-tis-r6` (a byte copy of
`src/lib.rs`, patched only to add the retry hint behind a Cargo feature; scratch workspace root
carries `[profile.release] lto = "thin", codegen-units = 1`, matching this repo's own
`[profile.release]`/`[profile.bench]`). The probe replicates the committed bench's
`contention/churn` shape (64 prefilled indices, barrier-released, 1 s window,
`DEADLINE_CHECK_INTERVAL = 256`) and additionally runs a **free-list conservation oracle** after
each timed window — it drains and asserts the exact multiset `0..64` — so a corrupted, cyclic
chain cannot inflate the op count. The oracle passed in all 18 timed windows.

Three arms: **BASE** (as shipped), **`spin_loop()` × 1** on each retry, and **exponential**
(`for _ in 0..(1 << spins.min(6)) { spin_loop() }`, `spins` a per-call local so it resets on
every `push`/`pop` entry).

| workload | BASE | `spin_loop()` × 1 | exponential (cap 64) |
| --- | --- | --- | --- |
| single-thread `churn` (ns/pair, 3 runs) | 51.04 / 52.06 / 51.03 | 54.02 / 53.13 / 53.71 | 52.64 / 50.92 / 51.00 |
| `contention/churn`, 2 threads (ops/s) | 16.20 / 14.72 / 12.52 M | 14.96 / 17.46 / 17.06 M | **37.63 / 37.46 / 37.42 M** |
| `contention/churn`, 4 threads (ops/s) | 4.16 / 4.17 / 4.22 M | 4.24 / 3.94 / 3.90 M | **32.71 / 33.23 / 32.76 M** |
| `contention/churn`, 8 threads (ops/s) | 2.93 / 3.07 / 3.26 M | 2.61 / 2.45 / 2.69 M | **28.54 / 28.40 / 28.22 M** |

(The single-thread row and the `spin_loop() × 1` column are from a first pass without LTO; the
LTO pass reproduced both, and the single-thread figures are identical to within noise in both.)

Medians: exponential backoff is **2.54× at 2 threads, 7.85× at 4, 9.24× at 8**, and
**0 % cost single-threaded** (51.00 vs 51.03 ns/pair) — expected, since the added code lives
exclusively in a branch an uncontended run never takes. A single un-scaled `spin_loop()` is
*not* enough: it helps slightly at 2 threads and does nothing at 4 or 8.

**Why this is not one of the two perf items `CHANGELOG.md:110-117` already declined.** That
bullet declines (a) `push`'s initial `Acquire` load → `Relaxed` and (b)
`compare_exchange_weak`, both on the honest grounds that neither shows up on x86-64 or LSE
AArch64 and this repo has no AArch64 gate. I independently agree with both declines. This one is
different in kind: it is **measured on x86-64, on this repo's own machine, with the committed
bench's own workload shape**, and the effect size is ~9×, not "within noise". `core::hint::spin_loop`
is in `core`, so it costs no dependency and does not touch `no_std`.

**What it needs before it can land** (this is a design decision, not a mechanical patch):
backoff trades worst-case per-operation latency and inter-thread fairness for aggregate
throughput, and the cap/growth constants are a tuning surface that should be pinned by the
committed bench, not by my scratch tree. The minimum acceptable resolution is: reproduce the A/B
with the committed `benches/tagged_index_stack_bench.rs` harness, then either land it or add a
CHANGELOG bullet declining it *with these numbers cited* — the current bullet reads as "we
looked at perf and there is nothing on x86", which is now falsified.

---

### P3-1 — `TAIL`'s published rustdoc contradicts itself in the same sentence and names the wrong end of the chain

`src/lib.rs:231-232`:

```rust
/// The "no next" sentinel stored in a slot's link to denote the BOTTOM of the
/// stack (the last-pushed index chains to this). `u32::MAX`.
```

"BOTTOM" is right; "the last-pushed index chains to this" is wrong, and the two halves of the
parenthetical contradict each other. `push` writes `TAIL` **only** when `is_empty(head)`
(`src/lib.rs:845-849`), i.e. into the link of the FIRST index pushed onto an empty (or
freshly-drained) stack — the bottom-most one. The LAST-pushed index is the head, and it chains
to the *previous* head, never to `TAIL`.

**Failure scenario:** a `Links` implementor reading this doc to decide what their backing must
be able to represent concludes the chain terminator sits at the top of the stack. The file's own
loom helper contradicts it three hundred lines later — `both_free()` (`tests/loom_aba.rs:111-122`)
documents the state it builds as "slot 0 on top, chained to slot 1, chained to TAIL", produced by
"pushing 1 then 0": index 1 is pushed FIRST and is the one holding `TAIL`.

Fix: "…the BOTTOM of the stack (the first index pushed onto an empty stack chains to this)".

---

### P3-2 — `MODEL_LOCK` membership is still enforced only by per-test discipline; the class that recurred twice has no structural guard

`tests/loom_aba.rs:104` declares the mutex; nine separate tests each independently remember to
type `let _g = MODEL_LOCK.lock().unwrap_or_else(|e| e.into_inner());` as their first line. That
is exactly the shape that failed twice: round 4 introduced the lock with pop-only membership,
round 5's own remediation added an oracle outside it, and both were caught only because a
reviewer read all ten functions.

Membership is correct TODAY (see §MODEL_LOCK above). The finding is that nothing makes the
eleventh test correct by construction.

**Failure scenario:** a future round adds an eleventh model — say a `push‖pop` conservation
model with a `PUSH_RETRY_COUNT` oracle — forgets the one line, and the suite stays green,
because a delta measured against a globally-shared counter is *satisfiable by another test's
increments*. That is a silently vacuous oracle, which is precisely what the counter exists to
prevent.

Concrete structural fix (~10 lines, no behavior change):

```rust
/// Every model in this file runs through here: the guard is acquired by
/// construction, so a new test cannot forget it.
fn model<F>(f: F) where F: Fn() + Sync + Send + 'static {
    let _g = MODEL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    loom::model::Builder::new().check(f);
}
```

Every current test becomes `model(|| { … })`; row 2 (the untagged counterfactual) can either use
it harmlessly — it costs one extra serialization — or keep its own explicit
`// exempt: drives no crate code` line, which is at least a *visible* opt-out rather than an
absence. Either way the default flips from opt-in to opt-out.

---

### P3-3 — the enumeration inside `pop_retry_after_failed_cas_…`'s own comment names 2 of the 8 tests that can contaminate its counter, and omits the two most obvious ones

`tests/loom_aba.rs:581-586`:

> Without it, any other test in this binary that also drives the real `pop` under contention
> (`aba_repush_keeps_free_list_conservation`,
> `tagged_stack_survives_the_same_resurrection_pattern`) could run concurrently…

Eight other tests in this file drive real `push`/`pop` (rows 1, 3, 4, 5, 7, 8, 9, 10 of the
§MODEL_LOCK table). The two named are rows 1 and 3. Missing are `pop_pop_conservation` — which
drives two concurrent real `pop`s and is the single largest `POP_RETRY_COUNT` contributor in the
file — and `push_push_conservation`, `pop_empty_transition_preserves_tag`,
`counterfactual_empty_transition_tag_reset_lets_aba_recur`,
`cas_retry_path_must_acquire_with_concurrent_push`, and
`counterfactual_relaxed_cas_failure_corrupts_free_list`.

**Failure scenario:** a maintainer widening or narrowing `MODEL_LOCK`'s membership uses this
list as the inventory of what the lock is protecting against, concludes rows 4/5/7/8/9/10 are
outside the hazard, and re-opens the round-4/round-5 hole with a comment that appears to justify
it. This is the same *stale-enumeration* class round 5 already fixed three instances of
(its P3-2/P3-4/P3-5) — the fix there was "prefer removing the counts over incrementing them",
and the same applies here: replace the two names with "any other test in this file that drives
the real `push`/`pop` — see the `MODEL_LOCK` doc" rather than maintaining a second inventory.

---

### P3-4 — the loom module doc's property (f) omits `pop_pop_conservation`'s activation oracle, added in the previous round

`tests/loom_aba.rs:59-65` describes property (f) and ends:

> `push_push_conservation` also asserts a `PUSH_RETRY_COUNT` activation oracle mirroring `pop`'s.

`pop_pop_conservation` asserts a `POP_RETRY_COUNT` oracle too (`tests/loom_aba.rs:905-912`), and
its own doc comment (`:852-858`) says so. The module doc — which its own header (`:14-16`)
declares to be "the source of truth for this per-model breakdown", with the crate rustdoc,
README and CHANGELOG all pointing back at it — was not updated when round 5's task added it.

**Failure scenario:** the file's designated source of truth undercounts its own non-vacuousness
guarantees, so a reader auditing "which models are protected from being vacuously green"
believes `pop_pop_conservation` is not, and either re-adds a duplicate oracle or discounts the
model's value.

---

### P3-5 — `src/lib.rs` is 75.1 % comment lines, and part of that is extraction archaeology pointing at files absent from the published tarball

Measured: 1162 lines total, 37 blank, **873 comment, 252 code — 75.1 % comment, 3.46 comment
lines per line of code.** For calibration against this project's own precedent, the sibling
`size-classes` crate had this raised at 62 % (round 1, P3-9) and again at 65.3 % (round 7,
P2-2, with an explicit "target ≤ 55 %").

Some of the volume is genuinely load-bearing and should stay (the tag-width derivation, `push`'s
`# Caller contract`, the `head` field's release-sequence `INVARIANT`). What is not:

- **`TaggedIndex::empty()`'s `#[doc(hidden)]` rationale, `src/lib.rs:406-428` — 23 lines** for a
  one-line `const fn`, of which `:414-423` explain that the second caller is
  "the `sefer-alloc` root crate's `#[cfg(loom)] mod loom_shim` (in `src/registry/bootstrap.rs`)".
  That path does not exist in the 16-file published tarball. A crates.io reader cannot resolve
  it; the *fact* that matters to them ("not freely removable in 0.2, it has an in-workspace
  consumer") survives in one clause.
- **`TaggedIndex`'s type doc, `src/lib.rs:250-259` — 10 lines** arguing uninhabited-enum vs.
  unit-struct. The decision is right; the argument is a design-review transcript.
- **`push`'s `# Panics`, `src/lib.rs:816-825`**, re-derives the `ArrayLinks<N>`-vs-`INDEX_BITS`
  independence that `ArrayLinks::load_next`/`store_next` (`:608-627`) already state twice.

**Failure scenario:** this is the drift surface every prior round has been paying for. Four of
this round's seven P3s and several P4s are stale prose, and the reason stale prose keeps
appearing is that the same fact is written down in four to six places (`src/lib.rs` crate doc,
the item doc, `README.md`, `CHANGELOG.md`, and a test's doc comment) with no mechanical link
between the copies.

---

### P3-6 — `#[track_caller]`'s effect is unpinned by any test, so removing either attribute is a silent regression

`push` carries `#[track_caller]` (`src/lib.rs:826`) and forwards to
`push_index_out_of_range`, also `#[track_caller]` (`src/lib.rs:909`), whose own doc says the
purpose is that "a consumer pushing from many call sites learns WHICH one violated the
contract". Both attributes were added by a prior round's remediation.

I verified out-of-tree that it works: a downstream crate calling `stack.push(&links, 0xFFFF)`
gets a panic whose `Location` is `tests\tc.rs:18` — its own call site, not `lib.rs`.

The in-tree test that covers this guard, `width_16_push_rejects_index_mask_itself`
(`tests/stack_unit.rs:239-270`), asserts only that the panic *message* contains
`"index must be < INDEX_MASK"`. Deleting `#[track_caller]` from either function leaves that test
green.

**Failure scenario:** a future refactor drops `#[track_caller]` from `push` (it is easy to read
as redundant next to the one on the `#[cold]` helper — it is not: without it the helper reports
`lib.rs`'s own line). Every consumer's panic then points into this crate's source, the
documented behaviour silently regresses, and CI is green.

Fix: extend the existing test with a `panic::set_hook` capture asserting
`info.location().file()` ends in the test file's name — about 10 lines, and the pattern is
already proven by the scratch probe above.

---

### P3-7 — `CHANGELOG.md` lists a *non-change* under `### Added`, and that bullet is now falsified by P2-1

`CHANGELOG.md:110-117`, inside the `### Added` section of a first release:

> **Two speculative perf changes evaluated and not landed:** …

Two problems. (a) Structurally it is not an addition; a first-release `Added` list is the
inventory of what a consumer gets, and "we considered X and didn't do it" is decision-log
content. (b) Substantively it now reads as "perf was evaluated; nothing measurable exists on
x86-64", which P2-1 falsifies with a 9× x86-64 result the bullet does not mention.

**Failure scenario:** a maintainer (or a later reviewer) reads this bullet as the closed record
of the crate's perf question and does not re-open it. That is exactly what it is written to do,
and it is now incomplete.

---

### P4-1 — `cas_head_for_test` is the only loom-gated `pub` item without `#[doc(hidden)]`

`src/lib.rs:1058-1059`. Its siblings `raw_head` (`:1043`), `TaggedIndex::empty` (`:429`),
`pop_retry_count_for_test` (`:1114`) and `push_retry_count_for_test` (`:1158`) all carry the
attribute plus this project's standard rationale paragraph; `cas_head_for_test` carries neither,
only prose saying "NOT part of the public API: it is compiled only under `--cfg loom`". Harmless
on docs.rs (which builds without the cfg), but it is the one inconsistency in an otherwise
uniform convention. Related: `README.md:170-172` says `raw_head` and `empty()` are the crate's
two `#[doc(hidden)]` `pub` items — true for a published build, not for a `--cfg loom` build,
where there are four.

### P4-2 — the first-release `Added` inventory omits three public items

`CHANGELOG.md` never lists `pub const TAIL` (it is mentioned only in passing at `:39`, as a
comparison target for `INDEX_MASK`), nor the `Default` impls for `TaggedIndexStack` and
`ArrayLinks`, nor the `Debug` derives on both. `TAIL` is part of the `Links` contract — an
implementor must know the sentinel — and both `Default` impls are pinned by dedicated tests
(`default_stack_behaves_like_new`, `default_array_links_behaves_like_new`,
`tests/stack_unit.rs:466-507`), so they are deliberate API, not incidental.

### P4-3 — `README.md:58`'s "~16 MiB bootstrap first-touch" is falsifiable at face value from the crate's own numbers

> (In the allocator this crate was extracted from, this saved a ~16 MiB bootstrap first-touch.)

A reader computes the largest possible `ArrayLinks` at the maximum legal width — 65535 × 4 B =
256 KiB — and concludes the figure is off by 64×. It is not: the number only makes sense for
*slot-resident* links, where eagerly chaining the free list first-touches a page of each SLOT,
and the slots are what total ~16 MiB. The sentence never says that, and it sits in the RAD-1
bullet, two sentences after the one about "OS-zeroed memory (a fresh `mmap`, a zeroed slot
array)". One clause ("…because the links were slot-resident, so chaining them would have
first-touched every slot's page") removes the apparent contradiction.

### P4-4 — the contention half of the bench is untracked and unconditional

`benches/tagged_index_stack_bench.rs:99` ends the `bench-scale-tool` harness; everything from
`:101` on is hand-rolled and `println!`-only. Consequences: (a) the two contention rows are not
in `bench-iters.txt`, so nothing regression-tracks the crate's headline metric — the very metric
P2-1 is about; (b) `run_harness` returns early on `--calibrate` (bench-scale-tool
`lib.rs:641-645`), so the documented `cargo bench … -- --calibrate 1` invocation still burns the
full 2 s of contention workload it cannot calibrate, against CLAUDE.md's "benchmarks must run as
fast as possible".

### P4-5 — `run_h2`'s A-side branch on the knob under test is inert

`tests/loom_aba.rs:474-482` selects `Tag::pack(Tag::empty_index(), tag)` vs `Tag::empty()` for
thread A's `new_head` depending on `preserve_tag_on_drain`. A `compare_exchange`'s success
depends only on the *expected* value (`head`, the stale snapshot), never on `new`, so this branch
cannot affect the asserted outcome — only thread B's drain behaviour can, and does. The code
reads as if the counterfactual is two-sided when it is one-sided; a one-line comment saying A's
branch exists for faithfulness, not for the result, would settle it.

### P4-6 — ten `Builder::new()` sites where `loom::model(…)` is byte-identical in behaviour

`loom::model(f)` is defined as `Builder::new().check(f)`, and no site sets any builder field. The
apparent intent (making "we deliberately set no `preemption_bound`" visible) is already carried
explicitly by the module doc at `tests/loom_aba.rs:72-78`. If P3-2's `model()` helper lands, all
ten collapse into it anyway.

### P4-7 — the crate-root doc and README never mention two public items in narrative form

`TaggedIndexStack::is_empty()` and `TaggedIndex::try_pack` both appear in `CHANGELOG.md` and in
their own rustdoc, but neither the crate-root doc's section list nor `README.md` mentions them —
so the two documents a reader meets first (docs.rs landing page, crates.io README) present the
API as exactly `push`/`pop`/`pack`/`unpack`/`empty_index`/`is_empty(word)`.

### P4-8 — `CHANGELOG.md:7` is still `## 0.1.0 - Unreleased`

Known release-commit checklist item, flagged in every prior round; no action while unpublished.

---

## What is genuinely good (verified, not assumed)

- **The `head` field's `INVARIANT` block (`src/lib.rs:656-679`) is the best comment in the
  crate.** It states the exact premise `pop`'s `Acquire`-only success ordering depends on, names
  every current writer, says what would break it, and prescribes the compensating change
  (`AcqRel`) if that ever happens. I re-derived the release-sequence argument from the C++20 rule
  and re-grepped the premise; both hold.
- **`pop_pop_conservation`'s doc comment derives its assertion from `pop`'s actual loop** instead
  of asserting a plausible-looking invariant, and explains why this case is strictly stronger
  than the file's other conservation oracles. I traced it independently and it is correct.
- **The three `#[should_panic]` counterfactuals are real.** `run_h2(false)` genuinely recurs the
  exact head word (seed push leaves tag 1; the buggy drain resets to 0; the refill computes
  0 + 1 = 1), which is why the seed comment at `:434-438` is right that tag 1 is the *only*
  seed that can exercise it.
- **The `#[cfg(debug_assertions)]` gate on `pop_debug_assert_fires_on_invalid_next_from_backing`
  works and its 20-line rationale is accurate.** I ran both profiles: 18 tests debug, 17 release,
  both green, and the neighbouring `array_links_*_panics_on_index_out_of_range` tests correctly
  do NOT carry the gate (slice bounds checks are not debug-only) and pass in release.
- **The `loom`-optional-dependency claim is true and I measured it.** A scratch consumer whose
  only dependency is this crate locks exactly 2 packages, matching `Cargo.toml:28-29`'s "31
  locked packages down to 2".
- **The bench's contention discipline has held through five rounds of edits** — re-push exactly
  what you popped; drain-and-assert-empty before the phase-2 prefill — without regressing the
  double-push bug round 1 found. My own P2-1 probe reused that discipline and its conservation
  oracle passed 18/18.
- **The crate-doc's uncontended-regime figure reproduces.** It cites 51.56 ns/pair; I measured
  58.24 ns/pair through the committed harness and 51.0–52.1 ns/pair in the LTO scratch build.
  The doc's own "the bound below only needs the order of magnitude" caveat covers the spread,
  and the derived ~2 × 10⁷ pushes/sec still holds at the slower figure (1.7 × 10⁷).

---

## Suggested order of work

1. **P2-1** — reproduce the backoff A/B on the committed
   `benches/tagged_index_stack_bench.rs` (the numbers above are from a scratch tree), then
   either land the exponential variant or decline it in `CHANGELOG.md` **with the measured
   numbers**. This is the only item gating an unconditional GO.
2. **P3-2** — the `model()` helper. Do this before any future round adds an eleventh model; it
   is what stops this defect class for good, and it also absorbs P4-6.
3. **P3-3 / P3-4** as one bundle — both are stale prose about the loom suite, both in
   `tests/loom_aba.rs`. Prefer deleting the enumerations over updating them.
4. **P3-1** — one sentence in `src/lib.rs:231-232`; it is a factual error in published rustdoc.
5. **P3-6** — extend `width_16_push_rejects_index_mask_itself` with a location assertion
   (~10 lines).
6. **P3-7** — move the perf bullet out of `### Added` and reconcile it with whatever P2-1
   decides.
7. **P3-5** — the comment-density trim. Largest edit, lowest urgency, but it is the root cause
   of items 3 and 4 recurring every round.
8. **P4-1 … P4-7** as one bundle; **P4-8** is a record, not an action.
