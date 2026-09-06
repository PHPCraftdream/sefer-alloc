# tagged-index-stack — pre-release review, round 26

**Reviewer:** fx (claude-fable-5-1, xhigh) — independent pass. Run-25 and cycle-23 were skimmed for
format calibration only; every finding below was derived from the tree at the reviewed revision, and
every count/number quoted was re-measured, not copied.

**Review timestamp:** 2026-09-06 11:00 (local)

**Reviewed revision:** `579652d7580a8e106eb3c794482fcb230757e736` (`main`)

**Delta since run-25 (`8c58ca8`):** 15 commits — the run-25 remediation (`1b5ce46`, `b67c550`,
`3509ee6`, `71182f8`, `9215cba`, `4a41001`, `3d044ed`, `5549e8d`) plus seven evidence/provenance
refreshes for the A/B runner and the perf trackers.

## Verdict

**GO on the production algorithm.** No P0/P1/P2. I re-derived the four load-bearing ordering
arguments independently (see BUG-0), checked every arithmetic edge at both ends of the legal width
range, re-counted the unsafe inventory with the crate's own two commands, and ran every gate the
crate defines (clippy on both cfg rows, tests under default and repository cfg, rustdoc on both
cfgs, the loom suite, `cargo package --list`) — all green on rustc 1.97.0.

**What still needs a pass before `cargo publish`** (all P3, all documentation): the crate-root
"Tag-width budget" derivation — the one paragraph that justifies the `INDEX_BITS <= 16` compile-time
cap — states the hardware mechanism backwards and quotes a contended-regime rate that its own cited
receipt contradicts by two orders of magnitude (INACC-1); the crate's primary use-case (slot-resident
links via a custom `StackStorage` impl) has no example anywhere a crates.io user will see it
(NONOPT-1); and two review-era codenames (`H-2`, `RAD-1`) are used as technical terms across the
published rustdoc, README and CHANGELOG (SLOP-2). The prose-volume problem three prior rounds named
is measurably smaller than at run-25 but still the crate's main quality defect: `src/imp.rs` is 1,480
comment lines to 518 code lines (74 %), the `StackStorage` trait doc alone is 282 lines for three
methods, and `push_index`'s `# Safety` clause 3 is still 62 lines (SLOP-1).

## Priorities

| Level | Count | Meaning |
|---|---:|---|
| P0 | 0 | soundness holes — none found |
| P1 | 0 | serious runtime defects — none found |
| P2 | 0 | release blockers — none found |
| P3 | 4 | fix before first publish: one wrong derivation in published rustdoc, one missing primary example, two published-prose defects |
| P4 | 26 | dangling cross-references, stale test docs, editing residue, layout/test/CI/bench nits |

Findings are grouped by the task's six axes; each carries its level.

## Scope and mode

Read in full: `src/lib.rs`, `src/imp.rs`, `Cargo.toml`, `README.md`, `CHANGELOG.md`, all ten
files under `tests/`, all ten `tests/compile_fail/*/` fixtures (`src/main.rs` + `Cargo.toml`),
`benches/tagged_index_stack_bench.rs`, `examples/backoff_per_call_latency.rs`,
`scripts/tis_p3_ab_runner.mjs` (2,430 lines) and its three templates. Cross-checked outside the
crate only where the crate's own text makes a claim about it: the root
`tests/tagged_index_stack_compile_fail.rs` driver, `src/registry/heap_registry.rs`'s `StackStorage`
impl and `bootstrap.rs`'s `MAX_HEAPS`, the crate's rows in `.github/workflows/ci.yml`,
`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md`, `docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md`,
`docs/perf/_raw_tis_backoff_cap_sweep_run1.log`, and the two ADRs the rustdoc cites (both exist).

Run, read-only with respect to the tree (`CARGO_TARGET_DIR=target/fx26`, `--locked`; `git status`
clean before and after):

| Command | Result |
|---|---|
| `cargo clippy -p tagged-index-stack --all-targets -- -D warnings` | clean |
| `RUSTFLAGS="--cfg tagged_index_stack_test" cargo clippy … --all-targets -- -D warnings` | clean |
| `cargo test -p tagged-index-stack` | 30 passed (3 integration binaries compile to 0 tests, as documented) |
| `RUSTFLAGS="--cfg tagged_index_stack_test" cargo test -p tagged-index-stack --release` | 38 passed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` | clean |
| `RUSTFLAGS="--cfg tagged_index_stack_test" RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items` | clean |
| `RUSTFLAGS="--cfg loom" cargo test --release --features loom --test loom_aba` | 15 passed; all 5 `should_panic` counterfactuals panicked as expected |
| `cargo package -p tagged-index-stack --list` | 22 files; `scripts/` and `tests/compile_fail/` absent |

Verified mechanically: the "exactly nine" `#[allow(unsafe_code)]` regions in `src/`
(`imp.rs:866,1143,1272,1306,1354,1493,1574,1743,1892`), "sixteen unsafe function declarations" (16),
"one unsafe trait" (1), "zero unsafe impls" (0), "nine local `unsafe {}` blocks" (9:
`imp.rs:1315,1326,1336,1371,1447,1500,1514,1584,1754`) — all four counts match `lib.rs:261-276`
under the two commands at `lib.rs:284-285`. The whole-crate grep now returns 20 (9 in `src/`, 11 in
`scripts/`); `lib.rs` no longer states a whole-crate number (run-25 BUG-1 closed by deletion, the
right fix). Seal arithmetic re-checked at both width ends (`INDEX_BITS = 1`: `TAG_BITS = 63`,
`TAG_MAX = 2^63 - 1`, `tag + 1 <= 2^63 - 1`, `new_tag << 1 <= 2^64 - 2`; `INDEX_BITS = 16`: as
documented). Every §3.4 figure quoted in `lib.rs:221-235` is present in
`TIS_BACKOFF_CAP_SWEEP_GATE.md:377-389`.

