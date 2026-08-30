# `tagged-index-stack` — independent publish-readiness review, round 3

- **Reviewer:** Claude (Opus 5), adversarial pass; prior review docs read only *after* forming
  my own findings, to check for overlap and for things a prior round papered over.
- **Date:** 2026-08-30 22:43:01 +0200 (CEST)
- **Revision reviewed:** `4836a01cfc8ac823cf49659596de57dd3ed603d0` (working tree clean w.r.t.
  `crates/tagged-index-stack/`)
- **Scope:** `crates/tagged-index-stack/**` (src, tests, benches, README, CHANGELOG, Cargo.toml,
  licenses), `.github/workflows/ci.yml` rows for this crate, and the in-tree consumers
  `src/registry/heap_registry.rs` / `src/registry/bootstrap.rs` where they touch the public API.
- **Verification actually performed** (not inferred from comments):
  - `cargo test -p tagged-index-stack` — **23 tests green** (15 `stack_unit`, 4 `proptest_pack_unpack`,
    4 `regression_counter_wrap`; `loom_aba` correctly compiles to 0 tests; 0 doctests).
  - `RUSTFLAGS="--cfg loom" cargo test --release -p tagged-index-stack --test loom_aba` —
    **8 tests green in 0.25 s**, including all three `#[should_panic]` counterfactuals.
  - `cargo bench -p tagged-index-stack --bench tagged_index_stack_bench` — real numbers,
    quoted in P3-1 below.
  - `cargo clippy -p tagged-index-stack --all-targets -- -D warnings` — clean.
  - `RUSTDOCFLAGS="-D warnings" cargo doc -p tagged-index-stack --no-deps` — clean.
  - `cargo package --list -p tagged-index-stack` — clean tarball (no stray files).
  - **Two purpose-built scratch crates outside the workspace** to measure this crate's real
    downstream dependency footprint and to test the proposed fix (P1-1). Both deleted afterwards.
- **Machine / toolchain for the measured numbers:** `rustc 1.97.0 (2d8144b78 2026-07-07)`,
  LLVM 22.1.6, host `x86_64-pc-windows-msvc`, 11th Gen Intel Core i7-11800H @ 2.30 GHz,
  8 threads used by the contention rows.
- **No code was changed.** This is a read-only review.

---

## Overall verdict: **CONDITIONAL-GO**

**I found no correctness defect in the shipping algorithm.** I did not take the existing
"this was fixed / this is proven" comments at face value; I re-derived each load-bearing
claim myself:

- **The ABA argument holds.** The tag is non-decreasing along the head's modification order
  (every successful `push` does `wrapping_add(1)`, every successful `pop` preserves it —
  *including* across the drain-to-empty transition), so the head word can only recur exactly
  after a full `2^TAG_BITS` wrap. Verified at the boundary: `pack`'s `tag << INDEX_BITS` with
  `tag == 2^TAG_BITS` does **not** trap even under `overflow-checks = true` — Rust's `<<`
  panics only on a shift *amount* ≥ the type width, never on lost bits — so the wrap is
  well-defined in every profile.
- **`pop`'s `Acquire`-without-`Release` success ordering is sound**, and for the reason the
  `head` field's INVARIANT states. I confirmed the premise independently: there is no plain
  `store` to `head` anywhere (`new` is initialization, `raw_head`/`is_empty` only load, and
  all three writers — push's CAS, pop's CAS, the loom-only `cas_head_for_test` — are RMWs),
  so under the C++20 release-sequence rule the sequence headed by any push's `Release` CAS
  extends through every later modification, and a later popper's `Acquire` head read still
  synchronizes back to the push that wrote the link it is about to follow. The two-deep
  chain (`P1` pushes A, `P2` pushes B→A, a popper pops B then A) works out.
- **`push`'s `Relaxed` CAS-failure ordering is sound** — push never dereferences anything
  reached through the head; it uses the index half as a value to store and the tag half as a
  number to bump.
- **The degenerate width `INDEX_BITS = 1` is safe** end-to-end (`INDEX_MASK == 1`, only index
  `0` is legal, `is_empty` is bit-0, the tag gets 63 bits).
- **`try_pack`'s `let () = Self::_CHECK_BITS;` is not redundant**, contrary to how it reads:
  the `||` genuinely short-circuits inside the *const interpreter*, so without the explicit
  statement a purely-const call taking the first branch could skip the guard.

What holds this back from an unconditional GO is **one packaging/honesty blocker and two
verification-claim overstatements**, none of which is a bug in the stack:

1. **P1-1** — `loom` is declared as a *normal* (non-dev) `cfg(loom)` dependency, so every
   downstream consumer's `Cargo.lock` gains **30 packages** (measured: 31 total vs. 2 with the
   one-line fix), including `cc`, `libc`, the whole `tracing`/`tracing-subscriber` tree, and —
   with some irony — `sharded-slab`, the crate this project's own README name-checks. Two
   shipped documents (`CHANGELOG.md:114`, `Cargo.toml:25`) state the opposite in as many words:
   "zero non-`std` dependencies". That claim is true of *compilation* and false of everything a
   consumer's supply-chain tooling looks at.
2. **P2-1** — every loom model runs under `preemption_bound = 3|4`, a deliberately incomplete
   search, while README/CHANGELOG sell them as "executable loom **proofs**". A preemption bound
   can only produce false negatives, which is exactly the failure mode the five *positive*
   models exist to exclude. This is the same species of overclaim that was round 2's P1-2
   blocker, one level down.
3. **P2-2** — no loom model asserts that its target interleaving was actually explored, and the
   one model the test file itself calls "the test that actually protects the shipped source" has
   neither a paired counterfactual nor an activation oracle — only a prose claim about a one-off
   manual experiment.

