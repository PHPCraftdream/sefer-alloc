# tagged-index-stack — pre-release static review, round 25

**Reviewer:** fx (claude-fable-5-1, xhigh) — independent pass; prior reports (run-21, run-22, cycle-23)
were skimmed for format calibration only, every conclusion below was re-derived from the tree.

**Review timestamp:** 2026-09-06 08:15 (local)

**Reviewed revision:** `8c58ca8518ae3fde05c34bb91d190f80e795c5b1` (`main`)

**Delta since the cycle-23 revision (`1a15a79`):** 40 commits, 33 touching this crate — the cycle-23
P3 closures (`63d4de8`, `c08e1f7`, `76d4e5d`), the `test-internals` feature removal and cfg migration
(`7e5ea47`, `c5c09a0`), the compile-fail diagnostic hardening (`7ca68b8`), and a long tail of A/B-runner
provenance work (`c00087f` … `8c58ca8`).

## Verdict

**GO on the production algorithm.** No P0/P1/P2 in `src/`. I re-derived the four load-bearing ordering
arguments (push's `Relaxed` initial load and `Relaxed` CAS-failure ordering; pop's `Acquire`-only
success ordering riding the all-RMW release sequence on `head`; the H-2 running-tag empty transition;
the seal check preceding `store_next`), the self-loop detector's false-positive impossibility under the
contract, the seal arithmetic at both ends of the legal width range, and the eight-region / ten-`unsafe
fn` / six-`unsafe {}` inventory — all hold. Unlike the prior three rounds I also *ran* the crate's own
gates (see "Scope and mode"): clippy on both cfg rows, the test suite under default and repository cfg,
rustdoc with `-D warnings` under the repository cfg and the loom cfg, and the loom suite — all green on
rustc 1.97.0.

**What still needs a pass before `cargo publish`:** two statements in the *published rustdoc* are
wrong (the whole-crate `#[allow(unsafe_code)]` count in "Where unsafe lives" is 19, not 8+4; the
`retry_counts_for_test` doc describes an assertion the harness does not make), the loom module doc that
`lib.rs` and the README both declare "the source of truth for the per-model breakdown" misclassifies one
model, the `SealedStorage` bridge justifies `store_next`'s `unsafe` with an argument it rejects for the
other two hooks, and the backoff cap's documented fairness trade is measured in a unit (`spin_loop`
hints) whose absolute cost varies by more than an order of magnitude across the microarchitectures the
crate supports — a caveat nowhere in the docs. The prose volume problem run-21 named is smaller but not
gone: `src/imp.rs` is still 1,545 comment lines to 508 code lines (75 %), and the same proofs are still
restated two to four times with "not repeated here" markers attached to the repetitions.

## Priorities

| Level | Count | Meaning |
|---|---:|---|
| P0 | 0 | soundness holes — none found |
| P1 | 0 | serious runtime defects — none found |
| P2 | 0 | release blockers — none found |
| P3 | 6 | fix before first publish: wrong published-doc claims, one structural inconsistency in the unsafe bridge, one unstated measurement caveat, residual prose volume |
| P4 | 13 | stale comments, test-layout nits, small API ergonomics, minor duplication |

Findings are grouped by the task's six axes; each carries its level.

## Scope and mode

Read in full: `src/lib.rs`, `src/imp.rs`, `Cargo.toml`, `README.md`, `CHANGELOG.md`, all ten files
under `tests/`, all seven `tests/compile_fail/*/` fixtures, `benches/tagged_index_stack_bench.rs`,
`examples/backoff_per_call_latency.rs`, `scripts/tis_p3_ab_runner.mjs` and its three templates.
Cross-checked against the root workspace where the crate's text makes claims about it: the root
`tests/tagged_index_stack_compile_fail.rs` driver, `src/registry/heap_registry.rs`'s `StackStorage`
impl and `src/registry/bootstrap.rs`'s loom shim, the crate's rows in `.github/workflows/ci.yml`,
`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` §3.4 and `docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md`
(every number quoted in `lib.rs` was checked against them — all match), `docs/perf/OPEN_ITEMS.md`
items 61–63, and the correctness tracker cards 141/142.

Run (read-only with respect to the tree; build artifacts under `target/` only):

| Command | Result |
|---|---|
| `cargo clippy -p tagged-index-stack --all-targets -- -D warnings` | clean |
| `RUSTFLAGS="--cfg tagged_index_stack_test" cargo clippy … --all-targets -- -D warnings` | clean |
| `cargo test -p tagged-index-stack` | 36 passed (3 of the 10 integration-test binaries compile to 0 tests — see NONOPT-3) |
| `RUSTFLAGS="--cfg tagged_index_stack_test" cargo test -p tagged-index-stack --release` | 44 passed |
| `RUSTFLAGS="--cfg tagged_index_stack_test" RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items` | clean |
| `RUSTFLAGS="--cfg loom" RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --features loom --document-private-items` | clean |
| `RUSTFLAGS="--cfg loom" cargo test --release --features loom --test loom_aba` | 15 passed, 5 `should_panic` counterfactuals all panicked as expected |
| `cargo package -p tagged-index-stack --list` | 22 files; `scripts/` and `tests/compile_fail/` absent as intended |

