# `aligned-vmem` — round-5 review (fresh pass over the post-CR-fix tree)

**Date:** 2026-08-13
**Scope:** `crates/vmem/` in full — `src/{lib,error,mock,fault_injection}.rs`, all 7 files under
`tests/`, `benches/vmem_bench.rs`, `examples/v20_849_unix_exact_reserve_hit_rate.rs`,
`Cargo.toml`, `README.md`; plus every `aligned-vmem`-touching part of
`.github/workflows/ci.yml`, `scripts/{check-all,vmem-doc-drift-guard}.mjs`,
`src/alloc_core/alloc_core_core_diag.rs`, `CHANGELOG.md` and
`docs/CORRECTNESS_OPEN_ITEMS.md`.
**Reviewed tree:** local `main` @ `7c6e4be95f79c2ce34e28253bb3b5107c33d04f7` (the round-4 CR-fix
commit). `git status --short` at session start showed exactly one entry, an untracked checkpoint
file (`?? docs/checkpoints/2026-08-13-0130.md`) unrelated to this crate.
`origin/main` = `8804fc91c1c0019c63afa605e9729a2f2475f576` — **confirmed by `git fetch`: 17 commits
unpushed, none of rounds 4/5 has ever run in real CI.** Every green claim below (and in rounds 1–4)
remains a local claim on one Windows 10 Pro x86_64 host with a 4 KiB page.
**Toolchain:** `cargo`/`rustc` stable as installed on this host.
**Nature:** read-only. Nothing in the repository was modified other than the creation of this
document. No `git add` / `git commit` / `git push` / version bump. Every command quoted below was
actually run on this host; every `file:line` citation was read in the current tree before being
written down.

**Two executed experiments, both run OUTSIDE the repository.** Q1's counterfactual required editing
`crates/vmem/src/lib.rs` and `crates/vmem/tests/lazy_commit.rs`; Q4/Q8's guard prototype required a
scratch `.mjs` file. Both were done under `%TEMP%` (`$TEMP/vmem_r5`, a `cp -r` of `src/`, `tests/`
and `Cargo.toml` with `[dev-dependencies]`/`[[bench]]`/`[[example]]` stripped and `[workspace]`
appended so it resolved standalone). The prototype guard was *run against the real repository
files* but only ever **read** them. The repository working tree was never written to; the temp
directory was deleted afterwards (`rm -rf`, verified). Everything reported from those experiments
is a real observed test result / exit code, not a prediction.

**Relationship to the prior four rounds.** This pass does not re-report V1–V21, W1–W16 + P-A/P-B/P-C,
F1–F11, R1–R13, or CR1–CR10. All of R1–R13's remediation and all of CR1/CR3–CR8/CR10's fixes were
spot-checked in the current tree and hold (see "Checked and explicitly NOT findings"). To stay
unambiguous against the `V`-, `W`-, `P`-, `F`-, `R`- and `CR`-series, this round's findings are
numbered **Q1…Q9**.

---

## Verdict up front

**Round 4's own closing note is right: the crate's *source* has converged, and its *verification*
has not.** Five rounds have now failed to find a soundness hole, a race, a panic-safety gap, a
provenance defect, or a leak in `lib.rs`. I re-read every `unsafe` block, every `#[cfg]` split, both
release paths and all three backends and found nothing new there. **Performance: null for the fifth
round running** — I am stating that rather than manufacturing an item, and R8's `VirtualAlloc2` note
remains the only unexplored lever.

**The one substantive finding is again a verification finding, and it is in the exact place round 4
leaned hardest (Q1, MEDIUM).** `tests/lazy_commit.rs:88-92` — the #848 regression test's *named*
oracle, whose failure message reads "the reservation must cover the full requested span" — is
**vacuous by construction**: `finish_reservation` (`lib.rs:833-841`) sets `len: size` from the
*requested* size on every path, so `assert_eq!(r.len(), size)` cannot fail for any successful
reservation on any platform in any feature configuration. I reproduced round 4's own counterfactual
(delete `&& commit_len == size` from `lib.rs:1478`) and the test does fail — **at line 105, the
`commit_range` assertion, not at line 88**. The span-coverage claim the test is named after is
carried entirely by a *different* assertion than the one that states it. `assert!(r.reservation_len()
>= size)` is the two-sided oracle: I verified it fails ("got 4096") under the reintroduced bug and
passes on the restored guard.

**Q2 (MEDIUM) is the fourth and fifth instance of the defect class CR5 named and the round-4 closing
review explicitly told the next pass to grep for** ("fixing them together is also the natural moment
to grep for the *fourth* instance rather than waiting for round 5 to find it"). That grep was not
done. R3 fixed two root-crate forwarders, CR5 fixed the third — and both instances **inside
`aligned-vmem` itself** still state the Windows dispatch condition as `align <= 64 KiB` vs
`align > 64 KiB`, omitting the `commit_len == size` half that caused bug #848:
`crates/vmem/src/lib.rs:181-185` (the `bench-internals` design comment that the counter rustdocs
sit directly under, and which *those rustdocs* get right) and `crates/vmem/Cargo.toml:101-105` (the
feature doc that ships in the `.crate` tarball).

**Everything else is small.** Q3 is a CI clippy row that R1's fix left behind (verified currently
clean, so latent). Q4 is a sixth member of the over-reserve sentence family, found *by* the Q8
prototype. Q5 and Q6 are two more instances of "the fix corrected the cited line and left the
identical defect one file over" — the campaign phenomenon now at its **fourth and fifth**
consecutive occurrence. Q7 is a structural-convention slip in the durable index. Q8 is a concrete,
executed fix for the known-open CR2.

