# Correctness / CI-debt open items -- [T] Tracked tier -- per-OS/arch runtime contracts (aligned-vmem, numa-shim)

**Part of the split index.** This file holds the full text of every **[T]**
(tracked, not yet actioned) card whose subject matches this file's own
criterion (below). Start at `docs/CORRECTNESS_OPEN_ITEMS.md` for the
purpose/scope/convention header and the round-start reading order, and for
the complete item-number to file lookup table; come here for these specific
card bodies. See `docs/correctness-open-items/ACTIVE.md` for the **[A]**
tier, `docs/correctness-open-items/RESOLVED.md` for the closure trail, and
the sibling `[T]`-tier files (`TRACKED_hook_safety.md`, `TRACKED_verification_coverage.md`, `TRACKED_ci_gate_coverage.md`, `TRACKED_test_flakiness.md`, `TRACKED_correctness_residuals.md`, `TRACKED_publish_readiness.md`, `TRACKED_process_record.md`, `TRACKED_misc.md`) for the rest of
the tier.

**Criterion for this file:** A card belongs here if it is about whether code behaves correctly on a specific OS or architecture (HugeTLB, Darwin madvise, Windows large pages, BSD/Android/tvOS/watchOS/MIPS, page-size constants, numa-shim syscalls), or whether that OS-specific behavior has been empirically verified on real hardware versus only reasoned-from-spec.

**Card count:** 13.