Verified mechanically rather than trusted: the "exactly EIGHT" `#[allow(unsafe_code)]` regions in
`src/` (8: `imp.rs:886,1164,1299,1327,1375,1614,1766,1915`), "TEN `unsafe fn`" (10), "SIX `unsafe {}`"
(6: `imp.rs:1336,1347,1357,1479,1624,1777`), "ONE `unsafe trait`" (1), "ZERO `unsafe impl`" (0);
the seal-time arithmetic in `lib.rs:166-183` (2^48/2×10^8 = 16.3 d, 2^48/10^9 = 3.26 d,
2^40/2×10^8 = 91.6 min, 2^32/2×10^8 = 21.5 s); the "~2 × 10^7 pushes/sec" churn receipt
(`_raw_tis_backoff_cap_sweep_run1.log:16`: 18,835,603 iters / 1.015 s = 1.86×10^7 pop+push pairs/s);
every §3.4 figure quoted in `lib.rs:217-231` (worst-pop, tail-band, p99.9 and speedup cells all
present in `TIS_BACKOFF_CAP_SWEEP_GATE.md:375-406`).

---

## 1. Bugs

### BUG-0 — production algorithm: nothing found (recorded so the next round can diff against it)

The reasoning the code relies on, re-derived:

- **Release sequence on `head`** (`imp.rs:409-433` INVARIANT): every modification of `head` is an RMW
  (`push` CAS `Release`/`Relaxed`, `pop` CAS `Acquire`/`Acquire`, `cas_head_for_test`); constructors
  initialise, `raw_head`/`is_empty`/`pushes_remaining` only load. So the release sequence headed by
  any push's `Release` CAS extends through every later pop's `Acquire`-only CAS and every later push,
  and a popper whose `Acquire` load reads a value written by *any* of those RMWs synchronises-with the
  push that wrote the link it is about to follow. This is exactly the C++11/C++20 release-sequence rule
  (RMWs by any thread continue the sequence regardless of their own ordering). Correct, and the only
  thing that would break it — a plain `store` — is forbidden in the same comment.
- **push's `Relaxed` initial load and failure ordering** (`imp.rs:1398,1514`): push consumes the
  observed word only as `(cur_idx, tag)` values and never dereferences a link through it; the
  happens-before edge a popper needs is carried by push's `Release` success CAS, not by anything push
  read. A contract-abiding push cannot observe its own `index` as head (its authority came from a pop
  whose successful RMW moved the head off `index`; coherence forbids reading an older value after
  that), so `next[index] == index` is unreachable without a contract violation and the self-loop
  detector cannot false-positive.