Fix P1-1 and P2-1/P2-2 and this is a clean GO. Everything else is polish, with the caveat that
the P3 block is unusually substantive for a third round: two of its items (P3-1's measured
falsification of the benchmark parenthetical, P3-5's dropped perf sub-items) are things prior
rounds raised and the remediation lost.

---

## P1 — blocking

### P1-1. `loom` is a normal `cfg(loom)` dependency: consumers lock 30 extra packages, while two shipped docs promise "zero non-`std` dependencies"

**Location:** `crates/tagged-index-stack/Cargo.toml:20-28` (the dependency), with the
contradicted claims at `crates/tagged-index-stack/CHANGELOG.md:113-114` and
`crates/tagged-index-stack/Cargo.toml:21-25`.

The manifest declares

```toml
[target.'cfg(loom)'.dependencies]
loom = { workspace = true }
```

This is loom's own documented idiom and it is *correct for compilation*: `cfg(loom)` is false in
a normal build, so loom is never built, and the accompanying comment ("A normal `cargo build`
(no `--cfg loom`) pulls in zero non-std deps") is literally true about `rustc` invocations.

It is not true about anything else a consumer sees. Cargo's resolver is platform- and
cfg-agnostic when producing `Cargo.lock`: a *normal* (non-`dev`, non-`optional`) target-cfg
dependency is resolved, locked, fetched, and vendored regardless of whether its cfg holds.

**Measured, not inferred.** I created a scratch crate outside this workspace whose only
dependency is `tagged-index-stack` (by path), ran `cargo generate-lockfile`, and read the
lockfile:

```
Locking 31 packages to latest compatible versions
```

The 31 are: `tis-probe` (the probe itself), `tagged-index-stack`, and then
`aho-corasick`, `cc`, `cfg-if`, `find-msvc-tools`, `generator`, `lazy_static`, `libc`, `log`,
`loom`, `matchers`, `memchr`, `nu-ansi-term`, `once_cell`, `pin-project-lite`, `regex-automata`,
`regex-syntax`, `rustversion`, `scoped-tls`, **`sharded-slab`**, `shlex`, `smallvec`,
`thread_local`, `tracing`, `tracing-core`, `tracing-log`, `tracing-subscriber`, `valuable`,
`windows-link`, `windows-result`, `windows-sys`.

**Concrete failure scenario.** A firmware team evaluates this crate for a `no_std` build
(precisely the audience `categories = [..., "no-std::no-alloc"]` targets). They `cargo add
tagged-index-stack`, then run their mandatory `cargo vendor` + `cargo deny check` +
`cargo audit` gate. `cargo vendor` downloads 30 crates they never asked for — including a
`cc`-driven build script (via `generator`) and a `windows-sys` tree — into a repo that is
supposed to have no C toolchain in its supply chain. `cargo audit` will thereafter report any
future RUSTSEC advisory against `tracing-subscriber`, `regex-automata`, `smallvec`, etc. as an
advisory *against their firmware*, because those crates are in their lockfile. Meanwhile the
crate's own CHANGELOG told them there would be zero non-`std` dependencies. This is a
credibility failure precisely with the audience the crate is aimed at.

**Fix — verified, one line.** Mark the dependency optional:

```toml
[target.'cfg(loom)'.dependencies]
loom = { workspace = true, optional = true }
```

I built a second scratch pair (a `depcrate` with exactly this manifest shape and a `#[cfg(loom)]
use loom::sync::atomic::AtomicU64;`, plus an `app` depending on it by path) and re-ran
`cargo generate-lockfile`:

```
Locking 1 package to latest compatible version
```

**2 packages in the consumer lockfile instead of 31.** The entire loom tree disappears from the
consumer's resolve, vendor set and audit surface.

**Ripples the fix has (all small, all in this repo):**

- The loom invocation must now enable the implicit feature as well as the cfg:
  `RUSTFLAGS="--cfg loom" cargo test -p tagged-index-stack --release --features loom --test loom_aba`.
  Two places to update: `.github/workflows/ci.yml:2451-2453` (the `loom-alloc-global` step) and
  `README.md:135-137`.
- The crate's feature map stops being empty. Two ci.yml comments assert it is `{}` and cite the
  `cargo metadata ... | jq` command that proves it (`.github/workflows/ci.yml:1928-1940` for the
  bare-metal row, `:1959-1972` for the clippy/doc rows). Both need one sentence updated —
  neither *step* needs changing, since neither passes `--all-features`.
- Consider guarding the mismatch: if someone passes `--cfg loom` without `--features loom`, the
  `#[cfg(loom)] use loom::...` fails with an unresolved-import error. A
  `#[cfg(all(loom, not(feature = "loom")))] compile_error!("--cfg loom requires --features loom")`
  next to the existing `target_has_atomic` guard would make that fail with a named reason, in
  exactly the style this crate already established at `src/lib.rs:172-181`.

**Why P1 rather than P2.** It is not a correctness bug, and it *is* technically fixable in
0.1.1 (adding a feature is a minor-compatible change). But `0.1.0` is the version that will sit
in the wild and in people's lockfiles forever, the fix is one line, and the crate's own shipped
documentation currently makes a claim a consumer can falsify in thirty seconds with
`cargo tree`. If the owner disagrees with the severity, the honest minimum alternative is to
*delete both "zero non-std dependencies" claims* and replace them with an accurate statement of
the lockfile cost — but that is strictly worse than just fixing it.

---

## P2 — should fix before publish

### P2-1. Every loom model runs a deliberately incomplete search (`preemption_bound`), while README and CHANGELOG call them "proofs"

**Location:** `crates/tagged-index-stack/tests/loom_aba.rs:87, 201, 292, 421, 551, 596` (six
`builder.preemption_bound = Some(3|4)` sites covering all eight tests); claims at
`README.md:20` ("with executable loom **proofs** run against the real type" — the sentence that
is the crate's stated differentiator versus `sharded-slab`), `README.md:127-133`,
`CHANGELOG.md:107-114` ("**Executable loom proofs** against the real type"), and
`src/lib.rs:137-144`.

Loom's `preemption_bound` caps how many preemptions the scheduler will introduce. Its documented
purpose is to trade completeness for runtime: with a bound set, `Builder::check` explores a
*subset* of interleavings and a clean run is no longer an exhaustive result. Nothing in the
crate's published documentation says this. A reader of `README.md:20` reasonably concludes the
crate ships exhaustive model-checking of the real type.

**Why it matters, precisely.** A preemption bound can only cause **false negatives** — missed
bugs. That asymmetry maps exactly onto the two kinds of test in this file:

- The three `#[should_panic]` counterfactuals are **self-guarding**: they pass only because the
  bounded search *did* find the planted bug, and a bound that hid it would make the test fail
  loudly ("test did not panic"). The bound costs them nothing.
- The five positive models —
  `aba_repush_keeps_free_list_conservation`,
  `tagged_stack_survives_the_same_resurrection_pattern`,
  `pop_empty_transition_preserves_tag`,
  `cas_retry_path_must_acquire_with_concurrent_push`,
  `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type` —
  claim *the absence* of a corrupting interleaving. That is exactly the claim a preemption bound
  weakens, and those five are what "proof" is being asserted about.

**Concrete failure scenario.** A future change to `pop` (say, hoisting the `load_next` above the
`is_empty` check, or promoting push's failure ordering in a way that alters which values a
retry can read) introduces a corruption reachable only at 4+ preemptions. All five positive
models stay green; the three counterfactuals stay green because their planted bugs are still
findable at ≤4. CI is green. The published README still says "proofs".

**There is headroom to just fix it.** The full suite runs in **0.25 s** for 8 models. That is a
lot of budget to spend. Suggested direction:

1. Try removing `preemption_bound` entirely on the four models with no spin loop
   (`aba_repush_keeps_free_list_conservation`, `tagged_stack_survives_the_same_resurrection_pattern`,
   and the two `run_cas_retry` variants) and on
   `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type`; measure. Two-thread models
   over ~10 atomic operations each are usually affordable unbounded.
2. `run_h2` (the H-2 pair) uses two `while ... { thread::yield_now() }` rendezvous spin loops,
   which are the classic loom state-space blow-up. Keep a bound there if unbounded is
   unaffordable — but **say so in the test's own doc comment**, and drop the word "proof" from
   `README.md:20` / `CHANGELOG.md:107` in favour of something defensible: "bounded loom
   model-check (`preemption_bound = 3`) against the real type, with `#[should_panic]`
   counterfactuals".

Either resolution is fine. Shipping the current wording is not, in a crate that had to fix an
identically-shaped overclaim ("structurally defeats ABA") one round ago.

### P2-2. No loom model asserts its target interleaving was reached — and the model that matters most has neither a counterfactual nor an oracle, only prose

**Location:** `crates/tagged-index-stack/tests/loom_aba.rs:535-584`
(`pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type`), particularly its own doc at
`:544-547`.

That test's doc comment says, of itself:

> "This is the test that actually protects the shipped source: unlike the hand-unrolled
> `cas_retry_path_must_acquire_with_concurrent_push` below (which pins one specific interleaving
> for exposition, using `cas_head_for_test` with hardcoded orderings rather than calling `pop`
> itself), this one fails if `pop`'s own `compare_exchange` failure ordering ever regresses.
> **Verified:** with `pop`'s failure ordering temporarily reverted to `Ordering::Relaxed`, this
> test FAILS (`left: [0, 0, 1], right: [0, 1]` — index 0 duplicated, a real double-allocated
> free-list slot), then passes again once reverted back to `Ordering::Acquire`."

That is a claim about a one-off manual experiment on a mutated working tree that no longer
exists — an unreproducible receipt, of exactly the shape `CLAUDE.md` warns about ("An agent's
statement is a claim, not a receipt"). Every *other* ordering claim in this crate is backed by a
shipped `#[should_panic]` counterfactual; this one, the one its own doc says is load-bearing,
is backed by a sentence.

Separately, **no model in the file counts how often its interesting branch was taken.**
`run_cas_retry` gets closest — it early-returns when `result.is_ok()` ("No race — B didn't
interpose, nothing to test") — but that only skips *individual* executions; a run in which
*zero* executions reached the retry path would still be green, silently vacuous. The end-to-end
test above has no such check at all: it asserts only free-list conservation, which a run where
`pop` never once retried would also satisfy.

**Mitigating evidence I did check.** `counterfactual_relaxed_cas_failure_corrupts_free_list`
passes, which proves the retry path *is* reachable within `preemption_bound = 4` for the
`run_cas_retry` harness; since the schedule space is driven by thread interleaving rather than by
the failure ordering, its `Acquire` twin almost certainly reaches it too. So this is a latent
vacuity risk, not a demonstrated vacuity. That is why it is P2 and not P1 — but it *is* the
risk the brief's "a loom test might not actually cover what it claims" points at, and this repo
has already institutionalised the fix for the analogous perf case (`CLAUDE.md`'s R30-8
path-activation-oracle rule).

**Fix — cheap and mechanical.** A *real* `std::sync::atomic::AtomicUsize` (not loom's) declared
outside `builder.check(...)`, incremented on the retry branch inside the closure, asserted
`> 0` after `check()` returns. Loom re-runs the closure many times; a plain std static survives
across those runs and gives an exact activation count. Applied to
`pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type`, the "did `pop` actually
retry?" question needs a hook — the least invasive is a `#[cfg(loom)]`-only counter inside
`pop`'s `Err(actual) => …` arm, which is already `#[cfg(loom)]` territory in this crate (see
`cas_head_for_test`). Second-best, if touching `pop` is unwelcome: promote the prose
"Verified: …" claim into a real shipped counterfactual by adding a `#[cfg(loom)]` variant of
`pop` whose failure ordering is `Relaxed` (the same trick `bug_pop_drain_to_empty` already uses
for H-2) and a `#[should_panic]` test driving it.

---

## P3 — worth fixing, not blocking

### P3-1. The ABA wrap-time derivation's stated mechanism does not cover the regime in which this crate's own benchmark measures its **highest** push rate — and the accompanying benchmark parenthetical is falsified by that same benchmark

**Location:** `src/lib.rs:106-127` and the mirrored `README.md:74-94`; the parenthetical at
`src/lib.rs:115-116` / `README.md:82-83`.

The derivation reads:

> "the rate term is bounded by HARDWARE, not by the workload. … every one of those pushes
> serializes on a single cache line **whose exclusive ownership must transfer between cores**.
> That transfer cost caps the aggregate rate at roughly `10^8` to `10^9` RMWs/sec no matter how
> many threads contend — more contention only makes the line's ownership transfers slower, never
> faster. (This crate's own benchmarks peak around `10^6` to `10^7` ops/sec, far under that
> ceiling.)"

Two problems, both measurable with the crate's own bench. Here is what
`cargo bench -p tagged-index-stack --bench tagged_index_stack_bench` actually printed on the
machine identified in the header:

```
  push_pop/single_thread     18925790 iters   1035.424 ms    54.71 ns/op
  pop/empty_fast_path       233827422 iters   1200.415 ms     5.13 ns/op
  churn                      18835603 iters   1030.952 ms    54.73 ns/op

contention/push_pop: 5256661 ops/sec total (8 threads, 1.001 sec measured)
contention/churn:    2893294 ops/sec total (8 threads, 1.001 sec measured, prefill=64)
```

**(a) The mechanism is wrong for the fastest regime.** The *highest* successful-push rate this
crate can sustain is the **uncontended, single-threaded** one — `churn` at 54.73 ns per
(pop + push) pair = **1.83 × 10^7 successful pushes/sec** — where the head line stays resident
and exclusive in one core's L1 and **no ownership transfer happens at all**. The contended rate
is `contention/churn`'s 2 893 294 ops/sec ÷ 2 (the bench counts `ops += 2` per pop+push pair) =
**1.45 × 10^6 pushes/sec** — *12.6× slower* than the uncontended case. So the paragraph's
argument ("more contention only makes the transfers slower, never faster") is directionally
right but describes the *slow* end; the fast end it must bound is governed by uncontended
`lock cmpxchg` latency (~20 cycles ⇒ ~2.5 × 10^8/s at 5 GHz), a completely different mechanism
that the text never mentions. The **conclusion survives** — 1.83 × 10^7 is still an order of
magnitude under the stated 2 × 10^8 working ceiling, and at the measured rate a 2^48 wrap takes
~178 days — but a passage that opens with "That is a derived claim, not a slogan"
(`src/lib.rs:5`) owes a derivation that covers the binding case.

**(b) The parenthetical is false at its top end, and mixes units.** "peak around `10^6` to
`10^7` ops/sec" — the single-threaded rows run at 54.71/54.73 ns per pop+push pair, i.e.
**3.65 × 10^7 ops/sec** by the same "ops" accounting the contention rows use. It is also the
wrong quantity: `wrap_time = 2^TAG_BITS / aggregate_successful_push_rate` needs *pushes*/sec,
and "ops/sec" here counts pops too — pops consume no tag budget, so the parenthetical
overstates the relevant rate by exactly 2× while understating the peak by ~3.6×. Finally the
number is uncited: no machine, no toolchain, no log — the number is doing real work in the
crate's headline safety argument.

**Fix.** (i) Add one sentence covering the uncontended case (the binding one) and state its
mechanism honestly — uncontended RMW latency, not coherence transfer. (ii) Replace the
parenthetical with measured *successful-push* rates and name the machine, or drop it entirely
and let the hardware ceiling carry the argument alone.

### P3-2. `pop` does not validate `load_next`'s return value, and the documented backing-corruption failure modes omit the truncation case

**Location:** `src/lib.rs:778-784` (`pop`'s use of `next`); contract at `src/lib.rs:653-658`
(`push`'s `# Caller contract`, rule 4).

`pop` does:

```rust
let next = links.load_next(index);
let new_head = if next == TAIL {
    TaggedIndex::<INDEX_BITS>::pack(TaggedIndex::<INDEX_BITS>::empty_index(), tag)
} else {
    TaggedIndex::<INDEX_BITS>::pack(next as u64, tag)
};
```

`pack` silently masks with `INDEX_MASK`. Rule 4 of the caller contract names exactly one
consequence of a misbehaving backing — a fresh zero-initialised `ArrayLinks` returning `0`,
making `pop`'s CAS a trivially-succeeding `current -> current` no-op and double-issuing one
index forever. It does not name the *truncation* consequence, which is a different and
arguably worse failure:

- `next = 0x1_0000` at `INDEX_BITS = 16` (neither `TAIL` nor in range) → `pack` masks to `0` →
  the head becomes index `0`, an index that may be live elsewhere. Same double-issue, reached
  by a completely different route than rule 4 describes.
- `next` whose low `INDEX_BITS` bits are all ones (e.g. `0xFFFF` at width 16, or `0x1_FFFF`)
  → `pack` produces a word that `is_empty` reads as **EMPTY**. The stack silently reports itself
  drained and **every remaining index in the chain is leaked at once** — no panic, no `None`
  anomaly, just a free-list that quietly shrinks to zero.

There is also a plain asymmetry worth naming: `push` pays an unconditional, release-active
`assert!` on `index` because "the failure mode is silent free-list corruption rather than a
merely-suboptimal fallback" (`src/lib.rs:677-680`), and `pop` pays nothing on `next` — although
both values funnel into the same `pack`, and this round's predecessor added `try_pack`
specifically to close this exact class of silent-truncation hazard in the *public* pack API
while leaving the internal path unguarded.

**Fix.** Minimum: extend rule 4 with the truncation mode (both variants — masked-to-a-live-index
and masked-to-the-empty-sentinel), since the current text implies the only hazard is the
zero-init one. Optional: a `debug_assert!(next == TAIL || (next as u64) < TaggedIndex::<INDEX_BITS>::INDEX_MASK, ...)`
in `pop`, which costs nothing in release and turns "silent whole-free-list leak" into a loud
failure in every test build a consumer runs.

### P3-3. `push`'s release-active `assert!` is not `#[track_caller]`, so a *caller*-contract violation points at the crate's own source line

**Location:** `src/lib.rs:691-695`.

```rust
pub fn push<L: Links + ?Sized>(&self, links: &L, index: u32) {
    assert!(
        (index as u64) < TaggedIndex::<INDEX_BITS>::INDEX_MASK,
        "index must be < INDEX_MASK (the empty sentinel is reserved)"
    );
```

The rustdoc justifies this being an `assert!` rather than a `debug_assert!` at length
(`:674-680`) on the grounds that it is "a caller-contract violation checked unconditionally".
For exactly that reason, the panic ought to name the caller. It doesn't: without
`#[track_caller]` the panic reports `crates/tagged-index-stack/src/lib.rs:692`, and a consumer
whose slab allocator calls `push` from eleven sites gets no indication which one.

The message also drops both operands. `"index must be < INDEX_MASK (the empty sentinel is
reserved)"` tells you neither the offending index nor `INDEX_MASK`'s value at this width — the
two facts you need. (`tests/stack_unit.rs:266-269` pins that exact string, so a fix must update
one `contains` assertion; the test matches on the prefix `"index must be < INDEX_MASK"`, so
appending `", got {index} (INDEX_MASK = {mask})"` keeps it green.)

**Fix.** `#[track_caller]` on `push`, or — if the extra implicit `&Location` argument on a hot
path is a concern — move the assert into a `#[cold] #[inline(never)] #[track_caller]` helper so
the caller-location cost lands only on the failing path.

### P3-4. `push`'s `Relaxed` CAS-failure ordering is the only ordering choice in the crate with no recorded justification — in a crate that ships a counterfactual for `pop`'s

**Location:** `src/lib.rs:724-727`.

```rust
// Release on success so a pop's Acquire sees the link we wrote;
// Relaxed on failure (retry).
```

Compare what the same crate spends on its neighbours: 25 lines on the `head` field explaining
why `pop`'s success ordering may be `Acquire`-only (`:537-560`), 12 lines inside `pop`
re-explaining it (`:785-790`), 5 lines on `pop`'s `Acquire` *failure* ordering (`:775-777`), a
45-line ordering essay on the `Links` trait (`:392-418`), and a shipped `#[should_panic]` loom
counterfactual proving `pop`'s failure ordering is load-bearing. `push`'s `Relaxed` failure
ordering gets four words and no reason.

It **is** sound — I derived it above (push never dereferences anything reached through the head;
the popper-side happens-before edge is carried by the release sequence, not by push's own
reads). But that derivation exists nowhere in the crate, which is a live hazard given the
crate's own documentation *elsewhere* teaches that a `Relaxed` CAS-failure ordering corrupts the
free-list. A maintainer reading
`counterfactual_relaxed_cas_failure_corrupts_free_list` and then seeing `Relaxed` in `push` has
every reason to "fix" it — or, worse, a maintainer optimising `pop` has no doc telling them the
two are asymmetric for a real reason.

**Related coverage gap:** no loom model runs two concurrent real `push`es, and none runs two
concurrent real `pop`s. Push's retry path is reachable only incidentally in the existing models
(via thread A's raw CAS interposing in `aba_repush_keeps_free_list_conservation`), and nothing
asserts anything about it.

**Fix.** Two sentences at `:724-727` stating why `Relaxed` suffices for push and does not for
pop; optionally a `push`-vs-`push` conservation model to make the retry path a first-class
covered path.

### P3-5. Two perf items raised by **both** prior rounds were silently dropped when the doc half of the same finding was actioned

**Location (process):** round-2's Sol-codex P2-1
(`docs/reviews/2026-08-30-181147-tagged-index-stack-review-Sol-codex.md:108-114`) listed three
sub-items under one heading; only the first became a task (`tis-r2-T5`). Round 1's task #1676
deferred the perf pair as "speculative perf, needs AArch64 bench gate" — a legitimate defer —
but the round-2 re-raise was never routed back to that deferral, so there is now no task and no
recorded decline for either.

**Substance (i) — `push`'s initial `head.load(Ordering::Acquire)` (`src/lib.rs:696`) can be
`Relaxed`.** Push never dereferences anything reached through the head: it uses the index half
as a *value* to write into a link and the tag half as a *number* to bump. Correctness of the
eventual publication rests entirely on the `Release` CAS. Note the loop is already internally
inconsistent about this — the retry read (`Err(actual) => head = actual`, at `Relaxed`) is the
same read at the same place in the algorithm, at a weaker ordering. On AArch64 this is `ldar`
vs. `ldr` once per push.

**Substance (ii) — both CAS loops are textbook `compare_exchange_weak` candidates.** Push and
pop both reload from `Err(actual)` and re-run the whole loop body, which is exactly `_weak`'s
contract; a spurious failure costs one wasted iteration and nothing else. On non-LSE AArch64
(the *default* for `aarch64-unknown-linux-gnu`, which targets baseline ARMv8.0), 32-bit ARM,
RISC-V and PowerPC, a *strong* `compare_exchange` compiles to an `ldaxr`/`stlxr` pair wrapped in
an additional inner retry loop whose only job is masking spurious store-exclusive failures this
code would handle anyway. Zero difference on x86-64 (`lock cmpxchg`) and on LSE AArch64
(`casal`) — which is why the measured numbers in P3-1 cannot show it, and why this needs an
AArch64 gate exactly as task #1676 said.

**Fix.** Either land both under an AArch64 measurement, or record an explicit decline (with the
reason) so the third re-raise doesn't cost another round. The current state — raised twice,
tasked zero times, declined zero times — is the worst of the three.

### P3-6. `src/lib.rs` is 73 % comment, past the ≤ 55 % target this repo set for its sibling crate

**Location:** `crates/tagged-index-stack/src/lib.rs` — 860 lines total, **631 comment/doc lines**
(`grep -cE '^\s*(//|/\*|\*)'`), 31 blank, leaving ~198 lines of actual code. That is 3.2 doc
lines per code line.

The precedent is this repo's own: task #1638 drove `size-classes`'s rustdoc from 65.3 % down to
a ≤ 55 % target for exactly this reason, and tasks #1545/#1589 before it. This crate is
*higher* than size-classes ever was.

The passages that read as review-response prose rather than API documentation:

| Location | Lines | What it is |
|---|---:|---|
| `src/lib.rs:218-254` | 37 | `_CHECK_BITS`'s rationale, including a paragraph on why `INDEX_BITS > 32` "could never buy reachable index range anyway" — a width the type can no longer express |
| `src/lib.rs:278-297` | 20 | `pack`'s index-truncation essay, ending in "Note the two bounds are deliberately different ranges, not a typo" — a reply to a reviewer, in published rustdoc |
| `src/lib.rs:339-359` | 21 | `empty()`'s doc, of which 12 lines are a disclaimer about what `#[doc(hidden)]` does and does not enforce |
| `src/lib.rs:392-418` | 27 | the `Links` ordering contract, including "read this as considered defence-in-depth for an openly-implementable trait, **not naivety**" |
| `src/lib.rs:537-560` | 24 | the `head` field INVARIANT (this one earns its length — it is the crate's most load-bearing internal note) |
| `src/lib.rs:601-672` | 72 | `push`'s `# Caller contract`, a numbered five-point list plus two prose restatements of the same failure mode |

Several of these are additionally duplicated near-verbatim across `README.md` and
`CHANGELOG.md` — which is what P3-7, P3-8, P4-1 and P4-2 below are: the drift that duplication
has already produced, in a crate that has been through two review rounds.

**Fix.** The mechanical version: one "Invariants" section in the crate doc holding H-2, RAD-1,
the caller contract and the ordering rationale *once*, with short references from each item's
own doc — the shape round 2's Sol-codex review already recommended
(`…:177`) and that nothing has acted on.

### P3-7. README says "**Three** hard-won subtleties"; the crate doc says "**The two** hard-won subtleties" — and calls both "structurally enforced", which the third is not

**Location:** `README.md:39` vs. `src/lib.rs:17`, `src/lib.rs:50`, `src/lib.rs:609`.

`README.md:39` heads a three-bullet list: H-2, RAD-1, and "**No double-push (caller-enforced).**"
`src/lib.rs:17-20` says:

> "The two subtleties people get wrong (documented below) are the **H-2 empty-transition tag
> preservation** and the **lazy link discipline** (internally: RAD-1); both are structurally
> enforced here."

and `src/lib.rs:50` heads the section "# The two hard-won subtleties".

This is round 1's P1-1 remediation landing asymmetrically: the no-double-push contract was added
to the README's list and to `push`'s `# Caller contract`, but the crate-root section it belongs
to was never updated. A docs.rs reader gets two subtleties and is told both are structurally
enforced; a GitHub reader gets three and is told the third is not. Since `push`'s own doc at
`:609-610` already writes "Unlike the two subtleties the crate root documents — H-2 and RAD-1 —
this one is enforced by caller discipline, not structurally", the crate is internally consistent
about the *fact* and inconsistent only about the *count* — which is the tell that the two
documents drifted rather than that anyone decided.

**Fix.** Make the crate doc's section three items and match the README's "(caller-enforced)"
qualifier, or make the README two-plus-a-separate-contract-section. Either way, once.

### P3-8. `TaggedIndex::empty()` is an undisclosed `#[doc(hidden)] pub` item — CHANGELOG lists it as an ordinary helper, and its own doc's justification for being `pub` is falsified inside this workspace

**Location:** `src/lib.rs:339-364` (the item), `CHANGELOG.md:41-43`, `README.md:141`.

Three separate problems, all in the same square inch:

1. **CHANGELOG contradicts the source.** `CHANGELOG.md:41-43` lists "helpers
   `pack`/`unpack`/`empty`/`empty_index`/`is_empty`, all `const fn`" — `empty` presented as an
   ordinary member of the public API, with no mention that it is `#[doc(hidden)]` and carries
   no stability guarantee. `src/lib.rs:348-359` says the opposite, at length.
2. **README's disclosure is incomplete.** `README.md:141`'s "Notes" section discloses exactly
   one hidden item, `raw_head`. `TaggedIndex::empty()` is the crate's *other* `#[doc(hidden)]
   pub` item and is not mentioned. Since the whole point of that Notes paragraph is telling
   consumers what not to depend on, listing one of two is worse than listing neither.
3. **The stated justification is factually wrong.** `src/lib.rs:355-357` says it is "`pub` for
   the `tests/` reason above alone, not as an invitation for other external use." I checked:
   `src/registry/bootstrap.rs:485` calls `TaggedIndex::<INDEX_BITS>::empty()` from *production*
   source (inside `#[cfg(loom)] mod loom_shim`) — a real non-`tests/` external caller, in this
   very repository, which is also the crate's flagship consumer. The parallel claim on
   `raw_head` ("not exercised by any production caller", `src/lib.rs:826-827`) **is** accurate —
   I verified `raw_head` appears only in this crate's own `tests/`. So the two doc-hidden items
   have the same disclaimer text and only one of them earns it.

**Fix.** Correct the CHANGELOG entry to flag `empty` as doc-hidden/unstable; add it to
README's Notes paragraph; and rewrite `src/lib.rs:355-357` to name the real second caller
(`sefer-alloc`'s `cfg(loom)` const-capable shim needs a `const fn` bootstrap word) instead of
claiming there isn't one. That third fix matters beyond accuracy: it means the item is not
freely removable in 0.2, which is worth knowing *before* 0.1.0 freezes.

### P3-9. The published loom-suite description overstates how much of the suite drives the real type

**Location:** `src/lib.rs:137-144`, `README.md:127-133`.

> "the shipped loom suite (`tests/loom_aba.rs`) model-checks the REAL `TaggedIndexStack` /
> `TaggedIndex` code, **not a transcription**"

I classified all eight models:

| Model | What it actually drives |
|---|---|
| `counterfactual_untagged_head_lets_aba_corrupt_free_list` | a locally-defined `UntaggedStack` — not the crate type at all |
| `counterfactual_empty_transition_tag_reset_lets_aba_recur` | thread B runs the locally-written `bug_pop_drain_to_empty` |
| `pop_empty_transition_preserves_tag` | thread B runs the **real** `pop`/`push`; thread A is hand-inlined |
| `aba_repush_keeps_free_list_conservation` | thread A hand-inlines `pop`'s body via `cas_head_for_test`; thread B is real |
| `tagged_stack_survives_the_same_resurrection_pattern` | same shape — A hand-inlined, B real |
| `cas_retry_path_must_acquire_with_concurrent_push` | thread A hand-unrolls **two** iterations of `pop` with hardcoded orderings |
| `counterfactual_relaxed_cas_failure_corrupts_free_list` | same harness, Relaxed |
| `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type` | **the only model where both threads call the shipped `pop`/`push` end-to-end** |

The test file itself is scrupulously honest about every one of these — `:145-148` ("This is the
ONE model that is not the crate type"), `:298-306`, `:539-543`. The *published* crate doc and
README are not; they generalise the file's strongest model to the whole suite. That is a real
inaccuracy for a reader choosing this crate over an alternative on the strength of that
sentence — and it compounds P2-2, since the single genuinely end-to-end model is also the one
with no counterfactual.

**Fix.** One clause: "…model-checks the real type — one model end-to-end through the shipped
`push`/`pop`, the rest driving the real head atomic and the real packing through
`cas_head_for_test` so an interleaving can be pinned."

---

## P4 — minor / cosmetic

**P4-1. Third copy of the unsupported-target list, already drifting.**
`CHANGELOG.md:102-106` names "(`thumbv6m-none-eabi`, `riscv32imc-…`, `armv5te-…`)" and omits
`thumbv7em-none-eabi`, which `src/lib.rs:154`, `src/lib.rs:177` and `README.md:121` all include.
Three hand-maintained copies of one list; one has already lost an entry.

**P4-2. "guards … are release-active `assert!`s" is plural; there is one.**
`CHANGELOG.md:99-101`: "Push's index-validity **and** sentinel **guards** are release-active
`assert!`**s**". `src/lib.rs:692` is a single `assert!` whose *one* condition covers both
purposes — which is the point `src/lib.rs:594-599` makes explicitly ("one guard covers both
conditions, no separate `TAIL` assertion is needed"). The CHANGELOG describes a shape the code
deliberately does not have.

**P4-3. A distinction without a difference, presented as a safety property.**
`src/lib.rs:706-710` says the code keeps "`is_empty` a dedicated check on the raw word rather
than deriving emptiness from the unpacked index … so the invariant never rests on any numeric
coincidence." But `TaggedIndex::is_empty(word)` *is* `(word & INDEX_MASK) == INDEX_MASK`
(`src/lib.rs:383-385`), i.e. bit-for-bit `cur_idx == INDEX_MASK` — literally the derivation it
claims to be avoiding, since `cur_idx` came from `unpack`'s `word & INDEX_MASK` three lines
earlier. The genuine protection is `INDEX_MASK != TAIL`, which the same comment already states
correctly one sentence up. Suggest deleting the parenthetical.

**P4-4. `#![cfg_attr(not(test), no_std)]` is dead configuration.**
`src/lib.rs:163`. `cfg(test)` is set only when the *lib* target is compiled as a test harness,
and this crate has no `#[cfg(test)]` items (this repo bans inline unit tests outright). So
`not(test)` is always true in every real build, and the only effect is that `cargo test`'s
lib-target build silently is **not** `no_std` — i.e. the one build that could cheaply prove the
`no-std::no-alloc` category claim doesn't. Unconditional `#![no_std]` is simpler and strictly
stronger; nothing in the crate touches `std` (`core::array::from_fn` under loom is the only
non-atomic import).

**P4-5. `[[bench]]` omits `test = false`, against this workspace's own established convention.**
`Cargo.toml:36-38`. The sibling `crates/once-ptr-cell/Cargo.toml:35-40` sets it explicitly with
a comment saying why: "Stated explicitly rather than left to cargo's default: `cargo test` must
not treat this bench as a test target. ci.yml's clippy row relies on that." Same bench shape,
same ci.yml row, opposite manifest.

**P4-6. CI comment carries a hardcoded test count that is now wrong.**
`.github/workflows/ci.yml:1734`: "12 of the crate's **16** tests". Measured today: 23 non-loom
tests (15 + 4 + 4) and 8 loom tests. This is the exact shape of hardcoded count this repo's own
convention rejects (`CLAUDE.md`, task #776/F10, which says to re-derive counts via a command
rather than quote a figure). The comment is historical narrative, so the fix is to phrase it
as history ("at the time, 12 of 16 …") rather than as current state.

**P4-7. MSRV is neither documented nor, apparently, chosen.**
`Cargo.toml:5` declares `rust-version = "1.88"`, inherited from the workspace root's "resocks5
ecosystem floor". Neither `README.md` nor `CHANGELOG.md` mentions an MSRV at all — the sibling
`size-classes` closed exactly this twice (tasks #1553, #1599). Separately, 1.88 is far above
this crate's real floor: the newest feature it uses is the inline-const array repeat
`[const { AtomicU32::new(0) }; N]` (`src/lib.rs:468`, stable 1.79); everything else
(`target_has_atomic` cfg 1.60, `assert!` in const 1.57, `core::array::from_fn` 1.63,
`[lints] workspace` 1.74) is older. For a dependency-free `no_std` leaf crate aimed at embedded
consumers — the population most likely to be pinned to an older toolchain — that is several
releases of unnecessary exclusion, inherited rather than decided.

**P4-8. The README example is compiled by nothing.**
`README.md:145-153`. This repo bans doctests in `src/**` (`CLAUDE.md`, "No doctests"), and the
README is not `#![doc = include_str!(...)]`'d, so the only executable-looking artefact a
prospective user reads has no compile gate at all. It happens to be correct today (I checked the
types and the `#[must_use]` binding). A six-line `tests/readme_example.rs` mirroring it would
pin it; `size-classes` did the equivalent (task #1635).

**P4-9. `single_slot_seeded`'s generality is dead, and its doc is inaccurate for the only call
site.** `tests/loom_aba.rs:386-417`. The function carries a 12-line doc and a
push/pop seeding loop parameterised on `target_tag`, and is called exactly once — with `1`,
where the loop runs zero iterations and the result is precisely one push away from bootstrap.
Its own doc calls that "the realistic steady state for the H-2 scenario, **not a bootstrap
artifact**", which is not true of `target_tag = 1`. (Nor can a higher tag be used: a
reset-to-0-then-one-push drain can only collide with a snapshot at tag 1, so 1 is the *only*
seed that exercises the counterfactual. The right fix is to say that, and drop the loop.)

**P4-10. `TaggedIndex` is a constructible public unit struct despite its doc calling it a
namespace.** `src/lib.rs:214-215`: `#[derive(Debug, Clone, Copy)] pub struct
TaggedIndex<const INDEX_BITS: u32>;`. Its own doc (`:212-213`) says "This is a zero-sized
namespace of `const fn` bit operations — no state, no memory". As declared, `TaggedIndex::<16>`
is a value anyone can construct, clone and `Debug`-print; adding a private field later to
prevent that would be a breaking change. The shape freezes at 0.1.0. (An uninhabited `enum`, or
a struct with one private `PhantomData`, expresses "namespace" and stays open. Low value, but
it is a one-line decision that becomes irreversible on publish.)

**P4-11. `try_pack` is tested at one width and absent from the property tests.**
`tests/stack_unit.rs:98-142` covers it at `INDEX_BITS = 16` only.
`tests/proptest_pack_unpack.rs` runs `pack`/`unpack` round-trips at widths 1, 12, 15 and 16 and
never touches `try_pack` — even though `try_pack`'s entire contract is "returns exactly what
`pack` returns for in-range inputs", which is a property, at every width, and which those four
existing properties already have both operands in scope for. One added line per property
(`prop_assert_eq!(T::try_pack(index, tag), Some(T::pack(index, tag)));`) covers the new API at
all four widths for free. Notably width 1 is where `try_pack`'s `1u64 << Self::TAG_BITS` runs at
`TAG_BITS = 63`, the closest this crate gets to a shift-overflow boundary — and that is exactly
the width it is not tested at.

**P4-12. `pack`'s doc documents half its own contract.**
`src/lib.rs:278-297` spends 20 lines on the *index* half's silent truncation (including its
sharpest sub-case) and says nothing whatsoever about the *tag* half's — `tag << INDEX_BITS`
silently drops every bit at or above `2^TAG_BITS`. That fact is stated only in `try_pack`'s doc
(`:310-312`) and is structurally relied upon by `push`'s `wrapping_add(1)` at the wrap boundary
(pinned by `tests/stack_unit.rs:164-188`). A caller reading `pack`'s own rustdoc — the
documented "trusted fast primitive" — learns one of its two truncation behaviours.

---

## Considered adversarially and **rejected** — recorded so they are not re-filed

- **"No `#[inline]` anywhere in the crate"** (0 occurrences; the same finding was real and
  actioned for `size-classes`, task #1538). **Not a finding here.** Every public function in
  this crate is generic over at least one parameter — `INDEX_BITS`, `N`, or `L` — so rustc's
  `cross_crate_inlinable` query returns true for all of them regardless of attributes
  (monomorphisation-requiring generics are cross-crate inlinable by construction), giving them
  `InstantiationMode::LocalCopy`: a per-CGU internal copy in each crate and CGU that uses them,
  which LLVM can inline without LTO. Adding `#[inline]` would be attribute noise with no
  codegen change. The `size-classes` case differed because that crate's hot entry points are
  *non-generic* inherent methods.
- **`pop`'s `Acquire`-only success ordering.** Re-derived from the C++20 release-sequence rule
  rather than trusting the field comment; I separately verified the premise it rests on (no
  plain `store` to `head` exists anywhere in the crate). Sound. The field's `INVARIANT` doc is
  accurate, including its warning about a future `clear()`/`Drop`.
- **`pack`'s `tag << INDEX_BITS` at the wrap boundary** (`tag == 2^TAG_BITS` after
  `wrapping_add`). Not an overflow panic in any profile: Rust's `<<` traps only on a shift
  *amount* ≥ the type width, never on shifted-out bits, so the wrap-to-0 is well-defined even
  under `overflow-checks = true`. Pinned by `tests/stack_unit.rs:164-188` and
  `tests/regression_counter_wrap.rs:23-56`.
- **`try_pack`'s "redundant" explicit `_CHECK_BITS` forcing** (`src/lib.rs:319-325`). The
  stated rationale — that `||` short-circuiting could skip both `TAG_BITS` and `pack` — reads
  like over-defensiveness but is *correct for const-evaluation contexts*, where the interpreter
  genuinely may never reach `Self::TAG_BITS` on the first branch. Keep it.
- **`INDEX_BITS = 1` degenerate width.** Checked `INDEX_MASK == 1`, `is_empty` == bit 0, the
  63-bit tag, `push`'s `< INDEX_MASK` guard admitting only index 0, and the empty word's
  encoding. All correct; `tests/stack_unit.rs:327-338` covers the stack path and
  `proptest_pack_unpack.rs:25-39` the packing.
- **`TaggedIndex::empty()` reachable from a runtime drain.** Verified by grep: the only
  in-crate caller is `TaggedIndexStack::new` (both cfg variants). The H-2 hazard the doc warns
  about is not reachable through the shipped code.
- **`RegistryLinks` (the flagship consumer) tripping round 2's P1-1 backing-identity hazard.**
  Verified independently: it is constructed fresh per call from the single `&Registry`
  (`src/registry/heap_registry.rs:608, 624`), the index→cell mapping goes through the one
  `reg.slot()` accessor, the orderings match the trait contract, and
  `src/registry/heap_registry.rs:570-574` pins `NEXT_FREE_TAIL == tagged_index_stack::TAIL` with
  a `const` assert. Clean.
- **Tarball hygiene.** `cargo package --list` yields exactly the four source dirs plus README,
  CHANGELOG and both licenses. No stray files, no `.rush/`, no review docs.

---

## What is genuinely good

- The algorithm is right, and I attacked it from every angle I could construct: tag monotonicity
  across pops, the drain transition, the release-sequence status of every head write, the
  `TAIL`/`empty_index` sentinel split at every legal width, the shift boundary, and the
  degenerate width 1. Nothing broke.
- The `1..=16` cap (round 2's P1-2 fix) is a genuine structural improvement, not a documentation
  patch: it makes the historical `INDEX_MASK == TAIL` coincidence *unrepresentable* rather than
  merely warned about, and `_CHECK_BITS` really is forced from every public associated item — I
  traced each one.
- The three `#[should_panic]` counterfactuals are the right idea, correctly built, and they all
  actually fire (verified by running them, not by reading their names). The H-2 pair's two-flag
  rendezvous is the right way to make the "stale CAS must fail" assertion non-vacuous, and its
  doc explains why a free race would false-positive.
- `try_pack` (round 2's P2-2) is a clean addition and its test asserts value-equality with
  `pack` rather than merely `Some`, which is the right oracle.
- The bench's `contention/push_pop` re-push-exactly-what-you-popped discipline, and the
  drain-then-assert-empty before the churn prefill, are both correct and both non-obvious —
  round 1 found a real bug there and the fix is right.
- The `head` field's `INVARIANT` block is the single best piece of documentation in the crate:
  it states a rule, gives the reason, and names the specific future change that would break it.
  If the P3-6 trim happens, this is the passage to keep verbatim.

---

## Suggested order of work

1. **P1-1** — one-line manifest change, plus the CI/README command update and the two ci.yml
   comment corrections. Measure the consumer lockfile again to confirm (2 packages).
2. **P2-1 / P2-2** — decide the loom story: drop the preemption bound where affordable, add one
   activation oracle, and align the "proof" wording with whatever is actually true afterwards.
   These two are one work item, not two.
3. **P3-1** — re-derive the wrap-time paragraph's fast-path bound and replace the benchmark
   parenthetical with measured push rates + machine identity (or delete it).
4. **P3-7 / P3-8 / P4-1 / P4-2** — the four concrete drift instances between `src/lib.rs`,
   `README.md` and `CHANGELOG.md`, fixed together with P3-6's single-source-of-truth
   restructure so the fifth instance doesn't appear in round 4.
5. **P3-2 / P3-3 / P3-4** — three small, independent hardening/doc edits in `push`/`pop`.
6. **P3-5** — land or explicitly decline the two twice-raised perf items; do not leave them
   unrouted a third time.
7. The remaining P4s as a single bundle.