**Why split by theme, not by item-number range (task #1222, 2026-08-20):**
task #1221 (same day) split the former single `TRACKED.md` into four
number-range files, balanced by line count. The owner rejected that split
and asked for a thematic split instead -- grouping cards by what they are
actually ABOUT, derived from reading all 70 cards rather than assumed.
Every citation of this index that points at ONE SPECIFIC ITEM carries
that item's number, in the form `` `docs/CORRECTNESS_OPEN_ITEMS.md`
item N `` -- task #1227 repaired the seven in `aligned-vmem` that did
not, and two outside it were still open as of that task (both are
recorded in the thin index's Structure section). Citations that point
at the FILE as a whole, at a named SECTION, or at a CLASS of items
rather than one item carry no item number and never needed one (task
#1227's finding; until #1236 these headers overclaimed it as a
universal, asserting that no citation ever pointed at anything but
an item number). Only the numbered citations depend on where item
numbers live, and `docs/CORRECTNESS_OPEN_ITEMS.md` (the thin index)
carries the complete, mechanically generated item-N -> file lookup
table covering EVERY `[T]`-tier number (including the `59a`/`59b`
sub-items) that keeps them resolving -- that table, not this file's
name, is what makes the thematic split safe: the lookup is two-hop
(index table, then this file), but mechanical and always correct. No citing-file
count is typed in this header on purpose: the "42+" typed here at
the split was already 43 (census against the split commit) -- #1230
removed it from one of these nine headers, #1236 from the other
eight; compare against this command's output, never a hardcoded
count:

```text
git grep -l "docs/CORRECTNESS_OPEN_ITEMS\.md" -- ':!docs/' | wc -l
```

(Split 2026-08-20, task #1222, superseding task #1221's number-range
split the same day.)

---
6. **[T, filed 2026-07-30 during R30-1/task #450's verification]
   `examples/r29_3_decomposition_gate.rs` crashes with
   `STATUS_ACCESS_VIOLATION` when run NATIVELY on Windows** (as opposed to
   under WSL2/Linux, which is where this example's own gate report,
   `docs/perf/R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE.md`, has always
   measured — see that doc's "Platform measured" line). The crash is in
   Measurement B: the `write_volatile` re-fault loop immediately after
   `HeapCore::dbg_decomp_decommit_payload`. Root cause: Windows
   `MEM_DECOMMIT` (`crates/aligned-vmem/src/os/windows.rs`'s
   `decommit_pages_impl` — post-split home, task #1082) genuinely UNMAPS the payload pages, unlike Linux
   `MADV_DONTNEED`, which keeps the VA mapping resident and transparently
   re-faults a fresh zero page on next write. The example's Measurement B
   loop assumes the Linux semantics unconditionally (write-after-decommit
   silently re-faults); on Windows a write to a decommitted-but-not-yet-
   recommitted page is a hard access violation without an explicit
   `VirtualAlloc(..., MEM_COMMIT, ...)` recommit call, which the example
   never makes. **Confirmed unrelated to R30-1's `small_cur` fix**: the
   crash reproduces identically with that fix applied or reverted, and
   lives in a code path (`dbg_decomp_decommit_payload` → `os::decommit_pages`
   → `crates/aligned-vmem`) R30-1's diff never touches; isolated by running just
   the R30-1-relevant hooks' pre-fill/A/C/A' loops (which never call
   `dbg_decomp_decommit_payload`) natively on Windows for hundreds of
   iterations with no crash. **Needs (future round):** either gate
   Measurement B's re-fault loop on `cfg(not(windows))` with an honest
   "irreducible floor not measured on this platform" note, or add the
   missing `VirtualAlloc(MEM_COMMIT)` recommit call before the
   `write_volatile` loop so the measurement is platform-correct everywhere
   (this would also make Measurement B's timing include the ACTUAL Windows
   recommit cost — currently assumed `0 ns` "implicit" per the doc's own
   §2 table, which is a Linux-only claim; Windows `MEM_COMMIT` is a real
   syscall, not implicit).

26. **[T, filed 2026-08-06, task #653/P19, `docs/reviews/2026-08-06-publish-readiness-sweep-closing-review.md` finding P3-4 item 2] The numa-shim macOS+miri `mod platform` duplicate-definition fix (dc003c9) is structurally sound but empirically unconfirmed until the new `numa-shim-macos-miri` CI job actually runs on real macOS.**

    - **Status:** OPEN — pending empirical confirmation. The fix itself
      (adding `not(miri)` to the macOS platform-stub `cfg`, matching the
      three sibling platform blocks) is landed and reasoned-through-correct.
    - **Current-number-or-verdict:** commit `dc003c957b40baacaa147ff35e81884e27b0b1b4`'s
      own body states its local verification was done on Windows (no macOS
      box available) and explicitly does NOT exercise the macOS
      `not(miri)` arm or the macOS+miri crossing itself — "that
      verification depends on the new `numa-shim-macos-miri` CI job
      actually running on `macos-latest`." The closing review
      (`docs/reviews/2026-08-06-numa-shim-publish-readiness-review.md` /
      the sweep-closing review's own re-check) independently verified the
      fix is structurally correct via cfg-disjointness analysis (the macOS
      stub and the `cfg(miri)` any-OS stub can no longer both satisfy their
      `cfg` simultaneously), but static analysis is not the same as a real
      `cargo miri test` run on `macos-latest` actually going green.
    - **Why filed instead of fixed here:** there is nothing to "fix" — this
      is a pending-confirmation trigger, not a defect. It only needed
      filing so a future round doesn't have to re-derive from the commit
      body that confirmation is still outstanding.
    - **Next trigger:** confirm the `numa-shim-macos-miri` job
      (`.github/workflows/ci.yml`) runs green on its first real GitHub
      Actions execution (it is a per-PR job, so this should happen on the
      next PR/push that touches a path triggering it, or can be confirmed
      via `workflow_dispatch`/inspecting the Actions run history directly).
    - **Evidence:** commit `dc003c957b40baacaa147ff35e81884e27b0b1b4`'s
      full commit body (verification section); `.github/workflows/ci.yml`
      `numa-shim-macos-miri` job; `crates/numa-shim/src/lib.rs` (the `not(miri)`
      guard on the macOS platform-stub `cfg`, ~line 763);
      `docs/reviews/2026-08-06-numa-shim-publish-readiness-review.md`;
      `docs/reviews/2026-08-06-publish-readiness-sweep-closing-review.md`
      finding P3-4 item 2.

43. **Deferred verification — `aligned-vmem`'s per-OS `_SC_PAGESIZE`
    constant table (task #714) is REASONED-FROM-SPEC for 4 of 6 affected
    targets, never empirically executed.** (Filed 2026-08-09, task
    #776/F13, round-closing review of the aligned-vmem round.)

    - **Status:** PARTIALLY OPEN — macOS half CLOSED (see "Recently
      resolved" #43 for the closure narrative and CI citation). The other
      3 targets (FreeBSD, NetBSD/OpenBSD, DragonFly) remain OPEN — no
      action needed unless a runner becomes available; filed so the gap
      is visible rather than silently load-bearing on an unverified
      constant.
    - **Current-number-or-verdict:** macOS family = 29, empirically
      confirmed correct (see "Recently resolved" #43). FreeBSD/DragonFly =
      47 and NetBSD/OpenBSD = 28 remain NOT independently executed on real
      hardware — reasoned from each OS's own `sys/unistd.h` header value,
      cross-compile-checked via `cargo check --target
      x86_64-unknown-{freebsd,netbsd}` only, which confirms the code
      COMPILES but not that the numeric constant is correct;
      `x86_64-unknown-dragonfly`/`x86_64-unknown-openbsd` have no prebuilt
      rustup std component on this session's Windows host, so those two are
      not even cross-compile-checked, only reasoned by sharing the
      identical cfg arm as their verified-by-citation siblings. A wrong
      `_SC_PAGESIZE` value would cause `page_size()` to query the WRONG
      name via `sysconf`, silently returning garbage (or an unrelated
      system parameter's value) on any of these targets if the
      header-citation reasoning is wrong — note also that `page_size()`'s
      own silent fallback to `PAGE` (4 KiB) on an implausible value means
      the crate's OWN generic test (`page_size_is_a_valid_os_page`, `is
      power of two && >= PAGE`) would NOT have caught a wrong macOS
      constant either, since the fallback value passes that check too;
      this is exactly why the (now-resolved) macOS test asserted the
      exact 16 KiB value rather than the generic invariant. A second,
      distinct consequence of this same unverified-constant gap was found
      and fixed in round 8 (task #897, finding U1):
      `try_reserve_aligned_exact` (`crates/aligned-vmem/src/os/unix.rs`, post-split home, task #1082) used to skip
      its own alignment-of-the-returned-base check whenever
      `align <= page_size()`, reasoning that `mmap` always returns
      page-aligned addresses in that range — true only if `page_size()`
      is itself <= the real OS page size. A wrong `_SC_PAGESIZE` constant
      returning a power-of-two ABOVE the real page size on one of the
      still-open BSD targets would have made that skip silently return a
      base NOT aligned to the requested `align`, violating
      `Reservation::as_ptr()`'s documented alignment guarantee with no
      error and no diagnostic. Fixed by making the check unconditional
      (the `align > page_size()` conjunct measured zero syscalls saved —
      see the fix's own comment in `crates/aligned-vmem/src/os/unix.rs` (moved there by task #1055's split) — so removing it is free).
    - **Evidence:** `crates/aligned-vmem/src/os/unix.rs`'s `_SC_PAGESIZE` constant
      definition and its own doc comment cite the per-OS header values
      directly; no BSD runner exists in `.github/workflows/ci.yml`'s
      current matrix for this crate.
    - **Next trigger:** BSD half only — if/when a FreeBSD, NetBSD,
      DragonFly, or OpenBSD CI runner becomes available for this crate (or
      this repo gains one for any purpose), run `page_size()` on it and
      assert the returned value matches the platform's actual page size
      (typically 4 KiB on all four, making a silent wrong-constant bug
      hard to notice without an explicit assertion against the OS's own
      reported value via a DIFFERENT API, e.g. comparing against
      `/proc/self/status` or equivalent, not just checking the result is a
      power of two).

44. **Deferred verification — `numa-shim`'s mbind path (`lib.rs:531`, the
    crate's key selling point) has no behavioral oracle anywhere.** (Filed
    2026-08-09, task #778/F4, round-closing review of the numa-shim round —
    a distinct §D1a MEDIUM audit finding from the cpumap-parser one task
    #721 closed; #721's own commit message declines this half explicitly
    but only in commit prose, exactly the failure mode this index exists to
    prevent.)

    - **Status:** OPEN — no test on this repository's own CI asserts the
      `mbind(2)` syscall this crate wraps actually succeeds or has the
      documented effect.
    - **Current-number-or-verdict:** `bind_range_impl_linux`'s `mbind`
      return value is silently discarded by design; every test suite that
      touches this path asserts only no-panic (`tests/smoke.rs`) or that a
      `MockCall::BindRange` record was emitted (`tests/mock_dispatch.rs` —
      a declaration, not a behavioral proof). Mutating `SYS_MBIND` or
      scrambling the argument marshalling would leave every current test
      green.
    - **Evidence:** `docs/reviews/2026-08-07-numa-shim-rust-intel-audit.md`
      §D1a (`lib.rs:531`); confirmed still true by
      `docs/reviews/2026-08-09-numa-shim-round-closing-review.md`, which
      re-checked this specific gap during the round-closing review and
      found no test filled it in the round's 9 commits.
    - **Next trigger:** add an env-guarded Linux test (the weekly
      `numa-real-kernel` CI job is the natural home) asserting the
      `mbind(2)` syscall return is `0` for a valid single-node bind (a
      wrong syscall number yields `-1`/`ENOSYS` and goes red) and/or a
      `get_mempolicy(2)` readback asserting `MPOL_PREFERRED` with the
      expected nodemask — this would also be the only test capable of
      catching a future `maxnode`/marshalling regression of the exact
      shape task #697 fixed.

47. **`numa-shim`'s entire round (tasks #697/#720-727) is REASONED-FROM-SPEC
    for its Linux-only code, never empirically executed on this session's
    host.** (Filed 2026-08-09, task #778/F4, round-closing review —
    `aligned-vmem`'s round filed the analogous gap as item 43; `numa-shim`'s
    had no counterpart until now.)

    - **Status:** OPEN — no action needed unless a Linux runner with
      `#[global_allocator]`-installed test binaries becomes available;
      filed so the gap is visible rather than silently load-bearing.
    - **Current-number-or-verdict:** tasks #697 (`mbind` `maxnode`
      arithmetic), #720 (cpumap loop-to-EOF read), and #723/#777 (the
      `OnceLock`-based topology cache and its allocation-free redesign) are
      all `#[cfg(all(target_os = "linux", not(miri)))]`-gated and have
      NEVER executed on this session's Windows host — verified only via
      `cargo check`/`clippy --target x86_64-unknown-linux-gnu` (confirms
      the code COMPILES and type-checks, not that its runtime behavior
      matches the stated reasoning) plus careful manual derivation from
      kernel/API documentation. This is not hypothetical risk: task #777
      itself exists because task #723's REASONED-FROM-SPEC design had a
      real defect (a reentrancy deadlock) that compiled cleanly, passed
      every test this session could run, and was only found by a
      round-closing review reasoning about a deployment scenario
      (`#[global_allocator]` + `numa-aware` on real Linux) this session
      cannot construct.
    - **Evidence:**
      `docs/reviews/2026-08-09-numa-shim-round-closing-review.md` §5 (the
      review's own explicit confirmation that the verification-honesty
      distinction was maintained consistently, which is a STATEMENT about
      what was labeled correctly, not a substitute for the missing
      execution); the weekly `numa-real-kernel` CI job (`.github/workflows/ci.yml`)
      exercises real Linux but its test binaries do not install
      `#[global_allocator]` (grep-verified), so it cannot reproduce a
      reentrancy scenario like the one #777 fixed even though it does run
      on real Linux hardware.
    - **Next trigger:** if/when this repo gains a Linux CI runner (or a
      local `crush`/agent session with Linux execution access) capable of
      running `cargo test -p numa-shim --all-features` AND a real
      `#[global_allocator] = SeferAlloc` + `numa-aware` allocation
      workload together, use it to (a) empirically confirm #697/#720's
      REASONED-FROM-SPEC fixes behave as derived, and (b) add the
      integration-level regression test item 44 above also asks for —
      both share the same missing infrastructure.

48. **`aligned-vmem`'s `decommit()` silently fails to release physical memory (or zero-fill on `recommit`) on macOS — `MADV_DONTNEED` is advisory-only for anonymous memory on Darwin, unlike Linux.** First confirmed as a REAL, failing test (not just a documented risk) by CI on 2026-08-13, the FIRST time this crate's real (non-mock, non-miri) test suite ever ran against real macOS CI — round 4 (task #867/R1) added the CI row that finally exercises the real macOS backend instead of the `mock` stub, but the push to `origin/main` was deferred for two more rounds, so this is the first time the row actually executed. `decommit_recommit_roundtrip` (`crates/aligned-vmem/tests/smoke.rs`) failed: a byte written before `decommit`+`recommit` (`0x77` = 119) was still present after the cycle, where Linux/Windows both correctly read back `0`. **The underlying hazard itself was NOT newly discovered — it was already known repo-wide since Round 9** (see "Prior knowledge" below); only this extracted crate's own docs/tests had never reflected it until the fix commit below. `9c777bc`'s commit message calling this "a real, previously-undiscovered functional gap" is accurate only about this crate's own docs/tests, not about the repository as a whole — corrected here (round 6, task #883) after an independent review flagged the overstatement.
    - **Prior knowledge (repo-wide, pre-dating this "discovery" by multiple rounds):** the exact same hazard — Darwin `MADV_DONTNEED` being advisory/lazy with no zero-fill guarantee — was already documented in at least four places before this item was filed: `.github/workflows/ci.yml` (the `test-macos` job's own comment above its aligned-vmem test rows: "MADV_DONTNEED on Darwin is advisory/lazy (no zero-fill guarantee)" — re-anchored by content at task #1060, since the workflow's growth had rotted the old line reference); `src/alloc_core/alloc_core_small_pool.rs` (a production code comment, currently around lines 1002-1021, stating the same fact as the load-bearing risk area for the `virgin-zero-skip` feature); and two `virgin-zero-skip` design docs, `docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md` (around lines 115-116 and 358) and `docs/perf/R11_8_SMALL_VIRGIN_ZERO_SKIP_DESIGN.md` (around line 32), whose entire safety argument is built on this fact. The honest story is: the repo knew this when `aligned-vmem` was extracted from `src/alloc_core/os.rs`, the extraction lost that knowledge, and CI finally made the gap fail loudly rather than "discovering" something new.
    - **R9_5 mis-citation, also corrected here:** `docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md:115-116` cites "`crates/aligned-vmem/src/lib.rs` §decommit note" as its source for this fact. `git log --oneline -S "advisory" -- crates/aligned-vmem/src/lib.rs` shows that note was created BY commit `9c777bc` (dated 2026-08-13) — i.e. R9_5's citation was unverifiable/forward-referencing a note that did not exist when R9_5 was written (2026-07-20). It is now accurate as of `9c777bc`, purely by coincidence of the fix landing there. See the one-line notes added to both design docs (R9_5 near lines 115-116/358, R11_8 near line 32) in this same task. **Post-split update (task #1060, 2026-08-17):** the cited "`crates/aligned-vmem/src/lib.rs` §decommit note" — `decommit()`'s rustdoc Darwin caveat — now lives in `crates/aligned-vmem/src/api/decommit.rs` (its "Darwin zero-fill gap" paragraph) after task #1055 (commit `a4b8e50`) split the former monolith; the `git log` quotation above is left as written because it accurately describes the pre-split tree it was run against.
    - **Status:** OPEN — mitigated across more surface than the original `9c777bc` fix covered. As of round 6 (tasks #880-886): the test is scoped to not assert the false guarantee on the Darwin family (macOS/iOS/tvOS/watchOS); `decommit()`'s rustdoc, `recommit()`'s rustdoc, `decommit_lazy()`'s rustdoc, the crate-root module doc, `recommit_pages_impl`'s code comment, AND `README.md`'s new "Platform caveats" section all carry a consistent Darwin-scoped caveat (task #880/S1, task #881/S5, task #885/S7); the empirical root-cause oracle (task #882/S2) has now run on real macOS CI and settled the H1-vs-H2 question (see Root cause below, updated round 7 / task #888); `decommit_lazy_roundtrip`'s own vacuousness (S4's second limb) is recorded below, not yet fixed. The underlying functional gap itself is NOT fixed. `decommit()`'s core purpose — "return page-granular physical backing to the OS" — is silently unmet on the Darwin family for ordinary (non-huge) reservations: RSS does not decrease, and re-access after `recommit` returns stale data instead of a fresh zero page.
    - **Current-number-or-verdict:** confirmed via real CI (`test macos (production)` job, run `31676133649`, landing SHA `e60e46a`) — deterministic, not flaky (byte value matches exactly what was written before decommit). Linux (`aligned-vmem package gates`, `test workspace members`) and Windows (`test windows (production)`) both passed the same assertion in the same run, confirming the guarantee genuinely holds on those two platforms and the gap is Darwin-specific.
    - **Root cause:** `recommit_pages_impl`'s Unix implementation (`crates/aligned-vmem/src/os/unix.rs` — its home since the task #1055 split, re-verified at task #1060; `#[cfg(all(unix, not(miri)))]`) is an unconditional no-op for ALL Unix platforms, justified by a comment claiming "re-access after MADV_DONTNEED is implicit — fresh zeroed pages on demand" — true on Linux, false on the Darwin family. `decommit`'s own eager path calls `madvise(MADV_DONTNEED)` uniformly across all Unix too. **This explanation was ASSERTED, not ESTABLISHED, when first written (task #882):** it was inferred from a single failing byte, which was equally consistent with a different hypothesis — the `madvise(2)` syscall itself FAILING on that CI runner for an unrelated reason (H2), since `libc_madvise` discards `madvise`'s return value by design (task #719) and nothing in the crate could previously distinguish "syscall succeeded but Darwin's semantics didn't reclaim the pages" (H1) from "syscall itself failed" (H2). Task #882 added an empirical oracle: under the `bench-internals` feature, `libc_madvise` now also records attempt/success counts into two new `#[doc(hidden)]` statics, `UNIX_MADVISE_ATTEMPTS`/`UNIX_MADVISE_SUCCESSES` (accessors `aligned_vmem::unix_madvise_attempts()`/`unix_madvise_successes()`, reset via the existing `reset_bench_internals_counters()`), and a new macOS-gated test, `macos_decommit_madvise_syscall_actually_succeeds` (`crates/aligned-vmem/tests/smoke.rs`), that asserts the `madvise` syscall itself returns success (`0`) for BOTH the eager (`decommit`, `MADV_DONTNEED`) and lazy (`decommit_lazy`, `MADV_FREE_REUSABLE`) call sites. **Updated round 7 (task #888, finding T1):** commit `1dbd6b4` was pushed and CI run `31692217669` (job `94421845398`, `test macos (production)`, image `macos-26-arm64`) ran green — `macos_decommit_madvise_syscall_actually_succeeds` passed, with `unix_madvise_attempts() == 2 && unix_madvise_successes() == 2`, ruling out H2 (the syscall itself did not fail). **This does NOT by itself confirm H1** — the H1 argument has two halves observed in TWO DIFFERENT CI runs: the stale-byte evidence (`decommit_recommit_roundtrip`'s pre-scoping failure) comes from run `31676133649`/commit `e60e46a`, before that assertion was scoped off Darwin, while the madvise-success evidence above comes from run `31692217669`/commit `1dbd6b4`; no single run has observed both the stale byte AND the successful `madvise` syscall in the same process. **Correct wording: H2 is ruled out by run `31692217669`; combined with run `31676133649`'s stale byte, H1 (advisory-only semantics) is the only remaining explanation** — NOT "H1 confirmed by CI".
    - **Darwin lazy-path alternative fix (round-6 review S9, spec-read, not verified on hardware, not a recommendation to implement without further review):** three connected observations. (1) On Darwin, `decommit_lazy` issues `MADV_FREE_REUSABLE` but nothing in this crate ever issues the paired `MADV_FREE_REUSE` before re-touching pages (`recommit_pages_impl`'s Unix implementation is an unconditional `Ok(())`, confirmed by reading it) — Apple documents these as a required pair, so this is a physical-footprint-accounting-drift concern (not a memory-safety issue), distinct from this item's own eager-`decommit` finding. (2) `decommit_lazy`'s own rustdoc describes the general "lazy is cheaper, reclaimed only under pressure" ordering (Linux `MADV_FREE` semantics), but on macOS/iOS specifically that ordering is INVERTED on the RSS axis: `MADV_FREE_REUSABLE` drops footprint immediately there, while eager `decommit`'s `MADV_DONTNEED` drops nothing at all — the opposite of the general case (on tvOS/watchOS, `decommit_lazy` falls back to the same `MADV_DONTNEED` as the eager path, so there both are equally no-ops). (3) Because of (2), a cheaper but PARTIAL alternative to the `MAP_FIXED` re-map idea below exists and is worth recording: route macOS/iOS's eager `decommit` to `MADV_FREE_REUSABLE` and issue `MADV_FREE_REUSE` from `recommit` — this would close the "return physical backing to the OS" half of `decommit`'s promise on macOS/iOS but NOT the "reads as zero" half (since `MADV_FREE_REUSABLE` preserves contents if the pages are re-touched before reclaim). **tvOS/watchOS coverage (round 7, task #895, TC3 — synchronized with `decommit_lazy`'s rustdoc and `madv_free_advice`'s doc comment, which this bullet must keep agreeing with if either changes):** this crate's cfg currently only names macOS/iOS for `MADV_FREE_REUSABLE`, so as written this alternative would not extend to tvOS/watchOS — but `MADV_FREE_REUSABLE`'s value comes from XNU, the kernel all four Darwin targets share, so it MAY work identically there too; this is REASONED-FROM-SPEC, not verified on tvOS/watchOS hardware or a tvOS/watchOS build target (neither is available to this crate's CI), not an established "no `MADV_FREE_REUSABLE` there" fact. Only re-mapping is confirmed to close both halves on all four targets; the lazy-path alternative's tvOS/watchOS coverage is an open question, not a settled no.
    - **Next trigger:** a future round should implement a real Darwin fix — the standard technique is re-`mmap`(`MAP_FIXED | MAP_ANONYMOUS`) over the decommitted range instead of (or in addition to) `madvise`, which forces the kernel to actually replace the mapping with fresh zero pages; needs its own safety analysis (interaction with concurrent access to the same reservation, `is_huge()` state, the existing `huge_pages` feature's `MAP_HUGETLB` path) and its own review round rather than a rushed fix under a CI-green-checking task. Until then, `decommit()`'s Darwin-family behavior should be treated as "hint only, no RSS/zero-fill guarantee" — the same posture already documented for the huge-page case.
    - **S4 remainder (round-6 closing review SC1, partially fixed):** the round-6 review's S4 finding had two limbs — "macOS lost its only decommit effect-oracle" (closed by task #882's new counters/test above) and "`decommit_lazy_roundtrip` (`crates/aligned-vmem/tests/smoke.rs`) is vacuous on EVERY platform, not just macOS" (still not fixed — that test only checks a post-recommit write/read round-trips, never whether `madvise` had any effect; its rustdoc previously claimed otherwise, corrected in the closing pass). The new oracle's counters are `unix`-wide, not macOS-specific (`libc_madvise` is `#[cfg(all(unix, not(miri)))]`), so the same assertion style would close the Linux half too — but no CI row currently runs `bench-internals` against the real (non-mock) Unix backend on Linux (the Linux rows in `ci.yml` are default-features, `--all-features` which turns `mock` on, or `fault-injection lazy-commit` without `bench-internals`). Closing this fully needs either a new Linux CI row or accepting the gap stays macOS-only for now. **Stale premise corrected (task #1060, 2026-08-17):** the sentence above described the pre-task-#962 CI, when `mock` was still a Cargo feature and `--all-features` selected the mock backend. Since task #962 (`mock` became the `--cfg aligned_vmem_mock` build flag), the `aligned-vmem-gates` job's `cargo test -p aligned-vmem --all-features` row on ubuntu-latest — and `test-workspace`'s aligned-vmem `--all-features` row — run `bench-internals` against the REAL Unix backend on Linux, so the "new Linux CI row" this bullet asked for already exists. What is still missing is only a Linux-side oracle TEST: the existing oracle (`macos_decommit_madvise_syscall_actually_succeeds`, `crates/aligned-vmem/tests/smoke.rs`) is `target_os = "macos"`-gated, and its own doc comment still repeats the stale no-Linux-row premise — a source file, outside this doc-only task's reach.
    - **Evidence:** CI run `31676133649` (`gh run view 31676133649 --json jobs`), job `test macos (production)`, step "Run cargo test -p aligned-vmem --features ... --no-fail-fast", failure at `crates/aligned-vmem/tests/smoke.rs:174` (pre-fix line number) — `assertion left == right failed: recommitted page must be zeroed / left: 119 / right: 0`; fixed in commit (this task's own commit, landing after `e60e46a`). Discovery-framing and mis-citation correction: `docs/reviews/2026-08-13-aligned-vmem-round6-review.md` finding S3 (task #883). Round-6 closing review: `docs/reviews/2026-08-13-aligned-vmem-round6-closing-review.md`, findings SC1-SC10.

52. **`decommit_lazy` leaves free BSD reclaim on the table** (Filed 2026-08-14, task #934/C-9, from `docs/reviews/2026-08-14-aligned-vmem-pre-release-review.md` finding V-4.) **CLOSED** — see "Recently resolved" in RESOLVED.md for the full closure narrative.

53. **`Reservation::from_raw_parts` hard-codes `granted_huge: false`, creating a fail-open hazard when callers follow documented decommit advice** (Filed 2026-08-14, task #934/C-9, sub-observation about item 48 from `docs/reviews/2026-08-14-aligned-vmem-pre-release-review.md`.) **CLOSED** — see "Recently resolved" in RESOLVED.md for the full closure narrative.

58. **[T] CI-coverage gap: i686-gnu and i686-musl targets are compile-only verified, runtime exact/huge path never executes in CI.** (Filed 2026-08-16, TaskList #1023, from aligned-vmem prerelease-audit-r4 "Coverage gaps" section.)

    The CI workflow's `aligned-vmem-gates` job (its `rustup target add i686-unknown-linux-{gnu,musl}` + `cargo check --target` steps) runs `cargo check --target i686-unknown-linux-{gnu,musl} --all-targets` for compile-time verification of the 32-bit Unix exact-size path and the FFI `off_t` type-correctness fixes (item 44, task #914), but does NOT run `cargo test --target i686-...` to execute the runtime behavior. The `try_reserve_aligned_exact` path (actual gate, re-read off the source on 2026-08-17 rather than restated from this card's own earlier text: `#[cfg(all(unix, not(miri), target_pointer_width = "32"))]`) and the 32-bit `OffT`-corrected `mmap` calls are therefore never exercised under actual 32-bit execution in CI, only verified for compile-time correctness. **Correction (task #1045, finding R7-9):** this card previously described the gate as `not(target_pointer_width = "64")` plus `not(target_os = "android")`, i.e. as EXCLUDING Android. It does not — there is no `target_os` clause in the gate at all, so 32-bit Android is INSIDE this path, and the CI-coverage gap this card describes therefore covers Android as well. The exclusion never existed; it appears to have been carried over from a neighbouring huge-page gate, which is where a `target_os` clause does live.

    - **Status:** OPEN — not urgent, because the compile-only check catches the known FFI ABI mismatch risk (item 44), but the runtime exact-size reservation path is unverified on 32-bit targets.
    - **Current-number-or-verdict (re-verified off `.github/workflows/ci.yml` at task #1060, 2026-08-17):** the i686 coverage is still exactly two compile-only steps in the `aligned-vmem-gates` job — `cargo check --target i686-unknown-linux-gnu` and `cargo check --target i686-unknown-linux-musl`, both `--all-targets --features "lazy-commit huge-pages fault-injection bench-internals" -p aligned-vmem` — and a `grep i686` across `.github/workflows/` finds no `cargo test --target i686-*` step in any job, so the 32-bit runtime path still never executes in CI (32-bit Android included, per the task #1045/R7-9 correction above). The #1045 gate itself re-verified post-split: `try_reserve_aligned_exact` in `crates/aligned-vmem/src/os/unix.rs` still carries exactly `#[cfg(all(unix, not(miri), target_pointer_width = "32"))]` — no `target_os` clause; the split moved the file, not the gate.
    - **Next trigger:** when a 32-bit Linux runtime test runner becomes available in CI (e.g., via GitHub Actions `i686-unknown-linux-gnu` self-hosted runner or QEMU-based emulation), add a `cargo test --target i686-unknown-linux-gnu` step to the `aligned-vmem-gates` job. Until then, the compile-only check is the best available coverage.
    - **Evidence:** the `cargo check --target i686-unknown-linux-{gnu,musl} --all-targets` steps in `.github/workflows/ci.yml`'s `aligned-vmem-gates` job (compile-only); `crates/aligned-vmem/src/os/unix.rs` `try_reserve_aligned_exact` function (the 32-bit-gated exact-size reservation path; its home since the task #1055 split); item 44 (the `OffT` fix that this compile-only check guards against regressions).

59. **[T] CI-coverage gap: Linux MAP_HUGETLB and Windows MEM_LARGE_PAGES success paths depend on configured hugetlb pool or SeLockMemoryPrivilege, neither present in standard CI.** (Filed 2026-08-16, TaskList #1023, from aligned-vmem prerelease-audit-r4 "Coverage gaps" section.) **Split task #1160/F5 into a Linux half (59a, now execution-covered — with a caveat) and a Windows half (59b, still fully open) — see those two cards below; this parent card is superseded by them and kept only as the historical filing record.**

    The Linux `MAP_HUGETLB` success branch (`libc_mmap` with `MAP_HUGETLB | MAP_HUGE_2MB`) only succeeds when the system has a configured hugetlb pool with 2 MiB huge pages available. Standard GitHub Actions `ubuntu-latest` runners do NOT pre-configure any hugetlb pool (`/proc/sys/vm/nr_hugepages` defaults to 0 on these runners), so all huge-page reservation attempts fall back to ordinary 4 KiB pages in CI. Similarly, the Windows `MEM_LARGE_PAGES` success branch requires `SeLockMemoryPrivilege` and appropriate large-page configuration, which standard `windows-latest` runners do not provide. The happy paths for both platforms are therefore never exercised in CI, only the fallback paths.

    - **Status:** SUPERSEDED — see 59a (Linux) and 59b (Windows). This card's own "Current-number-or-verdict" below is STALE (task #1160/F5 found the "zero hits" claim false: a grep now finds 5 hits, all in `ci.yml`, in the `aligned-vmem-hugetlb-real` job) and is left unedited here as the historical record of what task #1060 actually found at the time; do not read it as current state.
    - **Current-number-or-verdict (re-verified off `.github/workflows/` at task #1060, 2026-08-17):** unchanged — a grep for `nr_hugepages`/`hugepages`/`SeLockMemoryPrivilege` across all four workflow files returns zero hits (no step anywhere configures a hugetlb pool or large-page privilege), and every runner is a standard `ubuntu-latest`/`windows-latest`/`macos-latest` image, so both success branches (`libc_mmap`'s `MAP_HUGETLB | MAP_HUGE_2MB` grant; `winapi_virtual_reserve`'s `MEM_LARGE_PAGES` grant) still never execute in CI — only the fallback paths do.
    - **Next trigger:** superseded — see 59a/59b.
    - **Evidence:** `crates/aligned-vmem/src/os/unix.rs` `libc_mmap` function (Linux `MAP_HUGETLB | MAP_HUGE_2MB` handling); `crates/aligned-vmem/src/os/windows.rs` `winapi_virtual_reserve` function (Windows `MEM_LARGE_PAGES` handling); `crates/aligned-vmem/tests/huge_pages.rs` (the test that would exercise the success path if hugetlb were available); `/proc/sys/vm/nr_hugepages` on standard runners (observed to be 0 as of task #1060; a later job configures it to 64, see 59a).

59a. **[T] Linux half of item 59 — CLOSED for dispatch, kernel syscall-level acceptance, AND memory-content: the oracle exists and asserts the property (task #1160/F5, closed further by task #1164, kernel-acceptance assertion strengthened by task #1166/F5, memory-content oracle added by task #1174/commit `2828e04`).**

    The Linux `MAP_HUGETLB` success branch now DOES execute in CI: the `aligned-vmem-hugetlb-real` job (`.github/workflows/ci.yml`, added by tasks #1151/#1152) configures a real `nr_hugepages=64` pool and hard-asserts, via a path-activation oracle (`ci_hugetlb_real_pool_oracle_refuses_ordinary_page_fallback`), that `reserve_aligned_huge` actually received a real `MAP_HUGETLB` grant rather than silently falling back to ordinary pages. That job also runs the huge-decommit tests (`decommit_capability.rs`, `reservation_decommit_contract.rs`) under that real grant, proving the eligible-range decommit dispatch REACHES the real `madvise(2)`/`MADV_DONTNEED` backend call. **Task #1164 closed the next layer:** `ci_hugetlb_real_pool_kernel_actually_accepts_eligible_madvise` (`decommit_capability.rs`) additionally hard-asserts `unix_madvise_successes()` actually increased — i.e. the kernel returned `0`, not `-1`, for that call, on the same real grant. **Task #1166 (F5) strengthened that assertion from `successes_after > successes_before` to `assert_eq!(successes_after, attempts_after)`** — with the counters reset to zero immediately before the decommit call, this is exactly item 59a's own originally-stated bar (`unix_madvise_successes() == unix_madvise_attempts() > 0`), not a weaker stand-in for it; the strict-`>` form would have stayed green under a hypothetical future two-call-per-decommit dispatch where one call succeeds and one fails (attempts +2, successes +1), silently masking a partial failure the equality form catches. **Task #1174 (commit `2828e04`) closed the remaining layer:** `ci_hugetlb_real_pool_decommit_actually_zeroes_memory_on_reaccess` (`decommit_capability.rs`) writes a non-zero `0xAB` pattern across a real 2 MiB huge-aligned span, calls `decommit`, then reads every byte back and fails on the first non-zero byte with its offset — the deterministic write→decommit→read-zero postcondition the three fields below used to ask for as future work. **Honest scope, so this card is not read as claiming more than it does:** the test is `#[cfg(any(target_os = "linux", target_os = "android"))]` behind the same real-`MAP_HUGETLB`-grant gate as its siblings, so it is exercised only inside the `aligned-vmem-hugetlb-real` CI job; at the time it was written its own commit message states plainly that the assertion "cannot be [executed] from this host" (Windows has no hugetlb pool) and was "first exercised on the CI runner" — not reproduced by a local run, only argued structurally (the `0xAB` pattern means a no-op decommit fails on byte 0). It is proven when that job is green on the landing SHA, not merely by the test existing. `libc_madvise` (`src/os/unix.rs`) still discards the return value on every NON-bench-internals build (task #719's original design, unchanged) — the read-back exists only under `bench-internals`, which is why this proof is scoped to the one CI job that enables it. The pool/RSS half (whether huge pages are actually RETURNED to the kernel pool after decommit) is deliberately NOT an assert in that same test — `HugePages_Free` is a kernel-global counter shared by every other huge-page target in the same job, so it is logged, not asserted, and stays open (see Next trigger).
    - **Status:** CLOSED for the questions this card (59a, the Linux half) tracks — dispatch, grant, kernel syscall-level acceptance (an exact equality, not a bare inequality), AND memory-content (a write→decommit→read-zero oracle, task #1174/commit `2828e04`) are all now asserted by tests that run inside the `aligned-vmem-hugetlb-real` CI job. **Caveat, not a reopening:** the memory-content test is `#[cfg(linux/android)]` behind the real-hugetlb-grant gate, so on the date of this update it has not yet executed on this host (Windows) or been observed green on a CI landing SHA — the oracle exists and will run, which is different from the property having been observed proven. The pool/RSS-reclaim half (whether pages are actually returned to the kernel pool) remains a printed, non-asserted observation, not a proven property — see Next trigger. Parent item 59 as a whole is still only PARTIALLY closed: this card (59a, Linux) is closed in the sense above; 59b (Windows, immediately below) is still fully open — see that card, unedited by this update.
    - **Current-number-or-verdict (re-derived task #1193, 2026-08-20, off the current tree — the task-#1189/#1166 figures this field previously carried are kept below as the historical record):** `aligned-vmem-hugetlb-real` job runs **9** unique `test <name> ... ok` sentinels under a real `MAP_HUGETLB` grant, plus **5** unique literal `[oracle] ARMED: ...` output markers — **14** `grep -F` sentinel checks in total (`awk '/^  aligned-vmem-hugetlb-real:/,/^  aligned-vmem-miri:/' .github/workflows/ci.yml | grep -oE 'grep -F "[^"]*"' | sort -u | wc -l` → 14, split confirmed by direct listing: 9 test-shaped, 5 marker-shaped). **Task #1189's own count was 8 + 4 = 12 and was accurate then**; the job grew by one test-sentinel plus one marker in task #1188 (the align > 2 MiB amplification oracle, part of item 87's whole-file 47-count) landing after task #1189's derivation. Recorded plainly because this field is scoped to ONE CI job and drifts independently of item 87's whole-file totals — a round adding a sentinel to this job must update BOTH this card and item 87 (the same one-number-in-two-places coupling task #1161 documented). The `unix_madvise_successes() == unix_madvise_attempts()` equality and the release attempt/success equality are two of the asserted bars on this path; the write→decommit→read-zero postcondition (task #1174) is a third. **CI mechanics fix (task #1189, unchanged by this re-derivation):** the job's shared `cargo test` invocation previously passed `-- --nocapture` to the WHOLE multi-test run to make the `[oracle] ARMED: ...` markers observable — this was corrupting the very `test <name> ... ok` sentinel lines it was meant to coexist with, because libtest's default parallel runner writes each such line as three separate writes from the aggregating thread, and an unsynchronized worker-thread `println!` (either marker) can land between them, splitting another test's sentinel line. Verified by a 400-run local counterfactual against a 9/10-test harness mirroring this job's shape (11/400 runs corrupted at least one sentinel line under the old default-parallel `--nocapture` shape; `--test-threads=1 --nocapture` makes a printing test's OWN line corruption deterministic instead of rare, confirmed 5/5, so it is not a fix). Fixed by running the `test <name> ... ok` sentinels WITHOUT `--nocapture` (libtest captures a passing test's stdout by default, so the `println!`s are swallowed and the "... ok" lines are written intact — verified 400/400 clean) and observing each `[oracle] ARMED: ...` marker from its own isolated `--exact <name> -- --nocapture` invocation (verified 400/400 clean each). See `.github/workflows/ci.yml`'s `aligned-vmem-hugetlb-real` step comment for the full counterfactual summary.
    - **Next trigger:** none remaining for dispatch, kernel-acceptance, or memory-content — all three now have an asserting oracle in CI. Two things are NOT closed by this card and would need their own trigger: (1) confirm the memory-content oracle (task #1174) has actually run green on a real CI landing SHA at least once — it is `#[cfg(linux/android)]` behind the hugetlb-grant gate and had not yet been observed executing at the time this card was last updated, only argued structurally to be correct by construction; (2) the pool/RSS-reclaim half (whether huge pages are actually returned to the kernel pool after decommit) is deliberately left as a printed, non-asserted observation in the same test (`HugePages_Free` is a kernel-global counter shared by every other huge-page target in the job, so no single test can assert it without racing them) — re-open only if a concrete, non-racy way to assert that counter is proposed.
    - **Evidence:** `.github/workflows/ci.yml` `aligned-vmem-hugetlb-real` job; `crates/aligned-vmem/tests/decommit_capability.rs` `huge_aligned_range_takes_the_real_backend_path_not_the_skip_path`, `ci_hugetlb_real_pool_oracle_refuses_ordinary_page_fallback`, `ci_hugetlb_real_pool_kernel_actually_accepts_eligible_madvise` (task #1164, its kernel-acceptance assert strengthened to `assert_eq!` by task #1166), and `ci_hugetlb_real_pool_decommit_actually_zeroes_memory_on_reaccess` (task #1174/commit `2828e04`, the write→decommit→read-zero memory-content oracle — see that test's own doc comment for the full write/decommit/read-back design and its honest not-yet-executed-locally caveat); `crates/aligned-vmem/src/os/unix.rs` `libc_madvise` (return-value discard on non-bench builds, task #719) and its `bench-internals` `UNIX_MADVISE_ATTEMPTS`/`UNIX_MADVISE_SUCCESSES` counters, `decommit_pages_impl` (:423-447, confirms exactly one `libc_madvise` call per eager decommit today, which is why the equality form is sound and not tautological-different from the old `>` form on the CURRENT dispatch); `crates/aligned-vmem/src/reservation.rs` lines 42-48 (the documented "huge + eager decommit + Linux/Android >= 5.18 + both endpoints 2 MiB-aligned" postcondition the new test asserts); `crates/aligned-vmem/tests/smoke.rs` `macos_decommit_madvise_syscall_actually_succeeds` (the precedent pattern this task's test mirrors, Darwin-only).
    - **[Filed 2026-08-20, from `docs/reviews/2026-08-20-073908-aligned-vmem-publication-audit-Сол-кодекс.md` §C2] Fourth independent audit restates the pool/RSS-reclaim boundary this card's own "Next trigger" (2) already names — placed here rather than as a new card, since it is the SAME gap, not a new one.** The audit's own framing: HugeTLB zero-fill-on-readback IS hard-covered (task #1174's write→decommit→read-zero oracle, described above); physical reclaim to the kernel's hugetlb pool is NOT (`HugePages_Free`/RSS stay external, kernel-global metrics, logged around the test but never asserted, for exactly the shared-counter/racing reason already recorded above). The audit's own position, agreed with here: this gap is honestly acceptable PROVIDED the crate's publish-facing text does not also claim zero-fill itself is unverified — it does not. Checked against the two publish-facing sites (both already state the same boundary, so no wording drift exists to fix, per the "one formulation, not a second one" rule tasks #1194/#1161 established): `crates/aligned-vmem/README.md`'s HugeTLB decommit bullet (the "zero-fill-on-next-access is empirically proven for the eligible-range case; physical reclaim of the pages back to the hugetlb pool is not" sentence, in the Linux/Android sub-bullet under "Huge pages: decommit's behavior is platform- AND range-dependent") and `crates/aligned-vmem/src/reservation.rs`'s `try_decommit` rustdoc, `DecommitOutcome::Advised` bullet ("zero-fill on readback is proven for the real-HugeTLB/eligible-range case, on a Linux runner ... physical page return to the pool remains unmeasured. Do not conflate the two when reading `Advised`") — both already draw exactly the audit's line, in matching wording, so this bullet is a confirmation the gap is already honestly documented, not a new doc fix.

59b. **[T] Windows half of item 59 — still fully OPEN (task #1160/F5).**

    The Windows `MEM_LARGE_PAGES` success branch (`winapi_virtual_reserve` in `crates/aligned-vmem/src/os/windows.rs`) requires `SeLockMemoryPrivilege` to be granted and enabled on the process, plus a large-page-eligible allocation shape. No CI job configures this privilege — unchanged from the original item 59 filing.
    - **Status:** OPEN — unchanged from the original filing; nothing has closed this half.
    - **Current-number-or-verdict (task #1160/F5, 2026-08-19):** measured directly — `grep -n "SeLockMemoryPrivilege\|MEM_LARGE_PAGES" .github/workflows/*.yml` returns ZERO hits (no workflow file configures the privilege or requests large pages); every Windows job in `.github/workflows/ci.yml` is a standard `windows-latest` image. The `MEM_LARGE_PAGES` success branch still never executes in CI; only the fallback (ordinary-page) path does.
    - **[Filed 2026-08-20, from `docs/reviews/2026-08-20-073908-aligned-vmem-publication-audit-Сол-кодекс.md` §C3] Fourth independent audit restates this same gap — same finding, not a new one, folded into this card rather than filed separately.** Precise boundary the audit draws, confirmed against the current tree: what IS covered by CI today is (a) the unprivileged fallback path (every Windows job runs unprivileged, so the ordinary-page retry after a denied `MEM_LARGE_PAGES` request executes routinely) and (b) simulated/mock branch metadata (`is_huge()` and friends can be exercised against synthetic state without a real grant). What is NOT covered by any CI host: a runner that actually holds `SeLockMemoryPrivilege` and hard-asserts `is_huge() == true` off a REAL large-page grant, the decommit refusal behavior specific to a real large-page mapping (`VirtualFree`/`MEM_DECOMMIT` failing on a genuine large-page region, not a simulated one), and the release/free path for a real large-page mapping. Until such a privileged runner exists, the granted-path behavior stays reasoned/profiled (see the P3(b) cross-reference below), not execution-verified — this is the same distinction task #1189's own P3(b) update already draws for the unprivileged-cascade measurement it DID perform, which is adjacent to but not a substitute for this card's still-open gap.
    - **Next trigger:** when a CI runner (self-hosted or a future GitHub-hosted image) with `SeLockMemoryPrivilege` granted becomes available, add a dedicated Windows large-page CI step, mirroring `aligned-vmem-hugetlb-real`'s Linux pattern (configure the privilege, hard-fail if it cannot be obtained rather than silently skip, then run a path-activation oracle proving a real `MEM_LARGE_PAGES` grant was obtained before trusting downstream test results — including a hard-assert on `is_huge() == true`, a decommit-refusal assertion, and a release-of-a-real-large-page-mapping assertion, the three specific gaps the 2026-08-20 audit names).
    - **Evidence:** `crates/aligned-vmem/src/os/windows.rs` `winapi_virtual_reserve` function (the `MEM_LARGE_PAGES` handling and its `SeLockMemoryPrivilege` requirement, documented around line 447); `.github/workflows/ci.yml` (no `SeLockMemoryPrivilege`-granting step in any job); `docs/reviews/2026-08-20-073908-aligned-vmem-publication-audit-Сол-кодекс.md` §C3 (the restatement); `docs/perf/OPEN_ITEMS.md` item 57's P3(b) update, task #1189 (`examples/v1189_windows_large_page_native_profile.rs` — the UNPRIVILEGED profile that exists today; explicitly does not attempt to acquire the privilege).

60. **[T] CI-coverage gap: BSD, Android, tvOS and watchOS branches are reasoned-from-spec, not empirically executed on real hardware.** (Filed 2026-08-16, TaskList #1023, from aligned-vmem prerelease-audit-r4 "Coverage gaps" section.)

    The BSD platforms (FreeBSD, NetBSD, OpenBSD, DragonFly) are only compile-verified; no BSD CI runner exists (`.github/workflows/ci.yml` has no BSD job). The `decommit_lazy` and `_SC_PAGESIZE` handling for these platforms are based on reading headers and POSIX spec, not empirical runs. Similarly, Android, tvOS and watchOS platforms are not represented in CI; only macOS covers part of the Darwin family (iOS, tvOS, watchOS are missing). This is an honest limit of the current CI infrastructure, not a discovered bug.

    - **Status:** OPEN — not urgent, because the unconditional alignment check in `unix_reserve` prevents violation of reserve alignment even if `_SC_PAGESIZE` values were wrong, and decommit correctness is verified on Linux/macOS which are the primary deployment targets. However, the BSD/Darwin/Android branches remain empirically unverified.
    - **Current-number-or-verdict (re-verified off `.github/workflows/ci.yml` at task #1060, 2026-08-17):** unchanged — all 39 jobs in `ci.yml` run on standard `ubuntu-latest` (34), `windows-latest` (2), or `macos-latest` (3) images; the only target matrix (`multi-arch`) covers just `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`; no BSD, Android, iOS, tvOS, or watchOS job or target exists anywhere in the workflow. These branches remain reasoned-from-spec.
    - **Next trigger:** when a FreeBSD/NetBSD/OpenBSD/DragonFly CI runner becomes available (self-hosted or via a service like Cirrus CI), add a `test-bsd` job. Similarly, if iOS/tvOS/watchOS or Android CI runners become available, add corresponding jobs. Until then, these platforms remain spec-verified only.
    - **Evidence:** item 43 (BSD `_SC_PAGESIZE` values, partially open); item 48 (Darwin `MADV_DONTNEED` behavior, partially open); `.github/workflows/ci.yml` (no BSD/iOS/tvOS/watchOS/Android jobs); `crates/aligned-vmem/src/os/unix.rs` platform-specific constants (FreeBSD/NetBSD/OpenBSD/DragonFly `_SC_PAGESIZE` values, Android-specific handling, Darwin-family `decommit_lazy` behavior — i.e. the per-OS `_SC_PAGESIZE` cfg-match plus `madv_free_advice`'s Darwin arms, at this path since the task #1055 split; the `decommit_lazy` API wrapper itself is `crates/aligned-vmem/src/api/decommit_lazy.rs`).