---

## 1. Bugs

### BUG-0 — production algorithm: nothing found

Re-derived from the source, not from prior reports:

- **Release sequence on `head`** (`imp.rs:400-423`). Both writers are RMWs (`push` CAS
  `Release`/`Relaxed` at `:1471`, `pop` CAS `Acquire`/`Acquire` at `:1547`; `cas_head_for_test` is
  also a CAS; constructors initialise; the three readers only load). Under the C++20 rule Rust
  adopts, RMWs by any thread extend a release sequence, so a popper whose `Acquire` load reads a
  value installed by *any* later pop or push synchronises-with the push that wrote the link it is
  about to follow. `pop`'s `Acquire`-only success ordering is therefore sound; the comment's
  "no plain `store` ever" prohibition is exactly the invariant it rests on.
- **`push`'s two `Relaxed` reads** (`:1379`, `:1471` failure ordering). Push consumes the observed
  word only as `(cur_idx, tag)` values; the link it stores is published by its own `Release` CAS.
  A contract-abiding push cannot observe its own `index` as head (its authority came from a pop
  whose RMW moved the head off `index`; coherence forbids reading an older value afterwards), so
  `next[index] == index` is unreachable without a contract violation — the self-loop detector
  cannot false-positive.
- **Seal** (`:1389-1391`) precedes `store_next`, so a first-attempt refusal has no side effect
  (`tests/tag_seal.rs:64-84` pins `raw_head` byte-identity across it); two pushers racing at
  `TAG_MAX - 1` resolve correctly (loser fails on the tag, retries, observes `TAG_MAX`, refuses).
- **H-2** (`:1527-1532`) packs the observed tag on drain; `is_empty` masks the index half only; the
  tag never recurs across `(empty, t) -> (X, t+1) -> (empty, t+1)`.
- **Clause-4 guard** (`:1523-1526`): on this path `index < mask` always holds (it came from a
  non-empty head), so `next >= mask` and `next == index` are disjoint and
  `pop_link_out_of_range`'s message selection (`:1631-1648`) is well-defined for every input,
  including `next == INDEX_MASK` (the `AlwaysInvalidStorage` fixture).
- **`_CHECK_N`** (`:1694-1697`) is forced from both constructors and `with_tag_for_test`; the
  fields are private, so no other construction path exists. `ArrayIndexStack<16, 0>` compiles and
  is merely useless (every push panics in `store_next`), which is acceptable.
- **`SealedStorage` coherence** (`:1307`, `:1893`): the blanket bridge and the `ArrayIndexStack`
  impl overlap only if `ArrayIndexStack: StackStorage`, which the crate can negatively reason about
  for its own local type/trait pair; the E0119/E0117 argument in the type doc is correct.

The loom suite confirmed all of this at runtime in this review.

### BUG-1 (P4, doc-level) — `pop_index` cites a backoff comment on `push_index` that does not exist

`src/imp.rs:1171-1174`:

> On a lost CAS, the retry backoff (see [`push_index`](StackOps::push_index)'s identical backoff
> comment and `BACKOFF_SPIN_CAP`) is skipped when …

`push_index`'s doc (`:951-1150`) contains no backoff comment at all — the word does not occur in it.
The only backoff prose in the file is `BACKOFF_SPIN_CAP`'s own doc (`:28-47`) and `Backoff::spin`'s
(`:66-78`); the `backoff.spin()` call sites (`:1480`, `:1562`) carry no comment on the push side.
Dangling cross-reference in published rustdoc. Fix: "(see `BACKOFF_SPIN_CAP`)".

### BUG-2 (P4, doc-level) — a two-hop citation to a section that does not exist

`src/imp.rs:1544-1546`:

> Strong CAS over `compare_exchange_weak` — measured codegen-identical on aarch64 (see push's CAS
> note: `docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md` §0).

`TIS_LINK_ORDERING_WEAK_CAS_GATE.md` has no §0; its headings are Verdict / Accepted measurement
identity / Codegen matrix and observations / Runner contract / Next trigger / Current artifacts /
Reproduction commands. And "push's CAS note" (`:1469-1470`) cites the file without a section. The
gate doc was rewritten as a "current state" document (its title says so) and the section number
survived from an older layout. Fix: cite "Codegen matrix and observations".

---

## 2. Performance

### PERF-0 — nothing new to recommend in the hot path; tracked items not re-recommended

The loop bodies (`push_index_impl` `:1359-1484`, `pop_index_impl` `:1495-1567`) are the minimal
tagged Treiber shape: one `Relaxed`/`Acquire` head load, one link access, one strong CAS per
attempt, capped exponential backoff on loss. Everything a static reading could suggest has either
been measured and closed or is an open, instrument-ready item with a stated trigger:

- `compare_exchange_weak` at both CAS sites: codegen-identical to strong on x86-64 and on both
  aarch64 lowerings; asserted as a negative control by the runner. Closed NULL.