- **Seal** (`imp.rs:1408-1410`): checked before `store_next`, so a first-attempt refusal has no side
  effect (`tests/tag_seal.rs:64-84` pins `raw_head` byte-identity across the refusal); `tag + 1` cannot
  overflow because `tag < TAG_MAX <= 2^63 - 1` at every legal width; two pushers racing at
  `TAG_MAX - 1` resolve correctly (loser's CAS fails on the tag, retries, observes `TAG_MAX`, refuses).
- **H-2** (`imp.rs:1567-1572`): the drain packs the observed tag; `is_empty` masks the index half only;
  the tag therefore never recurs, including across `(empty, t) → (X, t+1) → (empty, t+1)`.
- **Clause-4 guard** (`imp.rs:1563-1566`): `next != TAIL && (next >= mask || next == index)`. `index`
  is always `< mask` on this path (it came from a non-empty head), so the two disjuncts cannot both
  hold and `pop_link_out_of_range`'s message selection is well-defined.
- **`compare_exchange` strong vs weak** and **pop success `Acquire → Relaxed`**: both measured
  codegen-identical on x86-64 and on both aarch64 lowerings (`TIS_LINK_ORDERING_WEAK_CAS_GATE.md`,
  items 63 closed as NULL). Not re-recommended.

The loom suite confirmed all of this at runtime in this review (15/15, every counterfactual panicking
where it should).

### BUG-1 (P3, doc-level) — "Where unsafe lives" miscounts the whole-crate `#[allow(unsafe_code)]` hits

`src/lib.rs:337-347`:

> — run from the workspace root — returns exactly eight hits, ALL in `src/imp.rs` … an unscoped
> whole-crate grep additionally returns **four** statement-scoped allows in the tracked perf
> Link-ordering/CAS A/B tooling (`scripts/tis_p3_ab/harness_bin.rs` and
> `scripts/tis_p3_ab/codegen_wrapper.rs.tmpl`)

Running exactly the command the doc gives (`grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]'`) over the
whole crate returns **19** hits: the 8 in `src/imp.rs` plus **11** in `scripts/` — five in
`codegen_wrapper.rs.tmpl` (lines 56, 82, 103, 117, 131) and six in `harness_bin.rs` (lines 103, 193,
206, 219, 234, 293). "Four" was true at some earlier revision of the templates and was never
re-derived after `0ac19f7`/`e1a1817` grew them. This is the crate's *self-verifying* inventory
paragraph, in published rustdoc, contradicted by its own command — exactly the drift class the
paragraph exists to prevent. Since the `src/`-scoped count is the one that matters and is correct,
the fix is to delete the whole-crate sentence (the scripts are not in the package anyway) rather than
re-hardcode a new number.

### BUG-2 (P4, doc-level) — `retry_counts_for_test` describes an assertion the harness does not make

`src/imp.rs:2091-2093`:

> reads both cumulative CAS-retry counters as `(pop, push)`. The instrumented A/B binary snapshots
> this tuple after warm-up around its separate observed window and **requires both deltas to be
> non-zero**.

Neither consumer requires that. The natural-workload record is validated by
`tis_p3_ab_runner.mjs:303-305`, which asserts `push_retries`/`pop_retries` are safe integers `>= 0`
— zero is accepted. The deterministic activation oracle (`harness_bin.rs:257`) requires
`push_retries == 1 && pop_retries == 0` — i.e. it requires the *pop* delta to be exactly **zero**,
the opposite of the doc. The sentence should say what the harness checks: the deterministic probe
pins `(pop, push) == (0, 1)`; the natural probe records both and requires only
`natural_push_attempts == ops_total + push_retries`.

---

## 2. Performance

### PERF-1 (P3) — the backoff cap is tuned in `spin_loop` units, and that unit is not portable

`src/imp.rs:37-55` presents `BACKOFF_SPIN_CAP = 6` as "a deliberate fairness-vs-throughput compromise
… caps 8/10 give more aggregate throughput but measurably worse per-thread fairness … caps 0/4 are
fairer but slower", and `lib.rs:211-239` reproduces the §3.4 latency trade in fifteen lines of
numbers. Every one of those numbers was measured on one Windows dev host (sweep §1: 16 logical CPUs;
the codegen receipt for the same host names an `11th Gen Intel Core i7-11800H`, i.e. Tiger Lake).

`Backoff::spin` (`imp.rs:88-97`) spins `1 << K` calls of `core::hint::spin_loop()`, up to 64 at the
cap. That hint lowers to `pause` on x86, whose latency Intel's optimisation manual puts at ≈140 cycles
from Skylake onward versus ≈10 cycles on earlier cores, and to `isb` on AArch64 (a pipeline
serialisation, different again). So the maximum per-retry wait at cap 6 is ≈9,000 cycles on the sweep
host but roughly an order of magnitude less on a pre-Skylake Intel, an AMD core, or an ARM core — the
*same* constant yields a backoff schedule that is 10x shorter. The whole measured trade (p99.9 vs
worst-case, the 16-thread `>1 ms` band flip, the 4-5x aggregate speedup) is therefore a property of
`(cap 6, ≈140-cycle pause)`, not of `cap 6`. The doc currently states it as the latter.

This is not a request to change the constant (the sweep is the only evidence there is, and the
arm64 wall-clock job measures link ordering, not the cap). It is a request for one honest sentence
next to `BACKOFF_SPIN_CAP` and in the "Lock-freedom and starvation" section: the schedule is in
hint units; the trade was measured on one x86 microarchitecture with a ≈140-cycle `pause`; consumers
on other cores should expect a different tail shape. If a future sweep is ever run on ARM, this is the
first thing it will find.

### PERF-2 (P4) — `ArrayIndexStack` pays two bounds checks per op that a slot-resident implementor does not

`push` → `index >= INDEX_MASK` guard (`imp.rs:1385`) then `self.next[index as usize]`
(`imp.rs:2008`); `pop` → `self.next[index as usize]` (`imp.rs:1991`). Both are predictable
compare-and-branch and the second is what makes `ArrayLinks` safe to expose as a public type with a
`# Panics` contract, so nothing to change — but the crate's own README positions `ArrayIndexStack` as
"for standalone use" without saying that the production shape (`Registry` with checked `slot()`, or
`UncheckedPool` with `get_unchecked` in `tests/narrow_domain_unchecked_storage.rs`) is where a
domain-proof lets an implementor drop the second check. One sentence in the `ArrayIndexStack` type doc
would set expectations for anyone benchmarking the owned type against a hand-rolled free list.

### PERF-3 — measured-and-rejected / already-tracked items, not re-recommended

- `compare_exchange_weak` (both CAS sites): codegen-identical on x86-64 and on aarch64 outlined and
  `+lse` (`TIS_LINK_ORDERING_WEAK_CAS_GATE.md`; identity asserted by the runner as a negative control).
- pop success `Acquire → Relaxed` (item 63): codegen-identical everywhere, closed NULL 2026-09-06.
- link-cell `Acquire`/`Release → Relaxed` (item 62): real `ldar`/`stlr` delta on aarch64, x86 NULL;
  native arm64 wall-clock still pending — the crate ships with the defence-in-depth orderings and says
  so. Correct posture.
- skip the redundant `store_next` on a tag-only retry (item 61, `store_elided`): instrument-ready,
  x86 codegen +14.5 % instructions in `push_index_impl` (a static count), pending the same arm64 run.
- `BACKOFF_SPIN_CAP` other than 6: swept 0/4/6/8/10; see PERF-1 for the caveat, not a re-sweep.
- push initial load `Acquire → Relaxed`: already landed (`67ea38d`), verified sound above.
- cache-line padding of `StackHead` / `ArrayLinks`: correctly delegated to the embedder
  (`imp.rs:375-389`, `:1936-1948`).
- `#[track_caller]` on `push_index_impl`/`pop_index_impl`: adds an implicit `&Location` argument
  only when the generic body is not inlined into the caller; measured ≈ free in round 9. Accepted.

---

## 3. Code smell

### SMELL-1 (P3) — `SealedStorage` rejects the "my only caller" argument for `store_next` and uses it for `head`/`load_next`

`src/imp.rs:1287-1292` (the trait doc):

> `store_next` is the one member that is an `unsafe fn`: the crate-private bridge forwards it
> verbatim, so the actual safety proof lives at the algorithm's call site in `push_index_impl`, not
> at the bridge — **a "my only caller is `push_index_impl`" privacy argument is not a proof and would
> silently break the next time an in-crate caller appears.**

Then the bridge impl, ten lines later, for the two *safe* members (`imp.rs:1330-1347`):

```rust
fn head(&self) -> &StackHead<B> {
    // SAFETY: ... The stack algorithm calls `head()` exactly once per operation and uses
    // the returned reference only as THIS binding's head — never building a second,
    // competing binding around it — discharging [`StackStorage::head`]'s caller-side contract.
    unsafe { StackStorage::head(self) }
}
fn load_next(&self, index: u32) -> u32 {
    // SAFETY: the pop algorithm calls this only on an index unpacked from a head word
    // observed through THIS binding's `head()`; ...
    unsafe { StackStorage::load_next(self, index) }
}
```

Both proofs *are* the "my only caller is the algorithm" argument the same trait doc says is not a
proof. Today it is true (grep confirms no other in-crate caller of `SealedStorage::head`/`load_next`),
but a safe `pub(crate) fn head()` that any future in-crate code can call with no `unsafe` block is
exactly the "silently break the next time an in-crate caller appears" shape the doc warns about — and
`head()` is the more dangerous of the three, since a second in-crate caller holding the reference
across a call is the clause-1 hazard. Two consistent resolutions: make all three `SealedStorage`
members `unsafe fn` with the proof at the two algorithm call sites (the `store_next` pattern — this
adds two `unsafe {}` blocks and moves the inventory to 8 regions / 8 blocks), or keep them safe and
rewrite `store_next`'s justification to say why *it* is different (it is not: all three hooks are
`unsafe fn` on `StackStorage`, and the argument for keeping any of them safe on the sealed side is
the same privacy argument). The first is the honest one.

