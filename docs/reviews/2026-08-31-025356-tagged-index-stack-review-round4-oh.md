# `tagged-index-stack` — independent publish-readiness review, round 4

- **Reviewer:** Claude (Opus 5), adversarial pass. Prior review docs read only *after* forming
  my own findings, then used to check overlap and to see what a prior round papered over.
- **Date:** 2026-08-31 02:53:56 +0200 (CEST)
- **Revision reviewed:** `8027d9a4375b37cfac118a7823fc162e4b815366` (landing SHA on `main`;
  working tree clean w.r.t. `crates/tagged-index-stack/`, `.github/workflows/ci.yml`,
  `scripts/loom.mjs`)
- **Scope:** `crates/tagged-index-stack/**` (src, tests, benches, README, CHANGELOG, Cargo.toml),
  `.github/workflows/ci.yml` rows for this crate, `scripts/loom.mjs`, the in-tree consumers
  `src/registry/heap_registry.rs` / `src/registry/bootstrap.rs`, and
  `docs/correctness-open-items/` entries naming this crate.
- **Verification actually performed** (every number below was produced by running something,
  not read off a comment):
  - `cargo test -p tagged-index-stack --no-fail-fast` — **25 tests green** (15 `stack_unit`,
    5 `proptest_pack_unpack`, 4 `regression_counter_wrap`, 1 `readme_example`; `loom_aba`
    correctly compiles to 0 tests; 0 doctests).
  - `RUSTFLAGS="--cfg loom" cargo test --release -p tagged-index-stack --features loom
    --test loom_aba` — **8 tests green in 0.11 s**, all three `#[should_panic]`
    counterfactuals included.
  - `cargo clippy -p tagged-index-stack --all-targets -- -D warnings` — clean.
  - `RUSTDOCFLAGS="-D warnings" cargo doc -p tagged-index-stack --no-deps` — clean.
  - `cargo package --list -p tagged-index-stack` → 15 files, no strays;
    `cargo package --no-verify` + reading the **generated** `Cargo.toml` out of the
    `.crate` tarball (workspace `[lints]` and the optional `loom = { version = "0.7",
    optional = true }` both inline correctly; `[dependencies]` is empty).
  - `cargo bench -p tagged-index-stack --bench tagged_index_stack_bench` — real numbers,
    quoted in P3-1 / P3-2 below.
  - **Two out-of-tree scratch A/B experiments** (copies of `src/lib.rs` + the bench outside
    the workspace, so nothing in the repo was touched) measuring the two perf hypotheses this
    round could actually test on x86-64 — see "Considered and rejected", items 1 and 2.
  - **One out-of-tree counterfactual experiment** proving P2-1 (a deliberately neutered loom
    model that still passes). Deleted afterwards.
  - Reproduction of the two `scripts/loom.mjs` invocations that are now broken (P2-2), with
    the exact compiler output.
- **Machine / toolchain for every measured number:** `rustc 1.97.0 (2d8144b78 2026-07-07)`,
  LLVM 22.1.6, host `x86_64-pc-windows-msvc`, 11th Gen Intel Core i7-11800H (8 cores /
  16 threads, 2.30 GHz base), Windows 10 Pro 19045. Same machine as the round-3 review.
- **No code was changed.** This is a read-only review.

---

## Overall verdict: **CONDITIONAL-GO**

**The shipping algorithm is correct.** I re-derived every load-bearing claim rather than
trusting the (extensive) comments:

- **Tag monotonicity holds along the head's modification order.** Every successful `push` does
  `wrapping_add(1)`; every successful `pop` preserves the tag, *including* across the
  drain-to-empty transition. So the head word `(X, t)` can only recur after a full
  `2^TAG_BITS` wrap whose last push re-installs `X` — the documented bound. `pack`'s
  `tag << INDEX_BITS` at `tag == 2^TAG_BITS` is well-defined in every profile (Rust's `<<`
  traps on shift *amount*, never on lost bits).
- **`pop`'s `Acquire`-without-`Release` success ordering is sound**, for the reason the `head`
  field's INVARIANT states. I re-verified the premise independently: `grep` finds no plain
  `store` to `head` anywhere; `new` is initialization, `raw_head`/`is_empty` only load, and
  all three writers (push's CAS, pop's CAS, the loom-only `cas_head_for_test`) are RMWs. Under
  the C++20 release-sequence rule the sequence headed by a push's `Release` CAS therefore
  extends through every later head modification, so a two-deep chain (P0 pushes A, P1 pushes
  B→A, a popper pops B then A) still forms the happens-before edge back to each link's writer.
- **`push`'s `Relaxed` CAS-failure ordering is sound** — push never dereferences anything
  reached through the head; it uses the index half as a *value* to store and the tag half as a
  *number* to bump.
- **`_CHECK_BITS` really is forced from every public associated item.** I traced each one
  (`pack`/`try_pack` force it directly; `INDEX_MASK`/`TAG_BITS` evaluate it in their own
  initializers; `unpack`/`empty_index`/`is_empty`/`empty` route through `INDEX_MASK` or
  `pack`), and `TaggedIndexStack::new`/`is_empty`/`push`/`pop` all reach it transitively.
- **Width 1 and width 16 are both safe end-to-end**, and `INDEX_MASK <= 0xFFFF` at every legal
  width, so `index < INDEX_MASK` genuinely implies `index != TAIL`.
- **The flagship consumer is clean.** `RegistryLinks` (`src/registry/heap_registry.rs:566-599`)
  uses a **dedicated** `next_free: AtomicU32` field per slot — not storage aliased with slot
  payload — so it satisfies the crate's rule-4 contract and cannot trip `pop`'s new
  `debug_assert!`. (That distinction turns out to matter for consumers generally — see P3-5.)