- pop success `Acquire -> Relaxed` (item 63): codegen-identical everywhere. Closed NULL.
- link cells `Acquire`/`Release -> Relaxed` (item 62): a real `ldar`/`stlr` delta on aarch64, x86
  NULL; native arm64 wall-clock pending (`workflow_dispatch`-only job, `ci.yml:2981-2996`). The
  crate ships defence-in-depth orderings and says so. Correct posture.
- skip the redundant `store_next` on a tag-only retry (item 61, `store_elided`): x86 static
  +14.5 % instructions in `push_index_impl`; same pending arm64 run.
- `BACKOFF_SPIN_CAP` other than 6: swept 0/4/6/8/10 on one host; the microarchitecture caveat
  run-25 asked for is now present (`imp.rs:43-44`, `lib.rs:241-243`).
- push's initial head load `Acquire -> Relaxed`: already landed; verified sound above.
- cache-line padding of `StackHead`/`ArrayLinks`: correctly delegated to the embedder.

Three micro-observations I looked at and am explicitly **not** recommending, recorded so the next
round does not re-derive them:

1. `pop`'s H-2 branch (`:1527-1532`) could be made branchless as
   `pack_truncating(next & INDEX_MASK_U32, tag)` because `TAIL & INDEX_MASK == INDEX_MASK` at every
   legal width. It removes one `cmov` next to a `lock cmpxchg` (unmeasurable), couples `TAIL`'s
   value to the sentinel the docs deliberately keep distinct, and contradicts `pack_truncating`'s
   "no masking takes place" contract. Not worth it.
2. `push` does not skip its backoff when the lost CAS's `actual` already carries `TAG_MAX` (the
   mirror of pop's empty-`actual` skip at `:1561`); the next iteration returns `Err` regardless.
   Only reachable on a sealed head under contention. Not worth a branch.
3. The owned type pays `index >= INDEX_MASK` (`:1364`) and then `index >= N` (`:1985`) — the first
   is implied by the second once `_CHECK_N` holds, but it lives in the shared body. One predictable
   compare. Not worth splitting the body.

### PERF-1 (P4) — neither contention row in the bench isolates the head cache line

`benches/tagged_index_stack_bench.rs:441-448` documents `contention/churn` as confounded by
link-array false sharing (64 contiguous prefilled indices = 4 cache lines of `AtomicU32` links
shared by 8 threads). `contention/push_pop` (`:391-395`) seeds `thread_id * LINKS_SIZE /
num_threads` = 32 indices apart = two lines apart, so it does not share link lines — but it counts a
`None` iteration as 0 ops and its steady state (one live index per thread, all contending for the
same head) is a different regime from churn's. The crate's own rate bound (`lib.rs:189-198`)
therefore leans on the *single-thread* churn row for its receipt, and the `TIS_BACKOFF_CAP_SWEEP_GATE`
fairness analysis is built on rows that mix head contention with link false sharing. The A/B harness
template already has the right shape (`scripts/tis_p3_ab/harness_bin.rs:37-40`, a
`#[repr(align(64))] RegistrySlot` per link); a third bench row over an aligned-per-link
`StackStorage` impl would give the bench one number that is head-CAS throughput and nothing else.
Bench design only; no runtime change.

---

## 3. Code smell

### SMELL-1 (P4) — the retry-counter hooks are still cfg-gated twice

Definitions `imp.rs:93-104` (`#[cfg(any(tagged_index_stack_test, loom))] fn note_pop_retry` /
`note_push_retry`) and both call sites `:1477-1478`, `:1553-1554` carry the identical predicate.
One predicate suffices; four sites drift. Run-25 SMELL-2 raised this; unchanged at this revision.
Either keep the call-site cfg and give the fns an unconditional signature with a cfg'd body, or the
reverse — not both.

### SMELL-2 (P4) — test-only statics declared 1,900 lines below their first use

`BACKOFF_SPIN_COUNT` (`imp.rs:2019`), `POP_RETRY_COUNT` (`:2015`), `PUSH_RETRY_COUNT` (`:2035`) are
referenced at `:82`, `:96`, `:103`. Legal, but a reader of `Backoff::spin` has to travel to the end
of the file to learn what `BACKOFF_SPIN_COUNT` is. All test-only items (three statics, two `note_*`
fns, three `*_for_test` free fns) belong in one contiguous
`#[cfg(any(tagged_index_stack_test, loom))]` block next to `Backoff`.

### SMELL-3 (P4) — `/// # Safety` on one of three identical verbatim forwarders

In both `SealedStorage` impls the `store_next` forwarder carries a `/// # Safety` doc
(`imp.rs:1328-1331`, `:1900-1902`) while `head`/`load_next` in the same impl blocks do not; all
three are `unsafe fn` forwarders of the same shape (run-25 SMELL-1's fix made them uniform, the docs
did not follow). Trait-impl methods get no `missing_safety_doc` lint, so this is convention only —
but the asymmetry reads as if `store_next` were special. All three or none.

### SMELL-4 (P4) — `Parasite` names a design that no longer exists

`tests/custom_storage_impl.rs:207-240`: the struct is named for the era when it parasitised an
`ArrayIndexStack`'s head. Its doc now spends a paragraph (`:209-214`, `:252-262`) explaining that
"the head no longer comes from `ArrayIndexStack::head()` — that extraction route is CLOSED … What
this test now demonstrates …". The test is "a forged acyclic backing evades the self-loop
detector"; name the fixture `ForgedAcyclicLinks` and delete the archaeology.