### SMELL-2 (P4) — retry-counter hooks are cfg-gated twice

`src/imp.rs:100-113` gate the `note_pop_retry`/`note_push_retry` *definitions* on
`#[cfg(any(tagged_index_stack_test, loom))]`, and the two call sites (`imp.rs:1520-1521`,
`:1593-1594`) gate the *calls* on the identical cfg. Either alone is sufficient; both together mean
the next person to change the cfg predicate has four sites to keep in sync instead of one. The
cheaper shape is an unconditional `#[inline] fn note_push_retry()` whose body is the cfg-gated
`fetch_add` — zero code in the default build, one predicate to maintain, no `#[cfg]` in the hot loop.
(Run-22 P3-3 asked for the gate to be *visible at the call site* so the default MIR provably has no
call; a cfg on the call alone satisfies that — the duplicate on the fn does not add anything.)

### SMELL-3 (P4) — one predicate, two spellings, four lines apart

`src/imp.rs:1403,1419`:

```rust
let (cur_idx, tag) = TaggedIndex::<B>::unpack(head);
...
let next_link = if TaggedIndex::<B>::is_empty(head) { TAIL } else { cur_idx };
```

`is_empty(head)` is `(head & INDEX_MASK) == INDEX_MASK`, and `cur_idx` is `(head & INDEX_MASK) as
u32` — the same mask already computed. `if cur_idx == TaggedIndex::<B>::empty_index()` says the
relationship the 8-line comment above it (`imp.rs:1411-1418`) spends its words explaining ("On an
empty head the observed index half IS the sentinel"). The compiler CSEs the AND either way; this is
about the reader.

### SMELL-4 (P4) — three test accessors for two counters

`pop_retry_count_for_test` (`imp.rs:2055`), `push_retry_count_for_test` (`:2087`) and
`retry_counts_for_test` (`:2103`, returns the pair) all read the same two statics. The loom suite
uses the first two; only the scratch harness template uses the third. One accessor returning the pair
serves both; the loom `model_with_oracle` snapshots already take a closure and would take
`|| retry_counts_for_test().0` unchanged.

### SMELL-5 (P4) — `ArrayIndexStack::push` out-of-`N` panics with rustc's slice message, not the crate's

`imp.rs:1385-1387` routes the `index >= INDEX_MASK` violation through a `#[cold]` helper with a
crate-owned message and `#[track_caller]` chaining, but the *more likely* mistake on the owned type —
`index >= N` with `N < INDEX_MASK`, e.g. `ArrayIndexStack::<16, 64>::push(64)` — lands in
`ArrayLinks::store_next`'s `self.next[index as usize]` and panics with `index out of bounds: the len
is 64 but the index is 64` at `imp.rs:2008`, with no track-caller chain (the `# Panics` sections at
`imp.rs:1155-1162` and `:1982-1988` document this honestly). A one-line `if index as usize >= N {
links_out_of_range(index, N) }` in `ArrayLinks::store_next`/`load_next` with the same cold-helper
shape would give the owned type one panic vocabulary. Ergonomics only.

