# `aligned-vmem` — round-6 CLOSING review (verification of the S1–S12 remediation)

**Date:** 2026-08-13
**Scope:** verification of the seven remediation tasks (#880–#886, letters A–G) that closed
`docs/reviews/2026-08-13-aligned-vmem-round6-review.md`'s findings S1–S12, plus the three
"zero-trust review of task …" fix commits the orchestrating agent made on top of the delegates'
own commits before merging, plus the two merge conflicts those parallel worktrees produced on
`docs/CORRECTNESS_OPEN_ITEMS.md` item 48. Every file in the round's diff
(`git diff 9c777bc..HEAD --stat`: 9 files, +447/−81) and the code each of those changes makes a
claim about. This round was delegated via the `Agent` tool (`subagent_type: "sh"`, 7 parallel
isolated git worktrees) rather than `/crush`.

**Reviewed tree:** local `main` @ `c1211a4` (the task #886 merge). `git status --porcelain` at
session start showed exactly two untracked entries, both pre-existing and neither in this crate:
`docs/checkpoints/2026-08-13-0130.md` and `docs/reviews/2026-08-13-aligned-vmem-round6-review.md`.
`origin/main` = `9c777bcb1a5d97a39ed3c2c391fffe3f3031d6e5`; **confirmed by `git fetch`:
`git log origin/main..HEAD --oneline | wc -l` = 17.** Round 6's own seven tasks (and the three
orchestrator fix commits) have **not** been pushed — so, exactly as the task brief predicted, there
is no new real CI signal for this round, and neither
`macos_decommit_madvise_syscall_actually_succeeds` nor `apple_silicon_page_size_is_16_kib` has ever
executed on Darwin hardware.

**Toolchain:** `rustc 1.97.0 (2d8144b78 2026-07-07)`, stable-x86_64-pc-windows-msvc; Windows 10 Pro
x86_64, 4 KiB page. Installed targets: `i686-pc-windows-msvc`, `thumbv7em-none-eabi`,
`x86_64-pc-windows-msvc`, `x86_64-unknown-freebsd`, `x86_64-unknown-linux-gnu`,
`x86_64-unknown-netbsd`, `x86_64-unknown-none` — **no Darwin target and no Darwin host**, which
bounds what could be verified by execution here and is stated explicitly wherever it matters below.

**Nature:** read-only. Nothing in the repository was modified other than the creation of this
document. No `git add` / `git commit` / `git push` / version bump / branch or worktree mutation.
The only files written outside the repo were a throwaway `/tmp/vmemchk/cf.mjs` (the guard
counterfactual harness) and two scratch `CARGO_TARGET_DIR`s used to force uncached rustdoc builds.
Every command quoted below was actually run on this host; every `file:line` citation was read in
the current tree before being written down.

**Relationship to the prior rounds.** This pass does not re-report V1–V21, W1–W16 + P-A/P-B/P-C,
F1–F11, R1–R13, CR1–CR10, Q1–Q9 or QC1–QC9. It verifies S1–S12's remediation and reports only what
is new. To stay unambiguous against the `V`-, `W`-, `P`-, `F`-, `R`-, `CR`-, `Q`-, `QC`- and
`S`-series, this pass's own findings are numbered **SC1…SC10** (round-6 closing series).

---

## Verdict up front

The four items the orchestrating agent personally found and fixed during zero-trust review **all
hold**, and one of them holds more completely than the orchestrator had evidence for: the
exhaustive grep the brief flagged as *not done* comes back clean (§"Four personally-fixed items",
item 4b). The SERIAL-mutex fix in particular is not merely plausible — it is mechanically complete
(4 of 4 counter-touching tests hold the lock; verified by parsing every `fn` body in `smoke.rs`, not
by reading the diff).

The full verification matrix is genuinely green on the current tree, re-executed here rather than
taken on trust (§"What was verified green").

And the campaign's unbroken pattern held for a sixth time. **Round 6's own remediation introduced
new residue**, in three places the round-6 review's scope structurally could not have looked:

1. **S4 is half-closed, and the artifact that closes the other half says otherwise in its own
   rustdoc.** S4 had two limbs: macOS lost its decommit oracle (closed) *and*
   `decommit_lazy_roundtrip` is "vacuous on **every** platform, not just macOS" (not closed). The
   new oracle's doc comment justifies its `target_os = "macos"` gating by asserting that
   "Linux/Windows already have a passing zero-fill assertion in
   `decommit_recommit_roundtrip`/`decommit_lazy_roundtrip`" — `decommit_lazy_roundtrip` has no
   effect-observing assertion of any kind, which is precisely what S4 said (**SC1**).
2. **S7's Darwin-family widening landed at 1 of 7 sites, and the one site that WAS widened is now
   contradicted by the crate's own `#[cfg]` behavior on two of the four targets it names**
   (**SC2**). This is the direct consequence of two parallel worktrees (#880 adding four new
   "macOS" cross-references, #885 widening one paragraph to "Darwin family") never seeing each
   other's text.
3. **Round 6 has no CHANGELOG entry for its own 17 commits** — the sixth instance of the gap
   `docs/CORRECTNESS_OPEN_ITEMS.md` item 1 exists to track, and the first since round 3 to escape
   the round's own remediation tasks. Task #886 wrote the entry for the *previous* commit
   (`9c777bc`, closing S11) and its closing sentence describes round 6's seven tasks as "tracked as
   separate follow-up tasks (#880-885), not fixed by this entry" — off by one task, and false as of
   the moment it was merged (**SC3**).

There is also a genuine **cross-crate ripple**: `reset_bench_internals_counters()` now resets six
counters instead of four, and the root crate's own `#[doc(hidden)]` forwarder
(`src/alloc_core/alloc_core_core_diag.rs:170`) still documents "reset all four `aligned_vmem`
bench-internals counters" and enumerates exactly four (**SC4**). This is the same blind spot that
produced task #864 in round 3 ("no round of `aligned-vmem`-scoped review had checked downstream
in-workspace consumers").

Nothing found in this pass is a soundness or memory-safety defect. SC1–SC3 are MEDIUM; SC4 is
LOW-MEDIUM; the rest are LOW/INFO. Publish readiness for 0.2.0 (task #658) is **improved but not
clean**: S5's README section landed and is well written, but SC2 means the crates.io landing page's
new "Platform caveats" section under-scopes the divergence to macOS while the rustdoc it points at
scopes it to four targets.

---

## What was verified green — every command below was executed on this host

```
$ git log --oneline 9c777bc~1..HEAD | wc -l
18                                    # 9c777bc + 17 round-6 commits (7 delegate, 7 merge, 3 fixup)

$ git fetch && git log origin/main..HEAD --oneline | wc -l
17                                    # round 6 is entirely UNPUSHED; origin/main == 9c777bc

$ git log --oneline --all --grep="zero-trust review of task"
ec7bbe4  task #885 — align remaining 'discovered 2026-08-13' phrasing
cd7f37a  task #882 — serialize decommit-touching smoke tests
1951802  task #881 — align README's macOS discovery framing
55fb6f0  task #879  (round 5, pre-existing)
aafb35d  task #878  (round 5, pre-existing)

$ grep -rn '^<<<<<<<\|^>>>>>>>\|^=======$' --include='*.rs' --include='*.md' --include='*.toml' \
      --include='*.mjs' --include='*.yml' .
(no output)                           # no leftover conflict markers anywhere in the repo

$ cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals"
fault_injection 5 / huge_pages 1 / lazy_commit 11 / min_page 2 / mock 0 / smoke 20 /
vmemerror_io_bridge 3 / doc-tests 0   => 42 passed, 0 failed

$ cargo test -p aligned-vmem --all-features
0/0/1/11/2/9/20/3/0                   => 46 passed, 0 failed

$ cargo clippy -p aligned-vmem --all-targets -- -D warnings                          -> clean
$ cargo clippy -p aligned-vmem \
    --features "lazy-commit huge-pages fault-injection bench-internals" \
    --all-targets -- -D warnings                                                     -> clean
$ cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings           -> clean
$ cargo fmt -p aligned-vmem --check                                                  -> clean

$ node scripts/vmem-doc-drift-guard.mjs
[vmem-doc-drift-guard] OK: no unconditional over-reserve/trim statements found   (exit 0)

$ node scripts/verify-commit-prefixes.mjs
[verify-commit-prefixes] linted 17 commit(s) ... PASS (with warnings above)       (exit 0)
   3 direction-2 warnings, all benign and all correct to ignore:
   79821d6 / 991bf4d touch crates/vmem/src/lib.rs under a `docs(vmem):` prefix (rustdoc-only
   edits — verified line by line: no executable statement changed in either), and e7ad1b7
   touches crates/vmem/README.md under `docs(vmem):` (the checker's path allowlist does not
   include `crates/*/README.md`). No prefix in the round is mis-stated under R30-12's taxonomy;
   e839b1a correctly uses `feat(vmem)` for the one commit that adds real (opt-in) code.

$ cargo check -p numa-shim --features vmem-integration --all-targets                 -> clean
$ cargo check -p sefer-alloc --features "internals bench-internals"                  -> clean
   (the two in-workspace consumers of this crate — checked because round 6 changed a public,
   cross-crate-called function's behavior; see SC4 for what it did NOT catch)

$ CARGO_TARGET_DIR=<scratch> cargo doc -p aligned-vmem \
    --features "lazy-commit huge-pages fault-injection bench-internals" --no-deps
   -> 0 warnings (fresh build, not a cache hit)
$ CARGO_TARGET_DIR=<scratch2> cargo rustdoc -p aligned-vmem \
    --features "..." -- --document-private-items
   -> 1 warning: unresolved link to `into_parts`  (PRE-EXISTING; untouched by round 6, and only
      visible under --document-private-items, which CI does not run — not a finding)

$ for i in 1 2 3; do cargo test -p aligned-vmem --features "..." --test smoke \
      -- --test-threads=16; done
   -> 20 passed, 0 failed, every iteration (SERIAL mutex introduces no deadlock/poison cascade
      on the platform where it can actually be executed)
```

---

## The four personally-fixed items — verification

### 1. README discovery framing (task #881 → fix commit `1951802`) — **HOLDS**

`crates/vmem/README.md:152-160` now reads *"Confirmed as a real, failing-test-level gap by this
crate's first real-macOS CI run on 2026-08-13 (the underlying hazard was already documented
elsewhere in this repository since Round 9, before this crate was extracted)"*. That is
substantively the same claim task #883/S3 landed in item 48, and it is accurate. Verified against
`git show 1951802` (4 insertions, 2 deletions, README only) and against the current file.

### 2. SERIAL mutex coverage (task #882 → fix commit `cd7f37a`) — **HOLDS, and is complete**

Not verified by reading the diff. Every top-level `fn` body in `crates/vmem/tests/smoke.rs` was
parsed and classified mechanically:

| test | calls `decommit`/`decommit_lazy` | touches `UNIX_MADVISE_*` / reset | holds `SERIAL` |
|---|---|---|---|
| `decommit_recommit_roundtrip` | yes | no | **yes** |
| `recommit_is_fallible_and_reports_success_on_the_happy_path` | yes | no | **yes** |
| `decommit_lazy_roundtrip` | yes | no | **yes** |
| `macos_decommit_madvise_syscall_actually_succeeds` | yes | yes | **yes** |

No other test in the file reaches either. **The orchestrator missed none.** The lock is also
poison-tolerant (`unwrap_or_else(|e| e.into_inner())`), so one panicking test cannot cascade-fail
the other three — an improvement over a bare `.unwrap()`.

Two further conditions the exact-count assertion (`attempts == 2`) depends on were checked by
enumeration, since they cannot be executed here:

- **No other `madvise` call site can perturb the counters on Darwin.** `libc_madvise`
  (`lib.rs:2404`) is the sole incrementer, and its only callers are `decommit_pages_impl`'s two
  arms (`lib.rs:2119-2120`). `libc_madvise_hugepage` is a separate helper that never touches the
  counters, and on non-Linux Unix it is the empty no-op arm (`lib.rs:2443-2447`) — so even under
  `huge-pages`, nothing else issues `madvise` on macOS. `reserve`/`release`/`leak_zeroed_pages`
  issue `mmap`/`munmap` only.
- **The offsets survive a 16 KiB page.** `span = 4 MiB`, halves at `0` / `2 MiB` / `4 MiB`, all
  multiples of 16384, so `decommit`'s `page_size()` guard (`lib.rs:1072-1075`) does not silently
  skip either call on Apple Silicon — which would otherwise turn `attempts == 2` into a spurious
  failure that looked like an H2 confirmation.

Cross-file racing is structurally impossible: `cargo test` gives each integration-test file its own
process, so `tests/mock.rs`'s `decommit`/`decommit_lazy` calls (which under `mock` never reach
`libc_madvise` anyway) cannot interfere.

### 3. Item 48 after the #882/#883 conflict — **HOLDS**

`docs/CORRECTNESS_OPEN_ITEMS.md:2115-2146`. Both contributions are present in full and neither is
duplicated: #883's `**Prior knowledge (repo-wide …)**` and `**R9_5 mis-citation…**` bullets, and
#882's `**Root cause is ASSERTED, not yet ESTABLISHED (added task #882):**` bullet. No conflict
markers (repo-wide grep above). No contradiction: the item states H1 as the working explanation in
its header while the #882 bullet explicitly says *"The H1-vs-H2 question is therefore still OPEN;
do NOT read this note as confirming H1"* — which is the honest reading and is exactly what S2
asked for. Two structural nits only, folded into SC7.

### 4. The #885 three-way conflict and the "discovered 2026-08-13" sweep — **BOTH HOLD**

(a) The second item-48 conflict resolved coherently — same evidence as item 3 above, plus #885's
own S9 three-observation note is present verbatim.

(b) **The exhaustive grep the orchestrator did not do, done here, comes back clean:**

```
$ grep -rni "discover" crates/vmem/
(no output)

$ grep -rni "discovered 2026-08-13" --include='*.rs' --include='*.md' --include='*.toml' \
      --include='*.mjs' --include='*.yml' .
docs/reviews/2026-08-13-aligned-vmem-round6-review.md:215   # the review quoting the old text

$ grep -rn "2026-08-13" crates/vmem/
crates/vmem/README.md:158       "…first real-macOS CI run on 2026-08-13 (the underlying hazard …)"
crates/vmem/src/lib.rs:1059     "…first real-macOS CI run, 2026-08-13 — the underlying hazard …"
crates/vmem/src/lib.rs:2133     "…first real-macOS CI run, 2026-08-13 -- the underlying hazard …"
crates/vmem/tests/smoke.rs:197  "…2026-08-13 -- the underlying hazard was already known repo-wide"
```

All four surviving date mentions carry S3's corrected framing. `previously-undiscovered` survives
repo-wide only in places where it is being *quoted and corrected* (`CHANGELOG.md:375`, item 48
itself) or in unrelated historical text (`CHANGELOG.md:238`, two checkpoints). The orchestrator's
belief was correct; it is now also verified.

---

## Per-finding closure status (S1–S12)

| # | Status in the current tree | Notes |
|---|---|---|
| S1 | **CLOSED** | All four sites carry a cross-reference: module doc `lib.rs:34-35`, `decommit()` opening `lib.rs:1019-1020`, `decommit_lazy()` `lib.rs:1101-1102`, `recommit()` `lib.rs:1136-1138`. Scope wording is inconsistent with S7's fix → **SC2**; the `decommit_lazy` sentence flattens S9 → **SC6**. |
| S2 | **CLOSED pending hardware** | Oracle added, item 48 correctly records H1-vs-H2 as still OPEN. The oracle is gated correctly and cannot run outside macOS+`bench-internals`+`!mock`. Verified that the macOS CI row (`ci.yml:823`) does enable `bench-internals` without `mock`, so it *will* run on the next push. |
| S3 | **CLOSED** | Item 48's framing corrected; R9_5/R11_8 annotated. The "now accurate" claim over-closes → **SC5**. |
| S4 | **PARTIALLY CLOSED** | macOS half restored; the "vacuous on every platform" half untouched, and the new artifact misdescribes it → **SC1**. |
| S5 | **CLOSED** | README `## Platform caveats`, all three divergences, well written. Under-scoped to macOS → **SC2**. |
| S6 | **CLOSED pending hardware** | `apple_silicon_page_size_is_16_kib` (`smoke.rs:337-341`) is gated `all(target_os="macos", target_arch="aarch64")` with **no** feature gate, so it runs on both macOS CI rows. Item 43's card correctly downgrades the old "verified" claim. |
| S7 | **PARTIALLY CLOSED** | 1 of 7 doc sites widened; behavior contradicts the widened site on 2 of the 4 named targets → **SC2**. |
| S8 | **CLOSED** | `reservation_len()`'s rustdoc now lists both under-reporting paths (`lib.rs:541-563`); `smoke.rs:82-91`'s QC8 comment corrected to "at least two paths … and until now nothing asserted this (Windows) one". |
| S9 | **CLOSED (recorded)** | All three observations recorded in item 48's Next trigger. The public rustdoc S9 named as inverted was not reconciled → **SC6**. |
| S10 | **CLOSED — verified by executed counterfactual** | See below. |
| S11 | **CLOSED for `9c777bc`, REOPENED for round 6 itself** | → **SC3**. |
| S12 | **CLOSED** | Folded into `docs/perf/OPEN_ITEMS.md` item 46 (`[L]` tier) as a "Mechanism to include in a future remeasurement" block, explicitly labelled unmeasured and not a recommendation. Hit/miss arithmetic checked: 34.375 / 46.6667 / 56.6667% hit → the quoted "43–66% miss" is correct. |

### S10 — re-run counterfactuals (the guard's own three regexes, verbatim, `node`, no file modified)

| sentence | old (strip whole code spans) | **current (strip `->`/`=>`/`<Ident…>`)** |
|---|---|---|
| ``Over-reserves `size + align` for `align > 64 KiB`.`` | violation **true** (false positive) | violation **false** ✔ |
| ``The Windows backend over-reserves for `align > 64 KiB`.`` | violation **true** (false positive) | violation **false** ✔ |
| `\| `reserve_aligned(size, align) -> Option<Reservation>` \| Over-reserves size + align and trims. \|` | true | **true** ✔ (QC2 stays closed) |
| `reserve_aligned -> Option<Reservation>: over-reserves and trims.` | false | **true** ✔ (QC2 closed more tightly than before) |
| `Returns Vec<Reservation> after it over-reserves and trims.` | false | **true** ✔ |
| `Unconditionally over-reserves … when align > PAGE.` | true | **true** ✔ (HARD_FAIL still wins) |

The fix is strictly better than what it replaced: it removes S10's false-positive class without
reopening QC2, and it closes two cases the old version let through. The header comment now states
the real trade-off rather than an argument for the opposite. `<=`/`>=` are provably unaffected
(`<` followed by `=` cannot match `<[A-Za-z_]…>`).

---

## Findings

### SC1 — MEDIUM — S4 is half-closed, and the artifact that closed the other half asserts, in its own rustdoc, a fact about `decommit_lazy_roundtrip` that is false — the same "invalid test oracle" shape S4 filed

**Where:** `crates/vmem/tests/smoke.rs:358-376` (`decommit_lazy_roundtrip`), `smoke.rs:401-407`
(the new oracle's justification), `.github/workflows/ci.yml:789/823/858/900/920`.

S4 had two limbs. The first ("macOS now has zero effect-observing coverage") is genuinely closed by
task #882. The second is quoted verbatim from the round-6 review:

> `decommit_lazy_roundtrip` (`smoke.rs:307-324`) — writes, calls `decommit_lazy`, recommits, writes
> again, reads back. Passes identically whether `madvise(MADV_FREE_REUSABLE)` succeeded, returned
> `EINVAL`, or was never compiled in. **Vacuous on every platform, not just macOS.**

That is still true today. The current body (`smoke.rs:359-376`) writes `0x9E`, calls
`decommit_lazy`, asserts `recommit(...) == true` (an unconditional `Ok(())` on all Unix), writes
`0x3C`, reads back `0x3C`. There is no assertion anywhere in it that observes any effect of the
`madvise` call.

The new oracle's own doc comment nevertheless states its gating rationale as:

> `target_os = "macos"`-gated (the H1-vs-H2 question is macOS-specific; Linux/Windows **already
> have a passing zero-fill assertion in `decommit_recommit_roundtrip`/`decommit_lazy_roundtrip`
> above**, so this test would be redundant, not wrong, on those platforms).

The claim is correct for `decommit_recommit_roundtrip` and false for `decommit_lazy_roundtrip`. The
counters it relies on are `unix`-wide, not Darwin-specific (`libc_madvise` is
`#[cfg(all(unix, not(miri)))]`), so a Linux instance of the same oracle would work verbatim and
would be the first thing in the crate's history to observe that `MADV_FREE` is actually issued.

A second, independent gap compounds it: **no CI row anywhere runs `bench-internals` against the
real Unix backend on Linux.** `ci.yml`'s Linux rows are default-features (`:858`), `--all-features`
(`:900`, which turns `mock` **on**, bypassing `decommit_pages_impl` entirely), and
`fault-injection lazy-commit` (`:920`, no `bench-internals`). Windows (`:789`) has the combination
but `libc_madvise` does not exist there. So macOS (`:823`) is the *only* CI configuration in which
these counters are ever non-zero — which is fine for S2's purpose, but means the Linux half of S4
cannot be closed by adding an assertion alone; it needs a matrix row too.

**Failure scenario (concrete).** Someone "simplifies" `madv_free_advice()` — e.g. collapses the
Linux arm to `MADV_DONTNEED` "since `MADV_FREE` is only a hint anyway", or a future `libc` constant
renumbering makes `MADV_FREE = 8` wrong for a new Linux target. `decommit_lazy` silently stops
issuing the intended advice on the crate's primary Unix platform. All three CI platforms stay
green: `decommit_lazy_roundtrip` passes by construction, and the only test that could notice is
`#[cfg]`'d to macOS. This is S4's original failure scenario, unclosed, with a doc comment now
asserting that it *is* closed.

**Fix:** either (a) widen the oracle's `#[cfg]` from `target_os = "macos"` to `unix` and add one
Linux CI row with `--features "lazy-commit huge-pages fault-injection bench-internals"`, or (b) at
minimum, correct the rustdoc sentence so it does not claim coverage that does not exist, and file
the remaining half of S4 in `docs/CORRECTNESS_OPEN_ITEMS.md` so a future round inherits it. (b) is
the honest floor; (a) is what actually closes it.

### SC2 — MEDIUM — S7's "Darwin family" widening reached 1 of 7 documentation sites, and the crate's own `#[cfg]`s contradict the one site it did reach on 2 of the 4 targets that site names

**Where:** `lib.rs:1058-1070` (widened), vs. `lib.rs:34-35`, `lib.rs:1019-1020`, `lib.rs:1101-1102`,
`lib.rs:1136-1138`, `lib.rs:2133-2143`, `README.md:152-160` (all still "macOS"); vs.
`lib.rs:2186-2195` (`madv_free_advice`) and `lib.rs:2260-2262` (`MADV_FREE_REUSABLE`).

Two separate defects, both created by round 6.

**(a) Doc-scope split.** After the round, the crate states the same gap at two different scopes:

| site | scope stated |
|---|---|
| `decommit()`'s caveat paragraph (`lib.rs:1058-1070`) — widened by #885 | "the Darwin family (macOS/iOS/tvOS/watchOS — all share XNU and the same `MADV_DONTNEED` semantics, not just macOS)" |
| crate module doc (`lib.rs:34-35`) — **new this round** (#880) | "on macOS this reclaim is advisory-only" |
| `decommit()`'s opening sentence (`lib.rs:1019-1020`) — **new** (#880) | "implicitly on Linux — see the macOS caveat below" |
| `decommit_lazy()` (`lib.rs:1101-1102`) — **new** (#880) | "On macOS, `decommit` itself is only advisory too" |
| `recommit()` (`lib.rs:1136-1138`) — **new** (#880) | "On macOS specifically…" |
| `recommit_pages_impl`'s code comment (`lib.rs:2133-2143`) | "does NOT hold on macOS… roundtrip on macOS… re-`mmap`(MAP_FIXED) over the range on macOS" |
| `README.md:152` — **new this round** (#881) | "**macOS**: no zero-fill, no RSS return, on ordinary reservations too." |
| `smoke.rs:209-216`'s `#[cfg]` — widened by #885 | `macos`, `ios`, `tvos`, `watchos` |

Five of those six "macOS"-only statements were *written this round*, by two agents (#880, #881)
working in worktrees that could not see #885's widening. The README one is the worst placed: it is
the crates.io landing page, it is brand new, and it is the exact artifact S5 asked for so that an
evaluating consumer would not have to read the 40th paragraph of a function's rustdoc.

**(b) The widened text is not true of two targets it names, as the crate is written today.**
`decommit()`'s new paragraph asserts all four Darwin targets "share XNU and the same
`MADV_DONTNEED` semantics". For the *eager* path that is fine (`MADV_DONTNEED` is used uniformly on
all Unix). For the *lazy* path the crate disagrees with itself:

```rust
// lib.rs:2186-2195
#[cfg(any(target_os = "macos", target_os = "ios"))]      { MADV_FREE_REUSABLE }
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "ios")))]
                                                          { MADV_DONTNEED }

// lib.rs:2260-2262
#[cfg(all(unix, not(miri), any(target_os = "macos", target_os = "ios")))]
const MADV_FREE_REUSABLE: i32 = 7;
```

On **tvOS and watchOS**, `decommit_lazy` therefore issues `MADV_DONTNEED`, i.e. the advisory no-op
item 48 is about — not the `MADV_FREE_REUSABLE` that `decommit_lazy`'s own rustdoc (`lib.rs:1091-1094`)
and item 48's S9 note both describe for "macOS". Note that S7's own evidence for the widening was
that "`madv_free_advice()` already treats `macos` and `ios` identically… the crate already models
them as one XNU family" — that is a 2-target list; the 4-target lists S7 pointed at
(`MAP_ANON` at `lib.rs:2205-2215`, `_SC_PAGESIZE` at `lib.rs:2288-2296`) are different lists. #885
adopted the 4-target list for the docs and the test `#[cfg]` without reconciling the 2-target ones.

**Failure scenario.** A tvOS/watchOS consumer reads `decommit()`'s rustdoc, sees "Darwin family
(macOS/iOS/tvOS/watchOS — all share XNU and the same `MADV_DONTNEED` semantics)", and correctly
concludes eager `decommit` is a no-op there — then falls back to `decommit_lazy` on the strength of
its documented `MADV_FREE_REUSABLE` footprint drop, which on those two targets silently resolves to
`MADV_DONTNEED`: the same no-op they were trying to avoid, with no test and no doc line anywhere
covering it. Meanwhile an iOS consumer evaluating from crates.io reads a README bullet headed
"**macOS**" and concludes their platform is unaffected — S7's original failure scenario, now on the
highest-traffic surface the crate has.

**Fix:** pick one scope word and use it at all seven sites, and either widen `madv_free_advice()` /
`MADV_FREE_REUSABLE` to the same four targets (Apple's `MADV_FREE_REUSABLE = 7` is XNU-wide) or
state explicitly, at the widened site, that the *lazy* path's Darwin coverage is macOS+iOS only.

### SC3 — MEDIUM — round 6 has no CHANGELOG entry for its own 17 commits: the sixth instance of `docs/CORRECTNESS_OPEN_ITEMS.md` item 1's tracked gap, the first since round 3 to escape the round's own tasks — and the one sentence that mentions the round is both off by one task and false

**Where:** `CHANGELOG.md:367-375`; `docs/CORRECTNESS_OPEN_ITEMS.md:63-77` (item 1).

Every prior round of this campaign received its own section, written inside the round:

```
$ grep -n '^#### `aligned-vmem`' CHANGELOG.md
236  rust-intel audit remediation (2026-08-09, tasks #699/#712-719)
252  round-closing-review follow-ups (2026-08-09, tasks #775-776)
261  code-quality/bug/perf review remediation (2026-08-12, tasks #842-850)
302  post-campaign closing review remediation (2026-08-12, tasks #851-857)
316  round-3 follow-up (2026-08-12, tasks #858-864)
330  round-4 follow-up (2026-08-13, tasks #867-874)
345  round-5 follow-up (2026-08-13, tasks #875-879)
367  macOS decommit CI-discovery fix (2026-08-13, commit `9c777bc`)   <- S11's backlog item
```

There is no `round-6 follow-up (…, tasks #880-886)` section. `grep -n '#880\|#881\|…\|#886'
CHANGELOG.md` returns exactly one line — `:375` — inside the `9c777bc` section, and it says:

> Those findings and the rest of S1-S12 are tracked as separate follow-up tasks (**#880-885**), not
> fixed by this entry.

Two problems with that single line: the range omits **#886**, the very task that wrote it (7 tasks
landed, #880–#886); and "tracked… not fixed" was already false when it merged — all seven tasks
were complete and merged into the same 17-commit branch. A reader reconstructing what shipped from
`CHANGELOG.md` alone sees the pre-round commit documented and the round itself invisible.

Item 1 of the correctness index tracks exactly this, and its own Current-number card is now stale:
it records "3 confirmed recurrences that went uncaught until the NEXT round … round 4 … and round 5
… are a 4th and 5th instance … both caught and closed within their own round". Round 6 is the
sixth, and it is being caught by the closing review rather than by the round's own remediation —
i.e. by the same mechanism the card itself calls out as "depends on a closing review actually
running every round rather than being skipped".

**Failure scenario.** 0.2.0 publishes (task #658) with a CHANGELOG whose most recent `aligned-vmem`
entry describes only a test-scoping commit and explicitly says the review findings behind it are
unfixed follow-ups — while the shipped crate in fact contains all seven fixes, two new
`#[doc(hidden)] pub static`s, two new accessor functions, a widened `reset_bench_internals_counters`
contract and a new README section. Downstream readers get a materially wrong picture of what 0.2.0
is.

**Fix:** write the `#### aligned-vmem — round-6 follow-up (2026-08-13, tasks #880-886)` section
before the round is closed, correct `:375`'s range and its now-false tense, and bump item 1's
Current-number card to 6.

### SC4 — LOW-MEDIUM — round 6 widened `reset_bench_internals_counters()` from four counters to six and left three enumerations of it stale, one of them in the ROOT crate — the downstream-consumer blind spot that produced task #864 in round 3

**Where:** `src/alloc_core/alloc_core_core_diag.rs:170-178`; `crates/vmem/src/lib.rs:169-195`;
`crates/vmem/Cargo.toml:100-118`.

Task #882 added `UNIX_MADVISE_ATTEMPTS`/`UNIX_MADVISE_SUCCESSES` and correctly updated the
function's own doc — `lib.rs:342` now reads "reset all **six** counters" and lists all six. That
update is what makes the following three demonstrably stale rather than merely vague:

1. **Root crate (cross-crate).** `src/alloc_core/alloc_core_core_diag.rs:170`:
   > `/// MEASUREMENT-ONLY (task #504, F11 step 1): reset all four`
   > `/// `aligned_vmem` bench-internals counters`
   followed by a four-item intra-doc list, then `pub fn dbg_reset_vmem_bench_internals_counters()`
   which forwards straight to `aligned_vmem::reset_bench_internals_counters()`. The forwarder now
   resets six. Nothing in `sefer-alloc` reads the two new ones, so there is no live bug — but the
   documented contract of a `#[doc(hidden)] pub fn` in the *shipping* crate is now wrong, and this
   file was invisible to a `crates/vmem/`-scoped review by construction.
2. **`crates/vmem/src/lib.rs:169-195`** — the module-level "bench-internals: path-activation
   counters" section, which is the documented index for these statics, still opens with
   "**Two** independent questions, one instrument each" and enumerates exactly two instrument
   families. There are now three.
3. **`crates/vmem/Cargo.toml:100-118`** — the `bench-internals` feature comment enumerates the same
   two families ("a Unix hit/total pair around `try_reserve_aligned_exact` … plus Windows-side
   counters for `win_reserve_commit`"). Not updated.

Round 5's Q2 (task #876) was specifically a `Cargo.toml`-doc-drift sweep for this crate, so (3) is
the same class recurring one round later.

**Failure scenario.** A future round measuring Windows reserve/commit calls a `dbg_reset_*` hook
documented as touching four counters, and silently loses a `UNIX_MADVISE_*` measurement window it
did not know it was resetting — or, more likely, wastes a task re-deriving which counters the reset
actually covers because the crate's own index says two instruments and the function says six.

**Fix:** three text edits. Also worth a one-line note in the round's own retro: a `crates/<x>/`-scoped
review must grep the workspace for callers of any function whose *contract* it changed, not only for
callers whose *signature* it changed (the latter the compiler catches; the former it does not).

### SC5 — LOW — task #883's R9_5 mis-citation fix asserts the citation "is now accurate"; S3 named two specific respects in which it is not, and neither was addressed

**Where:** `docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md:115-122`;
`docs/perf/R11_8_SMALL_VIRGIN_ZERO_SKIP_DESIGN.md:29-36`; `lib.rs:1058-1070`.

R9_5 line 115-116 reads: *"NOT guaranteed on decommit-then-recommit on macOS/XNU/\*BSD
(`MADV_DONTNEED` is advisory+lazy, no zero-fill — `crates/vmem/src/lib.rs` §decommit note)."* The
round-6 annotation appended beneath it says the citation "was unverifiable at the time. **It is now
accurate**; see `docs/CORRECTNESS_OPEN_ITEMS.md` item 48."

S3's own text named why that is only half true:

> R9_5's macOS argument now points at a note that says **something narrower than R9_5 needs** (it
> covers `decommit`, not `decommit_lazy`, and says nothing about `*BSD`).

Both still hold in the current tree. The vmem note (`lib.rs:1058-1070`) scopes itself explicitly to
"the Darwin family (macOS/iOS/tvOS/watchOS)" — it does not mention `*BSD` at all — and it is
attached to `decommit`, not `decommit_lazy`. So the annotation closes the *dating* half of S3's
mis-citation finding while asserting closure of a *scope* half that was never touched.

**Failure scenario.** Whoever revisits `virgin-zero-skip` follows R9_5's citation for its `*BSD`
claim, finds a Darwin-only note, and either (i) concludes `*BSD` is unaffected — it is not; the
crate's own `madv_free_advice()` fallback arm routes every non-Linux non-Darwin Unix to
`MADV_DONTNEED` — or (ii) re-derives the `*BSD` position from scratch, which is the cost S3 filed
the finding to avoid.

**Fix:** one clause. Either narrow the annotation ("now accurate for the Darwin family; the `*BSD`
and `decommit_lazy` halves of this sentence are still uncited") or widen the vmem note.

### SC6 — LOW — `decommit_lazy()`'s new cross-reference sentence flattens exactly the Darwin cost-ordering inversion that item 48 now records one file away

**Where:** `lib.rs:1091-1102`; `docs/CORRECTNESS_OPEN_ITEMS.md` item 48, S9 note (2).

`decommit_lazy`'s rustdoc still opens "*hint the OS it MAY reclaim … **cheaper than `decommit`***"
and still says "*Cheaper reclaim; the kernel takes pages only under pressure*". Task #880 appended:

> (On macOS, [`decommit`] itself is only advisory too — see its macOS caveat — **so this lazy
> variant inherits the same non-guarantee there**.)

Item 48's S9 note, landed the same round by a different agent, says the opposite about the RSS half:

> that ordering is **INVERTED on Darwin specifically**: `MADV_FREE_REUSABLE` drops footprint
> immediately there, while eager `decommit`'s `MADV_DONTNEED` … drops nothing at all.

Both can be read as consistent if "non-guarantee" is taken to mean *zero-fill only* — neither call
zero-fills on Darwin — but nothing in the sentence says so, and the surrounding paragraph is about
reclaim cost, not zero-fill. Read plainly, a consumer takes away "on macOS both are equally
advisory, so prefer the eager one for determinism", which is backwards for the RSS goal.

**Failure scenario.** An iOS/macOS consumer optimising for physical footprint (the platform where
jetsam reads that ledger) reads this paragraph, picks eager `decommit`, and gets no footprint
reduction at all — while the "lazy" call they avoided is the only one that would have worked.

**Fix:** qualify the sentence to the zero-fill axis and add one clause pointing at the inversion,
or drop the parenthetical from `decommit_lazy` and let `decommit`'s own caveat carry it.

### SC7 — LOW — item 48's Status card and internal structure were not resynchronised with what round 6 actually landed

**Where:** `docs/CORRECTNESS_OPEN_ITEMS.md:2118` (Status), `:2121-2145` (bullet order).

- The Status bullet still enumerates the mitigation as "*both `decommit`'s own rustdoc and
  `recommit_pages_impl`'s code comment now carry an explicit macOS caveat*" — written for
  `9c777bc`'s two sites. Round 6 added four more rustdoc cross-references (#880) and a README
  section (#881) and did not update it. Per CLAUDE.md's current-state-card rule, a card is supposed
  to read as the current state at the start of the next round.
- The item now carries **two** "Root cause" bullets — `- **Root cause:**` at `:2121` and
  `- **Root cause is ASSERTED, not yet ESTABLISHED (added task #882):**` at `:2146`, the latter
  placed *after* `- **Evidence:**`. The S9 three-observation note (#885) is nested inside the
  **Next trigger** bullet rather than standing on its own. All three are artefacts of two
  hand-resolved conflicts on the same item; none is a contradiction, but the item no longer reads
  top-to-bottom in the card order the file's convention uses.

**Failure scenario.** A future round reads the Status line, concludes only two sites document the
gap, and "helpfully" adds cross-references that already exist — or, reading the first "Root cause"
bullet and stopping there, treats H1 as established, which is precisely what S2 spent a finding
preventing.

### SC8 — INFO — the new counters' rustdoc says the private helper "is named here in code font rather than linked" while writing it as an intra-doc link, and links to a private enum

**Where:** `lib.rs:247-252`.

```rust
/// [`libc_madvise`] (Unix only — always 0 on Windows/miri; that internal
/// helper is private, so it is named here in code font rather than linked).
/// … (both [`DecommitKind::Eager`] … and [`DecommitKind::Lazy`] …)
```

`libc_madvise` is written as a link, in the same sentence that says it is not; and `DecommitKind`
is a private enum (`lib.rs:2545`). The two sibling counters this doc was copied from
(`UNIX_EXACT_RESERVE_ATTEMPTS` `:200`, `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` `:225`) use bare
backticks for exactly this reason and state the same rationale correctly.

**No live impact, and this was checked rather than assumed:** both statics are `#[doc(hidden)]`, and
a forced fresh `cargo doc` and a `cargo rustdoc -- --document-private-items` both emit zero
warnings about these links (the only warning anywhere is a pre-existing `into_parts` one, untouched
by round 6). Filed as INFO because it becomes a real `broken_intra_doc_links` warning the moment
either static loses `#[doc(hidden)]`, and because it is the same self-contradicting-rationale shape
as S10, in text written the same round S10 was fixed.

### SC9 — INFO — the macOS oracle resets counters it does not own, outside the SERIAL contract as documented

**Where:** `smoke.rs:434`; `lib.rs:352-359`.

`reset_bench_internals_counters()` zeroes all six counters, including
`UNIX_EXACT_RESERVE_ATTEMPTS`/`_HITS`, which are incremented by **every** `reserve_aligned` call —
i.e. by roughly fourteen other tests in `smoke.rs`, none of which holds `SERIAL` (correctly: the
mutex's documented contract is "every test that calls `decommit`/`decommit_lazy`"). Nothing in
`smoke.rs` asserts on the exact-reserve counters, so there is no live race today.

Recorded because the blast radius of the reset is wider than the lock's stated scope, and the next
person to add an exact-reserve assertion to this file will get a silently flaky test rather than a
compile error. The cheap hardening is a comment on the reset call naming which counters it clobbers
beyond the two the test cares about.

### SC10 — INFO — `attempts == 2` is an exact-count assertion whose validity rests on `libc_madvise` staying the sole counting site, and item 48 now records an idea that would break it

**Where:** `smoke.rs:445-449`; item 48's S9 note (3).

The assertion is correct today — verified by enumerating every `madvise` call site (see
§"Four personally-fixed items", item 2). But item 48 now explicitly records, as a candidate future
fix, "*route Darwin's eager `decommit` to `MADV_FREE_REUSABLE` and issue `MADV_FREE_REUSE` from
`recommit`*". Implementing that adds a second counted call per cycle, and this test will start
failing with a message that reads like an H2 confirmation ("the madvise SYSCALL ITSELF must
succeed…") when in fact only the call count changed.

Recorded so that the round which implements it knows to update the count and the message in the
same commit, rather than debugging a false H1/H2 signal. (`successes == attempts` is the robust
half; `attempts == 2` is the brittle half, and it is the one that carries the misleading message.)

---

## Checked and explicitly NOT findings

Recorded so round 7 does not re-derive them.

- **The `let ret = madvise(...)` / `#[cfg(not(feature = "bench-internals"))] let _ = ret;` pattern**
  (`lib.rs:2424-2433`) compiles clean in both feature states. Verified indirectly: this function is
  `#[cfg(all(unix, not(miri)))]`, so it never compiles on this Windows host — but CI's Linux
  `--all-features` row (`ci.yml:900`) does compile it with `bench-internals` on, and the
  `feature-powerset` job (`ci.yml:2040`) covers the off state at depth 2. No unused-variable hazard.
- **`macos_decommit_madvise_syscall_actually_succeeds`'s inner `use aligned_vmem::{decommit,
  decommit_lazy, …}`** shadows the file-level `decommit_lazy` import. That is legal Rust (a `use` in
  a block scope shadows an outer one) and emits no warning; not a latent compile error waiting on
  the next macOS run.
- **`apple_silicon_page_size_is_16_kib` will actually run.** It carries no feature gate, only
  `all(target_os = "macos", target_arch = "aarch64")`, so both macOS CI steps (`ci.yml:823` and
  `:828`) execute it — including the `--all-features`/`mock` one, where `page_size()` still queries
  the real OS (the `mock` feature replaces the reservation backend, not `sysconf`).
- **The `mock` / `--cfg`-conversion deferral (round-4 CR9's subject)** is tracked coherently:
  `docs/CORRECTNESS_OPEN_ITEMS.md` "Recently resolved" item 3, marked **CLOSED** with an explicit
  joint revisit condition for both `aligned-vmem` and `numa-shim`. Not an untracked open item; no
  action owed by round 6.
- **`docs/perf/OPEN_ITEMS.md` item 46's S12 block** sits in the `[L]` tier under a current-state
  card, is labelled unmeasured and non-recommending, and its `Full history:` line was updated in the
  same edit. Compliant with the round-start/current-state conventions.
- **Round 6 changed no public API reachable without an opt-in feature.** The two new statics and two
  new accessors are all `#[cfg(feature = "bench-internals")]`; `reset_bench_internals_counters`'s
  signature is unchanged. Semver-additive; no default-surface change ahead of the 0.2.0 publish.
- **No new `unsafe` was introduced.** `git diff 9c777bc..HEAD -- crates/vmem/src/lib.rs` adds no
  `unsafe` token; the counter increments live inside the existing `unsafe fn libc_madvise` body.
- **Delegate-content preservation across the seven merges** was checked per commit
  (`git show --stat` for each of `e7ad1b7 63427af 989a1e9 79821d6 e7303dd 991bf4d e839b1a` plus the
  three fixups) against the current tree. Every delegate's changes are present; the two hand-resolved
  item-48 conflicts lost nothing.
- **`tests/mock.rs`'s `decommit`/`decommit_lazy` calls** cannot perturb the new counters: separate
  test binary (separate process), and under `mock` the calls are recorded rather than forwarded to
  `decommit_pages_impl`.

---

## Categories with nothing to report

- **Memory safety / UB / soundness.** No new `unsafe` surface; no new safe `pub fn` taking a raw
  pointer and touching allocator metadata (CLAUDE.md's benchmark-hook rule); the one new
  measurement hook is a pair of `AtomicU64` reads with no pointer argument at all.
- **Error contracts / semver / API surface.** Unchanged this round beyond the additive
  `bench-internals` items noted above.
- **Performance.** Still null, seventh consecutive assessment. The new counter increments are
  `#[cfg]`'d out of every non-`bench-internals` build and sit on the `madvise` (syscall) path even
  when on.
- **Structure / CLAUDE.md file conventions.** No inline `#[cfg(test)] mod tests`, no `mod.rs`, zero
  runnable doctests (`Doc-tests aligned_vmem … 0 passed` in both feature rows run above).

---

## Recommended order

1. **SC3** — write the round-6 CHANGELOG section, fix `:375`'s `#880-885` → `#880-886` and its
   now-false tense, bump item 1's card to 6. Do this *before* the round is called closed, which is
   the whole point of the item-1 rule.
2. **SC1** — at minimum correct the oracle's false rationale sentence and file the remaining half of
   S4; ideally widen the `#[cfg]` to `unix` and add the missing Linux CI row.
3. **SC2** — one scope word, seven sites; and decide whether `madv_free_advice()`/`MADV_FREE_REUSABLE`
   widen to four targets or the doc narrows the lazy path to macOS+iOS. README first — it is the
   publish-blocking half.
4. **SC4** — three text edits, one of them in the root crate.
5. **SC5**, **SC6**, **SC7** — one clause / one sentence / one card each.
6. **SC8**, **SC9**, **SC10** — record-only hygiene; fold into whatever task next touches those
   lines.

Then push, and confirm CI green on the **landing SHA read from `origin/main`** — round 6 is the
first round in this campaign whose two headline artifacts
(`macos_decommit_madvise_syscall_actually_succeeds`, `apple_silicon_page_size_is_16_kib`) are
*designed* to produce their evidence only in CI. Until that run exists, S2's H1-vs-H2 question and
S6's `_SC_PAGESIZE` question are both open, and item 48 and item 43 correctly say so.

---

## On "did round 6's own remediation introduce residue?" — the honest answer

Yes, and the mechanism is newly legible this round. Rounds 1–5 delegated to `/crush`; round 6
delegated to seven `Agent`/`sh` instances in **isolated git worktrees**. Two of the three MEDIUM
findings here are direct products of that isolation:

- **SC2** exists because #880 wrote four new "macOS" sentences while #885, in a different worktree,
  was widening a fifth to "Darwin family". Neither could see the other; the merge was textually
  clean (different hunks), so no conflict flagged it, and a per-task zero-trust review of either
  diff in isolation would pass.
- **SC3**'s off-by-one exists because #886 wrote "tasks #880-885" from the round plan it was given,
  in a worktree where the seventh task's existence was not visible in the tree.

The orchestrator's own zero-trust pass caught the three *collisions* the worktrees produced (the
README wording, the SERIAL race, the two item-48 conflicts) — all three verified correct here. What
it could not catch by construction is the class above: changes that are individually correct, do not
conflict, and are wrong only *relative to each other*. That is a different review question from "is
this diff correct", and it is the question a closing review is for.

The third, **SC4**, is not about worktrees at all — it is the round-3 lesson recurring: a
crate-scoped review does not look outside the crate, and this round changed the *contract* (not the
signature) of a function the root crate calls, so nothing in the compiler, the test suite, or the
seven task scopes could have flagged it.