### SMELL-5 (P4) — small residue

- `imp.rs:38`: "The cap is 6 —" restates the literal on the next line (`= 6`); if the constant
  moves, the prose lies. Say "the shipped cap" and let the code carry the number.
- `imp.rs:49-52`: a `//` (non-doc) comment uses intra-doc-link syntax
  ``[`TaggedIndex::_CHECK_BITS`]``, which rustdoc never sees.
- `imp.rs:60`, `:71`: "`#[inline]`: hot path, monomorphised downstream" — `Backoff::new`/`spin` are
  not generic and are not monomorphised. The attribute is right for a different reason: a private
  non-generic fn called from a *downstream* monomorphisation of `push_index_impl` would otherwise be
  an out-of-line call into this crate's object code (absent LTO). Say that.
- `imp.rs:1289-1298`: ten lines justifying `StackStorage::head(self)` over `self.head()` (E0034
  ambiguity between two applicable trait methods). One line: "qualified: both traits declare
  `head`".
- `imp.rs:2069`: `backoff_spin_depths_for_test` reads the tuple field `backoff.0` from outside the
  `impl Backoff` — a `fn depth(&self) -> u32` keeps the state machine's one field private to its
  impl.
- `examples/backoff_per_call_latency.rs:135-141`: the range check compares against `LINKS_SIZE` but
  the message hardcodes "1..=64 (LINKS_SIZE)".
- `Cargo.toml:44-49`: an empty `[dependencies]` table whose only content is a comment about the
  `[target.'cfg(loom)'.dependencies]` table below it. Move the comment; drop the empty table.

---

## 4. "Neuroslop"

Measured at this revision (comment lines are lines whose first non-blank characters are `//`,
`///` or `//!`):

| File | comment | code | comment share |
|---|---:|---:|---:|
| `src/imp.rs` | 1,480 | 518 | 74 % |
| `src/lib.rs` | 359 | 23 | 94 % |
| `tests/loom_aba.rs` | 735 | 744 | 50 % |
| `tests/custom_storage_impl.rs` | 310 | 304 | 50 % |

Run-25 measured `imp.rs` at 1,545/508 (75 %). The archaeology purge is real (zero review-round
identifiers remain in `src/`); the structural repetition is what is left.

### SLOP-1 (P3) — the `StackStorage` and `push_index` docs are reference manuals, not API docs

Concrete sizes: the `StackStorage` trait doc is 282 lines (`imp.rs:584-865`) for three one-line
`unsafe fn` declarations; `push_index`'s doc is 200 lines (`:951-1150`); `pop_index`'s is 98
(`:1152-1249`). Inside those, the same fact is stated repeatedly with a pointer to the other copies:

- Each hook carries the identical three-line preamble "Implementor hook — callable only by
  upholding this caller-side contract inside `unsafe` (see the trait doc's 'The three hooks are
  unsafe fn' section for the full picture)" (`:884-886`, `:901-903`, `:925-927`), and the section it
  points to (`:712-744`, 33 lines) says the same thing a fourth time.
- Trait clause 5 (`:643-644`) is "**Same logical head every call** — see 'Mechanical requirement on
  `head()`' below" — a clause whose entire body is a pointer to a section (`:811-817`) that restates
  `head()`'s own doc (`:880-882`).
- `push_index` clause 3 is 62 lines (`:1006-1068`) containing a worked two-thread counterexample,
  the names of two loom tests and a paragraph on why physical-return timing is irrelevant — then
  `:1070-1100` restates liveness a second time ("Violating the liveness rule corrupts the
  free-list …"), and `README.md:172-181` and `tests/loom_aba.rs:931-995` carry the same
  counterexample again. Run-25 SLOP-4 asked for a four-line clause 3; it is unchanged.
- `pop_index`'s doc ends with its own "# Lock-freedom and starvation" section (`:1239-1246`) that
  summarises the crate-root section of the same name, which the README also summarises.

A `# Safety` section needs the preconditions and their consequences, stated once. The worked
examples, the test names and the "why we say it this way" paragraphs belong in the ADR the trait doc
already cites (`:667-669`). Target: `StackStorage` under 100 lines, `push_index` under 60.

### SLOP-2 (P3) — `H-2` and `RAD-1` are review-history codenames used as technical terms in published docs

Occurrences: `lib.rs` 3/5, `imp.rs` 8/9, `README.md` 3/2, `CHANGELOG.md` 1/1 (H-2 / RAD-1). The
crate introduces them as codenames — `lib.rs:37-38` "the **H-2 empty-transition tag preservation**
and the **lazy link discipline** (internally: RAD-1)" — and then uses them as nouns: "the H-2 fix"
(`imp.rs:307`), "the legitimate H-2 shape" (`:216-217`), "RAD-1: no eager free-list chaining"
(`:1911`), "the H-2 path" (`benches/…:243`), "Lazy links (RAD-1)" (`CHANGELOG.md:28`). A crates.io
reader has no idea what H-2 or RAD-1 are, and "internally" tells them it is not their business. The
descriptive names already exist in the same sentences; use them and drop the codes.

### SLOP-3 (P4) — fifteen one-host measurements in the crate-root rustdoc

`lib.rs:215-243` ("Lock-freedom and starvation") quotes 41-60 ms, 0.6-24 ms, 130-173 ms, 40-46 ms,
60-86, 0-8, 26-34, 0-2, ≈ 1 µs vs 54-182 µs, 249-285 vs 553-661, 4.85x, 4.05x, ~2.4x, 1.9-2.6x —
all from one Tiger Lake laptop. The qualitative trade (better p99.9 and aggregate throughput for
worse extreme outliers, thread-count-dependent tail band, unit is `spin_loop` hints) plus the link
is what an API reader needs; the numbers are the gate report's. `README.md:210-225` and
`imp.rs:1239-1246` are the second and third summaries of the same section.

### SLOP-4 (P4) — a rejected design that never shipped is argued against in the crate-root doc

`lib.rs:70-74`:

> The head↔links binding is expressed in ONE place — the implementor's own single [`StackStorage`]
> impl, a trait deliberately OPEN to external implementation (that is the extension point, not a
> crate-owned surface) — instead of being re-asserted per call through a caller-supplied `&L:
> Links` parameter.

No `Links` trait and no per-call `&L` parameter exist in this crate; the sentence contrasts the API
with an alternative only the review history knows. The sentence that follows (`:77-84`) is fifteen
lines long with three nested asides. Say what the API is.

### SLOP-5 (P4) — the README's thirteen-line sentence

`README.md:77-89`:

> The live obligation is implementor/caller discipline at the VALUE level: a head must stay bound
> to one backing for its whole life and be reachable through exactly ONE live implementor value at
> a time — but one-at-a-time liveness is not sufficient: it must never be rebound to different link
> storage across time, even if no more than one value is live at any instant (the trait doc's
> `# Safety` clause 1; clause 2 requires one backing consistently), and disjoint REACHABLE-index
> populations per binding over any shared link-cell population — cell sharing per se is harmless
> (two stacks over the same cells with disjoint populations coexist correctly); the hazard is one
> index reachable from two bindings (the trait doc's `# Safety` clause 3). These are obligations
> about head↔links BINDINGS — invisible to any audit of a single impl block, discharged by
> construction.

Three rules, three sentences: one head, one backing, for the head's whole life; never rebind a live
head to other links; never let an index be reachable from two bindings that share link cells.

### SLOP-6 (P4) — one lint explained four times

`#![deny(unsafe_op_in_unsafe_fn)]`'s effect ("an `unsafe fn` body still needs a local `unsafe {}`")
is explained at `lib.rs:341-347`, `imp.rs:1443-1446`, `:1582-1583` and `:1752-1753`. The crate
attribute's comment is the place; the three call sites need only the `// SAFETY:` line.

### SLOP-7 (P4) — nine "Tier-2 item-scoped allow" paragraphs

Each `#[allow(unsafe_code)]` region carries a 2-12-line block of the form "Tier-2 item-scoped allow
— one of the crate's audited lint-exception regions (see the crate docs' 'Where unsafe lives' for
the full inventory). Single documented reason to hold `unsafe`: …" (`imp.rs:867-878, 1144-1149,
1268-1271, 1299-1305, 1355-1358, 1491-1492, 1569-1573, 1744-1746, 1888-1891`; ~50 lines). The
workspace rule asks for one documented reason per region, not a preamble; `// unsafe: <reason>` per
site, with the tiering explained once in `lib.rs`, satisfies it.

### SLOP-8 (P4) — measurement-process language addressed to nobody

- `CHANGELOG.md:35-39` "**Pending measurement** — current codegen closes `cas_weak` and
  `pop_success_relaxed` as static NULL controls. Native ARM timing remains pending only for link
  ordering (`links_relaxed`) and `store_elided`; production code is unchanged. …" — internal A/B
  status in a first-release changelog. It will be stale the moment the arm64 job runs, without a
  release to update it.
- `README.md:242-243` "Static assembly differences are codegen observations only and do not
  constitute a runtime speedup claim." — a reviewer-to-reviewer disclaimer; the README never makes
  the claim it disclaims.
- `imp.rs:1224-1228` / `:1519-1520` / `CHANGELOG.md:29` "measured ≈ free next to the head CAS (see
  CHANGELOG.md)" / "measured at no measurable cost" — see INACC-7.

### SLOP-9 (P4) — test docs longer than their tests

- `tests/loom_aba.rs:931-995`: a 65-line doc for `counterfactual_same_index_concurrent_push_self_loops`
  (30 lines of code), including a 30-line "Why the retry gate" derivation, followed by a 17-line
  in-body comment (`:1047-1063`) proving via per-location coherence that the drain panics
  deterministically.
- `tests/loom_aba.rs:1076-1117`: 42 lines for `pop_repush_after_publish_conserves`, half of it on
  what the test does *not* prove.
- `tests/custom_storage_impl.rs:296-328`: a 33-line doc that narrates the exact link values the
  test body's own comments (`:363-374`) then narrate again.
- `tests/stack_unit.rs:196-210`: a comment whose content is "the compile-fail coverage lives in the
  root driver; find the trybuild-decline notes with `grep -rn trybuild --include=*.rs .`".

### SLOP-10 (P4) — history in test docs

`tests/proptest_pack_unpack.rs:9-10` "width 15 probes just under the ceiling the way 31 once sat
against the old 32 ceiling"; `:84-85`, `:98-99` "the old truncating pack". `tests/stack_unit.rs:428`
"Before the fix this call packed through `pack_truncating`". A test reader needs the property, not
the bug it once caught.

---

## 5. Inaccuracies

### INACC-1 (P3) — the Tag-width-budget derivation states the hardware mechanism backwards and its contended rate contradicts its own receipt

`src/lib.rs:159-169`:

> The rate term is bounded by hardware, not by the workload. The tag is global to the whole stack:
> every successful push is a compare-exchange (a locked RMW) on the one `AtomicU64` head word, so in
> the contended regime every push serializes on a single cache line whose exclusive ownership must
> transfer between cores, capping the aggregate rate at roughly `10^8` to `10^9` RMWs/sec no matter
> how many threads contend. The opposite regime — the uncontended single-threaded case, where the
> head line stays resident in one core's L1 — is governed instead by the latency of the bare RMW
> instruction itself (`lock cmpxchg` on x86-64): materially faster, but still bounded.

then `:195-198`:

> the single-threaded `churn` rows measure ~`2 × 10^7` successful pushes/sec … — an order of
> magnitude under the working ceiling above.

Three problems, in one paragraph that is the crate's stated reason for rejecting `INDEX_BITS > 16` at
compile time:

1. **Magnitude.** A cross-core ownership transfer is ~50-100 ns, so the mechanism the paragraph
   names caps a contended line at ~1-2 × 10^7 RMWs/s, not 10^8-10^9. The cited receipt agrees: at
   the shipped cap, `_raw_tis_backoff_cap_sweep_run1.log:224` gives `contention/churn` 27.06 M
   ops/s at 8 threads = 13.5 M pop+push pairs/s, and `:243` gives 21.97 M ops/s at 16 threads =
   11.0 M pairs/s. The stated contended ceiling is 10-100x above what the receipt shows.
2. **Direction.** The receipt's single-thread churn (`:214`, 56.08 ns/op → 17.8 M pairs/s; other
   sections 52-54 ns) is *faster* than every contended aggregate in the same log. The paragraph
   calls the uncontended regime "materially faster" than a ceiling it just placed at 10^8-10^9 —
   and then reports it at 2 × 10^7, "an order of magnitude under" that ceiling. Both halves cannot
   be true.
