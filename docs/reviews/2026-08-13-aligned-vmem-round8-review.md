# `aligned-vmem` — round-8 readonly review (post-round-7-closing, post-push, CI green)

**Scope:** `crates/vmem/` in full (`src/lib.rs` all 2,627 lines, `src/error.rs`,
`src/mock.rs`, `src/fault_injection.rs`, all seven `tests/*.rs`,
`benches/vmem_bench.rs`, `examples/v20_849_unix_exact_reserve_hit_rate.rs`,
`Cargo.toml`, `README.md`), the `.github/workflows/ci.yml` rows that touch this
crate, `scripts/vmem-doc-drift-guard.mjs`, `CHANGELOG.md`'s round-7 entries, and
both open-items indexes' entries for this crate (`docs/CORRECTNESS_OPEN_ITEMS.md`
items 1, 41, 42, 43, 48, 49 + the "Recently resolved" #43 entry;
`docs/perf/OPEN_ITEMS.md` item 46 including its S12/T10 blocks).

**Review type:** READ-ONLY. No file in the repository was modified by this review
other than the creation of this document. No `git add` / `git commit` /
`git push` / branch, worktree or ref mutation. Every command quoted below was
executed on this host (or read from the real GitHub Actions API); every
`file:line` citation was read in the current tree before being written down.