What holds this back from an unconditional GO is **two P2s, neither of them a bug in the
stack**, and both introduced *by round 3's own remediation*:

1. **P2-1** — the activation oracle round 3 added specifically to stop the loom suite passing
   vacuously **does not work under the test-harness configuration CI actually runs**. I proved
   this with a neutered model, not by argument: a model in which `pop`'s retry branch is
   structurally unreachable still reports `8 passed` under the default harness, and only fails
   with `--test-threads=1`.
2. **P2-2** — `scripts/loom.mjs` (`npm run loom`, the documented local mirror of the CI loom
   matrix) is **broken for 5 of its 16 models** by the same optional-dependency change that
   broke `loom-misc` in CI. `8027d9a` fixed the CI half; the script half was missed. Its own
   header comment says the map "MUST mirror the ci.yml `loom` matrix" — it no longer does.

Beyond those: the P3 block is again unusually substantive for a fourth round, and its centre of
gravity is that **round 3's remediation introduced at least six new drift instances** (P2-2,
P3-3, P3-4, P4-1, P4-2, and the in-code over-attribution in P3-3). That matters procedurally:
round 3's P3-6 (the 73 %-comment / neuroslop finding) was declined with an explicit, recorded
revisit condition — *"Revisit if a future round finds NEW drift instances — that would be
empirical evidence a restructuring is warranted, not assumed preemptively"* (commit `ab4497f`).
**That condition has now been met.** See P3-7.

**Perf: nothing actionable, and this time that is a measured result rather than an assumption.**
I A/B-tested the two hypotheses that could plausibly show on x86-64 (`#[track_caller]` on the
hot `push`; `#[inline]` on `push`/`pop`) with interleaved runs out-of-tree. Both are inside
noise. Details and raw numbers are in "Considered and rejected".

---

## P2 — should fix before publish

### P2-1. The loom activation oracle is defeated by libtest's default parallelism — proved with a neutered model that still passes

**Location:** `crates/tagged-index-stack/tests/loom_aba.rs:537-587` (the test and its
`retries_before` delta), `crates/tagged-index-stack/src/lib.rs:994-1033` (`POP_RETRY_COUNT` /
`pop_retry_count_for_test` and their docs), `.github/workflows/ci.yml:2469-2472` (the CI
invocation), `crates/tagged-index-stack/README.md:149-151` (the documented command).

Round 3's P2-2 fix added a process-global retry counter and this assertion:

```rust
let retries_before = tagged_index_stack::pop_retry_count_for_test();
// ... builder.check(...) ...
let retried = tagged_index_stack::pop_retry_count_for_test() - retries_before;
assert!(retried > 0, "activation oracle: `pop`'s CAS-retry branch was never reached ...");
```

with this justification in the test's own comment (`:539-543`):

> "A DELTA, not the raw count: other tests in this binary also drive the real `pop` (whose retry
> arm increments the same counter), so only the increment this test's own `check()` run
> produces is this test's own."

**That sentence is false, and its falseness is the whole defect.** `POP_RETRY_COUNT` is a
`core::sync::atomic::AtomicUsize` — deliberately a *real* static so it survives loom's re-runs
— and libtest runs the eight tests in this binary **in parallel** by default
(`--test-threads` defaults to available parallelism; the observed completion order in a plain
run is non-alphabetical, confirming it). At least two other tests in the same file drive the
real `pop` **under contention** and therefore hit the same retry arm:
`aba_repush_keeps_free_list_conservation` (`:125-129`) and
`tagged_stack_survives_the_same_resurrection_pattern` (`:336-351`). A delta measured over a
wall-clock window is not exclusive to its holder; it includes whatever those concurrent tests
increment. The crate's own rustdoc even *states* the contamination — `src/lib.rs:1025-1027`:
"across concurrently running test functions" — while the test 40 lines away asserts the
opposite.

**Failure scenario, executed rather than argued.** I copied the crate and the loom suite to a
scratch package outside the workspace and applied one change to
`pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type`: move thread B's
`push(&links, 0)` to *before* thread A is spawned, so A's `pop` is uncontended and **`pop`'s
CAS-retry branch is structurally unreachable in that model**. The free-list conservation
assertion still holds (A pops one index, the drain yields the other), so the only thing that
should catch this is the oracle. Results:

```
$ RUSTFLAGS="--cfg loom" cargo test --release --features loom --test loom_aba
test result: ok. 8 passed; 0 failed; ...          <-- oracle did NOT fire

$ RUSTFLAGS="--cfg loom" cargo test --release --features loom --test loom_aba -- --test-threads=1
---- pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type stdout ----
panicked at tests\loom_aba.rs:579:5:
activation oracle: `pop`'s CAS-retry branch was never reached in any explored schedule
 — this test is vacuously green, ...
test result: FAILED. 7 passed; 1 failed; ...      <-- oracle fires correctly
```

CI runs the first form (`.github/workflows/ci.yml:2470` passes no `--test-threads`), as does
the command README publishes. So the oracle is armed only in a configuration nobody runs.

**Why this is P2 and not P4.** It is not merely a weak test; it is a *shipped claim that is
false*. `pop_retry_count_for_test`'s public (`#[doc(hidden)]`, but source-visible and
tarball-shipped) doc says the suite "asserts this counter ADVANCES across an exploration so a
model whose schedules never actually reach `pop`'s retry path fails loudly instead of passing
vacuously." Under CI's own invocation it does not. And this oracle is currently the *only*
live guard on `pop`'s `Acquire` CAS-failure ordering against the real `pop` — the
`#[should_panic]` counterfactual (`counterfactual_relaxed_cas_failure_corrupts_free_list`)
drives a hand-expanded harness with a parameterised ordering, not `pop` itself (see P3-3).
Round 3 replaced an unreproducible prose receipt with this oracle precisely to close that gap;
the gap is still open.