3. **Which regime bounds the seal.** The fastest possible push rate is the uncontended one (the
   L1-resident `lock cmpxchg`, ~20 cycles, so at most ~10^8/s on today's cores, ~2-4 × 10^7/s
   through this harness); contention only lowers aggregate throughput. The "generous 2 × 10^8"
   working ceiling (`:170`) is therefore a valid upper bound for *both* regimes and the 16-day /
   92-minute / 21-second figures stand — but for the opposite reason from the one stated.

Fix: rewrite `:159-169` as "the push rate is bounded above by the uncontended L1-resident RMW
latency (~10^8/s); contention on the single head line only lowers the aggregate (the sweep's
8-16-thread rows measure ~1.1-1.4 × 10^7 pairs/s against ~1.8 × 10^7 single-threaded)", and keep
the 2 × 10^8 working ceiling as the explicitly generous bound it already is. Drop "10^8 to 10^9".

### INACC-2 (P4) — `custom_storage_impl.rs` claims to hold the crate's only working second implementor

`tests/custom_storage_impl.rs:5-11`:

> Every other test in this crate exercises only [`ArrayIndexStack`] (the exceptions are not working
> implementors: this file's own `AlwaysInvalidStorage` below deliberately violates the contract …
> and `tests/compile_fail/unsafe_impl_required/` deliberately fails to compile), so this file is
> where those claims are exercised by a real, WORKING second implementor.

`tests/narrow_domain_unchecked_storage.rs:66-138` defines `UncheckedPool<8>`, a contract-abiding
`StackStorage` implementor driven through `push_index`/`pop_index` in two tests — and it is the one
implementor the crate runs under Miri (`ci.yml:2311`). Two working second implementors exist; the
module doc should name both or drop the claim.

### INACC-3 (P4) — `proptest_pack_unpack.rs` cites two tests that were deleted

`tests/proptest_pack_unpack.rs:3-6` names `pack_unpack_round_trip_16` and
`max_legal_width_index_mask_never_equals_tail` as `stack_unit.rs` tests it complements. Neither
exists at this revision (collapsed in `1b5ce46`); the surviving items are
`pack_rejects_out_of_range_halves_and_accepts_the_full_index_range` (`stack_unit.rs:69`) and the
`const _` block (`:39-49`). `grep -rn` across the crate finds the two names only in this doc.

### INACC-4 (P4) — `stack_unit.rs` module doc undercounts its own gated tests and widths

`tests/stack_unit.rs:15-22` lists three `#[cfg(tagged_index_stack_test)]` probes; the file has five
(`:412-433` add `with_tag_for_test_accepts_the_exact_tag_max_boundary` and
`with_tag_for_test_panics_instead_of_silently_truncating_an_out_of_range_tag`). `:137-139` says
width 12 is "distinct from this file's other widths 1 and 16"; `:160-170` uses width 4.

### INACC-5 (P4) — the loom module doc's property list stops at (i); the file has a (j)

`tests/loom_aba.rs:34-105` enumerates properties (a)-(i). The body has a "(j) The PERMITTED
republish" section (`:1068-1074`, `pop_repush_after_publish_conserves`) with no entry in the list
that `lib.rs:257` and `README.md:257` both defer to as "the per-model breakdown".

### INACC-6 (P4) — editing residue in published prose