**Base revision:** local `main` @ `8380607` ("fix(vmem), docs: round-7 closing
review — fix TC1-TC9 …"). `git fetch && git rev-parse origin/main` →
`8380607778dca5489e97d9ec8dd483d0f384dfd0`, identical to local `HEAD`;
`git log origin/main..HEAD --oneline | wc -l` → **0**. `git status --porcelain`
shows exactly three untracked entries, all pre-existing checkpoints
(`docs/checkpoints/2026-08-13-{0130,1500,1730}.md`), none in this crate.

**Toolchain / host:** `rustc 1.97.0`, stable-x86_64-pc-windows-msvc; Windows 10
Pro, 4 KiB page. **No Darwin host and no Darwin target** — every Darwin claim
below is reasoned from spec or read from the real CI job log of the landing SHA,
never executed here.

**Finding prefix:** `U` (eighth round). Prior prefixes in use and deliberately
not reused: `V`/`W`/`P` (rounds 1–2), `F` (round 3), `R`/`CR` (round 4 + its
closing review), `Q`/`QC` (round 5 + closing), `S`/`SC` (round 6 + closing),
`T`/`TC` (round 7 + closing).

**Date:** 2026-08-13.

---

## Verdict up front

**Round 7's closing remediation (`8380607`) is materially clean.** All nine
TC findings landed, and eight of the nine landed on exactly the right content
(§"Round-7 closing pass — TC1–TC9 verification"). The whole verification matrix
is green here, re-executed rather than taken on trust, and CI is green on the
landing SHA read from the remote — including `test macos (production)` on
`macos-26-arm64`, where both of round 6's oracle artifacts ran and passed again
(`apple_silicon_page_size_is_16_kib ... ok`,
`macos_decommit_madvise_syscall_actually_succeeds ... ok`, 21/21 and 20/20).

**This round's headline is a code finding, not a process one — the first since
round 6.** **U1 (MEDIUM)** is a soundness-adjacent gap that rounds 3, 4 and 7
each *explicitly examined and cleared*: `try_reserve_aligned_exact`'s
`align > page_size()` alignment-check skip is safe when `page_size()` under-reports
(the direction all three rounds checked) and **silently violates the crate's
primary documented guarantee — "`as_ptr()` is aligned to the `align` you asked
for" — when it over-reports**, which is precisely the failure mode
`docs/CORRECTNESS_OPEN_ITEMS.md` item 43's still-open BSD half describes and
`page_size()`'s own guard comment anticipates in so many words ("a
plausible-looking power-of-two answer to a DIFFERENT question"). The fix is
deleting one `&&` conjunct on a code path whose own comment records that the
conjunct saves **zero** syscalls.

The rest is smaller and honest: a corrected mis-citation that is itself
mis-cited (**U2**), a "keep these two sites in sync" anchor that points at the
wrong function (**U3**), a copied `// SAFETY:` proof that describes a different
call (**U4**), a Windows crash footgun `decommit_lazy`'s rustdoc never mentions
(**U5**), three publish-facing WSL2-only numbers with no platform attribution
(**U6**), one genuinely uncovered half of a deliberately asymmetric contract
(**U7**), and a stale current-state header (**U8**). Three INFO items close it out.

**Performance: null, eighth consecutive round** — re-derived this round, not
inherited; see "Categories with nothing to report" for the specific checks.

**Safety: null at HIGH/MEDIUM.** Every `unsafe` block in `src/lib.rs` was read
against its call site. Nothing unsound was found on any tested platform. U1 is
filed under correctness rather than safety because it needs an unverified
`_SC_PAGESIZE` constant to fire; U4 is a wrong proof on a sound call.

**Publish readiness (task #658):** U6 is the only new publish-facing item and it
is two sentences. Nothing here blocks 0.2.0.

**Diminishing returns: yes, and this round says so with a number** — see the
final section. Eight rounds in, the marginal find rate is now one code-relevant
finding per round, and U1 was reachable only by contradicting three prior rounds'
explicit clearance of the same lines.

---

## What was verified green — every command below was executed on this host

```
$ git fetch && git rev-parse origin/main
8380607778dca5489e97d9ec8dd483d0f384dfd0        # == local HEAD; 0 unpushed

$ gh run list --commit 8380607778dca5489e97d9ec8dd483d0f384dfd0
completed  success  CI                 main  push  31701674673  31m20s
completed  success  Kani verification  main  push  31701674556  41s

$ gh run view 31701674673 --json jobs -q '.jobs[]|select(.name|test("macos"))|…'
test macos (production)   success   94452072878

$ gh api repos/PHPCraftdream/sefer-alloc/actions/jobs/94452072878/logs
Image: macos-26-arm64
  # step `cargo test -p aligned-vmem --features "lazy-commit huge-pages
  #   fault-injection bench-internals" --no-fail-fast`, tests/smoke.rs:
  running 21 tests
  test apple_silicon_page_size_is_16_kib ... ok
  test macos_decommit_madvise_syscall_actually_succeeds ... ok
  test result: ok. 21 passed; 0 failed
  # step `cargo test -p aligned-vmem --all-features --no-fail-fast`, tests/smoke.rs:
  running 20 tests
  test apple_silicon_page_size_is_16_kib ... ok
  test result: ok. 20 passed; 0 failed

$ cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" --no-fail-fast
fault_injection 5 / huge_pages 1 / lazy_commit 11 / min_page 2 / mock 0 /
smoke 20 / vmemerror_io_bridge 3 / doc-tests 0        => 42 passed, 0 failed

$ cargo clippy -p aligned-vmem --all-targets -- -D warnings                          -> clean
$ cargo clippy -p aligned-vmem \
    --features "lazy-commit huge-pages fault-injection bench-internals" \
    --all-targets -- -D warnings                                                     -> clean
$ cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings           -> clean
$ cargo fmt -p aligned-vmem --check                                                  -> clean

$ node scripts/vmem-doc-drift-guard.mjs
[vmem-doc-drift-guard] OK: no unconditional over-reserve/trim statements found  (exit 0)

$ cargo package -p aligned-vmem --list --allow-dirty
20 files: .cargo_vcs_info.json, Cargo.{lock,toml,toml.orig}, LICENSE-{APACHE,MIT},
README.md, benches/vmem_bench.rs, examples/v20_849_unix_exact_reserve_hit_rate.rs,
src/{error,fault_injection,lib,mock}.rs, tests/*.rs (7)
                                        # unchanged from rounds 6/7; still no docs/

$ grep -rnE '^(<<<<<<<|=======|>>>>>>>)$' crates/vmem/ docs/CORRECTNESS_OPEN_ITEMS.md \
      docs/perf/OPEN_ITEMS.md CHANGELOG.md
(no output)
```

Test counts (42) are identical to rounds 6 and 7, consistent with `8380607`
adding no test and disabling none. `Doc-tests aligned_vmem … 0 passed` in both
feature rows and on the macOS CI rows — the no-doctests convention holds.

**Not re-run this round, deliberately:** the `RUSTFLAGS="-W
unsafe_op_in_unsafe_fn"` sweep that produces item 49's count. Changing
`RUSTFLAGS` forces a full workspace rebuild (~17 min measured here for one
clippy row), and item 49's card — correctly, per TC7 — already instructs the
reader to re-derive it rather than trusting a hardcoded list. Its "9 of 10
remaining" figure is therefore recorded as *inherited*, not re-verified, by this
round.

---

## Round-7 closing pass (`8380607`) — TC1–TC9 verification

Checked before looking for anything new, because seven consecutive rounds have
found the closing fix to be the next round's bug source.

| # | Status in the current tree | Evidence |
|---|---|---|
| TC1 | **CLOSED** | `CHANGELOG.md` now carries `#### aligned-vmem — round-7 follow-up (2026-08-13, tasks #888-894)` with a bullet per task. I verified all seven cited merge SHAs against `git log` (`0cdb596`, `c1867d8`, `ceaa3fe`, `ac08342`, `e496071`, `b569cfe`, `1e532a7`) — all real, all matching their stated task numbers. Item 1's `Current number` bullet (`docs/CORRECTNESS_OPEN_ITEMS.md:75`) records the 7th instance. Its *headline* was not updated → **U8**. |
| TC2 | **CLOSED** | `git show --stat 8380607` includes both `docs/reviews/2026-08-13-aligned-vmem-round7-review.md` (+793) and `…-round7-closing-review.md` (+591). `git log --oneline -- <path>` now resolves for both; the four citations in `CHANGELOG.md` / items 43, 48, 49 resolve. |
| TC3 | **CLOSED on content; the sync anchor it created does not resolve** | `lib.rs:1124-1134` and `:2212-2218` now say the same thing ("MAY work identically … REASONED-FROM-SPEC … a plausible widening candidate, not an established fact"), and item 48's S9 bullet (`docs/CORRECTNESS_OPEN_ITEMS.md:2102`) was rewritten to agree — the contradiction is genuinely gone, in the direction the closing review recommended (hedged, not settled). The `lib.rs:1123-1130`/`:2204-2209` line citations inside that bullet are wrong → **U3**. |
| TC4 | **CLOSED, and more completely than the finding asked** | `Cargo.toml:104` and `:116` are both `https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/…` URLs now; `lib.rs:262` (the `#[doc(hidden)]` counter doc) and the freshly-introduced `lib.rs:1132` are too. Re-grepped the whole crate for bare `docs/` citations: the survivors are `lib.rs:2170` (a code comment), `smoke.rs:208`, `:335`, `:427`, `:451`, `examples/v20_849_…:1` — all non-rustdoc test/comment surfaces the finding explicitly deprioritised, none rendering on docs.rs. |
| TC5 | **The fix landed; the replacement precedent is also wrong** | `smoke.rs:353-361` no longer names the nonexistent `decommit_lazy_roundtrip` sibling — it names `recommit_is_fallible_and_reports_success_on_the_happy_path` instead, which has no `not(miri)` gate and no zero-fill read → **U2**. |
| TC6 | **CLOSED** | `smoke.rs:420-429` now reads "**This test HAS now run and passed on real macOS CI** … CI run `31692217669`, job `94421845398` … `== 2` -- H2 is ruled out", and keeps the two-run H1 caveat verbatim. A repo-wide grep for the pending-phrasing family (`next (real )?macOS( CI)? run\|has NOT yet run\|awaiting real CI\|not yet run on real`, excluding `docs/reviews/`) now returns only `CHANGELOG.md:383`/`:397`, both inside round 6's append-only historical section and both superseded by the follow-up paragraph at `:399`. |
| TC7 | **CLOSED** | `docs/CORRECTNESS_OPEN_ITEMS.md:2109` carries `Current-number-or-verdict: 9 of the original 10 sites remain unfixed` plus the exact `RUSTFLAGS` invocation to re-derive locations, explicitly declining to hardcode line numbers "the file has changed since the original review and will keep changing". All four R34-24 fields present. |
| TC8 | **CLOSED, with a scope word that over-reaches** | `README.md:171-175` no longer says the mock recorder "does expose addresses as `usize`"; it says addresses are "obtained via the non-exposing `.addr()`". `grep -rn "as usize" crates/vmem/src/` returns three integer widenings (`v as usize`, two `SystemInfo` field reads) and two comments — zero pointer casts, as claimed. The new scope word "anywhere in the crate" is false for `tests/` → **U9**. |
| TC9 | **CLOSED** | `lib.rs:2245-2265` states the rule instead of a roster ("every Linux architecture that uses `asm-generic/mman-common.h`, which is all of them except MIPS, Alpha, PA-RISC and Xtensa") and explicitly flags the parenthetical as "NOT an exhaustive tier-1/tier-2 roster — s390x and loongarch64 are tier-2 and also use `asm-generic/mman-common.h`". |

No conflict markers anywhere. `git show 8380607 -- crates/vmem/src/lib.rs | grep
'^+.*unsafe'` adds no `unsafe` token. No public item's signature changed; no
`#[cfg]` on any shipping item changed; no feature composition changed. The
closing commit's own claim that it changed no runtime behavior is **verified**:
every `src/` hunk in it is a doc comment or a comment.

---

## Category 1 — the one code finding

### U1 — MEDIUM — `try_reserve_aligned_exact`'s `align > page_size()` skip makes the crate's primary alignment guarantee depend on a constant the repo's own index records as unverified for four targets; rounds 3, 4 and 7 each cleared this check considering only the direction that is safe

**Where:** `crates/vmem/src/lib.rs:2109-2118` (the invariant comment and the
skip), `:385-407` (`page_size()` and its acceptance guard, specifically the
`queried >= PAGE && queried.is_power_of_two()` test at `:400`), `:1985-1989`
(`unix_reserve`'s unconditional trust of the fast path's result),
`:2354-2387` (the `_SC_PAGESIZE` per-OS table); versus
`docs/CORRECTNESS_OPEN_ITEMS.md:1888-1932` (item 43, BSD half OPEN).

The code:

```rust
// lib.rs:2109-2118
// Invariant: when `align <= page_size()`, the check below is always
// false because `mmap` always returns page-aligned addresses. Skip the
// check entirely in that case to eliminate a dead branch and document
// the invariant explicitly. Confirmed by measurement #849: 480/480 hits
// on page-size mode (no syscalls saved, just removes dead code).
if align > page_size() && !region_addr.is_multiple_of(align) {
    unsafe { libc_munmap(region_ptr.cast(), size) };
    return Err(VmemError::invalid_argument());
}
```

The stated invariant is true **iff `page_size()` is less than or equal to the
real OS page size**. `page_size()` accepts whatever `sysconf(_SC_PAGESIZE)`
returns as long as it is a power of two `>= PAGE`; its own guard comment
(`:391-399`) names the exact hazard it is defending against — *"exactly the
failure mode a wrong `_SC_PAGESIZE` constant on an untested target produces: a
plausible-looking power-of-two answer to a DIFFERENT question"* — and then
rejects only values **below** `PAGE`. A wrong constant that happens to yield a
power of two **above** the real page size is accepted, cached process-wide, and
becomes load-bearing here.

When that happens, for every `align` in `(real_page_size, page_size()]` the
alignment check is skipped, `mmap` returns an address aligned only to the *real*
page size, and `try_reserve_aligned_exact` returns
`Ok((base, base, size, …))` with `base` **not** aligned to `align`.
`unix_reserve` returns that result unchanged (`:1985-1989`), `finish_reservation`
copies it into `Reservation`, and `Reservation::as_ptr()` — documented at
`:507-509` as *"aligned to the `align` requested at reservation time"*, the
crate's single headline guarantee — hands the caller a misaligned pointer with
no error, no log, and no observable difference from a correct reservation.

The over-reserve path is unaffected: it computes `align_up_addr(region_addr,
align)` explicitly (`:2026-2030`), so it is correct for any `page_size()` value.
**The fast path is the only place in the crate where `page_size()`'s value can
change whether an alignment guarantee holds.** Every other consumer of
`page_size()` fails safe in this direction: an over-reported page size makes
`decommit`/`decommit_lazy` reject *more* ranges (`:1085`, `:1148`), degrading
into a silent no-op rather than an unsound result.

**Why this was not caught before, stated precisely.** Three rounds inspected
these exact lines and each reasoned in one direction only:

- round 3 (`…-round3-review.md:459-461`): *"The skip is sound: a kernel `mmap`
  result is page-aligned, so for `align <= page_size()` the second conjunct is
  provably false."*
- round 4 (`…-round4-review.md:632-636`): re-derived the boundary case
  `align == page_size()` on a 16 KiB host — *"an `mmap` result is 16 KiB-aligned
  by the kernel's own contract"*.
- round 7 (`…-round7-review.md:671-676`): *"It is also fail-safe under item 43's
  own hazard: **a too small `page_size()` makes the check fire more often, never
  less**."* (emphasis mine) — the too-large direction is the one item 43 is
  actually about, and it is the unexamined one.

All three arguments silently substitute "the real page size" for
`page_size()`'s *return value*. That substitution is exactly what item 43 says
cannot be assumed on FreeBSD, DragonFly, NetBSD or OpenBSD — four targets this
crate deliberately supports (they each have their own `MAP_ANON` arm at
`:2268-2282` and their own `_SC_PAGESIZE` arm at `:2365-2372`).

**Failure scenario (concrete).** A consumer builds `aligned-vmem` for
`x86_64-unknown-freebsd`. `_SC_PAGESIZE = 47` is REASONED-FROM-SPEC (item 43's
own words: "cross-compile-checked … which confirms the code COMPILES but not
that the numeric constant is correct"). Suppose 47 names a different `sysconf`
parameter on that release and returns, say, `65536`. `page_size()` accepts it
(power of two, `>= 4096`). The consumer — `sefer-alloc` itself is the archetype
— calls `reserve_aligned(4 MiB, 4 MiB)` for a segment. `align (4 MiB) > 65536`,
so the check *does* still run here; but `reserve_aligned(64 KiB, 64 KiB)` (the
crate's own default bench regime, and a plausible small-segment size) skips it
and returns a base that is only 4 KiB-aligned. The consumer then derives segment
metadata by masking (`ptr & !(SEGMENT-1)`, sefer-alloc's `os::segment_base_of_ptr`
shape), lands on an address that is not its own segment base, and reads or writes
allocator metadata belonging to a different span. No diagnostic anywhere points
at a `sysconf` constant.

**Counterfactual (would a test catch it?).** No. `page_size_is_a_valid_os_page`
(`smoke.rs:322-333`) asserts only power-of-two-ness and `>= PAGE`, which an
over-reported value satisfies by construction (item 43's card makes precisely
this point about the *fallback* value; it applies identically to an
over-reported queried value). `reserve_is_aligned_and_writable`
(`smoke.rs:143-162`) uses `align == 4 MiB`, above any plausible bogus
`page_size()`, so it stays green. There is no test on any platform with
`page_size() < align <= plausible-bogus-value`.

**Fix (cheap, and the code's own comment argues for it).** Delete the
`align > page_size() &&` conjunct and keep the comment as an explanatory note:

```rust
// Nearly always false — `mmap` returns page-aligned addresses, so for
// `align <= page_size()` this cannot fire (480/480 hits at page-size
// granularity, measurement #849). Checked unconditionally anyway: the
// `<=` reasoning depends on `page_size()` being <= the real OS page
// size, which item 43 records as unverified on FreeBSD/DragonFly/
// NetBSD/OpenBSD, and the failure mode of trusting it is a silently
// misaligned `as_ptr()`.
if !region_addr.is_multiple_of(align) { … }
```

The conjunct's own documented benefit is *"no syscalls saved, just removes dead
code"*; the cost of removing it is one `and`/`test` on a path that has just made
an `mmap` syscall. A weaker alternative — leaving the skip and adding a
`debug_assert!(region_addr.is_multiple_of(align))` — is *not* adequate here:
release builds are exactly where this matters, and `debug_assert` compiling out
in `--release` is the same mechanism that let R25-5's config bug through (see
CLAUDE.md's R26-4 rule). Worth adding in the same pass: one sentence to item
43's `Current-number-or-verdict` bullet naming this second consequence, since
that card currently records only the decommit-rounding one.

---

## Category 2 — citations that do not resolve

### U2 — LOW — TC5 replaced a mis-citation with a different mis-citation: the test it now names as the `not(miri)` precedent has no `not(miri)` gate and no zero-fill read; the wrong name was inherited from a neighbouring comment (task #716) that is wrong the same way

**Where:** `crates/vmem/tests/smoke.rs:353-361`; the named test at
`smoke.rs:236-266`; the real precedent at `smoke.rs:219-231`; the upstream
source of the error at `crates/vmem/tests/lazy_commit.rs:336-344`.

The corrected sentence reads:

> Matches the `not(miri)` exclusion this crate's other real-OS-property
> assertions already use (e.g. the zero-fill assertion above, the madvise oracle
> below, and this file's own
> `recommit_is_fallible_and_reports_success_on_the_happy_path`, **whose
> `not(miri)`-gated zero-fill read** is mirrored by `lazy_commit.rs`'s
> `sequential_commit_range_grows_incrementally` …)

`recommit_is_fallible_and_reports_success_on_the_happy_path` (`smoke.rs:236-266`)
contains **no `#[cfg]` of any kind and no zero-fill read**. Mechanically
checked: `grep -n miri crates/vmem/tests/smoke.rs` returns hits at `:200`,
`:220`, `:348-362`, `:447`, `:454`, `:459`, `:544`, `:566`, `:594`, `:686-695` —
none inside `:236-266`. Its only post-recommit assertion is
`base.add(span / 2).write(0x5C); assert_eq!(base.add(span / 2).read(), 0x5C)`
(`:263-264`), a write-then-read-back, which is true on every backend including
miri and mock.

The test that actually carries the pattern is `decommit_recommit_roundtrip`
(`smoke.rs:179-233`), whose `#[cfg(not(any(miri, feature = "mock", target_os =
"macos", …)))] assert_eq!(base.add(span / 2).read(), 0)` at `:219-231` is the
crate's canonical real-OS-zero-fill assertion.

The error is inherited, not invented: `lazy_commit.rs:336-342` (task #716) says
its own `#[cfg(not(miri))]` gate *"Mirrors the identical, already-established
gate in `tests/smoke.rs`'s
`recommit_is_fallible_and_reports_success_on_the_happy_path`"* — the same wrong
name, standing since task #716. TC5's fix trusted that neighbouring comment
instead of checking the test body, which is how a citation correction reproduced
the class of defect it was correcting.

**Failure scenario.** Exactly TC5's own, one name over: a contributor (or a
round-9 reviewer doing what this pass did) follows the pointer to learn the
established `not(miri)` pattern, opens
`recommit_is_fallible_and_reports_success_on_the_happy_path`, finds no gate, and
either concludes the pattern is invented for the macOS test or spends the time
this pass spent proving otherwise. The cost compounds because there are now
**two** files asserting it.

**Fix:** name `decommit_recommit_roundtrip` in both places (`smoke.rs:355-358`
and `lazy_commit.rs:340-342`) — one is the real zero-fill precedent, and fixing
only the copy would leave the original standing.

### U3 — LOW — item 48's TC3 "keep both in sync" anchor cites line numbers computed against the pre-edit tree; one of the two now points at a completely different function

**Where:** `docs/CORRECTNESS_OPEN_ITEMS.md:2102` (the S9 bullet's tvOS/watchOS
sub-note, written by `8380607`), citing `lib.rs:1123-1130` and `:2204-2209`;
versus the actual sites at `crates/vmem/src/lib.rs:1124-1134` and `:2212-2218`.

The bullet's own text designates those two ranges as the sites this index entry
*must be kept in agreement with*:

> **tvOS/watchOS coverage (round 7, task #895, TC3 — synchronized with
> `lib.rs:1123-1130`/`:2204-2209`'s wording, which this bullet must keep
> agreeing with if either changes):** …

Verified against the tree:

- `lib.rs:2204-2209` is `reserve_aligned_huge_raw`'s `#[cfg]`, signature and
  body — no Darwin wording at all. The `madv_free_advice` doc comment it means
  is at `:2212-2218`.
- `lib.rs:1123-1130` starts one line before the paragraph (`:1123` is the tail
  of the preceding sentence) and stops four lines short of its end: `:1131-1134`
  — which carry the URL and the reciprocal *"item 48's S9 note, which must agree
  with this wording -- keep both in sync if either changes"* clause — fall
  outside the cited range.

Both citations were correct against `8380607^`: `git show
8380607^:crates/vmem/src/lib.rs | sed -n '2204,2209p'` is exactly the
`madv_free_advice` doc comment, and `:1123-1126` was the pre-fix tvOS/watchOS
paragraph. **The same commit that wrote the citation rewrote both cited
passages** (TC3 lengthened the `lib.rs:1124` paragraph; TC4 inserted the URL at
`:1131`), shifting everything below by eight lines. The pointer was stale on
arrival, not merely aged.

The irony is load-bearing rather than decorative: the same commit's TC7 fix
explicitly refused to hardcode line numbers in item 49 because *"the file has
changed since the original review and will keep changing, so a hardcoded list
would itself go stale"* — the lesson was applied in one bullet of one file and
violated in another bullet of the same file, in the same pass.

**Failure scenario.** A future round takes up item 48's `Next trigger` (choose
between the `MAP_FIXED` re-map and the S9 `MADV_FREE_REUSABLE` route). Its first
step is the sync check the bullet demands. It opens `lib.rs:2204-2209`, finds
`reserve_aligned_huge_raw`, and concludes either that the anchor is stale
(re-deriving it by grep — the cost the anchor exists to eliminate) or, worse,
that the second sync site was removed and the bullet is now unilateral.

**Fix:** replace both with symbol names — "`decommit_lazy`'s rustdoc and
`madv_free_advice`'s doc comment" — matching TC7's own precedent in the
neighbouring item 49.

---

## Category 3 — unsafe-proof and platform-contract wording

### U4 — LOW — `win_reserve_commit`'s single-call retry carries a `// SAFETY:` proof copied verbatim from the two-call path, describing a live reservation that does not exist at that point

**Where:** `crates/vmem/src/lib.rs:1617` (the comment) above `:1618-1623` (the
retry call); versus the genuine sibling at `:1702-1706`.

```rust
// lib.rs:1610-1623 (single-call path)
match NonNull::new(p as *mut u8) {
    Some(n) => n,
    None => {                                    // the first VirtualAlloc FAILED
        if extra_commit_flags != 0 {
            // Best-effort retry: try without extra_commit_flags (e.g.
            // MEM_LARGE_PAGES). This matches the two-call path's fallback
            // behavior.
            // SAFETY: same range within the same live reservation.
            let plain = VirtualAlloc(
                core::ptr::null_mut(), commit_len, MEM_RESERVE | MEM_COMMIT, …
            );
```

There is no "same range" and no "same live reservation": the preceding
`VirtualAlloc` returned `NULL`, so nothing is reserved, and the retry is a fresh
`VirtualAlloc(NULL, commit_len, MEM_RESERVE | MEM_COMMIT, …)` that asks the
kernel to pick an address. The sentence is a verbatim copy of `:1703`, where it
*is* accurate — the two-call retry re-commits `base.as_ptr()` inside the
`MEM_RESERVE` region obtained at `:1651`.

The call is sound (it passes a null address and an integer length; there is no
pointer precondition to uphold at all), so this is a wrong *proof*, not a wrong
*call*. It is filed because this crate's stated discipline is that every
`unsafe` operation carries a proof attached to its own operation, and a proof
whose stated premise is false is worse than none: the next reader has to decide
whether the author believed something untrue about the control flow, or merely
pasted a line. It also matters that the round-7 closing review recorded this
exact comment as CLOSED **because** it was "byte-identical to the two-call path's
sibling at `:1681-1686`" — byte-identity is the defect, not the evidence.

**Fix:** one line — `// SAFETY: fresh anonymous reserve+commit at a
kernel-chosen address; NULL is checked below.` (The truthful proof for this site
is also strictly simpler than the one it replaces.)

### U5 — LOW-MEDIUM — `decommit_lazy`'s rustdoc teaches the Linux "a write afterwards is fine" model and never mentions `recommit`; on Windows that same call is the eager `MEM_DECOMMIT` path, where the write is a hard `STATUS_ACCESS_VIOLATION` — the divergence that already crashed an in-repo consumer

**Where:** `crates/vmem/src/lib.rs:1102-1145` (`decommit_lazy`'s whole rustdoc),
specifically `:1108-1112`; versus `decommit`'s own divergence paragraph at
`:1045-1054` and the code at `:1748-1756` (the Windows
`decommit_pages_impl`, which ignores `DecommitKind` entirely).

`decommit_lazy`'s summary correctly says "Windows falls back to the eager
[`decommit`] path, which has no lazy equivalent" (`:1105-1106`). The very next
paragraph then says:

> Unlike [`decommit`], **on Linux** the pages are NOT necessarily zeroed on next
> access if the kernel has not yet reclaimed them (**a write before reclamation
> keeps the old contents and cancels the free**) — so this is appropriate only
> for memory whose contents the caller no longer needs but has not yet
> overwritten.

That sentence is correctly scoped to Linux and is not false. But it is the only
statement anywhere in this rustdoc about *writing into the range after the
call*, and it describes that write as benign and, implicitly, as the reason to
prefer the lazy variant. The word `recommit` does not appear **once** in
`decommit_lazy`'s rustdoc — while `decommit`'s opening sentence says "Re-access
after decommit produces fresh zero-filled pages (after [`recommit`] on Windows…)"
and its `:1045-1054` paragraph spells the crash out. The safety section says
only "Same as [`decommit`]", and `decommit`'s `# Safety` section (`:1033-1037`)
is the two-line ownership contract — the Windows crash paragraph lives in the
rustdoc *body*, outside what "Same as" naturally pulls in.

**Failure scenario (with precedent in this repository).** A consumer writes the
Linux-shaped pattern the doc describes: `decommit_lazy(base, a, b)` on a cache
region, then later writes into `[a, b)` without recommitting — which on Linux is
not merely legal but is the documented cheap path (`MADV_FREE` + a write simply
cancels the free). On Windows the same code takes `MEM_DECOMMIT` and the write
is a hard `STATUS_ACCESS_VIOLATION`. This is the identical crash
`docs/CORRECTNESS_OPEN_ITEMS.md` item 6 records for `decommit`, from an in-repo
consumer that assumed Linux semantics — and `decommit_lazy` is the entry point
where the crate itself teaches the assumption.

**Fix:** one clause in `:1108-1112` — e.g. "…(on Windows this call is the eager
[`decommit`] path: a write into the range before [`recommit`] is a hard
`STATUS_ACCESS_VIOLATION`, not a re-fault — see [`decommit`]'s platform-divergence
paragraph)". The README's `decommit_lazy` table row (`:49`) is a second, cheaper
surface for the same clause.

---

## Category 4 — publish surface, coverage, and index hygiene

### U6 — LOW, publish-relevant (task #658) — three hit-rate percentages ship on the crates.io landing page and in `reserve_aligned`'s rustdoc as "measured", with no platform attribution; their own source card labels them WSL2-only and explicitly not definitive

**Where:** `crates/vmem/README.md:40` and `crates/vmem/src/lib.rs:849-853`;
versus `docs/perf/OPEN_ITEMS.md:1130-1136` (item 46's own header and
current-state card).

Both shipped surfaces read: *"measured hit rate: 34.4% at 64 KiB align, 46.7% at
1 MiB, 56.7% at 4 MiB — commit `35d51e6`, task #849"* (the README omits even the
commit). The card those numbers come from is titled:

> 46. **R-V20-849 — Unix exact-reserve hit rate (aligned-vmem, **WSL2 only**).**
>   - **Status:** measured, **requires bare-metal Linux remeasurement before
>     decision is definitive**.
>   - **Next trigger:** bare-metal Linux re-measurement — this measurement was on
>     WSL2/Hyper-V, and **a native Linux kernel's VA layout/ASLR entropy may
>     differ**. The WSL2-only number is not strong enough evidence to close
>     V20/P17 outright in either direction.

Three qualifications are lost in transit: (1) the numbers are from WSL2 under a
Hyper-V-backed kernel, not a native one; (2) the repo's own index says they may
not transfer to bare-metal Linux; (3) they are Linux-specific, while the
sentence that carries them is the general **Unix** cost paragraph — and the same
rustdoc says two sentences earlier that the hit rate "depends on the OS's
placement heuristics", which makes a bare Linux-derived number actively
misleading for a macOS or FreeBSD reader. This is the shape CLAUDE.md's own
evidence rules exist for (a measured number must carry its regime; a percentage
must name what it is over).

**Failure scenario.** A prospective consumer sizing VA budget on macOS or native
Linux reads "56.7% at 4 MiB" as this crate's behaviour, budgets ~43% of segments
to hold an extra `align` bytes of VA, and gets a materially different miss rate —
with no way to discover the number was WSL2-only, because the qualifier lives in
a workspace-root file that is not in the published package (`cargo package
--list` confirms: no `docs/`). Post-0.2.0 this needs a version bump to correct;
pre-publish it is two clauses.

**Fix:** "(measured on WSL2/Linux, x86_64; 30-run aggregate — the hit rate is
kernel- and ASLR-dependent and is not expected to transfer to other Unix
platforms)". The already-URL-ified citation style TC4 established gives a place
to point for the full card.

### U7 — LOW — the deliberately asymmetric decommit-vs-recommit contract is only half-tested: nothing anywhere passes a contract-violating range to `decommit`/`decommit_lazy`, so the "silently skip" half — and its `page_size()`-vs-`PAGE` basis — has zero coverage

**Where:** `crates/vmem/src/lib.rs:1084-1087` and `:1147-1150` (the two silent
guards); the documented asymmetry at `crates/vmem/README.md:100-106` and
`:112-127`; the tested half at `crates/vmem/tests/smoke.rs:269-305`
(`recommit_rejects_contract_violating_offsets`) and
`crates/vmem/tests/lazy_commit.rs:164-200`
(`commit_range_rejects_contract_violating_offsets`).

Mechanically checked: every `decommit(` / `decommit_lazy(` call site in
`crates/vmem/tests/` — `mock.rs:21`, `:22`, `smoke.rs:194`, `:257`, `:393`,
`:491`, `:492` — passes a well-formed, `page_size()`-aligned range. Not one test
passes `start == 1`, a non-page-multiple `end`, or `start > end` to either
function, on any platform, under any feature set.

The two `recommit`-side tests each open with a paragraph about task #712's real
crash, and the README devotes two paragraphs to arguing the asymmetry is
*intentional* ("`decommit`'s `()` return has no write-permitting sentinel to
misuse, so silently skipping is safe"). The half that is argued for at length is
the half with no oracle.

**Counterfactual (this is what makes it worth filing).** Change `:1085` and
`:1148` from `ps` to `PAGE` — the exact "unification" a contributor would
attempt after reading the README's asymmetry paragraph and noticing that
`recommit` validates against `PAGE` while `decommit` validates against
`page_size()`. The whole suite stays green on every platform and every feature
set, including the macOS CI rows, because no test decommits at a
`PAGE`-but-not-`page_size()` offset. On a 16 KiB-page host that change silently
forwards a 4 KiB-aligned `addr` to `madvise(2)`, which rejects the entire call
(all-or-nothing) — the crate's own `page_size()` rustdoc (`:377-383`) explains
this hazard, and nothing tests it. Deleting the guard outright is likewise
undetectable.

**Fix (cheap; both seams already exist).** Under `mock`: assert
`aligned_vmem::mock::drain()` records **no** `Call::Decommit` for
`decommit(base, 1, PAGE)` — portable, no real syscall, runs on every platform in
the existing `--all-features` rows. Under `bench-internals` on Unix: assert
`unix_madvise_attempts()` does not advance across a misaligned call, using the
`SERIAL` mutex the file already has.

### U8 — LOW — item 1's headline still says the CHANGELOG gap "has recurred THREE times" while its own current-state bullets, updated by the same commit, say seven

**Where:** `docs/CORRECTNESS_OPEN_ITEMS.md:63` (the item's first visible line)
versus `:74-75` (its `Status` and `Current number` bullets).

`:63` reads: *"This has recurred **THREE** times across the aligned-vmem
campaign alone:"* followed by three sub-bullets for rounds 1–3. `:75` reads:
*"round 7 (tasks #888-894) is a **7th instance**, caught by TC1 in the round-7
closing review the identical way"*. `8380607` updated `:74` and `:75` and left
`:63` untouched.

This is the R34-24 defect in its literal form — CLAUDE.md's rule requires an
open item to *read* as current state, and singles out a "stale header" as a
structural defect on par with a missing Status update. It bites harder here than
usual because item 1's entire content **is** a count: the argument for adopting
the standing rule rests on how many times the gap has recurred, and the first
sentence a round-start reader sees under-reports it by four.

**Failure scenario.** A round-9 session performs CLAUDE.md's mandatory
round-start read, skims item 1's headline plus its three dated sub-bullets
(rounds 1, 2, 3), and concludes the last recorded recurrence is round 3 — i.e.
that the closing-review catch mechanism has been holding for four rounds. The
opposite is true and is stated 12 lines below: rounds 6 and 7 both reproduced the
gap and both were caught only by their closing review. That is the reader who
then decides the standing rule is not urgent.

**Fix:** rewrite `:63` to "This has recurred **seven** times across the
aligned-vmem campaign alone (rounds 1–7; see the Current-number bullet for the
per-round breakdown and which were caught within their own round)", and let the
three historical sub-bullets stand as the narrative for the first three.

---

## Category 5 — INFO

### U9 — INFO — TC8's rewording widened the provenance claim from "in the public API" to "anywhere in the crate"; nine exposed-address casts ship in the crate's own test files, one of them in the "runnable form" of the snippet whose cast the same round converted

**Where:** `crates/vmem/README.md:171-175`; the casts at
`crates/vmem/tests/smoke.rs:148`, `:529`, `:629`, `:630`,
`tests/lazy_commit.rs:24`, `:286`, `:287`, `tests/huge_pages.rs:52`, `:118`.

The paragraph now reads: *"The returned pointers preserve provenance (**no
exposed-address `as usize` casts anywhere in the crate** — the mock backend's
diagnostic-only call recorder stores addresses as `usize` for
comparison/logging, obtained via the non-exposing `.addr()` …)"*. The claim is
exactly true of `src/` (verified: `grep -rn "as usize" crates/vmem/src/` yields
three integer widenings and two comments, zero pointer casts) and false of the
package as shipped, which includes all seven `tests/*.rs`.

The sharpest instance: round 7's T9 converted the showcase snippet at
`lib.rs:61` and `README.md:26` from `base as usize % span` to
`base.addr() % span`, and `lib.rs:72` tells the reader *"Runnable form:
`tests/smoke.rs`"* — where `smoke.rs:148` is still `assert_eq!(base as usize %
span, 0, …)`. The doc example and its designated runnable form now teach
different idioms.

Zero runtime consequence; recorded because (a) the previous scope word ("in the
public API") was accurate and the new one is not, (b) these nine would be
flagged under `-Zmiri-strict-provenance` if item 41's miri CI step lands, and
(c) `base.addr()` is the same length as `base as usize`.

### U10 — INFO — four of the six `bench-internals` counters have no test anywhere; the Windows pair has neither a test nor an example, despite making precise documented claims

**Where:** `crates/vmem/src/lib.rs:206-252` (the four reserve-path counters and
their accessors at `:282-330`); `grep -rn "windows_reserve_commit\|unix_exact_reserve"
crates/vmem/tests/` → **no output**.

`UNIX_EXACT_RESERVE_{ATTEMPTS,HITS}` are exercised only by
`examples/v20_849_unix_exact_reserve_hit_rate.rs` (which is compiled, not run,
in CI). `WINDOWS_RESERVE_COMMIT_{SINGLE_CALLS,TWO_CALL_PAIRS}` are exercised by
nothing at all, on the one platform where they are the only instrument, while
their rustdoc makes two checkable behavioural claims: a large-page retry "issues
a second syscall but is still counted as 1 in this counter" (`:233-236`), and
the two-call path issues "exactly 2 syscalls … plus a possible third best-effort
retry … not counted here" (`:244-249`).

This is the natural path-activation oracle (CLAUDE.md R30-8) for
`win_reserve_commit`'s dispatch condition — and that condition's `commit_len ==
size` half was a **real bug** (task #848), currently regression-tested only
indirectly, through `reservation_len()`
(`tests/lazy_commit.rs:71-117`, `tests/smoke.rs:104-113`). A direct assertion
(`align = PAGE` → single-call count advances by 1, two-call by 0;
`align = 1 MiB` → the reverse) is a dozen lines on the existing Windows CI row.
Distinct from round 5's QC8, which was about `reservation_len()` and is closed.

### U11 — INFO — `page_size()`'s sanity guard, which item 43's card leans on and U1 turns load-bearing, is structurally untestable: `query_os_page_size()` has no injection seam

**Where:** `crates/vmem/src/lib.rs:390-406` (the guard), `:409-445` (the three
`#[cfg]` arms of `query_os_page_size`), `:168` (`PAGE_SIZE_CACHE`).

Task #714 added the guard specifically to defend against a wrong `_SC_PAGESIZE`
constant, and item 43's card cites it as the reason a wrong value "silently
returns garbage" rather than crashing. No test exercises it: the queried value
comes from a private, platform-gated `fn` with no seam, and the result is cached
in a process-wide atomic on first call, so even a hypothetical test could not
re-run the guard. The rejection branch (`queried < PAGE`, or non-power-of-two)
has never executed in any test or CI run in this repository.

Recorded rather than filed as a defect because the cheap remedies each have real
costs (a `#[cfg(feature = "bench-internals")] fn dbg_set_page_size_for_test`
would be a safe `pub fn` mutating a value the alignment fast path trusts —
precisely the shape CLAUDE.md's benchmark-hook rule warns about; splitting the
guard into a pure `fn sanitize_page_size(queried: usize) -> usize` and testing
*that* is the clean version and costs one small refactor). If U1's fix is taken,
this becomes purely cosmetic, which is one more argument for taking it.

---

## Checked and explicitly NOT findings

Recorded so round 9 does not re-derive them.

- **The Windows `granted_huge` reporting on both paths.** Single-call:
  `Ok((base, base, commit_len, extra_commit_flags != 0))` is reached only when
  the `MEM_LARGE_PAGES` call itself succeeded (`:1641`); the retry returns
  `false` (`:1628`). Two-call: `:1710` returns `false` on the ordinary-page
  retry, `:1732` returns the requested flag with the documented
  "unreachable in practice" note at `:1727-1731`. Consistent with W2.
- **`try_recommit`/`try_commit_range` validating against `PAGE` while
  `decommit` validates against `page_size()`.** Re-derived on the
  now-confirmed 16 KiB macOS runner: `recommit_pages_impl` is a bare `Ok(())`
  on all Unix (`:2162-2181`), and Windows' page size is 4 KiB on every
  supported target, so no reachable failure. Documented at `README.md:100-106`.
  (U7 is about the *test coverage* of the `decommit` side of this asymmetry,
  not about the asymmetry itself.)
- **`RawReservation` / `finish_reservation_huge`'s remaining 4-tuple.**
  Already filed and settled as **W7** in
  `docs/reviews/2026-08-12-aligned-vmem-post-campaign-closing-review.md`, whose
  chosen resolution — record in `RawReservation`'s own doc that it is a
  call-site convenience, not the hazard's elimination — is present verbatim at
  `lib.rs:884-888`. Not re-filed.
- **The `SERIAL` mutex's coverage in `smoke.rs`.** Re-verified by reading every
  `fn` body: the four tests reaching `decommit`/`decommit_lazy` (`:179`, `:236`,
  `:384`, `:462`) all take it; no other test in the file reaches either.
  `libc_madvise` (`:2473-2502`) remains the sole incrementer of the madvise
  counters and is called only from `decommit_pages_impl`'s two arms.
- **Overflow discipline.** Re-checked every arithmetic site that could wrap:
  `size.checked_add(align)` on both backends (`:1644-1646`, `:1990-1992`),
  `align_up_addr`'s `checked_add` (`:2622-2626`), `leak_zeroed_pages`'s round-up
  (`:1511`), `a.checked_add(size)` / `region_addr.checked_add(over)` in both fit
  computations, and both `len = end - start` sites preceded by their ordering
  guard.
- **`unix_reserve`'s hugetlb alignment guard.** Task #714's argument re-derived:
  with `size` and `align` both `LINUX_HUGE_PAGE_SIZE` multiples (`:1978-1984`),
  `over = size + align` is too, so all four reachable `munmap` lengths are
  huge-page multiples.
- **`fault_injection`'s atomics.** Unchanged since #718/#775: `Release`/`Acquire`
  pair at `:108`/`:139`, `fetch_update` with the lazy-`then` underflow note at
  `:125-133`, the third disarm-vs-rearm race still declared out of scope in the
  module doc (`:47-57`). `fail_next_is_atomic_under_concurrent_callers`'s
  post-#775 `armed == calls / 2` oracle is genuinely two-sided.
- **`src/error.rs` and `src/mock.rs`** — read in full. The three-way
  classification, the `std::io::Error` bridge, the `invalid_argument` vs
  `os_refusal_unknown_code` split, and the "capture immediately after the
  failing syscall" timing rule are consistent between code, docs and tests at
  every site. `mock`'s partial-backend shape and feature-unification hazard are
  documented at three surfaces and settled by QC1.
- **The `alloc-lazy-commit` compat alias (`Cargo.toml:44`).** Looked at for a
  publish-surface finding and dropped: it is a documented one-release
  compatibility decision with its own rationale, and deciding it properly needs
  archaeology on what 0.1.0 actually published. Not a defect; noted only so
  round 9 knows it was considered.
- **`fault-injection` not reaching `recommit`.** `should_fail_commit` is
  consulted only from `try_commit_range` (`:1315-1325`), never from
  `try_recommit`. `arm_fail_next`'s own doc names exactly
  `try_commit_range`/`commit_range`, so the documentation is accurate; the
  asymmetry is a scope choice, not drift.
- **Structure / CLAUDE.md conventions.** No inline `#[cfg(test)] mod tests`
  anywhere in `src/`; no `mod.rs`; zero runnable doctests (confirmed by
  `Doc-tests aligned_vmem … 0 passed` locally and on both macOS CI rows); every
  illustrative snippet uses a ```` ```text ```` fence (`lib.rs:54`,
  `mock.rs:15`, `benches/vmem_bench.rs:6`). The four-files-vs-"single-file seam
  crate" question is R13's and settled.
- **Semver / API surface.** `8380607` changed no public item's signature and no
  `#[cfg]` on any shipping item. `#[non_exhaustive]` placement, the `is_empty`
  deprecation, and `ReservationParts`'s shape are unchanged.
- **CI coverage.** No new gap beyond the one round 6 filed (no Linux row runs
  `bench-internals` against the real, non-`mock` Unix backend — item 48's S4
  remainder bullet, `docs/CORRECTNESS_OPEN_ITEMS.md:2104`; note that round 7's
  review cited this as `:2123`, which the file's own growth has since
  invalidated — the same class as U3, in a review doc rather than an index, so
  not filed). Re-verified against `ci.yml`: the
  Linux rows are `:858` (default features), `:900` and `:167` (`--all-features`,
  which turns `mock` on), and `:920` (`fault-injection lazy-commit`, no
  `bench-internals`). Exactly as described.

---

## Categories with nothing to report

- **Memory safety / UB.** Every `unsafe` block in `src/lib.rs` was read against
  its call site this round. No safe `pub fn` accepts a raw pointer at all, let
  alone one that touches allocator metadata (CLAUDE.md's benchmark-hook rule);
  the six `bench-internals` accessors are `AtomicU64` reads with no arguments.
  Provenance discipline (`.addr()`/`.with_addr()`) is intact on both native
  backends' aligned-base derivations (`:1667`, `:1688`, `:2025`, `:2045`,
  `:2108`, `:2445`). U4 is a wrong proof on a sound call; U9 is a README scope
  word.
- **Performance — null, eighth consecutive round, re-derived not inherited.**
  The specific checks run this round: (1) counted syscalls per entry point on
  each backend — Windows single-call reserve = 1, two-call = 2, Unix fast-path
  hit = 1, Unix miss = 3 (`mmap`→`munmap`→`mmap`), unchanged; (2) confirmed
  `page_size()` is one relaxed load after first call and is invoked exactly once
  per `decommit`/`decommit_lazy` (`:1084`, `:1147` — P-B's double-load stays
  fixed) and once per exact-reserve attempt; (3) confirmed every
  `bench-internals` counter, its storage, and its `use` are all behind the
  feature (`:203-204`, `:215-280`), so a plain build carries no extra
  instruction; (4) re-read the two live filed ideas and found no third one —
  `docs/perf/ALIGNED_VMEM_VIRTUALALLOC2_VA_OPTIMIZATION_OPPORTUNITY.md`
  (Windows `VirtualAlloc2`, BSD `MAP_ALIGNED`) and item 46's S12 (Linux/Darwin
  `mmap` hint retry) between them cover the whole fast-path design space this
  crate has. Nothing in this crate is on an allocation hot path — reservation is
  a per-segment cold path. **Note that U1's fix is perf-neutral by the crate's
  own measurement**, which is what makes it cheap: the conjunct it removes
  saves no syscall.
- **Error contracts.** No drift found between `VmemError`'s three kinds, their
  rustdoc, the `io::Error` bridge, and `tests/vmemerror_io_bridge.rs` +
  `vmem_error_kinds_are_distinguishable`.

---

## Recommended order

1. **U1** — one `&&` conjunct, plus a sentence appended to item 43's
   `Current-number-or-verdict`. It is the only finding this round with a
   soundness-shaped consequence, and it is cheaper to fix than to keep
   re-adjudicating: three rounds have now cleared these two lines with an
   argument that only covers half the hazard.
2. **U5** — one clause in `decommit_lazy`'s rustdoc (plus the README row). A
   documented crash footgun with in-repo precedent (item 6), on the entry point
   that teaches the wrong mental model.
3. **U6** — two clauses, before 0.2.0 publishes (task #658).
4. **U2** — name `decommit_recommit_roundtrip` in both `smoke.rs:355-358` and
   `lazy_commit.rs:340-342`; fixing only the copy leaves the source standing.
5. **U3**, **U8** — replace two line-number citations with symbol names; fix one
   stale count in a headline. Both are round-start-read hazards.
6. **U7** — one test under `mock`, ~15 lines, closing the untested half of a
   contract the README argues for at length.
7. **U4** — one line.
8. **U9**, **U10**, **U11** — record-only; fold into whatever task next touches
   those lines. U10 is worth doing if any round adds a Windows-side judge.

---

## On diminishing returns — the honest answer this round was asked for

**Yes, this crate has reached diminishing returns for this methodology, and the
data says so fairly precisely.**

The find curve, counting only findings that changed shipped behaviour or shipped
text rather than index/process hygiene:

| Round | HIGH | MEDIUM | Code-relevant (LOW+) | Nature of the headline |
|---|---|---|---|---|
| 1–3 | several | several | many | real bugs (V1 munmap alignment, W2 `is_huge`, #712 write-permitting sentinel) |
| 4–5 | 0 | few | several | contract/API-surface corrections |
| 6 | 0 | 2 | ~8 | **the world changed** — real macOS CI ran for the first time |
| 7 | 0 | 1 (process) | ~5 | **the world changed** — the pushed evidence arrived and nothing recorded it |
| 8 | 0 | **1 (code)** | 7 | one unexamined direction in a check three rounds had cleared |

Rounds 6 and 7 were non-vacuous because *external reality changed between
rounds* (a Darwin runner executed; its results landed). No such change happened
before round 8: `8380607` was pushed, CI went green, and nothing new was
learned from the world. Round 8 had to find its content inside the code, and
what it found — U1 — required contradicting three prior rounds' explicit
clearance of the same two lines. That is a real result, and it is also exactly
what "diminishing returns" looks like at the point where the remaining findings
are the ones that survived multiple prior passes.

Three further signals, all measured this round rather than asserted:

1. **The remediation quality is now high.** Eight of nine TC fixes landed on
   exactly the right content. The three findings that *are* about round 7's own
   diff (U2, U3, U9) are all citation/scope-word defects, not logic — and two of
   them are second-order (a correction that inherited an older error; an anchor
   invalidated by its own commit). The campaign is now mostly finding defects in
   its own bookkeeping.
2. **Whole categories have gone quiet, verifiably.** Memory safety: null for
   four consecutive rounds, with every `unsafe` block re-read each time.
   Performance: null for eight, with the design space provably closed by two
   filed items. The benchmark-hook rule: no candidate has existed since R25-1.
3. **The remaining real risk is not reachable by reading.** Item 43's BSD half,
   item 48's Darwin fix, and item 41's miri CI step are all blocked on
   *hardware/CI*, not on review effort. U1 is the exception that proves it: it
   is a reasoning gap about an unverified constant, and its fix is precisely to
   stop depending on the unverified thing.

**What I would recommend instead of a round 9 of the same shape.** Fix U1 and
U5, land the four one-line citation/text fixes, and then convert the campaign
from "review the crate again" to "buy the evidence the crate cannot reason its
way to": a Linux CI row with `bench-internals` against the real backend (closes
item 48's S4 remainder and gives U10 a home), the `cargo miri test` step item 41
already owns, and — if any BSD runner ever becomes cheap — item 43's assertion.
Those three are the only remaining items with a genuinely unknown answer. A
ninth read of `lib.rs` is not.

**If a round 9 does happen**, the highest-yield target is not the source at all:
it is the pattern U2, U3 and U8 share — three separate stale or wrong pointers
between durable records, all mechanically checkable. A ~50-line script that
resolves every `file:line` and every `` `test_name` `` citation in
`docs/CORRECTNESS_OPEN_ITEMS.md`, `docs/perf/OPEN_ITEMS.md` and this crate's
comments would have caught all three, and would keep catching them for free —
which is a better use of a round than another human pass over 2,627 lines that
four rounds have now found clean.
