# `aligned-vmem` — round-7 readonly review (post-round-6-closing, post-push, first round with real Darwin evidence in hand)

**Scope:** `crates/vmem/` in full (`src/lib.rs`, `src/error.rs`, `src/mock.rs`,
`src/fault_injection.rs`, all seven `tests/*.rs`, `benches/vmem_bench.rs`,
`examples/v20_849_unix_exact_reserve_hit_rate.rs`, `Cargo.toml`, `README.md`),
the `.github/workflows/ci.yml` rows that touch this crate,
`scripts/vmem-doc-drift-guard.mjs`, the root crate's `aligned_vmem` forwarders
(`src/alloc_core/alloc_core_core_diag.rs`), `CHANGELOG.md`'s round-6 entries,
and both open-items indexes' entries for this crate.

**Review type:** READ-ONLY. No file in the repository was modified by this
review other than the creation of this document. No `git add` / `git commit` /
`git push` / branch, worktree or ref mutation. Every command quoted below was
executed on this host (or read from the real GitHub Actions API); every
`file:line` citation was read in the current tree before being written down.

**Base revision:** local `main` @ `1dbd6b4` ("fix(vmem), docs: round-6 closing
review — fix SC1-SC10 …"). `git log origin/main..HEAD --oneline | wc -l` → **0**
— nothing unpushed; `origin/main` and local `main` are the same commit.
`git status --porcelain` shows exactly two untracked entries, both
pre-existing checkpoints, neither in this crate.

**Toolchain / host:** `rustc 1.97.0`, stable-x86_64-pc-windows-msvc; Windows 10
Pro, 4 KiB page. Installed cross targets include `x86_64-unknown-linux-gnu`
(used below for the Unix arm's lint checks). **No Darwin host and no Darwin
target** — but, for the first time in this campaign, that does not bound what
could be verified about macOS, because the round-6 artifacts finally *ran in
CI* (see "What was verified green").

**Finding prefix:** `T` (seventh round). Prior prefixes in use and deliberately
not reused: `V`/`W`/`P` (rounds 1–2), `F` (round 3), `R`/`CR` (round 4 + its
closing review), `Q`/`QC` (round 5 + its closing review), `S`/`SC` (round 6 +
its closing review).

**Date:** 2026-08-13.

---

## Verdict up front

Round 6's own remediation is, as far as this pass can tell, **clean**: I went
looking for the campaign's signature pattern (round N's fix creates round N+1's
findings) in `1dbd6b4`'s diff specifically, and the SC1–SC10 fixes hold. All
ten are correctly applied; the guard, the three clippy rows, `cargo fmt`, and
both feature-set test runs are green here; the drift guard passes; the root
crate's forwarder doc is corrected. That is the first time in seven rounds the
closing pass's *own* diff did not visibly generate the next round's headline.

What generated this round's headline instead is the **push**. `1dbd6b4` went to
`origin/main` and CI ran it — and the two artifacts round 6 built *specifically
so that CI would answer their questions* both ran on real `macos-26-arm64`
hardware and both passed:

```
test apple_silicon_page_size_is_16_kib ... ok
test macos_decommit_madvise_syscall_actually_succeeds ... ok
```

Those two lines settle, empirically, two questions this repository still
records as open in two current-state cards:

- **item 43's macOS half** — `_SC_PAGESIZE = 29` for Darwin is now
  *verified on hardware*, not reasoned from a header. Its card still says
  "awaiting real CI confirmation … this has not happened yet as of this
  filing."
- **item 48's H1-vs-H2 root cause** — `madvise(2)` returned `0` for BOTH the
  eager `MADV_DONTNEED` and the lazy `MADV_FREE_REUSABLE` call sites, so **H2
  (the syscall itself failed) is ruled out** and H1 (Darwin's advisory-only
  semantics) is the only remaining explanation. Its card still says "**The
  H1-vs-H2 question is therefore still OPEN**; do NOT read this note as
  confirming H1."

Neither card was updated. That is **T1**, this round's only MEDIUM, and it is
the exact failure mode CLAUDE.md's R34-24 rule ("OPEN_ITEMS indexes are
CURRENT-STATE, not archives") exists to prevent — with the aggravating detail
that item 43's own **Next trigger** spells out the action verbatim ("if
`apple_silicon_page_size_is_16_kib` passes, move the macOS half of this item to
'Recently resolved' with the run's citation") and it was not taken.

The rest is smaller and honest: one near-vacuous test whose doc comment
misattributes it (**T2**), one new test missing a `not(miri)` gate that the
round-6 closing review inspected and passed (**T3**), a four-site
macOS-vs-macOS+iOS inconsistency the SC2 sweep walked past (**T4**), seven
publish-facing citations of a path that is not in the published tarball
(**T5**, publish-relevant for task #658), and one genuine portability
asymmetry in the constant table (**T6**). Four INFO items close it out.

**Performance: null, seventh consecutive round.** Re-checked freshly rather
than inherited — see "Categories with nothing to report".

**Safety: null at HIGH/MEDIUM.** Every `unsafe` block was read against its call
site this round. Nothing unsound was found. Two INFO-level discipline items
(**T7**, **T8**) and one provenance-hygiene item (**T9**) are recorded.

**Publish readiness (task #658):** T5 should land before 0.2.0 — it is a
README/rustdoc edit, and it is exactly the class of thing that gets expensive
once external consumers exist. Nothing else here blocks.

---

## What was verified green — every command below was executed on this host

```
$ git fetch && git log origin/main..HEAD --oneline | wc -l
0                                       # HEAD == origin/main == 1dbd6b4

$ gh run list --commit 1dbd6b4ace30344d9b7eddccdb4d733bd03fa070
completed  success  CI                 main  push  31692217669  25m12s
completed  success  Kani verification  main  push  31692217668  44s

$ gh run view 31692217669 --json jobs -q '.jobs[]|select(.name|test("macos"))|"\(.name)\t\(.conclusion)\t\(.databaseId)"'
test macos (production)   success   94421845398

$ gh api repos/PHPCraftdream/sefer-alloc/actions/jobs/94421845398/logs
Image: macos-26-arm64                   # Apple Silicon => 16 KiB pages
  # step `cargo test -p aligned-vmem --features "lazy-commit huge-pages
  #   fault-injection bench-internals" --no-fail-fast`, tests/smoke.rs:
  running 21 tests
  test apple_silicon_page_size_is_16_kib ... ok
  test macos_decommit_madvise_syscall_actually_succeeds ... ok
  ... test result: ok. 21 passed; 0 failed
  # step `cargo test -p aligned-vmem --all-features --no-fail-fast`, tests/smoke.rs:
  running 20 tests
  test apple_silicon_page_size_is_16_kib ... ok          # runs under `mock` too
  ... test result: ok. 20 passed; 0 failed

$ cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" --no-fail-fast
fault_injection 5 / huge_pages 1 / lazy_commit 11 / min_page 2 / mock 0 /
smoke 20 / vmemerror_io_bridge 3 / doc-tests 0        => 42 passed, 0 failed

$ cargo test -p aligned-vmem --all-features --no-fail-fast
0/0/1/11/2/9/20/3/0                                   => 46 passed, 0 failed

$ cargo clippy -p aligned-vmem --all-targets -- -D warnings                          -> clean
$ cargo clippy -p aligned-vmem \
    --features "lazy-commit huge-pages fault-injection bench-internals" \
    --all-targets -- -D warnings                                                     -> clean
$ cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings           -> clean
$ cargo fmt -p aligned-vmem --check                                                  -> clean

$ node scripts/vmem-doc-drift-guard.mjs
[vmem-doc-drift-guard] OK: no unconditional over-reserve/trim statements found  (exit 0)

$ node scripts/verify-commit-prefixes.mjs
[verify-commit-prefixes] range: @{u}..HEAD (0 commits) — PASS  (nothing unpushed to lint)

$ cargo package -p aligned-vmem --list --allow-dirty
20 files: Cargo.{toml,lock,toml.orig}, .cargo_vcs_info.json, LICENSE-{MIT,APACHE},
README.md, src/{lib,error,mock,fault_injection}.rs, tests/*.rs (7),
benches/vmem_bench.rs, examples/v20_849_unix_exact_reserve_hit_rate.rs
                                        # NO `docs/` anywhere — see T5

$ RUSTFLAGS="-W unsafe_op_in_unsafe_fn" cargo clippy -p aligned-vmem \
    --features "lazy-commit huge-pages fault-injection bench-internals" --all-targets
4 warnings (Windows arm)                # see T7
$ ... --target x86_64-unknown-linux-gnu
5 warnings (Unix arm)                   # see T7
$ RUSTFLAGS="--cfg miri -W unsafe_op_in_unsafe_fn" cargo check -p aligned-vmem \
    --features "..." --all-targets
1 warning (miri arm); compiles clean otherwise
```

CI is green on the landing SHA read from the remote, not assumed. The
round-6-closing commit's own claim that all ten SC findings were fixed is
**verified** against the current tree, item by item, in the section below.

---

## Round-6 closing pass (`1dbd6b4`) — SC1–SC10 verification

Checked before looking for anything new, because six consecutive rounds have
found the closing fix to be the next round's bug source.

| # | Status in the current tree | Evidence |
|---|---|---|
| SC1 | **CLOSED (the honest floor, option (b))** | The oracle's rustdoc (`smoke.rs:405-416`) no longer claims `decommit_lazy_roundtrip` has a zero-fill assertion; it states the opposite and points at item 48's S4 sub-note. The remainder is filed at `docs/CORRECTNESS_OPEN_ITEMS.md:2123`. Option (a) (widen the `#[cfg]` to `unix` + a Linux CI row) was **not** taken — correctly recorded as still-open, not silently dropped. |
| SC2 | **CLOSED for the decommit gap; a *different* scope split survives** | All seven Darwin-gap sites now read "Darwin family (macOS/iOS/tvOS/watchOS)": `lib.rs:34-36`, `:1027`, `:1065-1078`, `:1151-1153`, `:2148-2159`, `smoke.rs:195-216`, `README.md:152-161`. The `decommit_lazy` **advice** scope was not swept → **T4**. |
| SC3 | **CLOSED** | `CHANGELOG.md:377` `#### aligned-vmem — round-6 follow-up (2026-08-13, tasks #880-886)` exists; `:370` corrected from "#880-885 … tracked, not fixed" to "#880-886 … landed and merged"; item 1's recurrence card bumped to 6. |
| SC4 | **CLOSED, all three sites** | `src/alloc_core/alloc_core_core_diag.rs:170-178` now says SIX and names the two it does not forward; `lib.rs:174` "Three independent questions, one instrument family each"; `Cargo.toml:110-113` names the madvise pair. |
| SC5 | **CLOSED** | `R9_5_VIRGIN_ZERO_SKIP_DESIGN.md:117-122` and `R11_8_SMALL_VIRGIN_ZERO_SKIP_DESIGN.md:32-36` both now scope the "now accurate" claim to Darwin + `decommit` and name `*BSD`/`decommit_lazy` as still uncited. |
| SC6 | **CLOSED, and better than asked** | `lib.rs:1109-1117` states the inversion on the RSS axis explicitly, keeps the zero-fill axis separate, and names the tvOS/watchOS fallback. |
| SC7 | **CLOSED** | Item 48's Status (`:2118`) enumerates all six caveat sites + the README; the duplicate "Root cause" bullets are merged into one (`:2120`); the S9 note stands on its own (`:2121`). |
| SC8 | **CLOSED** | `lib.rs:255-260` uses bare backticks for `libc_madvise` / `DecommitKind::*`; matches its two sibling counters. |
| SC9 | **CLOSED (comment)** | `smoke.rs:444-450`. |
| SC10 | **CLOSED (comment)** | `smoke.rs:460-467`. |

No conflict markers anywhere (`grep -rnE '^(<<<<<<<|>>>>>>>|=======)$'` over
`*.rs`/`*.md`/`*.toml`/`*.mjs`/`*.yml` → no output). No new `unsafe` token was
introduced by `1dbd6b4` (`git show 1dbd6b4 -- crates/vmem/src/lib.rs` adds no
`unsafe`). No public API changed.

---

## Category 1 — the evidence arrived and nothing recorded it

### T1 — MEDIUM — the round-6 push answered item 43's macOS half and item 48's H1-vs-H2 question with a green CI run 25 minutes later; both current-state cards still record both as unanswered, and item 43's own Next trigger names the exact action that was not taken

**Where:** `docs/CORRECTNESS_OPEN_ITEMS.md:1893-1897` and `:1908-1913`
(item 43's Status + Current-number), `:1941-1944` (item 43's Next trigger),
`:2118` and `:2120` (item 48's Status + Root cause); context at
`CHANGELOG.md:397`.

Round 6 built two artifacts whose *entire evidentiary purpose* was to run in
real CI, and correctly recorded them as unverified because no Darwin hardware
was available to the session that wrote them. Then `1dbd6b4` was pushed, CI run
`31692217669` completed green, and job `94421845398` (`test macos
(production)`, image `macos-26-arm64`) executed both of them, twice each (the
`bench-internals` row and the `--all-features` row), all passing. The raw job
log lines are quoted verbatim in "What was verified green" above.

What each result actually establishes:

1. **`apple_silicon_page_size_is_16_kib ... ok`** ⇒ `page_size()` returned
   exactly `16384` on aarch64 Darwin. That value is only reachable through
   `sysconf(_SC_PAGESIZE)` succeeding with the crate's Darwin constant `29`
   (`lib.rs:2303-2313`); a wrong constant would have fallen through
   `page_size()`'s silent guard (`lib.rs:399-403`) to `PAGE` = 4096 and failed
   the assertion. **item 43's macOS half is now empirically confirmed** — the
   first genuine hardware verification of any entry in that table.
2. **`macos_decommit_madvise_syscall_actually_succeeds ... ok`** ⇒
   `unix_madvise_attempts() == 2` **and** `unix_madvise_successes() == 2`
   (`smoke.rs:468-481`), i.e. `madvise(2)` returned `0` for both the eager
   `MADV_DONTNEED` and the lazy `MADV_FREE_REUSABLE` call sites on that runner.
   **H2 ("the madvise call itself failed") is ruled out.**

Item 43's card nevertheless still reads *"macOS half: assertion added, awaiting
real CI confirmation"* and *"the NEXT macOS CI run that includes this test will
be the first genuine empirical confirmation (or refutation) … this has not
happened yet as of this filing"*. Item 48's card still reads *"**has NOT yet
run on real macOS hardware** — this repo has none available"* and *"**The
H1-vs-H2 question is therefore still OPEN**; do NOT read this note as
confirming H1."*

**Failure scenario (concrete, and it nearly happened in this very session).** A
round-8 session performs CLAUDE.md's mandatory round-start read of both
indexes. It reads item 48's card, which instructs it in bold **not** to treat
H1 as confirmed. It therefore either (a) re-derives the question and re-runs
the oracle it has no hardware for, spending a task on an answer that already
exists in a job log, or — worse — (b) declines to start item 48's own Next
trigger (the `mmap(MAP_FIXED)` Darwin re-map), because that work's entire
justification is recorded as *asserted, not established*. S2 was filed
precisely to stop a future round implementing a real unsafe-surface-adding fix
on an unestablished premise; the premise is now established and the card says
it is not, which blocks the same work from the other side.

**Sub-note the update must not overstate (and this is the one place I would
push back on a naive closure).** The two halves of the H1 argument come from
**two different CI runs**: byte-survival from run `31676133649`
(`e60e46a`, before the assertion was scoped off) and madvise-success from run
`31692217669` (`1dbd6b4`). **No single run has ever observed both**, because
the run that could observe the stale byte is exactly the run in which the
assertion is now `#[cfg]`'d off for Darwin. The honest wording is "H2 ruled out
by run `31692217669`; combined with run `31676133649`'s stale byte, H1 is the
only remaining explanation" — not "confirmed by CI". If a round wants that
tightened to a single-process observation, the cheap version is a
`bench-internals` + macOS-gated test that asserts `successes == attempts` **and**
reads back the pre-decommit byte in the same process, i.e. the assertion
`9c777bc` deleted, re-added with its polarity inverted (`assert_ne!(…, 0)`
would be wrong — it should assert nothing about the value and simply record it,
or assert the documented non-guarantee explicitly).

**Fix:** update item 43's Status/Current-number/Next trigger and move its macOS
half to "Recently resolved" with the run citation (`31692217669`, job
`94421845398`); update item 48's Status and Root-cause bullets to record H2 as
excluded and H1 as the remaining explanation, with the two-run caveat above;
`CHANGELOG.md:397`'s closing instruction ("confirm the landing SHA goes green
… before treating S2/S6 as settled") has now been satisfied and nothing
anywhere records that it was — a one-line follow-up entry is the natural place.

---

## Category 2 — test oracles

### T2 — LOW — `ordinary_reservation_never_reports_huge` is near-vacuous: `reserve_aligned` hard-codes `granted_huge: false` two call frames up, so the assertion pins a literal against itself and cannot fail against the W2 bug its own doc comment names

**Where:** `crates/vmem/tests/smoke.rs:71-80`; `crates/vmem/src/lib.rs:955-967`
(the `granted_huge: false` literal at `:963`); the genuine regression test at
`crates/vmem/tests/huge_pages.rs:61-62`.

The test's doc comment says: *"Non-huge reservation never reports huge
(regression for W2 fix: non-Linux Unix used to return true for ordinary-page
reservations)."* The data flow is dispositive and needs no execution to check:

```
reserve_aligned(size, align)                          lib.rs:856-858
  -> try_reserve_aligned(size, align)                 lib.rs:941-967
       -> reserve_aligned_raw(..).map(|(b, r, rl)| RawReservation {
              base: b, reservation: r, reservation_len: rl,
              granted_huge: false,                    lib.rs:963  <-- literal
          })
       -> finish_reservation(..)                      lib.rs:900-913
          -> Reservation { granted_huge: r.granted_huge, .. }
```

`reserve_aligned` never requests huge pages and never consults the backend's
grant decision; `is_huge()` on its result is `false` on every platform, in
every feature configuration, unconditionally. W2's actual defect lived in
`HUGE_SUPPORTED` (`lib.rs:2245-2248`) on the *huge* path — reachable only via
`reserve_aligned_huge`. Reverting `HUGE_SUPPORTED` to an unconditional `true`
(W2's pre-fix state) leaves this test green; the only assertion in the crate
that would go red is `huge_pages.rs:61-62`'s
`#[cfg(not(target_os = "linux"))] assert!(!r.is_huge())`.

The strongest true statement about this test is: *it pins `is_huge()` to read
the `granted_huge` field rather than some other `bool` field.* That is worth
having; it is not what its doc comment claims.

**Failure scenario.** Round 4's **R4** filed *"`Reservation::is_huge()` has zero
test assertions anywhere in the crate, after three consecutive rounds rewrote
its contract."* This test was added in response. A future round reading
`smoke.rs:71-72` concludes the ordinary path's grant reporting is
regression-covered, and a change to `try_reserve_aligned`'s construction (the
natural refactor once `RawReservation` carries the field — e.g. propagating the
backend's real value, or a copy-paste of `try_reserve_aligned_huge`'s
`finish_reservation_huge` shape) ships with no test able to notice it. The
cheap honest fix is a one-sentence doc correction naming `huge_pages.rs:61-62`
as the real W2 regression test and stating what this one actually pins.

### T3 — LOW — `apple_silicon_page_size_is_16_kib` carries no `not(miri)` gate, and the crate's own miri backend hard-codes `page_size()` to `PAGE` — so the test fails by construction under miri on the one platform where it runs; the round-6 closing review inspected this test's `#[cfg]` and passed it

**Where:** `crates/vmem/tests/smoke.rs:337-341`; `crates/vmem/src/lib.rs:440-444`.

```rust
// lib.rs:440-444
#[cfg(miri)]
fn query_os_page_size() -> usize {
    // Miri has no real OS page; use the crate's constant granularity.
    PAGE
}
```

`page_size()` (`lib.rs:384-406`) accepts `PAGE` (4096) — it is `>= PAGE` and a
power of two — so under miri `page_size() == 4096` on every host, including
aarch64 Darwin. The test asserts `page_size() == 16 * 1024` gated only on
`all(target_os = "macos", target_arch = "aarch64")`.

Every other real-OS-property assertion in this crate excludes miri explicitly,
and each says why:

- `smoke.rs:209-216` — the zero-fill assertion: `#[cfg(not(any(miri, feature = "mock", target_os = "macos", …)))]`
- `smoke.rs:420-425` — the new madvise oracle: `not(miri)` in its own `#[cfg]`
- `smoke.rs:650-660` — `try_reserve_huge_size_…`: `#[cfg_attr(miri, ignore)]`
- `tests/lazy_commit.rs:343` — the zero-page read: `#[cfg(not(miri))]`

**Failure scenario (live today, not latent).** A macOS-arm64 contributor runs
`cargo +nightly miri test -p aligned-vmem --features "lazy-commit huge-pages
fault-injection bench-internals"` and gets
`assertion `left == right` failed: left: 4096, right: 16384` — a failure caused
entirely by the crate's own miri stand-in, with a message that reads like a
broken `_SC_PAGESIZE` constant, i.e. exactly the alarm this test exists to
raise. Separately, `docs/CORRECTNESS_OPEN_ITEMS.md:1872-1878` (item 41) records
as an owned next step *"a future task should add a `cargo miri test -p
aligned-vmem` CI step"*; the moment that lands on any Darwin runner, this test
is a hard red.

**Why this survived the closing review:** the round-6 closing review checked
this exact test and concluded (correctly, as far as it went) *"it carries no
feature gate, only `all(target_os = "macos", target_arch = "aarch64")`, so both
macOS CI steps execute it"* — it verified the **feature** gating, which was the
question S6 raised, and did not ask whether `miri` was the one cfg the test
*should* have excluded. The current CI does not catch it either: the miri gate
in `aligned-vmem-gates` is `RUSTFLAGS="--cfg miri" cargo check -p aligned-vmem
--all-features` (`ci.yml:171`) with no `--all-targets`, so integration tests
are not even compiled under that cfg, and it runs on `ubuntu-latest` where the
test's `target_os` gate excludes it anyway.

**Fix:** `#[cfg(all(target_os = "macos", target_arch = "aarch64", not(miri)))]`,
plus one sentence in the test's doc comment saying why (the crate's miri
aperture answers `PAGE` by construction, so the assertion is meaningless
there) — matching the pattern its four siblings already use.

---

## Category 3 — documentation scope and publish surface

### T4 — LOW — the lazy-decommit advice is documented as "macOS" at four sites while the code covers macOS **and iOS**; SC2's Darwin-scoping sweep added a paragraph that contradicts the summary line nine lines above it, inside a single rustdoc

**Where:** `crates/vmem/src/lib.rs:1098-1101` (`decommit_lazy`'s summary
sentence), `:1109-1117` (the paragraph added by `1dbd6b4`), `:1116` ("see the
`other Unix` arm above"), `:2192-2193` (`madv_free_advice`'s own doc comment),
`:2275` (`MADV_FREE_REUSABLE`'s doc), `crates/vmem/README.md:49`; versus the
code at `:2204-2207` and `:2276-2277`.

The code is unambiguous:

```rust
// lib.rs:2204-2207
#[cfg(any(target_os = "macos", target_os = "ios"))]
{ MADV_FREE_REUSABLE }
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "ios")))]
{ MADV_DONTNEED }
```

`decommit_lazy`'s **summary sentence** — the one rustdoc renders in the module
index and an IDE renders on hover — says:

> "(Linux `MADV_FREE`, **macOS `MADV_FREE_REUSABLE`, other Unix falls back to
> `MADV_DONTNEED`**; Windows falls back to the eager `decommit` path …)"

which places iOS in "other Unix". The paragraph `1dbd6b4` added nine lines
below it says the opposite, correctly: *"On **macOS/iOS** specifically … this
lazy variant's `MADV_FREE_REUSABLE` DOES drop the physical footprint
immediately"*, and then refers back to *"the `other Unix` arm above"* for
tvOS/watchOS — a back-reference to a sentence that, as written, swallows iOS
too. `madv_free_advice`'s own doc comment (`:2192-2193`) repeats the
macOS-only wording **two lines above** the `any(macos, ios)` cfg it describes,
and the README's API table (`:49`) is the third repetition on the crates.io
landing page.

This is the same class SC2 fixed (a scope word split across sites), one axis
over: SC2 swept the **decommit-gap** sentences and did not touch the
**advice-selection** sentences.

**Failure scenario.** An iOS consumer optimising for physical footprint reads
the summary line, classifies iOS as "other Unix" ⇒ `MADV_DONTNEED` ⇒ per item
48 a Darwin no-op, and concludes `decommit_lazy` buys nothing on iOS, so they
skip reclaim entirely. In fact iOS receives `MADV_FREE_REUSABLE` — per the
crate's own S9 note the *only* call on that platform that actually drops the
physical footprint, on the platform where jetsam reads that ledger. The
consumer gives up the one working reclaim path because of a summary sentence
the same rustdoc later contradicts.

**Fix:** four text edits, one scope word each ("macOS/iOS"), plus rewording
`:1116`'s back-reference so "other Unix" is not the referent for tvOS/watchOS.

**Adjacent, and worth a decision rather than a text fix:** the tvOS/watchOS
fallback is a **crate cfg omission, not an OS limitation** — `MADV_FREE_REUSABLE
= 7` is XNU-wide, and this crate already lists all four Darwin targets in its
`MAP_ANON` (`:2222-2236`) and `_SC_PAGESIZE` (`:2303-2313`) cfgs. SC2 offered
two options (widen the cfg, or document the narrowing) and the closing pass took
the documenting one; the resulting text at `:1115-1117` reads as a statement
about the platforms rather than about this crate's coverage. One clause
("because this crate's `madv_free_advice` currently only names macOS and iOS")
would keep it honest and keep the widening on the table.

### T5 — LOW-MEDIUM, publish-relevant (task #658) — seven publish-facing surfaces tell the reader to consult `docs/CORRECTNESS_OPEN_ITEMS.md`, which is not in the published package

**Where (all shipped):** `crates/vmem/README.md:143`, `:161` — the crates.io
landing page; `crates/vmem/src/lib.rs:1051`, `:1068`, `:1076` (`decommit`'s
rustdoc), `:1168` (`recommit`'s), `:1225` (`commit_range`'s — inside the
`lazy-commit` feature set that `[package.metadata.docs.rs]` enables, so it
renders on docs.rs). Plus `crates/vmem/Cargo.toml:103`, `:113`, which ship via
`Cargo.toml.orig`.

`cargo package -p aligned-vmem --list` returns 20 files (quoted in full above).
There is no `docs/` directory in it, and there cannot be — `cargo package`
packs the package directory, and `docs/` lives at the workspace root. The
citations are plain inline code spans, not markdown links, so nothing rewrites
them into repository URLs on crates.io either.

This is not a stale-reference problem — the citations are *accurate* about the
repository (I checked: item 6 at `docs/CORRECTNESS_OPEN_ITEMS.md:387-417` really
is the native-Windows `STATUS_ACCESS_VIOLATION` incident, and item 48 at
`:2115-2124` really is the Darwin gap). It is a *reachability* problem: they are
accurate about a file the reader does not have.

**Failure scenario.** Exactly the reader S5 wrote the README `## Platform
caveats` section for. Someone evaluating the crate from crates.io reads
"**Darwin (macOS/iOS/tvOS/watchOS): no zero-fill, no RSS return, on ordinary
reservations too** … no fix is implemented yet — see
`docs/CORRECTNESS_OPEN_ITEMS.md` for the open item", wants to know whether the
gap is being worked on and how it would affect them, runs `cargo add` (or
vendors the crate), searches for that filename, and finds nothing — in a crate
whose entire published file list they can read in one screen. The status,
severity and candidate fixes for the one divergence most likely to affect them
exist only behind a path they cannot resolve. Post-publish this needs a version
bump to correct; pre-publish it is seven one-line edits.

**Fix:** replace each with an absolute URL to the public repository (e.g.
`https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md`),
or inline the two-sentence status. The `repository` field in `Cargo.toml:9`
already points at the right place, so the URL is not new information — it is
just not written where the reader is standing.

---

## Category 4 — portability and unsafe discipline

### T6 — LOW (REASONED-FROM-SPEC, not executed) — the Linux `MAP_ANON` / `MAP_HUGETLB` constants are gated on `target_os` while their values are `target_arch`-dependent — the same class of mistake the neighbouring `_SC_PAGESIZE` table spends twenty lines documenting, one level down

**Where:** `crates/vmem/src/lib.rs:2220-2221` (`#[cfg(… target_os = "linux")] const MAP_ANON: i32 = 0x20;`),
`:2237-2239` (`MAP_HUGETLB: i32 = 0x40000`); contrast `:2281-2336`
(`_SC_PAGESIZE`'s per-OS table and its rationale comment).

Task #714's comment above `_SC_PAGESIZE` establishes the principle explicitly:
*"`_SC_PAGESIZE`'s numeric value is NOT portable across the `unix` family — it
is an index into each OS's own `sysconf(3)` name table, not a POSIX-wide
constant"*, and the prior code's Linux-value-for-everything-else assumption is
called out as a real defect. The `MAP_*` constants immediately above carry the
identical structure with no such note: `MAP_ANON = 0x20` is
`asm-generic/mman-common.h`'s value, correct on x86/x86_64/aarch64/arm/riscv/
powerpc — and **not** on every Linux architecture. Per the kernel UAPI headers,
`arch/mips/include/uapi/asm/mman.h` defines `MAP_ANONYMOUS = 0x0800` and
`MAP_HUGETLB = 0x80000`; `0x20` on MIPS is the IRIX-compat `MAP_RENAME`, which
Linux ignores. (Alpha, PA-RISC and Xtensa diverge similarly; of the divergent
set only the `mips*-unknown-linux-*` family is a current Rust target, tier 3.)

I could not execute this — no MIPS target is installed here and none is in CI —
so it is filed exactly as the crate files its own equivalents:
REASONED-FROM-SPEC.

**Failure scenario.** On such a target `libc_mmap` (`lib.rs:2369-2396`) issues
`mmap(NULL, len, PROT_READ|PROT_WRITE, MAP_PRIVATE, -1, 0)` with no anonymous
flag and `fd = -1` ⇒ `EBADF` ⇒ `MAP_FAILED` ⇒ `reserve_aligned` returns `None`,
**always**. A consumer allocator reports OOM on its very first reservation, on
a fully healthy machine, with no diagnostic pointing anywhere near a wrong
constant. It fails closed — no memory unsafety, no silent corruption — which is
why this is LOW and not higher; the round-6 review already recorded the same
fail-closed shape for the `unix` targets outside the `MAP_ANON` cfg list
(Android, Solaris/illumos, Haiku), where the constant is simply undefined and
the crate does not compile at all. The gap this finding names is the one
*between* those two: an architecture where the crate compiles, links, and is
silently wrong.

**Fix (cheapest honest one is not new constants):** the crate does not publish a
supported-target list anywhere (`README.md`, `Cargo.toml`). Either add the same
kind of prose note `_SC_PAGESIZE` already carries — naming the arch assumption
and which architectures it excludes — or narrow the cfg to the architectures
the value is right for and let the others hit a `compile_error!`, matching the
loud-failure posture the crate already prefers for the undefined-constant case.

### T7 — INFO — ten FFI call sites rely on the edition-2021 implicit `unsafe fn` body; the crate uses the explicit `unsafe {}` form everywhere else, and edition 2024 turns all ten into hard errors

Measured, not estimated:

```
$ RUSTFLAGS="-W unsafe_op_in_unsafe_fn" cargo clippy -p aligned-vmem \
    --features "lazy-commit huge-pages fault-injection bench-internals" --all-targets
lib.rs:1721 (winapi_virtual_release)   lib.rs:1873 (VirtualAlloc)
lib.rs:1881 (VirtualFree)              lib.rs:1888 (VirtualFree)          -> 4

$ ... --target x86_64-unknown-linux-gnu
lib.rs:2117 (libc_munmap)  lib.rs:2381 (mmap)     lib.rs:2412 (munmap)
lib.rs:2438 (madvise)      lib.rs:2454 (madvise)                          -> 5

$ RUSTFLAGS="--cfg miri -W unsafe_op_in_unsafe_fn" cargo check -p aligned-vmem ...
lib.rs:2495 (std::alloc::dealloc)                                         -> 1
```

`unsafe_op_in_unsafe_fn` is deny-by-default in edition 2024; this crate is
`edition = "2021"`, `rust-version = "1.88"`. None of the ten is unsound — each
sits inside an `unsafe fn` whose callers uphold the contract — so this is
INFO, not a defect. It matters for two reasons:

1. **The discipline is half-applied.** The crate's own convention is that every
   unsafe *operation* carries a `// SAFETY:` proof attached to its own block;
   `decommit_pages_impl` (`:2124-2137`) and `unix_reserve` (`:1977-2001`) do
   exactly that inside `unsafe fn`s, while the ten sites above rely on the
   implicit body and attach their SAFETY comment to the function instead.
2. **One of them reads wrong today.** At `lib.rs:2379-2380`:
   ```rust
   let _ = huge; // silence unused on non-linux / no huge-pages builds
                 // SAFETY: anonymous private mapping; kernel chooses the address.
   let p = mmap(
   ```
   rustfmt has glued the `// SAFETY:` line into the trailing comment of
   `let _ = huge;`, so it visually annotates the discard rather than the
   `mmap` call it is about. An explicit `unsafe { … }` block would move it back
   onto its operation.

### T8 — INFO — `win_reserve_commit`'s single-call large-page retry is a second FFI call inside an `unsafe` block whose `// SAFETY:` covers only the first; the two-call path's identical retry does carry its own

**Where:** `crates/vmem/src/lib.rs:1580-1616` (the block's SAFETY at `:1581-1584`
covers the first `VirtualAlloc`; the retry at `:1598-1603` is introduced by
`// Best-effort retry: try without extra_commit_flags …` with no SAFETY line)
versus `:1681-1686` (`// Best-effort large pages: retry the commit with ordinary
pages.` **`// SAFETY: same range within the same live reservation.`**).

Both calls are sound and the retry is trivially as safe as the call it
retries. The finding is the internal asymmetry inside one function: an
`unsafe` block spanning two distinct FFI calls with a proof for one of them is
the shape the rust-intel audit's §B5 finding already targeted once in this
file, and it is cheaper to keep uniform than to re-litigate.

### T9 — INFO — the mock recording path still performs six exposed-address `ptr as usize` casts — the exact pattern task #717 removed from the native backends, and that this crate's own test file goes out of its way to avoid

**Where:** `crates/vmem/src/lib.rs:775` (`Drop`), `:986` (`release`), `:1086`
(`decommit`), `:1136` (`decommit_lazy`), `:1194` (`try_recommit`), `:1283`
(`try_commit_range`) — all inside `#[cfg(feature = "mock")] mock::record(…)`
calls; versus `.addr()` at `:1647`, `:2005`, `:2088`, `:2391`; versus
`crates/vmem/tests/fault_injection.rs:209-213`, whose `SendPtr` wrapper's
comment says it exists to avoid *"an exposed-address `as usize` round-trip —
the exact pattern task #717 removed from this crate's own internals"*.

Under Rust's strict-provenance model `ptr as usize` is an *exposure*; none of
these six is ever round-tripped back into a pointer, so there is no soundness
issue and no live consequence — which is why this is INFO. It is recorded
because (a) `.addr()` is a drop-in replacement the crate already uses four
times, (b) these six would be flagged under `-Zmiri-strict-provenance` if item
41's miri CI step ever lands, and (c) `README.md:168-169`'s guarantee — *"The
returned pointers preserve provenance (no exposed-address `as usize`
round-trips in the public API)"* — is literally true only because these are
exposures rather than round-trips, a distinction the sentence does not draw.
The README's own showcase snippet (`README.md:26`, mirrored at
`lib.rs:61`) writes `base as usize % span` three lines above that guarantee,
teaching the pattern the paragraph disclaims; `base.addr() % span` is the same
length.

### T10 — INFO — perf item 46's recorded S12 mechanism omits the one piece of plumbing it needs: the address it wants to hint from is discarded before the caller can see it

**Where:** `docs/perf/OPEN_ITEMS.md:1139-1157` (the S12 block); the code at
`crates/vmem/src/lib.rs:2066-2111` (`try_reserve_aligned_exact`) and
`:1965-1969` (`unix_reserve`'s call site).

S12 records, as a mechanism a future bare-metal remeasurement should include:
*"on a miss, before falling back to over-reserve, retry `mmap(align_up(p,
align), size, …)` with a non-`MAP_FIXED` hint address."* The `p` in that
sentence is `try_reserve_aligned_exact`'s `region_addr` (`lib.rs:2088`) — and
that function, on an alignment miss, munmaps the region (`:2096`) and returns
`Err(VmemError::invalid_argument())` (`:2097`). The address is dropped;
`unix_reserve` receives only a `VmemError` and has nothing to hint from.

Implementing S12 therefore requires first widening
`try_reserve_aligned_exact`'s error channel (or its return type) to carry the
missed address — a small change, but one the recorded note does not mention.

**Failure scenario.** The round that picks S12 up reads it as a two-line change
("add a retry before the fallback"), discovers on contact that the plumbing
does not exist, and either re-derives the design mid-task or — the outcome
worth avoiding — hints from an address obtained some other way, e.g. reading
`region_ptr` before the munmap and hinting into a range that is still mapped,
which silently returns a *different* address and makes the retry look useless.
(The ordering does work in the current code: the munmap at `:2096` precedes the
return, so a hint issued from `unix_reserve` targets a freed range. That is
worth stating in the note, since it is the non-obvious part.)

One line appended to the S12 block closes this.

---

## Checked and explicitly NOT findings

Recorded so round 8 does not re-derive them.

- **`recommit`/`commit_range` validate against `PAGE` while `decommit` validates
  against `page_size()`.** Re-checked on the now-confirmed 16 KiB macOS runner:
  still no reachable failure. `recommit_pages_impl` is a bare `Ok(())` on all
  Unix (`lib.rs:2142-2161`), and Windows' page size is 4 KiB on every supported
  target. The asymmetry's rationale is documented at `README.md:100-106`. Round
  6 reached the same conclusion; nothing about the macOS CI result changes it.
- **The `SERIAL` mutex's coverage in `smoke.rs`.** Re-verified mechanically by
  reading every `fn` body in the file: the four tests that reach
  `decommit`/`decommit_lazy` (`:169`, `:226`, `:359`, `:427`) all take it, and
  no other test in the file reaches either. `reserve_aligned` does not touch
  `UNIX_MADVISE_*` (the only incrementer is `libc_madvise`, `lib.rs:2419-2448`,
  called only from `decommit_pages_impl`'s two arms, `:2134-2135`), so the ~14
  lock-free tests cannot perturb the oracle's counters. The `attempts == 2`
  assertion held on real hardware this round, which is the empirical
  confirmation of exactly this analysis.
- **`macos_decommit_madvise_syscall_actually_succeeds` under `--all-features`.**
  Correctly excluded (`not(feature = "mock")`), and the CI log confirms it: 21
  tests in the `bench-internals` row, 20 in the `--all-features` row.
- **`benches/vmem_bench.rs` on a 16 KiB-page host.** `RESERVE_SIZE = 64 KiB` and
  every decommit is `[0, len)`, so nothing silently no-ops on Apple Silicon.
  (Benches are not run in CI.) Round 6 checked this before the page size was
  confirmed; it now rests on a measured value.
- **The Windows single-call fast path's `granted_huge = extra_commit_flags != 0`
  on the two-call path (`lib.rs:1712`).** Still unreachable per MSDN's
  single-call `MEM_RESERVE | MEM_COMMIT | MEM_LARGE_PAGES` requirement, still
  documented as such at `:1707-1711`, still indirectly checked by
  `huge_pages.rs:61-62` on the real Windows CI row.
- **`try_recommit` / `try_commit_range` returning `Ok(())` for a misaligned but
  empty range (`start == end == 1`).** The early return at `lib.rs:1185-1187` /
  `:1274-1276` precedes alignment validation. Harmless — an empty range grants
  no write permission — and documented as "a genuinely empty range … is a no-op
  returning `true`".
- **Overflow discipline.** Every arithmetic site that could wrap is checked:
  `over = size.checked_add(align)` (`:1624-1626`, `:1970-1972`),
  `align_up_addr` (`:2568-2572`), `leak_zeroed_pages`'s round-up (`:1492`),
  and both `len = end - start` sites are preceded by their ordering guard.
- **`try_reserve_aligned_exact`'s `align > page_size()` skip (`lib.rs:2094`).**
  Re-derived: when `align <= page_size()` and both are powers of two, `align`
  divides `page_size`, and `mmap` returns page-aligned, so the check can never
  fire — the skip is correct, not an optimisation that trades correctness. It is
  also fail-safe under item 43's own hazard: a *too small* `page_size()` makes
  the check fire more often, never less.
- **`unix_reserve`'s hugetlb alignment guard (`lib.rs:1958-1964`).** Re-checked
  all four `munmap` paths reachable under `huge`: exact-path success
  (`munmap(size)`), exact-path alignment miss (`munmap(size)`), over-reserve fit
  failure (`munmap(over)`), and `release_reservation` (`munmap(over)`). With
  `size` and `align` both huge-page multiples, all four lengths are huge-page
  multiples. Task #714's argument holds.
- **`src/error.rs`, `src/mock.rs`, `src/fault_injection.rs`** — read in full.
  No new findings. `fault_injection`'s third, unhandled disarm/re-arm race is
  disclosed in its own module doc (`:47-57`) with an accurate scope statement;
  `mock`'s partial-backend-replacement shape and feature-unification hazard are
  documented at three sites and settled by QC1.
- **Structure / CLAUDE.md conventions.** No inline `#[cfg(test)] mod tests`
  anywhere in `src/`; no `mod.rs`; zero runnable doctests (`Doc-tests
  aligned_vmem … 0 passed` in both feature rows, and on the macOS CI row);
  every illustrative snippet uses a ```` ```text ```` fence. The
  four-files-vs-"single-file seam crate" point is R13's, already settled.
- **Semver / API surface.** `1dbd6b4` changed no public item's signature; the
  only additions since 0.1.0 that are reachable without an opt-in feature were
  settled in rounds 4–5. `#[non_exhaustive]`, the `is_empty` deprecation, and
  `ReservationParts`'s shape are unchanged.
- **`mock`'s `CALLS` thread-local grows unbounded until `drain()`/`reset()`
  (`src/mock.rs:201-208`).** By design for a recording log, test-only, and the
  module doc tells callers to `drain`. Not filed.

---

## Categories with nothing to report

- **Memory safety / UB.** Every `unsafe` block in `src/lib.rs` was read against
  its call site this round. No safe `pub fn` takes a raw pointer and touches
  allocator metadata (CLAUDE.md's benchmark-hook rule); the two `bench-internals`
  accessors added in round 6 are `AtomicU64` reads with no pointer argument at
  all. Provenance discipline (`.addr()`/`.with_addr()`) is intact on both native
  backends' aligned-base derivations; T9 is a hygiene note about the mock
  recording path, not a soundness issue.
- **Error contracts.** `VmemError`'s three-way classification, the
  `std::io::Error` bridge, the `invalid_argument` vs `os_refusal_unknown_code`
  distinction, and the "capture immediately after the failing syscall" timing
  rule are consistent between code, docs and tests at every site.
- **Performance — null, seventh consecutive round, re-checked rather than
  inherited.** Nothing in this crate is on `sefer-alloc`'s allocation hot path
  (reservation is a per-segment cold path); `page_size()` is one cached relaxed
  load and is now called exactly once per `decommit`/`decommit_lazy` (P-B's
  double-load was fixed); every `bench-internals` counter compiles out entirely
  when the feature is off, and the two added in round 6 sit on the `madvise`
  syscall path even when on. I looked specifically for anything the macOS
  result newly enables and found nothing: the confirmed 16 KiB page size does
  not change any dispatch decision (`try_reserve_aligned_exact`'s skip is
  fail-safe in that direction, see above), and the confirmed H1 answer is a
  correctness input to item 48, not a speed one. The two live perf ideas
  (`VirtualAlloc2` in
  `docs/perf/ALIGNED_VMEM_VIRTUALALLOC2_VA_OPTIMIZATION_OPPORTUNITY.md`, and
  S12 in `docs/perf/OPEN_ITEMS.md` item 46) remain unmeasured and correctly
  filed; T10 is a note on S12's *description*, not a new mechanism.
- **CI coverage.** No new gap beyond the one round 6 already filed (no Linux row
  runs `bench-internals` against the real, non-`mock` Unix backend —
  `docs/CORRECTNESS_OPEN_ITEMS.md:2123`). Verified against `ci.yml`'s Linux rows
  (`:858` default-features, `:900` `--all-features` which turns `mock` on,
  `:920` `fault-injection lazy-commit`) — the gap is exactly as described.

---

## Recommended order

1. **T1** — update item 43's and item 48's cards with the run `31692217669` /
   job `94421845398` citation, including the two-run caveat. This is the one
   finding whose cost compounds: every future round that reads those cards
   inherits a stale "still open" instruction. Do it before anything else in
   this round.
2. **T5** — seven URL edits, before 0.2.0 publishes (task #658).
3. **T3** — one `not(miri)`, plus a sentence saying why.
4. **T4** — four scope words, plus the back-reference reword; consider the
   `madv_free_advice` widening decision at the same time.
5. **T2** — correct the test's doc comment to say what it actually pins and
   name `huge_pages.rs:61-62` as the real W2 regression test.
6. **T6** — one prose note (or one narrowed cfg). Not urgent; no supported
   target is affected.
7. **T7**, **T8**, **T9**, **T10** — record-only hygiene; fold into whatever
   task next touches those lines. T7 is worth doing as one mechanical pass if
   an edition-2024 migration is ever on the table.

---

## On "is round 7 padding?" — an honest answer

Six rounds have combed this crate and the seventh had a genuinely low prior. I
went looking first for the campaign's own signature — round N's fix producing
round N+1's findings — in `1dbd6b4`'s diff, and **did not find it**. All ten
SC fixes hold. That is a real result and it is the first time it has happened
here; the delegation change (parallel worktrees) that produced SC2 and SC3 was
not repeated, and the single-author closing pass did not reproduce their
failure mode.

What made this round non-vacuous is the same thing that made round 6
non-vacuous: **the world changed between rounds.** Round 6's world-change was
that the real backend finally ran on real hardware. Round 7's is that the
*evidence round 6 built for itself arrived* — and nothing in the repository
noticed. T1 is not a defect in anyone's code; it is the gap between "CI
answered the question" and "the durable index a fresh session reads records the
answer", which is the precise gap CLAUDE.md's round-start convention and the
R34-24 current-state rule exist to close, failing on its first real test in
this campaign.

Of the rest, I would defend T3 and T5 as load-bearing (a live failing test for
any macOS-arm64 contributor, and a publish-facing dead reference that gets
expensive at 0.2.0), T2 and T4 as ordinary but genuine oracle/scope corrections,
T6 as a documentation-symmetry finding rather than a bug, and T7–T10 as
hygiene I would not have filed on their own.

The one I would not want lost is **T1's sub-note**, not T1's headline. It is
tempting to close item 48 with "H1 confirmed by CI." It was not. H2 was
excluded by run `31692217669`; the stale byte came from run `31676133649`; the
assertion that would show both in one process is `#[cfg]`'d off on the platform
where it matters. Writing "confirmed by CI" would be a smaller version of
exactly the overstatement S2 was filed to catch — a correct-looking conclusion
resting on evidence that does not quite reach it. The two-run wording costs one
clause and is true.