---

## 4. "Neuroslop"

The tree is materially better than run-21 found it — the review-round archaeology is gone from
`src/` (zero `P[0-9]-[0-9]`/`run-N`/`Sol-codex` hits in `src/`, `tests/`, `benches/`, `examples/`),
CHANGELOG is 39 lines, README leads with the API. What remains is *structural* repetition: one fact,
several statements, each pointing at the others. Concrete instances:

### SLOP-1 (P3) — the retry-overwrite proof is written twice in the same 60-line block, the second copy labelled "not repeated here"

`src/imp.rs:1424-1455` is a 32-line comment deriving why a stale write from a failed push iteration
is never observable ("for two disjoint reasons — the normative retry-overwrite proof (the SAFETY
comment below cross-references it instead of repeating)"). The SAFETY comment below, `imp.rs:1457-1478`
(22 lines), then reads:

> the failed iteration's stale write is never observable in the stack's read-set (normative two-case
> proof — unreachable-before-publication, and a prior-cycle stale popper whose CAS expectation is
> already permanently displaced — on the retry-store comment directly above, **not repeated here**);

— which is the proof, repeated. A SAFETY comment needs the *precondition discharge* ("`next_link` is
`TAIL` or the observed head; the publishing CAS follows; domain/liveness/authority forwarded from the
caller") — four lines. The mechanism belongs once, in the block above it.

### SLOP-2 (P3) — the hot-loop CAS comment discusses a different candidate's measurement status

`src/imp.rs:1501-1513`, inside `push_index_impl`'s loop:

> Strong `compare_exchange`, deliberately NOT `compare_exchange_weak`: measured equivalent on x86-64,
> and `weak` codegen-IDENTICAL to strong on aarch64 under both the outlined-atomics default and the
> `+lse` lowerings (multi-target link-ordering/CAS A/B harness, `scripts/tis_p3_ab_runner.mjs`) —
> inline-LL/SC spurious-failure win does not exist on this toolchain. See
> `docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md` current codegen oracle; the driver asserts identity,
> so a toolchain change fails loudly. This concerns the CAS KIND only; the separate LINK-ordering
> relaxation's native AArch64 wall-clock cost remains unmeasured — its static multi-target A/B codegen
> comparison IS done (a real `ldar`/`stlr` delta exists) — see `StackStorage`'s "Ordering contract".

Thirteen lines, the last five about link ordering (a `StackStorage` concern) in a comment on the head
CAS. "Strong, not weak: codegen-identical on every measured lowering — see
`TIS_LINK_ORDERING_WEAK_CAS_GATE.md`" is the whole content. Pop's twin at `imp.rs:1584-1586` already
has that shorter form.

### SLOP-3 (P3) — "Where unsafe lives" states the boundary-vs-contents distinction four times

`src/lib.rs:255-361` (107 lines of published crate-root rustdoc). The single idea "an
`#[allow(unsafe_code)]` count is a boundary count, not a count of unsafe operations inside the
regions" is stated at `:260-270`, again at `:306-309` ("re-derived by grepping … not by counting …
regions — see the boundary-vs-contents distinction below"), again at `:320-331` ("a region count is a
boundary check, not a contents check, and must not be read as one"), and again at `:356-361` ("a
content CHECKLIST to audit against, not a self-checking assertion the way the region-boundary command
is"). A crates.io reader needs: the eight regions listed by role (`:272-289`, keep), the two grep
commands (keep), and one sentence on why there are two. ~40 lines. The rest of the section — and
BUG-1's wrong count — is what happens when the same paragraph is edited by four rounds of reviewers.

### SLOP-4 (P4) — clause 3's worked counterexample exists in three places

`src/imp.rs:1049-1076` (28 lines: threads A and B, the retry, `next[index] = index`, the test names),
`tests/loom_aba.rs:960-1024` (the same walk-through as the test's doc, 65 lines including a 30-line
"Why the retry gate" derivation), and `README.md:168-185`. The rustdoc copy is the normative one and
should keep a *four-line* version; the test doc is the right home for the schedule-gating derivation;
the README needs the one-sentence rule and the test name.

### SLOP-5 (P4) — the `TAIL` doc argues with a reader who is not there

`src/imp.rs:20-35`:

> (their low bits do agree — `TAIL & INDEX_MASK == INDEX_MASK` is a mathematical identity at every
> legal width, all-ones AND all-ones-low-bits — but their ROLES are distinct: an empty head's index
> half is the sentinel `INDEX_MASK`, NOT `TAIL`). The mappings between the two in `push_index` /
> `pop_index` are therefore REQUIRED, not a readability choice: the sentinels' values differ, so each
> site must map the sentinel it observes to the other by an explicit branch.

This is run-21 BUG-2's *correction*, and it over-corrected into a rebuttal of the sentence it
replaced. "`TAIL` (`u32::MAX`) terminates a link chain; the head's empty sentinel is `INDEX_MASK`
(`<= 0xFFFF`); push and pop translate between the two" is the fact.

### SLOP-6 (P4) — the two-cause self-loop disjunction, four homes

`pop_index`'s `# Panics` (`imp.rs:1219-1243`, 25 lines), the in-loop comment (`:1556-1562`, "not
restated here"), `pop_link_out_of_range`'s doc (`:1653-1658`, "not restated here"), and the trait doc
(`:807-816`). Three of the four say they are not restating it and then summarise it. One normative
home (`# Panics`) plus bare pointers is enough.

### SLOP-7 (P4) — "a repository file, not part of the published package" ×11

`lib.rs:188,237`, `imp.rs:51,679,709,885`, `README.md:209,227`,
`examples/backoff_per_call_latency.rs:3-4,39,78`. State the convention once in the crate-root doc
("paths under `docs/` are repository files, not in the package") and drop the parentheticals.

### SLOP-8 (P4) — test/bench doc comments that narrate their own bodies

- `tests/loom_aba.rs:203-223` (`model_with_oracle`): 21 lines describing the order
  "acquire lock → snapshot → check → snapshot → verify → drop lock" of a 10-line function whose body
  is exactly that order.
- `tests/custom_storage_impl.rs:1-70`: a 70-line module doc that re-lists every test with its status
  ("this module doc is the source of truth for that per-test status list") — the test names and their
  `#[should_panic]` attributes already are that list.
- `benches/tagged_index_stack_bench.rs:103-131` (`run_contention_phase`): the published-window
  protocol in 29 lines above a function whose two barriers and one `OnceLock` make it legible in 15.

---

## 5. Inaccuracies

### INACC-1 (P3) — the loom module doc misclassifies the tiny-tag seal model, and it is the declared source of truth

`tests/loom_aba.rs:5-17`:

> **seven** models (`pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type`,
> `push_push_conservation`, `counterfactual_same_index_concurrent_push_self_loops`,
> `pop_repush_after_publish_conserves`, `pop_pop_conservation`,
> `pop_pop_single_element_loser_sees_empty_actual`,
> `tiny_tag_seal_rejects_stale_cas_at_the_real_width`) run end-to-end through ArrayIndexStack's
> shipped `push`/`pop` **for their whole schedule** (the eighth,
> `counterfactual_bypassed_seal_lets_stale_cas_double_issue`, runs the shipped `push`/`pop` for
> everything except its one deliberately bypassed final step …

`tiny_tag_seal_rejects_stale_cas_at_the_real_width` does not. Its thread P (`loom_aba.rs:1512-1534`)
is a hand-inlined pop: `stack.raw_head()`, `stack.load_next_for_test(p_idx)`, then
`stack.cas_head_for_test(p_head, p_new_head, Acquire, Acquire)` — the same split-pop shape as
`aba_repush_keeps_free_list_conservation`. Only thread Q drives the shipped `push`/`pop`. The same is
therefore true of the "eighth" (its P is identical). Six models, not seven, run the shipped API for
their whole schedule; two more run it on one side. This matters beyond a count: `lib.rs:241-253` and
`README.md:247-259` both say "See `tests/loom_aba.rs`'s own module doc for the per-model breakdown"
and the module doc says of itself "This module doc is the source of truth for this per-model
breakdown; other published copies … point back here rather than repeating a specific count." The
one place allowed to carry the count carries the wrong one.

### INACC-2 (P3) — hazard shape 1 is attributed to three different clause sets in three places

- `src/imp.rs:788-790` (trait doc, normative): "Shape 1 VIOLATES per-implementor **clauses (3 and 4)**".
- `tests/custom_storage_impl.rs:424-428` (test doc): "implementor-enforced (the `StackStorage` trait
  doc's `# Safety` **clauses 3 and 4**)".
- `tests/custom_storage_impl.rs:450-452` (the `unsafe impl`'s SAFETY comment, i.e. the formal
  assertion): "DELIBERATE contract violation — **clause 2** (one backing, consistently): load_next and
  store_next read and write DIFFERENT backings."
- `tests/custom_storage_impl.rs:59` (module doc status list): "`internally_disagreeing_storage_still_double_issue` (**clause 2**; guard fires)".

Clause 2 is "One backing, consistently. `load_next` and `store_next` must read and write the same link
storage" (`imp.rs:622-624`) — the shape's exact definition. Clause 3 is about *two bindings* sharing
cells; shape 1 has one binding. Clause 4 ("valid answers") is a *consequence* (the foreign backing
answers 0), not the violated obligation. The trait doc and the test's own rustdoc should say clause 2,
as the SAFETY comment already does. (This is the residue of cycle-23 P3-4, which fixed the domain
numbers in these SAFETY comments but not the clause numbers around them.)

### INACC-3 (P4) — `threaded_conservation.rs` cites an oracle the file no longer contains

`tests/threaded_conservation.rs:67-72`, on the start barrier:

> … collapsing what should be 8-way real contention into several near-sequential runs with little to
> no overlap. **That is A staggered start** against a shared stack still conserves the free-list (no
> thread ever needs a SECOND concurrent writer to stay correct), so **only the oracle** -- not the
> conservation check -- can tell "ran without contention" apart from "the retry path is broken".

The activation oracle (`retry_counts_for_test` delta, cap-reach counters) was removed from this file
in `76d4e5d` when the backoff oracle became deterministic; the file now contains exactly one
conservation assertion and no oracle. The sentence "That is A staggered start …" is also a splice
artefact of that edit (two sentences fused mid-clause). The barrier is still worth keeping — it makes
the *conservation* run represent contention — but the comment should say that, in one sentence.

### INACC-4 (P4) — `imp.rs` cites a test file that is not in this crate

`src/imp.rs:763` and `:783`: "pinned by `tests/tagged_index_stack_compile_fail.rs`". That file is the
*workspace root's* `tests/tagged_index_stack_compile_fail.rs`; this crate's `tests/` has no such file,
and the fixtures it drives (`tests/compile_fail/`) are excluded from the package. Every other citation
of it in the tree says "root `tests/…`" (e.g. `tests/stack_unit.rs:308`, all seven fixture docs); the
two rustdoc sites should too, with the repository-file caveat.

### INACC-5 (P4) — README's probe inventory misfiles one probe and omits another

`README.md:290-294`: "read-only/counter test probes (`raw_head`, `load_next_for_test`,
`retry_counts_for_test`, retry-counter accessors, and `backoff_spin_depths_for_test`) compile under
`tagged_index_stack_test` or `loom`; the raw CAS/write probes (`cas_head_for_test`,
`store_next_for_test`) remain loom-only." `backoff_spin_count_for_test` (`imp.rs:2060-2065`) is a
counter probe that is loom-only (`#[cfg(loom)]`), and `with_tag_for_test` (`imp.rs:523-533`,
`:1893-1901`), which *constructs* a head, is neither listed nor obviously "read-only". The list should
be derived from the `#[doc(hidden)]` items, or dropped from the README (a consumer cannot call any of
them).

### INACC-6 (P4) — `CHANGELOG.md:22` / README: "read-only/counter probes … absent from default builds entirely" is true, but three packaged test binaries are then empty

Not a false statement — see NONOPT-3 for the consequence.

---

## 6. Non-optimalities

### NONOPT-1 (P4) — `ArrayIndexStack<B, N>` has no compile-time `N <= INDEX_MASK` relation

`ArrayIndexStack::<4, 1024>` compiles, exposes 15 usable indices, and wastes 1,009 cells;
`ArrayIndexStack::<16, 1 << 20>` compiles and can never address cells above 65,534. The crate already
has the `_CHECK_BITS` post-monomorphisation pattern; a `const _CHECK_N: () = assert!(N as u64 <=
TaggedIndex::<B>::INDEX_MASK, …)` forced from `new()` would reject both at compile time for free. The
`N` vs `INDEX_BITS` independence is documented three times (`imp.rs:1155-1162`, `:1982-1988`,
`:2001-2006`); enforcing the only sensible direction of it would let two of those paragraphs go. A
semantic tightening, so a pre-publication decision — after 0.1.0 it is a breaking change.

### NONOPT-2 (P4) — a runtime test of a compile-time constant

`tests/stack_unit.rs:170-180` `max_legal_width_index_mask_never_equals_tail` asserts
`TaggedIndex::<16>::INDEX_MASK == 0xFFFF` and `0xFFFF != u32::MAX as u64`. Both are literals; the test
cannot fail without the `_CHECK_BITS` fixture (`index_bits_seventeen`) failing first. Either delete it
or make it a `const _: () = assert!(…)` next to the `Send + Sync` pin at `:40-48`, where it costs no
test-runner time and fails at compile time.

### NONOPT-3 (P4) — three of the ten packaged integration-test binaries run zero tests for a downstream `cargo test`

In the default build `tests/loom_aba.rs`, `tests/backoff_oracle.rs` and `tests/tag_seal.rs` all
compile to `0 passed` (verified: "test result: ok. 0 passed" ×3 in the default run above). The loom
one is expected. The other two mean the crate's *headline property* — the seal — has no test a
crates.io consumer can run without knowing the undocumented `--cfg tagged_index_stack_test`. That is
a consequence of `with_tag_for_test` being the only way to reach `TAG_MAX` in finite time, so it is
structural, but it is worth one sentence in the README's test-probe paragraph ("`cargo test` in the
package skips the seal and backoff oracles; CI runs them under the repository cfg") so the empty
binaries do not read as broken.

### NONOPT-4 (P4) — `[lints.clippy] incompatible-msrv = "allow"` is crate-wide, so the library is not MSRV-linted locally

`Cargo.toml:35-41` allows the lint for the whole crate because dev-only tests use a 1.81 API
(`PanicHookInfo::location` through type inference in `push_guard_track_caller.rs:64-66`). The
library's MSRV floor is therefore enforced only by CI's pinned `cargo +1.79 check` rows
(`ci.yml:2247-2252`), never by a local clippy run. Acceptable given those rows exist; the manifest
comment should say "the library target is not MSRV-linted by clippy" rather than only "the floor's
real oracle is the msrv job".

### NONOPT-5 (P4) — round-trip / empty-sentinel coverage at width 16 is still four tests

`tests/stack_unit.rs`: `pack_unpack_round_trip_16` (`:55-68`), `empty_sentinel_16` (`:124-139`),
`empty_sentinel_never_collides_with_a_live_index` (`:196-228`, with a `const CAP: u32 = 4096`
"representative pool cap" inherited from the allocator that has no meaning in this crate), and
`empty_word_with_running_tag_reads_empty_through_tag_max` (`:233-243`) all pack `(idx, tag)` at width
16, unpack, and check `is_empty`. Run-21 NONOPT-3 collapsed the *rejection* boundary tests; the
*acceptance* side still overlaps. One table-driven test (the `:84-121` shape) plus the proptests is
complete.

### NONOPT-6 (P4) — the `tag_seal.rs`/`stack_unit.rs` per-test cfg includes a dead `loom` arm

Both files are `#![cfg(not(loom))]` (`tag_seal.rs:15`, `stack_unit.rs:25`) and then gate individual
tests on `#[cfg(any(tagged_index_stack_test, loom))]` (`tag_seal.rs:18,35,117`; `stack_unit.rs:366,
409,484,518,530`). Inside a `not(loom)` file the `loom` disjunct can never be true. Harmless, but it
reads as if the tests could run under loom; `#[cfg(tagged_index_stack_test)]` is the true predicate.

### NONOPT-7 (P4) — the `--mode summary` default target names a CSV that does not exist

`scripts/tis_p3_ab_runner.mjs:1830` `WALLCLOCK_CSV_TARGET = 'x86_64-pc-windows-msvc'` is the
fallback when `--target` is omitted, but `docs/perf/` holds no wallclock CSV at all today (`ls
docs/perf | grep TIS_` → two codegen CSVs, no wallclock, no summary), so a bare `--mode summary` can
only fail closed with "required artifact missing". Since the only caller (`ci.yml:3008`) always passes
`--target`, the default is dead configuration; make `--target` required for summary mode and delete
the constant and its two comment paragraphs (`:1826-1829`, `:2154-2157`).

### NONOPT-8 — out-of-scope observation, recorded only

`docs/correctness-open-items/TRACKED_publish_readiness.md` card 142 (the cycle-23 review) carries
`Status: OPEN` while its own first sentence says all four P3 findings are closed; per CLAUDE.md's
current-state rule the card should read CLOSED with a pointer, or state what specifically remains
open (the arm64 dispatch, which is item 62's trigger, not a cycle-23 finding). Not a crate file; not
counted above.

---

## Prior-round items verified closed at this revision

- Cycle-23 P3-1 (README "at a time"): `README.md:78-83` now says "for its whole life … must never be
  rebound to different link storage across time". Closed.
- Cycle-23 P3-2 (README "no detector exists"): `README.md:155-160` now says "no FULL runtime detector
  … catches the current-head double-push … misses a deeper-than-head re-push". Closed.
- Cycle-23 P3-3 (scheduler-dependent backoff-depth oracle): `tests/backoff_oracle.rs` pins
  `[1,2,4,8,16,32,64,64,64]` deterministically through `backoff_spin_depths_for_test`;
  `Backoff::spin` returns `()`. Closed.
- Cycle-23 P3-4 (wrong link domains in SAFETY comments): `custom_storage_impl.rs:125,219,388,525,613`
  now name `0..8` / `0..64`. Closed (but see INACC-2 for the clause numbers next to them).
- Run-22 P3-3 (instrumentation in default MIR): `note_*` calls cfg-gated at the call site
  (`imp.rs:1520,1593`). Closed (see SMELL-2 for the double gate).
- Run-22 P3-4 (`TaggedIndex::empty()` hidden-pub): now private `bootstrap_empty` (`imp.rs:306`); the
  registry loom shim builds its word from `pack(empty_index(), 0)` (`bootstrap.rs:562-570`). Closed.
- Run-22 P4-4 (`test-internals` feature): removed in `7e5ea47`; the repository cfg is declared in
  `[lints.rust] unexpected_cfgs` (`Cargo.toml:32-33`). Closed.
- Run-21 BUG-1/BUG-2 (`Backoff` overflow mode; "purely for readability"): both corrected
  (`imp.rs:81-86`, `:1411-1418`). Closed (see SLOP-5 for the over-correction).
- Run-21 SMELL-3 (Node driver in the tarball): `cargo package --list` shows no `scripts/`. Closed.

## Closing summary

**Publishable on correctness.** The lock-free core, its orderings, the seal, the unsafe boundary and
the test infrastructure hold up to a fresh derivation and to actually running every gate the crate
defines; nothing in `src/` needs to change before `cargo publish`.

**Do before publishing** (P3): delete or correct the whole-crate `#[allow]` count in "Where unsafe
lives" (BUG-1) and, while there, cut the section to the eight regions plus the two commands (SLOP-3);
fix the loom module doc's "seven models" claim, since three other documents defer to it (INACC-1);
make the shape-1 clause attribution consistent across the trait doc and the test (INACC-2); resolve
the `SealedStorage` asymmetry by making all three sealed hooks `unsafe fn` (SMELL-1); add the
one-sentence microarchitecture caveat to the backoff cap's documented trade (PERF-1); and remove the
duplicated proofs that the code marks "not repeated here" while repeating them (SLOP-1, SLOP-2).

**Can wait** (P4): the `retry_counts_for_test` doc, the stale `threaded_conservation` comment, the
root-driver path citations, the README probe list, the compile-time `N <= INDEX_MASK` relation, the
constant-assert test, the dead `loom` cfg arms, the summary-mode default target, and the remaining
test-doc narration.