**Publish posture (task #658).** Nothing here is a soundness blocker and nothing here is a breaking
change. Before `cargo publish`: **Q2**'s `Cargo.toml` half ships to crates.io inside the tarball;
**Q6**/CR9 is the honest-premise decision round 4 deliberately left to a maintainer; **Q1** is the
one that costs real regression coverage if anyone ever tidies that test.

---

## What was verified green — every command below was executed on this host

| command | result |
|---|---|
| `cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" --no-fail-fast` | **green**, exit 0 — `fault_injection` 5, `huge_pages` 1, `lazy_commit` 11, `min_page` 2, `mock` 0, `smoke` 19, `vmemerror_io_bridge` 3, doctests 0; 0 failed |
| `cargo clippy -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" --all-targets -- -D warnings` | **green**, exit 0 (full recompile, 3m20s — not a cache hit). This row does **not** exist in `ci.yml`; see Q3 |
| counterfactual: delete `&& commit_len == size` (`lib.rs:1478`), run `cargo test --features lazy-commit --test lazy_commit` on the scratch copy | **FAILED** — 10 passed, 1 failed, panic at `tests\lazy_commit.rs:105`, **not** `:88` (Q1's evidence) |
| same, with `assert!(r.reservation_len() >= size)` added | **FAILED** at the new assertion: `PROPOSED ORACLE: … (got 4096)` (Q1's proposed fix, negative side) |
| same, guard restored, proposed assertion kept | **ok** — 11 passed, 0 failed (Q1's proposed fix, positive side) |
| prototype per-sentence drift guard vs. the real `lib.rs` + `README.md` + `Cargo.toml` | **1 flag** — `lib.rs:615` (Q4); zero false positives elsewhere |
| prototype vs. counterfactual A (synthetic), B (verbatim round-3 F4 sentence), C (verbatim round-4 R6 sentence) | **exit 1 on all three** — the current guard passes clean on B and C (CR2); see Q8 |
| `git fetch && git rev-parse origin/main` / `git log origin/main..HEAD --oneline \| wc -l` | `8804fc9` / **17** — round 4 + the CR-fix commit are still unpushed |
| `grep -rn "cfg(test)" crates/vmem/src` | no match — no inline test module, CLAUDE.md-conformant |
| `Doc-tests aligned_vmem … running 0 tests` | no doctests, CLAUDE.md-conformant |

---

# Findings

## Category 1 — verification: does the test actually test what it says?

### Q1 — MEDIUM — the #848 regression test's headline assertion (`assert_eq!(r.len(), size, "the reservation must cover the full requested span")`) is vacuous by construction; `Reservation::len` is copied from the *request*, never from what the backend reserved. Executed counterfactual: with the guard deleted, the test fails at the OTHER assertion, 17 lines below

**Citations.** `crates/vmem/tests/lazy_commit.rs:88-92` (the assertion), `:104-108` (the assertion
that actually discriminates), against `crates/vmem/src/lib.rs:833-841` (`finish_reservation`, which
hardcodes `len: size`), `:1273-1283` (`try_reserve_aligned_lazy`'s call into it), and `:1478` (the
guard).

**The mechanism.** Every reservation in this crate is finished through one of two helpers, and both
set the usable length from the caller's own argument:

```rust
// lib.rs:833-841
fn finish_reservation(size, align, raw) -> Result<Reservation, VmemError> {
    raw.map(|r| Reservation { base: r.base, len: size, /* … */ })
}
```

`len` is therefore `== size` for **every** successful `reserve_aligned` / `reserve_aligned_lazy` /
`reserve_aligned_huge` call, on every platform, in every feature configuration, no matter what the
backend actually reserved. `assert_eq!(r.len(), size)` is an identity check on the caller's own
input. The field that carries what the OS really gave back is `reservation_len`
(`:503-505`) — which on the buggy single-call path would be `commit_len` (`:1521`), i.e.
`initial_commit`.

**Executed counterfactual (real Windows host, throwaway copy under `%TEMP%`).** Replaced
`if align <= WIN_ALLOCATION_GRANULARITY && commit_len == size {` with
`if align <= WIN_ALLOCATION_GRANULARITY {` — nothing else changed — reproducing exactly bug #848:

```
thread 'lazy_reserve_small_align_still_reserves_full_span' panicked at tests\lazy_commit.rs:105:5:
commit_range beyond initial_commit must succeed even when align <= 64 KiB
test result: FAILED. 10 passed; 1 failed; 0 ignored
```

Line **105**, not line **88**. The assertion whose message is "the reservation must cover the full
requested span" passed while the reservation was 4 KiB and the requested span was 64 KiB.

**Failure scenario, concrete.** The test is 46 lines long with a 12-line comment; a future
maintainer trimming it (or splitting the "span coverage" claim into its own focused test, which is
exactly the shape the comment invites) keeps the assertion that *names* the invariant and drops the
`commit_range` block that *checks* it. Every CI row stays green — including the Windows
everything-except-`mock` row round 4 added specifically to make this guard non-vacuous — and the
`commit_len == size` guard is back to zero discriminating coverage, which is the state round-4's R1
opened a whole round to fix.

**Secondary consequence, stated separately because it is a coverage gap in its own right:** with
`r.len()` unable to observe the backend, **no test anywhere in this crate asserts anything about
`reservation_len()` on the Windows single-call fast path**. `grep -rn "reservation_len" crates/vmem/tests`
returns exactly two live assertions, both in `smoke.rs`
(`reservation_parts_prevents_parameter_swap:74`, `parts.len >= 4 * MIB`, on a 4 MiB align that never
takes the fast path) — so the one path where `reservation_len()` is documented to be *not the true
reservation size* (`lib.rs:491-500`, R6's own caveat) has no assertion at all.

**Fix (verified two-sided on this host).** Add one line next to the existing assertion:

```rust
assert!(
    r.reservation_len() >= size,
    "the OS reservation must cover the full requested span (got {})",
    r.reservation_len()
);
```

Under the reintroduced bug this fails with `got 4096`; with the guard restored the whole file is
`11 passed; 0 failed`. It is portable: the Unix fast path returns `reservation_len == size`, the
Unix/Windows over-reserve paths return `size + align`, and the Windows single-call path returns
`size` — all `>= size`. Consider also correcting the existing `assert_eq!(r.len(), size)`'s message
so it stops claiming to check the reservation (it checks that `len` echoes the request, which is
worth keeping, just not under that name).

## Category 2 — the recurring residue class, at its fourth and fifth occurrence

### Q2 — MEDIUM — the half-stated Windows dispatch condition R3 and CR5 chased through three root-crate accessors survives in **both** sites inside `aligned-vmem` itself, including the one that ships to crates.io

**Citations.** `crates/vmem/src/lib.rs:181-185`, `crates/vmem/Cargo.toml:101-105`, against
`crates/vmem/src/lib.rs:1478` (the real condition), `:219-227` and `:232-240` (the two counter
rustdocs, which are **correct**), and `src/alloc_core/alloc_core_core_diag.rs:129-137`, `:145-155`,
`:157-166` (the three root-crate forwarders, all now correct after R3 + CR5).

The real dispatch condition is `align <= WIN_ALLOCATION_GRANULARITY && commit_len == size`. Both
surviving sites state only the first half:

```rust
// lib.rs:181-185 — the bench-internals design comment
// - Windows: `win_reserve_commit` issues reserve+commit in either
//   one syscall (the fast path for `align <= 64 KiB`, over-reserving nothing —
//   base == region) or two syscalls (the traditional path for larger
//   alignments, over-reserving `size + align` and keeping the full mapping —
//   Windows cannot partially release a `MEM_RESERVE` region).
```

```toml
# Cargo.toml:101-105
# split by fast/slow path (`aligned_vmem::windows_reserve_commit_single_calls()` /
# `aligned_vmem::windows_reserve_commit_two_call_pairs()` — the fast path
# issues a single syscall for `align <= 64 KiB`, the traditional path issues
# two syscalls for `align > 64 KiB`), for cross-platform comparison.
```

Both are *descriptions of `win_reserve_commit`'s dispatch*, so unlike `reserve_aligned`'s own
rustdoc (where `commit_len == size` always holds and the align-only phrasing is accurate in
context), the omission is simply wrong here. The `lib.rs` instance is the more pointed one: R11's
fix made both counter rustdocs state the condition in full (`:223`: "when `align <= 64 KiB` **and**
`commit_len == size`"; `:238-239`: "when `align > 64 KiB` **or** when `commit_len != size`"), and
they sit 35 lines below a design comment that contradicts them. The `Cargo.toml` instance is inside
the `bench-internals` feature comment, which is shipped verbatim in the published `.crate` tarball.

**Failure scenario, concrete.** A measurement round on a `bench-internals` build (the root crate's
`bench-internals` pulls in `aligned-vmem/lazy-commit` — root `Cargo.toml:579`,
`bench-internals = ["aligned-vmem?/bench-internals", "aligned-vmem?/lazy-commit"]`, re-read here) reads
either of these two sentences, sees `WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS` incrementing on a
workload whose `align` is 4 KiB, and concludes the counters are broken or the build is
misconfigured — when in fact `commit_len != size` is routing every lazy reservation down the
two-call path exactly as designed. This is the same mis-attribution R3 item 2 spelled out for the
root-crate forwarders, one layer down.

**Also in the same family, weaker and arguably defensible, listed so it is not silently skipped:**
`Cargo.toml:7`'s published `description` and `README.md:40` both say "Windows with align <= 64 KiB
uses single-call fast path with no over-reserve" without the `commit_len == size` half. Both are
scoped to `reserve_aligned`, where the second half always holds, so I am **not** filing them as
defects — but if the `lib.rs`/`Cargo.toml` pair is fixed, one clause ("…for a full-span reservation")
in each would retire the whole family.

**Fix.** `lib.rs:182` → "the fast path for `align <= 64 KiB` **on a full-span commit
(`commit_len == size`)**"; `lib.rs:183-184` → "the traditional path for larger alignments **or a
partial initial commit**". `Cargo.toml:103-105` → the same two clauses. Then grep the crate for the
sixth instance rather than waiting for round 6 —
`grep -rn '64 KiB' crates/vmem/ src/alloc_core/` is the whole search space and takes a minute.

### Q5 — LOW — CR8's fix corrected the `task #854/R6` mis-citation in `scripts/check-all.mjs` and left the identical mis-citation in the guard script's own header, four lines into the file it is about

`scripts/vmem-doc-drift-guard.mjs:4` against `scripts/check-all.mjs:237-238` (CR8's fix) and
commit `7c6e4be`'s own message.

```js
// path, Windows align <= 64 KiB fast path). See task #854/R6 for the full
// history.
```

CR8's finding was, verbatim, "`R4` this round is the `is_huge()` coverage finding, not a doc-drift
guard. Task #854 is round-2's W5 fix. The guard is task #871, closing R6. Three identifiers, none of
which pair correctly." The fix corrected `check-all.mjs`'s copy to
`// R6 (task #871; the guard W5/task #854 asked for two rounds ago):`. The guard's own header — the
canonical statement of the guard's provenance, and the one a reader of the guard actually sees —
still pairs round-2's task #854 with round-4's R6.

**Failure scenario, concrete.** Someone auditing the drift guard (which they will, because CR2 is
open and Q8 below proposes rewriting it) follows "task #854/R6" to `git log --grep '#854'`, lands on
round 2's W5 hygiene bundle, finds no guard in that commit, and concludes the header is describing a
different script. Cosmetic, but it is the **fourth consecutive occurrence** of the exact pattern
this campaign has now named three times (F3 → R3 → CR5/CR6 → this): the fix corrects the lines the
finding cited and leaves the identical defect in the sibling site the finding did not cite.

**Fix.** One line: `// See R6 / task #871 (implementing what W5 / task #854 asked for) for the full history.`

### Q6 — LOW, publish-relevant — CR9's false-premise text has a second, uncited site in `src/mock.rs`, and it contradicts what `docs/CORRECTNESS_OPEN_ITEMS.md` item 42 already gets right

*(This extends a known-open item — CR9 — with a site CR9 did not cite. It is not a re-report of CR9
itself, and I am **not** proposing the design decision CR9 correctly reserves for a maintainer.)*

`crates/vmem/src/mock.rs:52-53` and `:38`, against `crates/vmem/Cargo.toml:77-79` (CR9's cited site)
and `docs/CORRECTNESS_OPEN_ITEMS.md:1910` (item 42's `Current-number-or-verdict` card).

CR9 flagged one occurrence of the "not yet published + wrong task number" premise
(`Cargo.toml:78`: "this crate has not yet had its first `crates.io` publish (task #659)"). There is
a second, in the crate's own source:

```rust
// mock.rs:52-53
/// is not a hypothetical). Decided now, before this crate's first publish
/// (task #659) — adding it retroactively after publish would itself be the
/// breaking change this is meant to prevent.
```

and a third, weaker, at `mock.rs:38` ("…considered and deferred for this crate's first publish").
Task **#659** is *racy-ptr-cell*; `aligned-vmem`'s publish task is **#658**, whose own title records
that crates.io still shows 0.1.0 — i.e. the crate **has** had a first publish, so "before this
crate's first publish" is false for both sites, exactly as CR9 established for the `Cargo.toml` one.

**What makes this worth its own finding rather than a footnote to CR9:** `docs/CORRECTNESS_OPEN_ITEMS.md`
item 42 — the durable index, the artifact designed to be trusted without re-derivation — **already
has the right number**: "neither crate has real external consumers before its first publish
(`aligned-vmem`: task #658; `numa-shim`: task #657)". So the index and the source now disagree about
which task owns this crate's publish, and the source is the wrong one. That is the same shape as
CR3 (the round's own CHANGELOG desynchronized from the file it described) and CR4 (the durable index
mis-citing a SHA), except with the polarity reversed.

**Fix (text only — the design question stays open for the maintainer).** Correct `#659` → `#658` at
`Cargo.toml:78` and `mock.rs:53`; restate both premises honestly ("0.1.0 is already on crates.io, so
removing `mock` as a Cargo feature is already a breaking change; the deferral now rests on the
absence of real external consumers, not on the absence of a publish"). The `--cfg`-vs-Cargo-feature
conversion itself remains CR9's open maintainer decision, jointly with `numa-shim` per item 42's own
"Revisit condition (both crates jointly)".

## Category 3 — CI coverage

### Q3 — LOW-MEDIUM — R1's fix added everything-except-`mock` **test** rows and left the two **clippy** rows at default / `--all-features`, so the real-backend `lazy-commit` + `fault-injection` code has no `-D warnings` coverage on any CI row. Verified currently clean, so this is a latent gap, not an active break

`.github/workflows/ci.yml:154` (clippy, default features), `:157` (clippy, `--all-features`), against
`crates/vmem/src/lib.rs:1189-1209` (the unlinted block) and `:1268-1269`.

Round 4's R1 established the general principle — `--all-features` turns on `mock`, which *replaces*
the backend, so an `--all-features` invocation does not exercise the real code — and fixed it for
the six `cargo test` rows. The two `cargo clippy` rows in the `aligned-vmem-gates` job were not
revisited, and they inherit the identical property. Precisely:

* the **default** row compiles neither `lazy-commit` nor `fault-injection`, so
  `try_commit_range` does not exist;
* the **`--all-features`** row has `mock` on, so `try_commit_range`'s
  `#[cfg(not(feature = "mock"))]` block (`:1189-1209`) — including the
  `fault_injection::should_fail_commit()` call site and the `commit_range_impl` call — and
  `try_reserve_aligned_lazy`'s `#[cfg(not(feature = "mock"))] let raw = reserve_aligned_lazy_raw(…)`
  (`:1268-1269`) are `#[cfg]`-erased before clippy sees them.

Every other real-backend site *is* covered: `decommit`/`decommit_lazy`/`try_recommit`'s
`cfg(not(mock))` arms are linted by the default row, and `commit_range_impl` /
`reserve_aligned_lazy_raw` / the whole `unix_reserve` and `win_reserve_commit` machinery are linted
by the `--all-features` row (`#[cfg_attr(feature = "mock", allow(dead_code))]` suppresses only
`dead_code`, not other lints). So the uncovered surface is small and precisely bounded — which is
why this is LOW-MEDIUM and not higher.

**Honest current state:** I ran the missing row here —
`cargo clippy -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" --all-targets -- -D warnings`
— as a full recompile (3m20s, not a cache hit) and it is **green**. There is no latent lint today.
The finding is that a new one would land on `main` unnoticed, in the one region of the crate whose
lint coverage depends on a feature interaction nobody has re-checked since R1 taught this campaign
that the interaction exists.

**Failure scenario, concrete.** Someone adds a `let _ = …;` or a redundant closure to the
fault-injection branch of `try_commit_range` while wiring a new hook. `cargo clippy -p aligned-vmem
--all-targets` (default) does not compile the function; `--all-features` compiles the mock branch
instead; the `test-windows` / `test-macos` / `test-workspace` rows compile the code but run
`cargo test`, not clippy. It merges green and is discovered by whoever next runs clippy with an
explicit feature list — which, before this review, was nobody.

**Fix.** One line in the `aligned-vmem-gates` job, mirroring the wording R1's own fix used for the
test rows:

```yaml
      # R1/Q3: `--all-features` enables `mock`, which REPLACES the backend, so the
      # row above does not lint try_commit_range's real-backend branch. Explicit
      # everything-except-`mock` list, matching test-windows/test-macos.
      - run: cargo clippy -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" --all-targets -- -D warnings
```

### Q9 — INFO — three CI/coverage observations checked and deliberately NOT filed as findings, plus one that is genuinely uncovered but not worth a row

Stated explicitly so round 6 does not re-derive them:

1. **`tests/huge_pages.rs`'s four Linux-only tests run on Linux only under `--all-features`
   (i.e. with `mock` on) — and they are non-vacuous there.** I traced it: `try_reserve_aligned_huge`
   (`lib.rs:1348-1361`) records a mock `Call` but then calls the **real**
   `reserve_aligned_huge_raw`, which is *not* mock-gated (`:2069-2075`), so `unix_reserve`'s
   huge-page alignment rejection (`:1858-1864`) genuinely executes. The file's own module doc
   (`:19-32`) claims exactly this and the claim is correct. **Not a finding.**
2. **`benches/vmem_bench.rs` and `examples/v20_849_unix_exact_reserve_hit_rate.rs` are both
   compiled in CI** (by `--all-targets` on the two clippy rows; the example's
   `required-features = ["bench-internals"]` is satisfied by `--all-features`). V18's original gap
   stays closed. **Not a finding.**
3. **The `bench-scale-tool = "0.1"` dev-dependency resolves from crates.io**
   (`Cargo.lock:64-67`, `source = "registry+…"`, real checksum), so it is not a `cargo publish`
   blocker. **Not a finding** — noted because round 4 had to strip it to build its scratch copy,
   which could read as a local-only dependency.
4. **Genuinely uncovered, filed as INFO only:** the `alloc-lazy-commit = ["lazy-commit"]` compat
   alias (`Cargo.toml:44`) — documented as "kept for one release to allow downstream consumers
   pinning the old name to continue building" — has no positive coverage anywhere. A *typo* in it
   would fail every cargo invocation, so the only silent failure mode is outright deletion, which is
   a semver break with no CI signal. A `cargo check -p aligned-vmem --features alloc-lazy-commit`
   row would close it; I do not think it earns one, and record it here instead.

## Category 4 — documentation drift

### Q4 — LOW — a sixth member of the over-reserve sentence family, in `from_raw_parts`'s rustdoc; found *by* the Q8 guard prototype on its first run against the real tree

`crates/vmem/src/lib.rs:613-616`.

> …it needs `base`/`len` too because the adopted reservation's usable span need not start at the OS
> reservation's own base (**this crate itself over-reserves `size + align` and keeps the full
> mapping**, which is exactly that shape).

Stated unconditionally, and false for two of the crate's four reservation paths: the Unix
exact-size fast-path **hit** (34.4–56.7% of reservations by the crate's own measured numbers,
`README.md:40`) over-reserves nothing, and the Windows `align <= 64 KiB` single-call path returns
`base == region` with no over-reserve at all (`lib.rs:1519-1521`). It is the same sentence family
W5 fixed three times, F4 a fourth, R6 a fifth — this is the sixth, and it is the *only* site the
Q8 prototype flags on the current tree, which is a useful calibration result in both directions:
the prototype has zero false positives across `lib.rs` + `README.md` + `Cargo.toml`, and its single
true positive is a real (if mild) instance nobody had noticed.

**Why LOW and not lower.** It is one parenthetical in an `unsafe fn`'s rationale, not a contract
statement, and no caller can be harmed by it. It is filed because this specific sentence family has
now drifted six times across five rounds, and because leaving it in place would make the Q8 guard
un-adoptable without either fixing it or adding an exemption on day one.

**Fix.** "…(this crate over-reserves `size + align` and keeps the full mapping **whenever the
exact-size fast path misses, or on Windows when `align > 64 KiB`**, which is exactly that shape)."

## Category 5 — process / conventions

### Q7 — INFO — `docs/CORRECTNESS_OPEN_ITEMS.md` item 42 is `Status: CLOSED` but still sits in the `[T] Tracked, not yet actioned` tier carrying its full closure narrative — the exact structural defect CLAUDE.md's R34-24 rule names

`docs/CORRECTNESS_OPEN_ITEMS.md:79` (the `### [T] Tracked, not yet actioned` heading), `:1880-1924`
(item 42, whose Status line reads "**Status:** CLOSED (updated 2026-08-09, task #778/F5 …)"),
`:2126` (`## Recently resolved (closure trail — do not re-list as open)`).

CLAUDE.md, "OPEN_ITEMS indexes are CURRENT-STATE, not archives (R34-24/task #543)", is explicit:
"**when an item is closed, its full closure narrative moves to the archive file (or, for the
correctness index which has no archive file yet, to its 'Recently resolved' section); the main index
keeps only a one-line pointer**… A closed item that still sits in an active tier (`[A]`, `[D]`,
`[L]`, `[T]`) with no Status-card update is a structural defect."

Item 42 is half-compliant: its Status card *was* updated to CLOSED (so it does not read as active to
anyone who reads the card), but its ~45-line narrative was never moved and no pointer was added to
"Recently resolved". A reader scanning the `[T]` tier's headings — which is the reading path the
rule exists to protect — sees an aligned-vmem `mock` deferral sitting under "Tracked, not yet
actioned".

**Not attributed to round 4:** item 42 was closed in August by task #778/F5, long before this
campaign's round 4. It surfaces here because CR9 (open) and Q6 both point at it as the authoritative
statement of the `mock` deferral policy, and it is the item a maintainer settling CR9 will read
first.

**Fix.** Move `:1880-1924` to the "Recently resolved" section and leave a one-line pointer in `[T]`,
per the rule's own prescription. Worth doing in whatever commit settles CR9, since that commit will
be editing this item's subject matter anyway.

### Q8 — INFO (closing a known-open item, not a new finding) — a concrete, executed fix for CR2: per-sentence + positional predicate, `unconditional` as an outright trigger, heading-aware sentence splitting, widened file list. Verified to catch all three counterfactuals the current guard passes, with zero false positives on the current tree except Q4

`scripts/vmem-doc-drift-guard.mjs:85-106` (the current predicate), `:16-28` (its KNOWN LIMITATION
header), `:40` (the single-file scope).

CR2 established that the current guard's `hasQualifier = /align|\bconditional\b|Windows/` is an OR
over words that *every* real historical drift sentence already contains, and asked for "a
per-sentence (not per-comment-block) positional check". I built one and ran it. The predicate:

```js
const TRIGGER   = /over-reserv|\btrim(s|med|ming)?\b/i;
const HARD_FAIL = /unconditional/i;          // a trigger sentence saying this is a violation, full stop
const SCOPE     = /\bif\b|\bwhen\b|\bunless\b|\bmay\b|\bmiss\b|fast[- ]path|slow[- ]path|
                   fall(s|ing)?[- ]?(back|through)|fallback|<=|>=|<|>|\bonly\b|\beither\b|
                   \bpaths?\b|rather than|no longer|instead/i;
// violation  ⟺  TRIGGER && (HARD_FAIL || !SCOPE)   — evaluated PER SENTENCE
```

Three implementation details that turned out to be load-bearing, each discovered by running it
rather than by reasoning about it:

1. **Sentence splitting must be heading-aware.** With a naive
   `split(/(?<=[.!?])\s+(?=[A-Z`*(\[])/)`, the `from_raw_parts` block's trigger sentence silently
   absorbed the following `# Safety` section (because `#` is not an opening token), and the `>=` in
   "`align` is a power of two `>= PAGE`" then "rescued" it — a false **negative** produced by the
   splitter, not the predicate. Adding `#` and `-` to the lookahead class fixed it and is what
   surfaced Q4.
2. **`align`/`Windows` must stop being qualifiers.** That is the whole of CR2: they are the words
   the drift sentences are *made of*. The replacement asks whether the sentence contains an actual
   condition (`if`/`when`/`<=`/`>`/`only`/`fast path`/`on a miss`/`may`/…).
3. **`unconditional` becomes a trigger, not a non-qualifier.** The `\bconditional\b` word-boundary
   fix round 4's zero-trust review added is correct but merely stops "unconditionally" from
   *helping*; it should actively *convict*.

**Executed results (all on this host, prototype under `%TEMP%`, repo files read-only):**

| input | current guard | prototype |
|---|---|---|
| A — synthetic `/// this crate unconditionally over-reserves memory and keeps the mapping` | FAIL | **FAIL** (exit 1) |
| B — **verbatim round-3 F4 sentence** re-injected into `reserve_aligned`'s rustdoc | **PASS (exit 0)** | **FAIL** (exit 1) |
| C — **verbatim round-4 R6 sentence** re-injected into the module `//!` doc | **PASS (exit 0)** | **FAIL** (exit 1) |
| current `crates/vmem/src/lib.rs` + `README.md` + `Cargo.toml` | OK | **1 flag: `lib.rs:615`** (Q4) — no other false positive |

B and C are CR2's own decisive cases; the prototype converts both from silent passes into failures,
which is the property CR2 said the guard has never had.

**Scope widening, also verified.** The current guard reads exactly one file (`:40`,
`${REPO_ROOT}/crates/vmem/src/lib.rs`), so two of the historical drift sites —
`crates/vmem/Cargo.toml:7` (the published crates.io description, named by round-3 F4) and
`crates/vmem/README.md:40` (named by W5) — are invisible to it by construction. The prototype reads
all three; both currently pass (each contains `<=`/`>`), so widening the file list costs nothing
today and closes the sixth-recurrence hole.

**Two honest limitations of the prototype, stated so it is not adopted as more than it is.**
(a) It scans only `///`/`//!` doc comments in `.rs` files, so plain `//` implementation comments —
including the `bench-internals` design block at `lib.rs:168-192` that Q2 is about — stay out of
scope. Widening to `//` produced one false positive immediately (`lib.rs:1926`, "Keep the entire
over-reserve mapping as the reservation", a correct statement of what the code at that exact point
does), so I left it out; Q2's defect is a *different* drift class (dispatch condition, not
over-reserve) and this guard is the wrong tool for it either way.
(b) The `SCOPE` list is a heuristic and will need one-off additions as prose evolves — but unlike
the current OR-qualifier, adding a term is a deliberate act with a visible diff, not something the
drift sentence supplies for free.

**If this is not adopted**, CR2's fallback still stands and is cheap: rewrite the guard's own
KNOWN LIMITATION header to say plainly that it catches only fully-unqualified sentences and **has
never been shown to fail on a real historical drift** — because as of `7c6e4be` it still has not,
and `npm run check` prints it green on every pre-push run.

---

## Checked and explicitly NOT findings

Round 4's R1–R13 and the closing review's CR1/CR3–CR8/CR10 were all re-verified in the current tree.
All hold:

* **CR1 (the deleted `--all-features` rows) — CLOSED and verified structurally.** `ci.yml:781` +
  `:785` (`test-windows`) and `:814` + `:819` (`test-macos`) are now two `-p aligned-vmem` steps
  each: the everything-except-`mock` row and the `--all-features` row. The comment blocks that CR1
  found attached to nothing ("The step below … runs with `--all-features`"; "This is the only row
  that runs `tests/mock.rs`") now describe steps that exist. `tests/mock.rs` runs on Windows and
  macOS again, which is what F1's 16 KiB-page fix needs.
* **CR3 — CLOSED.** `Cargo.toml:106-110` now names both crates' actual attribute placement
  ("the statics are `#[doc(hidden)]`, the accessor functions are public — sefer-alloc's own
  convention is the inverse attribute placement"), which I checked against `lib.rs:206-208`,
  `:215-217`, `:228-230`, `:241-243` (four `#[doc(hidden)] pub static`) and `:247-252`, `:278-283`,
  `:288-293` (accessors with no `doc(hidden)`). Correct about this crate for the first time.
* **CR5 — CLOSED.** `src/alloc_core/alloc_core_core_diag.rs:130-133` now reads "the single-call fast
  path (`align <= 64 KiB` and `commit_len == size`) and the two-call traditional path (everything
  else)". All three root-crate accessors now agree with `lib.rs:1478`. The two remaining instances
  are inside `aligned-vmem` itself — Q2.
* **CR6 — CLOSED.** `lib.rs:1825-1835` no longer describes tail `munmap` calls; it now states the
  failure mode in terms of the surviving whole-mapping `munmap` in `release_reservation`. I
  re-derived it: `grep -n libc_munmap crates/vmem/src/lib.rs` returns `:1918`, `:1996`, `:2017`,
  all whole-mapping, none a sub-range trim, exactly as the rewritten text says.
* **CR7, CR8, CR10 — CLOSED** (the design note's phantom `VirtualFree` is gone; `check-all.mjs:237`
  now cites R6/#871; `CHANGELOG.md` has the #867–874 entry and both round-4 review docs are tracked
  — `git ls-files docs/reviews | grep aligned-vmem` returns 9 files). CR8's sibling site is Q5.
* **R1/R2 — still closed.** The everything-except-`mock` rows exist on both platform jobs and I
  re-ran that exact feature set here: `fault_injection` 5 tests (0 under `--all-features`), `mock` 0
  (9 under `--all-features`) — the inversion R1 predicted, reproduced.
* **R4 — still closed and still non-vacuous.** `smoke.rs:56-63`
  (`ordinary_reservation_never_reports_huge`) and `huge_pages.rs:61-62` (the
  `#[cfg(not(target_os = "linux"))] assert!(!r.is_huge())`) both execute; smoke ran 19 tests here.
* **R5 — still closed.** `lib.rs:733-734` is `#[non_exhaustive] #[derive(Debug, PartialEq, Eq)]`; no
  `Clone`. A whole-repo grep finds no `.clone()` on a `ReservationParts`.
* **R9/R10/R11 — still correct.** `lib.rs:190-192` states the storage is gated too; `:376-379`'s
  note about the `debug_assert!`'s reach is accurate (I re-derived the call graph:
  `query_os_page_size` has exactly one caller, `page_size()`'s cold path at `:332`); both Windows
  counter docs say "successful" and `UNIX_EXACT_RESERVE_ATTEMPTS`'s doc discloses the
  before-the-`mmap` increment, verified at `:1972-1973` vs `:1976`.
* **R13 — still closed.** CLAUDE.md's exception 3 cites `crates/numa/src/lib.rs` and
  `crates/malloc-bench/src/lib.rs`; `crates/vmem/src/` (4 files) is no longer cited.

Also re-verified from rounds 1–3, weighted toward what a later commit could have undone:

* **V1's no-trim fix, W1's miri 3-tuples, W2's `HUGE_SUPPORTED`, P-A's free alignment-check skip,
  P-B's hoisted `page_size()`** — all intact at `:1926-1948`, `:2391-2409`, `:2126-2133` +
  `:1898`/`:2010`, `:1994`, `:992`/`:1032`.
* **`fault_injection`'s atomics unchanged and still correct** — `Release`/`Acquire` pair at
  `:108`/`:139`, `fetch_update` with the lazy-`then` underflow note at `:125-133`, the third
  disarm-vs-rearm race still declared out of scope at `:47-57`, and `tests/fault_injection.rs:34`'s
  `SERIAL` mutex still serialising the process-global hooks against libtest's thread pool.
* **`error.rs` unchanged and fully covered** — the three-way classification, the `io::Error` bridge
  (`:138-148`), and the de-duplicated `last_os_error_code` (`:150-160`), all three arms asserted in
  `tests/vmemerror_io_bridge.rs`.
* **`mock`'s partial-replacement shape is internally consistent.** `try_reserve_aligned_lazy`'s
  mock branch re-routes to the eager backend (`:1266-1269`) and records exactly one `ReserveLazy`;
  `reserve_aligned_raw` does not itself record, so there is no double-record. `release()` returns
  before recording on a null pointer while `Drop` always records — a deliberate asymmetry, not a bug.
* **`decommit`/`recommit` granularity asymmetry (`page_size()` vs `PAGE`) is deliberate and
  documented** — `README.md:100-106` states it and gives the reason (`decommit`'s `()` return has no
  write-permitting sentinel to misuse). On a 16 KiB-page host `recommit(base, 0, 4096)` returns
  `true`, but the Unix backend is a no-op and the pages are already accessible, so there is no
  reachable hazard. **Not a finding**, recorded so round 6 does not re-derive it.

---

## Categories with nothing to report

Stated explicitly, per the review mandate, rather than left silent:

* **Soundness / UB.** Nothing new, for the fifth round. I re-read every `unsafe` block in
  `lib.rs`; each carries a `// SAFETY:` comment that matches what the code does. The
  strict-provenance discipline (`.addr()` / `.with_addr()`) is complete at all four
  address-computation sites (`:1547`/`:1568`, `:1905`/`:1925`, `:1988`, `:2276`).
  `from_raw_parts`'s construction-time `Layout` validation (`:679-686`) covers both halves of the
  Drop-reachable-panic hazard. The Windows single-call path's `Ok((base, base, commit_len, …))`
  and the two-call path's `Ok((base, region, over, …))` both hand `release_reservation` a pointer
  it can legally `VirtualFree(.., 0, MEM_RELEASE)`. The Unix huge fallback's `granted_huge`
  assignment is correct on both branches. No aliasing, no double-free, no use-after-free,
  no leak path found.
* **Concurrency.** One shared-mutable-state module (`fault_injection`), re-audited above and
  unchanged since #718/#775. `PAGE_SIZE_CACHE` (`:166`, `:327-349`) remains a benign racy-init
  cache with an unambiguous `0` sentinel. `mock`'s state is thread-local and libtest gives each
  test its own thread. `Reservation`'s `Send`-not-`Sync` posture is unchanged and still pinned by
  `const _: () = assert_send::<Reservation>();` (`smoke.rs:20-21`).
* **Panic safety.** `#![deny(missing_docs)]` on; the only reachable panics are `from_raw_parts`'s
  two `expect`s and its `assert!`, all at construction time, none reachable from `Drop`.
* **Performance.** **Null, for the fifth round running**, and I am saying so rather than
  manufacturing an item. Every public entry point is one syscall deep; `page_size()` is a single
  relaxed load after the first call; `align_up_addr` is two arithmetic ops; the `bench-internals`
  counters are compiled out by default; the `fault-injection` hook is two relaxed loads that
  branch-predict not-taken and is compiled out entirely when the feature is off. The only
  unexplored lever remains `docs/perf/ALIGNED_VMEM_VIRTUALALLOC2_VA_OPTIMIZATION_OPPORTUNITY.md`
  (R8/task #874), which I re-read and am **not** re-deriving: its `Alignment`-parameter reasoning,
  its `GetProcAddress`-vs-link-time trade-off, and its Windows-version-floor gating question are
  all still the correct framing, and it correctly labels itself DESIGN-ONLY. I found nothing new
  on any platform.
* **Code smell / structure.** No `mod.rs` anywhere; no inline `#[cfg(test)] mod tests`; no
  doctests (`Doc-tests aligned_vmem … 0 tests`, and the module-doc example is a ```` ```text ````
  fence at `:52`); every `unsafe fn` has a `# Safety` section; no TODO, no placeholder, no
  commented-out code, no orphaned helper. Clippy is green on the default row, the `--all-features`
  row, and the row Q3 says should be added. The twelve
  `#[cfg_attr(feature = "mock", allow(dead_code))]` attributes remain individually justified at
  `:92-110` and remain, read the right way, an exact inventory of what `--all-features` cannot
  execute.
* **New safe `pub fn` accepting a raw pointer and touching allocator metadata** (CLAUDE.md's
  benchmark-hook rule). None. `decommit` / `decommit_lazy` / `recommit` / `commit_range` all take
  raw pointers and are all correctly `unsafe fn`; nothing in this crate matches the R25-1 shape.

---

## Recommended order

1. **Q1** — one `assert!(r.reservation_len() >= size)` in `tests/lazy_commit.rs`, plus a corrected
   message on the assertion above it. Verified two-sided on this host. It is the only finding here
   that touches real regression coverage of a bug that has already shipped once.
2. **Q2** — two doc clauses (`lib.rs:182-184`, `Cargo.toml:103-105`), then the one-minute
   `grep -rn '64 KiB' crates/vmem/ src/alloc_core/` that the round-4 closing review already asked
   for and that would have made this a round-4 fix instead of a round-5 finding.
3. **Q3** — one clippy row in `aligned-vmem-gates`. Green today; the point is that it stays that
   way for a reason instead of by luck.
4. **Q8 + Q4** — adopt (or explicitly decline) the guard rewrite. If adopted, Q4 is fixed in the
   same commit because the guard flags it. If declined, downgrade the guard's own header honestly
   per CR2's fallback — a green check that cannot fail on the thing it guards is the worst of the
   three states, and `npm run check` prints it on every pre-push run.
5. **Q5, Q6, Q7** — three text corrections, batchable in one pass. Q6's `#659` → `#658` should land
   whether or not CR9's design question is settled, since the wrong number is simply wrong.
6. **Push.** Everything above is downstream of the standing precondition round 3 set and rounds 4
   and 5 have both inherited: **none of this has ever run in CI.** `origin/main` is 17 commits
   behind. Per CLAUDE.md's own "Then confirm CI went green — do not assume it", the push and the
   landing-SHA confirmation are the real next gate, and the fixes above are worth folding in on the
   near side of it.

Nothing here is a breaking change. Nothing here reopens a V-, W-, P-, F-, R- or CR-series finding:
Q2 is *adjacent* to R3/CR5, Q4 to R6, Q5 to CR8 and Q6 to CR9, but each is a distinct site that the
corresponding finding did not cite.

## On "is round 5 padding?" — an honest answer

Round 4 predicted it: "A fifth round of reading `lib.rs` would be padding. A round that asks 'for
each guard in this crate, which CI invocation would fail if I deleted it?' would not be." That
prediction is confirmed in both directions. Reading `lib.rs` produced nothing — Q4 is a
parenthetical and I would not have filed it if a tool had not flagged it. **Deleting a guard and
watching which assertion noticed** produced Q1, the only MEDIUM here, and it produced it in the
single test round 4 had already executed a counterfactual against — because round 4 asked "does the
test fail?" and got the right answer, while the question that mattered was "which assertion fails,
and does it match the one the test is named after?"

The generalisation for round 6, if there is one: this campaign has now found, five times running,
that **the artifact describing the verification and the verification itself disagree** — R1 (a CI
comment claiming backend coverage it did not have), CR1 (a CI comment describing a deleted step),
CR2 (a guard whose green light does not discriminate), Q1 (an assertion message describing a check
it does not perform), Q3 (a clippy row believed to cover code it `#[cfg]`-erases). Every one was
found by executing the artifact and comparing, never by reading it. That is the technique with
remaining yield; a sixth pass over the source is not.