**Fix — small, and independent of `--test-threads`.** Add a file-local
`static MODEL_LOCK: std::sync::Mutex<()>` and take it at the top of every test in
`loom_aba.rs` that drives the real `pop` (there are four), so the counter delta is exclusive
to the holder:

```rust
static MODEL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
// in each test: `let _g = MODEL_LOCK.lock().unwrap_or_else(|e| e.into_inner());`
```

`unwrap_or_else(into_inner)` matters because three of the tests are `#[should_panic]` and would
otherwise poison the mutex. The whole suite runs in 0.11 s, so full serialization costs
nothing. Adding `-- --test-threads=1` to the CI step is a weaker alternative: it fixes CI but
leaves a plain local `cargo test` running the defeated configuration, which is exactly how this
regression got in.

### P2-2. `scripts/loom.mjs` was not updated alongside `8027d9a` — `npm run loom` is broken for 5 of its 16 models

**Location:** `scripts/loom.mjs:37` (`loom_aba`), `scripts/loom.mjs:47, 63, 65, 67` (the
`alloc-global,alloc-xthread` group), against the already-fixed
`.github/workflows/ci.yml:2636` and `:2470`.

The user's brief asked whether the recently-fixed `loom-misc` omission had analogues elsewhere.
It does — in the local sweep script, whose own header (`scripts/loom.mjs:13`) states the
invariant it now violates:

> "Per-test feature sets — **MUST mirror the ci.yml `loom` matrix**."

Since round 3 made `loom` an optional dependency (`3a426e9`), `--cfg loom` alone no longer
resolves it and the crate's own `compile_error!` fires. `scripts/loom.mjs` builds its cargo
invocation as `['test','--release', ...scopeArgs, ...testArgs]` with
`RUSTFLAGS=... --cfg loom` (`:131-138`) and never adds `--features loom` /
`tagged-index-stack/loom`. Both affected groups reproduce, verbatim:

```
$ RUSTFLAGS="--cfg loom" cargo test --release -p tagged-index-stack --test loom_aba
error: building with --cfg loom requires --features loom (loom is now an optional dependency)
   --> crates\tagged-index-stack\src\lib.rs:203:1
error[E0433]: cannot find module or crate `loom` in this scope

$ RUSTFLAGS="--cfg loom" cargo check --release --features "alloc-global,alloc-xthread" --tests
error: building with --cfg loom requires --features loom (loom is now an optional dependency)
error[E0433]: cannot find module or crate `loom` in this scope
error: could not compile `tagged-index-stack` (lib) due to 2 previous errors
```

That is `loom_aba` plus `loom_magazine_ring_compose`, `loom_overflow_first_retry`,
`loom_heap_overflow`, `loom_heap_overflow_drain_guard` — **5 of the 16 entries in `FEATURES`**,
including every model of the shipping `HeapOverflow` ring and overflow-first composition. A
developer running `npm run loom` before a push gets a compile error for a third of the sweep;
the script's own 0-tests-ran guard (`:142-146`) does not even get to run, since cargo fails
first.

**Fix.** Two entries: `loom_aba: `${CRATE_PREFIX}tagged-index-stack``'s crate branch must add
`--features loom` (`:126-130`), and the `alloc-global,alloc-xthread` feature string must
become `alloc-global,alloc-xthread,tagged-index-stack/loom` — matching `ci.yml:2636` exactly.
Worth also adding a one-line comment at `:13` pointing at `ci.yml:2636` as the specific row to
diff against, since "mirror the matrix" is the invariant that just failed silently.

---

## P3 — worth fixing, not blocking

### P3-1. Both single-threaded bench rows measure the *same* code path; the ordinary (non-empty) push/pop path is not benchmarked at all

**Location:** `crates/tagged-index-stack/benches/tagged_index_stack_bench.rs:44-53`
(`push_pop/single_thread`) and `:78-87` (`churn`).

`churn` seeds the stack with exactly **one** index and then pops-and-repushes it:

```rust
let stack = Stack::new();
stack.push(&links, 1u32);
h.bench("churn", move || {
    let idx = stack.pop(&links).unwrap();   // drains to empty  -> pop's `next == TAIL` branch
    stack.push(&links, idx);                // pushes onto empty -> push's `next_link = TAIL` branch
});
```