- `lib.rs:189-194`: "confirmed by this repository's own bench receipts
  ([`docs/perf/_raw_…`](URL)\n Re-run `cargo bench …` for a fresh sample); the bound needs only …"
  — no separator between the link and "Re-run", so the rendered sentence reads "receipts
  (docs/perf/_raw_… Re-run cargo bench … for a fresh sample); the bound …".
- `lib.rs:240-241`: "[`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` §3.4](URL)\n Measured with
  `examples/backoff_per_call_latency.rs`." — renders "§3.4 Measured with …" (missing period).
- `examples/backoff_per_call_latency.rs:1-7`: "… `BACKOFF_SPIN_CAP`'s CAS-retry backoff — the axis
  the cap sweep\n Backoff-cap measurements are described in …: per-thread ops … cannot distinguish
  … slow")." — two sentences fused mid-clause with a dangling `)`.
- `examples/backoff_per_call_latency.rs:36-38`: "(see the raw log this probe's output is appended to
  `docs/perf/_raw_tis_backoff_per_call_latency.log`." — unclosed parenthesis.
- `imp.rs:667-669`: "The full design/audit detail beneath this contract — the per-clause
  elaboration and worked corruption examples are in the repository ADR" — unbalanced dash.
- `imp.rs:863-865`: "recorded in the repository ADRs `docs/adr/…storage-binding-closure.md`" —
  plural, one path.
- `README.md:108-109`: "For standalone use, `ArrayIndexStack<INDEX_BITS, N>` is the owned
  standalone stack" — "standalone" twice in one clause.

### INACC-7 (P4) — a measurement cited in a circle

`imp.rs:1224-1228` and `:1519-1520` say the release-active guards "measure ≈ free next to the head
CAS (see CHANGELOG.md)"; `CHANGELOG.md:29` says "measured at no measurable cost next to the head
CAS" and cites nothing. No committed artifact under `docs/perf/` mentions the guard (grep for
"release-active"/"clause-4 guard" hits only the two ADRs' prose). Either cite the receipt or say
"a predictable compare; not separately measured".

### INACC-8 (P4) — README probe paragraph

`README.md:292-295`: "All are `#[doc(hidden)]` (docs.rs included)." The probes are `#[cfg]`-gated
out of a default build; docs.rs builds default features, so it contains none of them — the
parenthetical implies the opposite. `README.md:253-256` lists three `#[should_panic]`
counterfactuals ("untagged corruption, the H-2 tag-reset ABA, and a Relaxed-CAS-failure-ordering
regression"); there are five, and the two omitted (`counterfactual_same_index_concurrent_push_self_loops`,
`counterfactual_bypassed_seal_lets_stale_cas_double_issue`) are the ones that prove the crate's
headline property — the seal — non-vacuous.

### INACC-9 (P4) — published rustdoc names the unpublished parent's internals

`imp.rs:976-977` "`sefer-alloc::Registry`'s is `0..MAX_HEAPS`", `:648-650` "a lazily-materialised
backing (like `sefer-alloc::Registry`'s chunked slot array)", `lib.rs:292-293` "sefer-alloc's
registry free-list today". `MAX_HEAPS` exists (`src/registry/bootstrap.rs:735`, 4096) — the claims
are true — but `sefer-alloc` is not on crates.io and a `tagged-index-stack` user cannot resolve any
of them. "a fixed-capacity slot table's `0..CAPACITY`" says the same without the dangling name.

---

## 6. Non-optimalities

### NONOPT-1 (P3) — the crate's primary use-case has no example a user will see

`README.md:106-108`: "A production allocator keeps its links **slot-resident** … via a custom
`StackStorage` impl." `imp.rs:849-851`: "slot-resident links in implementor-owned storage … is the
whole design point." Neither the README, the rustdoc nor `examples/` contains a `StackStorage`
implementor. The two that exist — `VecStorage` (`tests/custom_storage_impl.rs:52-88`) and
`UncheckedPool` (`tests/narrow_domain_unchecked_storage.rs:66-138`) — ship in the package but are
not rendered on docs.rs and are not where a `cargo add` user looks. The README's only example
(`:33-44`) is the owned `ArrayIndexStack`, and the README never shows that `StackOps` must be
imported (`use tagged_index_stack::StackOps as _;`, as the root crate does at
`src/registry/heap_registry.rs:579`) for `push_index`/`pop_index` to resolve. One 25-line README
section (or `examples/slot_resident.rs`) mirroring `VecStorage`, with the import and a
clause-by-clause `// SAFETY:` in the `unsafe impl`, is the single highest-leverage doc change left.

### NONOPT-2 (P4) — CI lints rustdoc on the default cfg only

`ci.yml:2076` runs `RUSTDOCFLAGS="-D warnings" cargo doc -p tagged-index-stack --no-deps` once. The
`#[doc(hidden)]` probe docs (`raw_head`, `load_next_for_test`, `with_tag_for_test`,
`retry_counts_for_test`, `backoff_spin_depths_for_test`, and the loom-only items) exist only under
the repository cfg / `--cfg loom`, so their intra-doc links are checked by nobody in CI (this review
and run-25 ran them by hand — both clean today). One extra row with
`RUSTFLAGS="--cfg tagged_index_stack_test" … --document-private-items`, mirroring the clippy row at
`:2075`, closes it.

### NONOPT-3 (P4) — the self-verifying commands assume the workspace layout

`lib.rs:284-285` gives `rg -n … crates/tagged-index-stack/src/imp.rs`. Inside the published package
(and a `cargo vendor` copy) the crate root *is* the root; `src/imp.rs` works in both places,
`crates/tagged-index-stack/src/imp.rs` only in this repository. Same for `README.md:98`
(`tests/compile_fail/array_index_stack_head/` — excluded from the package) and `imp.rs:1667`.

### NONOPT-4 (P4) — the A/B runner's effort/evidence ratio

`scripts/tis_p3_ab_runner.mjs` is 2,430 lines (plus a 425-line harness template): host-context
JSON captured, SHA-256'd and base64'd into every CSV row (`:1175`, `:1774`), sanitized-environment
fingerprints, ODB-pinned source reads, a 40-assert provenance parser (`:1955-2092`), a summary mode
that re-derives medians. Its evidentiary output at this revision is two static codegen tables; the
wall-clock leg is `workflow_dispatch`-only (`ci.yml:2996`) and has never produced a committed
wallclock CSV (`docs/perf/` holds only the two codegen CSVs), so `--mode summary` — which requires
one — has never run green against committed artifacts. Of the 15 commits since run-25, seven touch
this script or its outputs. Excluded from the package, so not a publish concern; recorded because
the maintenance cost is real and the pending arm64 run is the only thing that would justify it.

### NONOPT-5 (P4) — small coverage gaps

- `TagExhausted`'s `Display` text (`imp.rs:343-352`) — the crate's only user-visible error string —
  is never asserted.
- `pushes_remaining()` on a fresh head equals `TAG_MAX` is never asserted (only the seeded cases
  in `tag_seal.rs`).
- `StackHead::is_empty` is exercised only through `ArrayIndexStack`; a custom implementor never
  calls it after `push_index`.

### NONOPT-6 (P4) — large owned stacks and the stack

`ArrayIndexStack<16, 65535>` is 256 KiB by value. `new()` being `const fn` makes `static` placement
free, and that is the shape the README example should show for anything past a few thousand
indices; today neither `ArrayIndexStack`'s doc nor the README says that a `let` of a large instance
is a stack-overflow in a debug build.

### NONOPT-7 — local-tree observation, not a crate finding

`crates/tagged-index-stack/tests/compile_fail/array_index_stack_capacity/src/` is an empty
untracked directory (the pre-`71182f8` fixture's leftover), and the two split fixtures carry
fixture-local `target/` directories (ignored by `**/target/`) from a manual build — the driver
builds fixtures under `CARGO_TARGET_TMPDIR`, so these are stale developer artefacts, not something
the driver creates. Harmless; noted so nobody reads them as part of the fixture contract.

---

## Prior-round items checked at this revision

Closed (verified, not trusted): run-25 BUG-1 (whole-crate allow count — sentence deleted), BUG-2
(`retry_counts_for_test` doc), PERF-1 (microarchitecture caveat, `imp.rs:43-44`, `lib.rs:241-243`),
PERF-2 (`README.md:113-115`), SMELL-1 (all three `SealedStorage` hooks `unsafe fn`), SMELL-3
(`cur_idx == empty_index()`), SMELL-4 (one `retry_counts_for_test`), SMELL-5
(`array_links_out_of_range`), SLOP-1 (SAFETY comment reduced to the precondition discharge), SLOP-2
(CAS comment shortened), SLOP-3 ("Where unsafe lives" is 50 lines), SLOP-5 (`TAIL` doc), INACC-1
("six models"), INACC-2 (shape 1 → clause 2 everywhere), INACC-3 (`threaded_conservation.rs`
comment), INACC-4 ("root `tests/…`"), INACC-5 (probe list), NONOPT-1 (`_CHECK_N` + two fixtures),
NONOPT-2 (`const _` assert), NONOPT-3 (README sentence), NONOPT-4 (`Cargo.toml` comment), NONOPT-5
(pack tests collapsed), NONOPT-6 (dead `loom` cfg arms), NONOPT-7 (`--target` required for summary).

Still open: run-25 SMELL-2 (double cfg gate — SMELL-1 here), SLOP-4 (clause 3 in three places —
inside SLOP-1 here), SLOP-6/SLOP-8 partially (the self-loop disjunction is now two homes; the test
narration is SLOP-9 here).

## Closing summary

**Publishable on correctness.** The lock-free core, its orderings, the seal, the unsafe boundary and
the test infrastructure hold up to an independent derivation and to running every gate the crate
defines; nothing in `src/` needs to change before `cargo publish`.

**Do before publishing** (P3): rewrite the Tag-width-budget mechanism paragraph so it matches
physics and its own receipt (INACC-1); add one `StackStorage` implementor example where users will
see it (NONOPT-1); replace the `H-2`/`RAD-1` codenames with their descriptive names (SLOP-2); and cut
the `StackStorage`/`push_index` docs to the contract, moving the worked examples to the ADR that
already exists for them (SLOP-1).

**Can wait** (P4): the two dangling cross-references (BUG-1, BUG-2), the stale test-doc citations
(INACC-2..5), the splice residue (INACC-6), the circular "measured ≈ free" citation (INACC-7), the
README probe/counterfactual lists (INACC-8), the parent-crate names in rustdoc (INACC-9), the
double cfg gate and the bottom-of-file statics (SMELL-1, SMELL-2), the `Parasite` rename, the extra
rustdoc CI row, the workspace-relative commands, and the bench's missing head-isolated row.