Those are bit-for-bit the two branches `push_pop/single_thread`'s own comment
(`:31-37`) claims as *its* distinguishing content ("the empty→non-empty push … and the
drain-to-empty pop … the H-2 path"). The two rows differ only in the order of the pair inside
one iteration. Measured today on the machine in the header:

```
  push_pop/single_thread   18925790 iters   993.914 ms   52.52 ns/op
  churn                    18835603 iters   985.421 ms   52.32 ns/op
```

— 0.4 % apart. Round 3's own run recorded 54.71 / 54.73, 0.04 % apart. Two independent
measurements, two runs, same conclusion: these are one row printed twice.

The consequence is not the duplication, it is what is therefore **missing**: no row ever pushes
onto a *non-empty* stack (push's `cur_idx as u32` branch, `src/lib.rs:782`) or pops leaving one
(pop's `next != TAIL` branch, `src/lib.rs:902`). Every single-threaded number this crate
publishes — including the one its ABA safety argument cites (P3-2) — comes exclusively from the
sentinel-transition path. The contention rows do cover the ordinary path, but they report
ops/sec across 8 threads and cannot isolate it.

**Fix.** Change `churn`'s prefill to more than one index (e.g. push `0..8` before the closure,
pop-then-repush inside it) so it becomes a genuine steady-state row on the non-empty path, and
leave `push_pop/single_thread` as the sentinel-transition row it documents itself to be. That
is a ~3-line change and it makes the two names mean two different things. It also gives P3-2's
citation a row that matches what the prose says it measures.

### P3-2. The `50.75 ns` figure in published rustdoc and README is uncited and does not reproduce

**Location:** `crates/tagged-index-stack/src/lib.rs:122-125`, mirrored at
`crates/tagged-index-stack/README.md:92-95`.

> "this crate's own bench measures even that fastest one at ~`2 × 10^7` successful pushes/sec
> … (The single-threaded `churn` bench row: `50.75 ns` per pop+push pair — exactly one
> successful push per pair.)"

Round 3's P3-1 asked for exactly two things: cover the uncontended regime (**done, well**), and
"replace the parenthetical with measured *successful-push* rates **and name the machine**, or
drop it entirely". The number landed; the identity did not. There is no machine, no toolchain,
no date, no log anywhere in the crate or the repo backing `50.75`.

The three independent measurements of that row now on record:

| Source | `churn` ns/op |
|---|---:|
| round 3 (`docs/reviews/2026-08-30-2243-…:311`), same machine | 54.73 |
| the shipped doc's own figure | 50.75 |
| this round, same machine, `cargo bench` as documented | 52.32 |

An 8 % spread across runs of the same row on the same host. The *conclusion* is unaffected —
all three are ~2 × 10^7 pushes/sec, an order of magnitude under the 2 × 10^8 ceiling the next
paragraph adopts, so the safety argument is not in question — but a bare, unattributed decimal
presented to three significant figures in published API docs reads as a fact and behaves as a
sample. This is the same standard `CLAUDE.md` applies to gate reports ("cite the machine /
toolchain / raw log"), applied to the one measured number this crate publishes to consumers.

Secondary, same passage: "even that fastest one" is not the crate's fastest push rate. Each
measured pair pays for a pop as well, and the bench has no push-only row, so the true peak
burst-push rate is roughly 2× the quoted figure. Conservative in the safe direction, but the
sentence claims to name a maximum.

**Fix.** Either (a) name the machine/toolchain/date inline and round to one significant figure
("~50 ns/pair on an i7-11800H, rustc 1.97, 2026-08"), or (b) drop the parenthetical and let the
`10^8`–`10^9` hardware ceiling carry the argument alone — which it does. (a) pairs naturally
with P3-1's fixed `churn` row.

### P3-3. The published loom-suite description over-generalizes in two places — one of them introduced by round 3's own fix for the same defect

**Location:** `crates/tagged-index-stack/src/lib.rs:150-157`, mirrored at
`README.md:141-147`; plus `src/lib.rs:798-803`.

Round 3's P3-9 found the crate doc generalising the suite's strongest model to the whole suite,
and suggested a replacement clause. The clause was adopted verbatim:

> "One model runs end-to-end through the shipped `push`/`pop`; **the rest drive the real head
> atomic and the real packing** through `cas_head_for_test` so an interleaving can be pinned."

`counterfactual_untagged_head_lets_aba_corrupt_free_list` (`tests/loom_aba.rs:161-284`) drives
neither. It is a locally-defined `UntaggedStack { head: AtomicU32, next: [AtomicU32; N] }` with
its own hand-written `push`/`pop` — no `TaggedIndexStack`, no `TaggedIndex`, no
`cas_head_for_test`. The test file's own module doc is precise about this ("**two** models
drive locally-defined buggy stand-ins", `:9-10`; "This is the ONE model that is not the crate
type", `:156-158`); the two *published* documents are not. The fix for an over-generalization
reintroduced a smaller one.

Second instance, in code rather than rustdoc — `src/lib.rs:798-803`, added by the same round
(`d745617`):

> "pop's failure ordering MUST stay Acquire (the loom counterfactual
> `counterfactual_relaxed_cas_failure_corrupts_free_list` proves Relaxed **there** corrupts the
> free-list)."

That counterfactual calls `run_cas_retry(Ordering::Relaxed)`, which hand-expands two iterations
of pop's loop through `cas_head_for_test` with the ordering as a parameter (`:597-686`). It
does not touch `pop`'s own `compare_exchange`, whose failure ordering is hardcoded `Acquire`.
The harness is a faithful expansion, so the *inference* is reasonable — but "proves … there" is
attribution to the shipped function, and round 3 spent a whole P2 finding on precisely that
distinction. Combined with P2-1 (the end-to-end test that *does* guard `pop` has a vacuous
oracle under CI's configuration), the evidence chain for `pop`'s failure ordering is weaker
than either sentence suggests.

**Fix.** "the rest" → "most" plus one clause naming the untagged model as the exception (both
files); and at `:798-803`, "proves Relaxed corrupts the free-list in a faithful hand-expansion
of this loop" plus a pointer to the end-to-end test as the guard on `pop` itself.

### P3-4. `tests/loom_aba.rs`'s own "How to run" command no longer works — and it ships in the tarball

**Location:** `crates/tagged-index-stack/tests/loom_aba.rs:52-56`.

```text
RUSTFLAGS="--cfg loom" cargo test -p tagged-index-stack --release --test loom_aba
```

Missing `--features loom`. That is the exact command I ran in P2-2's first reproduction; it
fails with the crate's own `compile_error!`. `README.md:149-151` and `CHANGELOG.md:151-152`
were both updated by round 3's P1-1 fix; this third copy was not. `tests/` is in the published
tarball (`cargo package --list` confirms `tests/loom_aba.rs` ships), so a consumer reading the
source gets a broken command.

Same class as P2-2, different file — and the third hand-maintained copy of one command string,
which is the structural point P3-7 makes.

### P3-5. Rule 4 silently outlaws the classic overlay-the-link-on-the-payload free-list idiom that the README's own pitch invites — and the new `debug_assert!` turns that into debug-build panics on a benign race

**Location:** `crates/tagged-index-stack/src/lib.rs:707-727` (push's `# Caller contract`,
rule 4), `:886-897` (pop's new `debug_assert!`), `:497-505` + `:444-496` (the `Links` trait),
`README.md:31-37`.

Rule 4 requires that `load_next` "must return only `TAIL` or a currently-valid index", and
round 3 added a `debug_assert!` in `pop` enforcing it on every pop. Both are correct *as
written*. The gap is that neither says what that rule costs a consumer, and the README markets
the very shape it rules out:

> "so a production allocator keeps its links **slot-resident** (an `AtomicU32` field inside a
> slot it already owns) rather than paying for a second array."

In the canonical Treiber free-list — the one mimalloc/tcmalloc-style allocators use, and the
first thing a reader of that sentence is likely to build — the link *shares storage with the
free block's payload*: the cell is a link while the block is free and user data while it is
live. Under that layout, this sequence is ordinary and by design:

1. Thread A enters `pop`, reads `head = (X, t)`.
2. Thread B pops `X`, hands it to a consumer, which writes payload bytes over the link cell.
3. Thread A calls `links.load_next(X)` and reads **arbitrary user data**.
4. A's CAS then fails (the head moved), A retries, no corruption — this is exactly the window
   the tag exists to tolerate, and step 4 is why it is benign.

But step 3 now trips the `debug_assert!` in every debug/test build the consumer runs, with a
message ("neither TAIL nor a valid index") that reads like a corruption report for what is a
correct, expected interleaving. The crate's own flagship consumer is safe from this only
because `RegistryLinks` uses a **dedicated** `next_free: AtomicU32` field rather than aliased
storage (`src/registry/heap_registry.rs:587-598`) — a distinction nothing in the published
contract calls out.

**Fix — one paragraph, no code change.** State explicitly in the `Links` trait doc (the place a
prospective implementor reads) that link storage must remain a *dedicated* cell whose contents
are not overwritten while the index is out of the stack, i.e. that overlaying the link on the
popped slot's payload is **not** supported — and say why (`pop` may legitimately read the link
of an index another thread has already popped). One sentence in rule 4 pointing at the trait
doc closes it. Optional, larger: exempt the `debug_assert!` when the read value would fail the
subsequent CAS anyway — but that costs a re-read of the head and is not worth it; documenting
the restriction is the right resolution.

### P3-6. Loom covers no push‖push and no pop‖pop interleaving, and push's retry arm has no activation oracle

**Location:** `crates/tagged-index-stack/tests/loom_aba.rs` (whole file);
`src/lib.rs:791-806` (push's `Relaxed` failure-ordering rationale), `:916-926`
(pop's retry counter).

Classified by which real operations race, the eight models are: A-hand-inlined ‖ real
pop+push (×2), real pop ‖ real push (×1, the end-to-end one), A-hand-inlined ‖ real pop+push
under rendezvous (×2, the H-2 pair), A-hand-unrolled ‖ real push (×2), and one wholly local
untagged model. **No model runs two concurrent real `push`es, and none runs two concurrent
real `pop`s** — the two most ordinary interleavings a free-list sees in production (two
threads freeing, two threads allocating).

Round 3's P3-4 named this as a "Related coverage gap" and the remediation landed only the
doc half (`d745617` — truncation contract, `track_caller`, Relaxed rationale). Unlike that
round's other deferrals, this one is recorded nowhere: not in a commit body, not in
`docs/CORRECTNESS_OPEN_ITEMS.md`, not in the CHANGELOG (which *does* durably record the
declined AArch64 perf pair). Raised once, actioned half, recorded zero times.

The asymmetry is sharpest around the oracle: `push`'s `Relaxed` CAS-failure ordering now
carries a 16-line justification (`:793-806`) and is exercised only *incidentally*, while
`pop`'s equivalent got a dedicated counter. I derived push's ordering to be sound (see the
verdict section), so this is a coverage gap rather than a suspected bug — but the suite runs
in 0.11 s, and two more two-thread models plus a `PUSH_RETRY_COUNT` twin would make the crate's
own strongest selling point ("exhaustive loom model-check against the real type") mean what a
reader assumes.

**Fix.** Add `push_push_conservation` and `pop_pop_conservation` (both trivially derived from
`pop_retry_after_failed_cas_…`'s shape), and a `#[cfg(loom)] PUSH_RETRY_COUNT` mirroring the
pop counter — after P2-1's serialization fix, so the new oracles are sound from birth. If
declined, record it somewhere durable this time.

### P3-7. Round 3's recorded revisit condition for the neuroslop finding has been met: `src/lib.rs` is still 73 % comment and has grown 20 % since

**Location:** `crates/tagged-index-stack/src/lib.rs` (whole file).

Round 3's P3-6 (73 % comment) was declined in `ab4497f` with a stated condition:

> "Revisit if a future round finds NEW drift instances — that would be empirical evidence a
> restructuring is warranted, not assumed preemptively."

This round found **six**, and all six were introduced or left behind by round 3's own
remediation wave: P2-2 (`scripts/loom.mjs`), P3-3 (both halves), P3-4
(`loom_aba.rs`'s run command), P4-1 (two sites still describing an `assert!` that the same
round replaced), P4-2 (`ci.yml`'s "all four `tests/` files", now five). Every one is a
hand-maintained duplicate of a fact stated elsewhere drifting from its source. That is the
condition, met empirically.

Measured (`grep -cE '^\s*(//|/\*|\*)'`), round 3 → now:

| | round 3 | now | Δ |
|---|---:|---:|---:|
| total lines | 860 | 1033 | +20 % |
| comment/doc lines | 631 | 760 | +20 % |
| code lines | ~198 | 238 | +20 % |
| ratio | 73 % | **73.6 %** | — |

The precedent this repo set for its own sibling is `size-classes`, driven 65.3 % → ≤ 55 %
(tasks #1638, #1589, #1545). The concrete passages that read as review-response prose in
**published** rustdoc (i.e. excluding the `#[doc(hidden)]` items, which rustdoc does not
render, and excluding the private `head` INVARIANT block, which round 3 rightly called the
best documentation in the crate):

| Location | Lines | What it is |
|---|---:|---|
| `:236-246` | 11 | why `TaggedIndex` is an uninhabited `enum` "rather than a unit `struct`" — a design-decision defence answering round 3's P4-10, in the type's landing doc |
| `:250-286` | 37 | `_CHECK_BITS`'s rationale, still including a paragraph on why `INDEX_BITS > 32` "could never buy reachable index range anyway" — a width the type has been unable to express since round 2 |
| `:310-342` | 33 | `pack`'s truncation essay for a two-line function, still ending "Note the two bounds are deliberately different ranges, **not a typo**" — a reply to a reviewer, in published API docs |
| `:444-496` | 53 | the `Links` ordering contract, still including "read this as considered defence-in-depth for an openly-implementable trait, **not naivety**" |
| `:657-741` | 85 | `push`'s `# Caller contract` — a five-point numbered list plus two prose restatements of the same failure mode (72 lines at round 3; +18 % since) |

**Fix.** The condition round 3 named has fired, so the mechanical version it already specified
now has evidence behind it: one "Invariants" section in the crate doc holding H-2, RAD-1, the
caller contract, the ordering rationale and the loom-suite description **once**, with short
references from each item's own doc — which also removes the substrate that produced five of
this round's six drift instances. Keep the `head` INVARIANT block verbatim.

### P3-8. `docs/correctness-open-items/TRACKED_ci_gate_coverage.md` item 25 is stale on four counts, and its own closing condition was already satisfied two rounds ago

**Location:** `docs/correctness-open-items/TRACKED_ci_gate_coverage.md:82-113`.

The item's Status card reads `OPEN — not fixed`, and:

1. Its "Current-number-or-verdict" says `_CHECK_BITS` "enforces `INDEX_BITS in 1..=32`". The
   cap has been `1..=16` since round 2 (`f23db29`). The card's central number is wrong.
2. Its "Next trigger" names `TaggedIndex::<33>` as the width to pin. 33 has not been the first
   rejected width for two rounds; 17 is.
3. The same trigger writes "`TaggedIndexStack<33, _>`, whichever the crate's public generic
   surface exposes" — `TaggedIndexStack` has one const parameter, not two.
4. Both line citations rotted: `src/lib.rs ~179-195` (`_CHECK_BITS` is at `:280-286`) and
   `tests/stack_unit.rs ~137-144` (the recorded-gap comment is at `:272-289`).

And the trigger's own second branch — "**OR** document an explicit accepted-risk rationale if
compile-fail infra is judged not worth adding for a single-crate, single-assertion case" — was
satisfied in round 2 by task #1689 / `edbe05f`, which wrote exactly that rationale into
`tests/stack_unit.rs:272-289`, naming the two sibling crates that declined `trybuild` for the
identical tradeoff (`crates/sefer-region/tests/handle_static_asserts.rs`,
`crates/aligned-vmem/tests/smoke.rs`).

`CLAUDE.md`'s own rule is directly on point: "OPEN_ITEMS indexes are CURRENT-STATE, not
archives … a closed / null / rejected item must NOT look active due to a stale header … A
closed item that still sits in an active tier with no Status-card update is a structural
defect." This is that defect, on this crate, on the eve of its publish.

**Fix.** Update the card to `Status: CLOSED (accepted risk, documented)`, cite `edbe05f` /
`tests/stack_unit.rs:272-289` as the closure evidence, correct the `1..=16` / width-17 facts,
and move the narrative to the index's "Recently resolved" trail per the same rule.

---

## P4 — minor / cosmetic

**P4-1. Two shipped sites still describe push's guard as an `assert!`; it hasn't been one since
round 3.** `src/lib.rs:740` ("the `index < INDEX_MASK` bound this very method's `assert!` DOES
enforce on every call" — published rustdoc) and `CHANGELOG.md:103-105` ("a single release-active
`assert!` — one guard covers both conditions"). `d745617` replaced the `assert!` with
`if (index as u64) >= mask { Self::push_index_out_of_range(index, mask); }` plus a
`#[cold] #[inline(never)] #[track_caller]` helper (`:760-833`). Behaviour is identical
(release-active, one condition, both purposes) — only the named construct is gone. Round 3's
P4-2 fixed this same CHANGELOG sentence from plural to singular; the same round then falsified
it a different way.

**P4-2. `ci.yml` says the clippy row covers "all four `tests/` files"; there are five.**
`.github/workflows/ci.yml:1958-1964` enumerates `stack_unit.rs`, `regression_counter_wrap.rs`,
`proptest_pack_unpack.rs`, `loom_aba.rs`. `tests/readme_example.rs` was added by `270448e`
(round 3, P4-8) and is genuinely covered by `--all-targets` — only the comment's count and list
are stale. Same hardcoded-count class as `CLAUDE.md`'s task #776/F10 convention, which this
very file cites at `:1813-1818`.

**P4-3. `Cargo.toml` ships ~24 lines of review archaeology into the crates.io tarball**,
including an explicit internal-process citation: "measured in the round-3 review: 31 locked
packages downstream of a crate whose only dependency is this one; 2 once optional"
(`crates/tagged-index-stack/Cargo.toml:21-30`). The *technical* content (why `optional = true`
and why both `--cfg loom` and `--features loom` are needed) is worth keeping; the round-N
provenance is not. Exactly the defect `size-classes` closed at task #1607 ("Cargo.toml ships a
9-line review-ID citation into the crates.io tarball").

**P4-4. Four `Sol-codex review run 2, P3-N` citations ship in the tarball's `tests/` and
`benches/`.** `tests/stack_unit.rs:278-279`; `benches/tagged_index_stack_bench.rs:118`, `:154`,
`:248`. Same class as P4-3 and as `size-classes` tasks #1561 / #1607. The reasoning each one
introduces is worth keeping; the review IDs are meaningless outside this repo.

**P4-5. The `[lints]` comment names two cfgs; the workspace lint carries four, and all four
ship.** `crates/tagged-index-stack/Cargo.toml:16-17` says "`loom` / `kani` cfg declarations are
shared at the workspace root … `kani` is root-only and harmlessly unused here", but the root
`[workspace.lints.rust]` now also carries `cfg(aligned_vmem_page_size_override)` (task #1080)
and `cfg(numa_shim_mock)` (task #1288). I confirmed against the **generated** manifest inside
the `.crate` tarball: all four `check-cfg` entries are published to crates.io, three of them
naming cfgs this crate has never heard of. Harmless; the comment should either name all four or
stop enumerating.

**P4-6. The README example warns if copy-pasted.** `README.md:161-169` binds
`let idx = stack.pop(&links);` and never uses `idx` → `unused variable: idx`. The mirroring
test (`tests/readme_example.rs`) asserts on it, so the pin is real, but the snippet a user
actually copies is the one that warns. Either use `idx` in the snippet (a `println!`/`assert`
line, matching what the test does) or bind `let _idx`.

**P4-7. No test exercises a non-`ArrayLinks` `Links` implementation, nor `&dyn Links`.** The
trait's own doc calls itself "intentionally OPEN to external implementation — slot-resident
links in caller-owned storage … is the whole design point" (`src/lib.rs:490-496`), and
`push`/`pop` carry a deliberate `?Sized` bound admitting `&dyn Links` — neither is covered by
any test in the crate. The production shape the README sells is exercised only in the
*consumer* repo. A ~20-line `tests/` file with a tiny `Cell`-free custom backing (and one call
through `&dyn Links` to pin object-safety, which is part of the frozen 0.1.0 surface) closes
both.

**P4-8. `regression_counter_wrap.rs` substantially duplicates `stack_unit.rs`.**
`tag_wraps_at_2_pow_48_and_index_survives` vs `tag_wraps_at_2_pow_48`; `split_is_16_48` vs the
first two assertions of `pack_unpack_round_trip_16`; `empty_sentinel_never_collides_with_a_live_index`
vs `empty_sentinel_16`. Both files are `#![cfg(not(loom))]`, both run in the same CI rows. The
regression file's distinct value (its header's "unrepresentable on a 32-bit tag" argument) is
one assertion, not four tests.

**P4-9. Two documented panic paths have no test.** `ArrayLinks::{load_next,store_next}`'s
`index >= N` panic — documented at `src/lib.rs:544-564` as "a second panic source the guard
above does not cover", with a worked example ("a `TaggedIndexStack<16>` accepts indices up to
65534 even over an `ArrayLinks<256>`") — and pop's rule-4 `debug_assert!` (`:886-897`, added
round 3) are both unexercised. The `debug_assert!` in particular is a new guard with a
two-branch message and zero coverage: invert its condition and nothing in the suite fails.
`width_16_push_rejects_index_mask_itself` (`tests/stack_unit.rs:240-270`) is the established
pattern to copy.

**P4-10. `CHANGELOG.md:7` still reads "0.1.0 - Unreleased".** Known release-commit checklist
item; recorded, no action expected before the publish commit itself.

---

## Considered adversarially and **rejected** — recorded so they are not re-filed

1. **`#[track_caller]` on `push` costs measurable throughput on the hot path.** Round 3's P3-3
   fix put `#[track_caller]` on `push` itself (needed: without it the panic names
   `lib.rs:761`, not the caller) *in addition to* the `#[cold]` helper, adding an implicit
   `&Location` argument to every call of a lock-free primitive. **Measured, not assumed.** Two
   out-of-workspace copies of `src/lib.rs` — identical but for that one attribute — built
   against the same bench, three interleaved runs each:

   | run | `push_pop/single_thread` base → no-`track_caller` | `churn` base → no-`track_caller` |
   |---|---|---|
   | 1 | 53.01 → 51.67 | 51.81 → 52.23 |
   | 2 | 51.98 → 52.68 | 52.11 → 52.95 |
   | 3 | 52.69 → 51.97 | 52.90 → 51.67 |
   | mean | 52.56 → 52.11 (−0.9 %) | 52.27 → 52.28 (0.0 %) |

   Individual runs of the two variants interleave and cross over in both directions. **No
   effect on x86-64.** Keep the attribute.

2. **Adding `#[inline]` to `push`/`pop`.** Round 3 rejected this on the theory that every
   public fn here is generic and therefore already `cross_crate_inlinable`. I tested the
   residual worry — that `inlinehint` still moves LLVM's cost heuristic for a function
   containing a loop — with the same interleaved out-of-tree A/B:

   | run | `push_pop/single_thread` base → `#[inline]` | `churn` base → `#[inline]` |
   |---|---|---|
   | 1 | 52.94 → 52.32 | 51.80 → 52.33 |
   | 2 | 52.13 → 52.27 | 51.18 → 51.93 |
   | 3 | 52.06 → 51.95 | 51.57 → 51.76 |
   | mean | 52.38 → 52.18 | 51.52 → 52.01 |

   Inside noise, both directions. Round 3's rejection now has a measurement behind it rather
   than only a theory.

3. **`pop`'s CAS success ordering could be `Relaxed`.** It could: the value the successful CAS
   reads is the value this thread already read with `Acquire`, so the edge is already
   established (and Rust has allowed failure-stronger-than-success since 1.64, well under this
   crate's 1.88 MSRV). But LLVM lowers a cmpxchg to a single instruction chosen from the
   stronger of the two orderings — `lock cmpxchg` on x86-64, `casa` on LSE AArch64 — so there
   is no codegen difference to win. Not worth the churn on a load-bearing ordering.

4. **Skipping `push`'s `store_next` when the link value is unchanged.** Replaces an
   unconditional store with a load + branch + conditional store; on x86 the store is the cheap
   half. Under contention the value is rarely unchanged. Rejected on inspection, not measured.

5. **Tarball hygiene / publishability.** `cargo package --list` → 15 files, no strays. The
   generated manifest inside the `.crate` correctly inlines the workspace `[lints]`, emits an
   empty `[dependencies]`, and carries `loom = { version = "0.7", optional = true }` under
   `[target."cfg(loom)".dependencies]`. `bench-scale-tool 0.1.0` (the bench's dev-dependency)
   resolves from crates.io with a real checksum, so `cargo publish`'s verify step will not
   trip on it. Clean.

6. **Other `--cfg loom` CI jobs missing the `tagged-index-stack/loom` feature** (the specific
   analogue the brief asked about). Checked all four: `loom-alloc-global` passes
   `--features loom` explicitly and its second step is `-p once-ptr-cell`; `loom-xthread`
   (`alloc-core alloc-xthread`) and `loom-experimental` (`experimental`) never pull
   `dep:tagged-index-stack` — only `alloc-global` does, per the root `[features]` table;
   `loom-misc`'s first step runs default features. `loom-misc`'s second step is the one
   `8027d9a` fixed. **CI is complete.** The gap is `scripts/loom.mjs` — P2-2.

7. **`pop`'s `Acquire`-only success ordering, push's `Relaxed` failure ordering, tag
   monotonicity, the `2^TAG_BITS` wrap bound, `_CHECK_BITS` routing, width-1 degeneracy, and
   `INDEX_MASK != TAIL` at every legal width.** All re-derived from scratch this round rather
   than inherited from round 3's verdict. All correct. Details in the verdict section above.

---

## What is genuinely good

- **The algorithm survived a fourth independent attack.** I went at tag monotonicity across
  pops, the drain transition, the release-sequence status of every head write, the
  `TAIL`/`empty_index` split at both extreme widths, the shift boundary, and the interaction
  between `pop`'s stale-snapshot window and a reused link cell. Nothing broke.
- **Round 3's two headline fixes are real fixes, not documentation patches.** `loom` really is
  optional (the generated tarball manifest proves it), and the `preemption_bound`s really are
  gone — the suite is now genuinely exhaustive over these models and *still* runs in 0.11 s.
  The activation-oracle idea is right too; only its interaction with the test harness is wrong
  (P2-1), which is a much better failure than the prose receipt it replaced.
- **The `head` field's `INVARIANT` block (`src/lib.rs:593-616`) remains the best thing in the
  crate.** It states a rule, gives the standards reason, and names the specific future change
  (`clear()`/`Drop`) that would silently break it. If P3-7's consolidation happens, this is the
  passage to move verbatim, not rewrite.
- **The bench's contention discipline is still correct and still non-obvious** — re-push exactly
  what you popped, and drain-then-assert-empty before the phase-2 prefill. Round 1 found a real
  double-push bug there and the fix has held through two more rounds of edits.
- **`try_pack`'s test asserts value-equality against `pack`'s own output** rather than merely
  `Some(_)`, and round 3 extended that to all four proptest widths including the
  `1u64 << 63` shift boundary. That is the right oracle, applied at the right widths.

---

## Suggested order of work

1. **P2-1** — serialize the loom models with a file-local mutex so the activation oracle is
   sound under any `--test-threads`. Re-run the neutered-model counterfactual from this report
   to confirm the oracle now fires. Do this *before* P3-6.
2. **P2-2** — two edits in `scripts/loom.mjs` mirroring `ci.yml:2470` and `:2636`; then run
   `npm run loom` end-to-end to confirm all 16 models compile.
3. **P3-4 / P4-1 / P4-2** — the three concrete stale-string fixes (broken run command, the two
   `assert!` sites, the four-vs-five test-file count). Cheap, mechanical, and they are three of
   the six drift instances P3-7 counts.
4. **P3-3** — the two over-attribution clauses (published loom description, in-code
   counterfactual attribution).
5. **P3-1 / P3-2** — fix `churn` to prefill more than one index so the two single-threaded rows
   measure different paths, re-measure, and then either attribute or delete the `50.75 ns`
   citation with the new number.
6. **P3-5** — one paragraph in the `Links` trait doc ruling out payload-aliased link storage.
7. **P3-8** — correct and close the stale `TRACKED_ci_gate_coverage.md` item 25 card.
8. **P3-7** — the consolidation, now that its recorded revisit condition has fired; or an
   explicit second decline that engages with the six new drift instances rather than
   restating round 3's reasoning.
9. **P3-6** and the remaining P4s as one bundle.
