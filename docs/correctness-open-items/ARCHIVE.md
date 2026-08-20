# Correctness open items — archived closure narratives

**Relocated 2026-08-20 (task #1217) from `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md`
to `docs/correctness-open-items/ARCHIVE.md`, as part of splitting the whole
correctness index into a folder** (reversing item 86's 2026-08-19 deferral
of that split — see `docs/CORRECTNESS_OPEN_ITEMS.md` item 86 for the
reversal record). This move is itself a byte-identical relocation, same
contract as the move this file's own header describes below: no narrative
text was edited, only this header's self-references were updated to the new
paths (`docs/CORRECTNESS_OPEN_ITEMS.md` stays the correct citation for the
top-level index — it now itself only summarizes and points into
`docs/correctness-open-items/ACTIVE.md` / `TRACKED.md` / `RESOLVED.md`; this
archive's own former path is dead and must not be cited going forward).

**Purpose.** This file holds the full dated historical closure narratives
for items tracked in `docs/CORRECTNESS_OPEN_ITEMS.md` — the round-by-round
"here's the failure, here's the root cause, here's the fix and its
verification" prose that used to sit inline in that index's
§"Recently resolved (closure trail — do not re-list as open)". It was split
out (task #1109, 2026-08-18) because `CORRECTNESS_OPEN_ITEMS.md` had grown
past the ~1,000-line threshold at which CLAUDE.md's R34-24 rule requires the
same archive split the perf index already performed (R29-6, task #437, which
created `docs/perf/OPEN_ITEMS_ARCHIVE.md`) — CLAUDE.md reads, verbatim: "If
`docs/CORRECTNESS_OPEN_ITEMS.md` grows past ~1,000 lines, the same split
applies (create `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md`)." The main index
must read as a current-state checklist at the start of every round; the
interleaved closure narratives made that read progressively more expensive
without adding CURRENT-STATE information. **No text was deleted or reworded
in this split** — every paragraph below is byte-identical to what previously
lived inline in the main index's "Recently resolved" section, only
relocated. Item numbers are unchanged and still cited as "item N" by the
rest of the repository; nothing was renumbered.

**How to use this file.** Each entry below keeps its original item number
and opens with its original header line. Going the other direction, the
top-level index's "Recently resolved" pointer trail — now
`docs/correctness-open-items/RESOLVED.md` — keeps a one-line pointer per
moved entry (the original header line plus a relocation note), so a
citation of the form "see Recently resolved §N" or "item N" still lands:
follow the pointer from `RESOLVED.md` to the full narrative here.

**Convention.** This file is purely an archival extension of
`docs/CORRECTNESS_OPEN_ITEMS.md` — it does NOT have its own independent
"round start" reading requirement (CLAUDE.md's "Phased delivery" mandates
reading the main index end-to-end; this file is consulted on demand, when a
specific item's full closure history is needed). New closures follow the
same convention going forward: the closing round writes the full narrative
here and leaves only the one-line pointer (plus the closing round + task
number + one-line evidence, per the main index's own convention rule 2) in
`docs/correctness-open-items/RESOLVED.md`.

---

## Structure — how this file differs from its perf sibling (task #1118)

`docs/perf/OPEN_ITEMS_ARCHIVE.md`, the file this archive's shape was copied
from, additionally carries a `## Table of contents` with anchor links and a
`###` heading per item, so a pointer in the main index resolves by clicking.
This file does NOT: its entries are plain `NN. **…**` list items, so following
a pointer means "scroll and find item N". That gap is recorded rather than
closed, for one concrete reason: **item numbers here collide.** The closure
trail inherited two entries numbered `3` — the run after the pre-split block
(`77 51 54 41+61 69 68 46 66 50-U10`) is `1 3 2 3 4 5 30 31 32 33 34 15 35
…`, with the second `3` further down the file than the first and `15`
sitting between `34` and `35`, not before `30` — a defect that predates the
R34-24 split (task #1109) and was carried
across verbatim by design, since the split's whole contract was byte-identical
relocation. Adding anchors on top of colliding numbers would manufacture two
identical anchor targets and make "follow item N" LESS reliable, not more.

Sequence, if a future round wants to close this: renumber the collisions
first (a real edit to historical entries, so it needs its own decision), THEN
add the TOC and per-item headings. Doing it in the other order bakes the
ambiguity into link targets.

## Recently resolved — full closure trail

*(Entries below are in the order they appeared in the main index's
"Recently resolved" section at split time. Each is byte-identical to its
pre-split text.)*

77. **[M1, record correction — closed on filing] Commit bodies `d58bd67` (task #1086) and `a988e51` (task #1085) both claim a below-real-page skip treatment that only TWO of the THREE forced-page test files actually received: the record, not the code, was wrong.** (Filed and corrected 2026-08-18, task #1096/finding M1.)

   - **The false claims, verbatim.** `d58bd67`'s body: "Both forced-page suites now SKIP any forced page below the host's real page ... instead of asserting it must be accepted, with an `executed >= 1` guard so a host where every arm is unusable fails loudly rather than passing vacuously." `a988e51`'s body: (the same two files) "handled where those files are owned (tasks #1086 and #1087) by skipping below-real arms with an `executed >= 1` guard so a fully-skipped run fails loudly instead of passing vacuously." Both sentences cover the two then-existing forced-page suites — `tests/lazy_initial_commit_forced_page.rs` and `tests/decomp_hooks_forced_page.rs` — and only ONE of them got that treatment.
   - **What each file actually has (verified by grep and read, 2026-08-18):** `tests/decomp_hooks_forced_page.rs` (:247-346) and `tests/segment_state_reconciliation_oracle.rs` (:369-412, added by task #1087) both carry the skip-below-real-page arm plus the `executed >= 1` anti-vacuity guard exactly as described. `tests/lazy_initial_commit_forced_page.rs` has NEITHER — zero occurrences of `executed` or `real_page` in the file; its `assert!(set_page_size_override(Some(64 * 1024)))` (:218) keeps asserting acceptance, with only its justification comment and panic message reworded by `d58bd67` (16 changed lines, comment/message-only).
   - **The asymmetry is deliberate and correct — "fixing" it would mean breaking the file.** That file forces exactly ONE page size (64 KiB); adopting the skip treatment there would let the single arm skip on a hypothetical >=128 KiB-page host, where the `executed >= 1` guard would then turn "oracle unavailable on this host" into a hard red — strictly worse than the current behavior. The file's own comment (:209-217) states this in place: "there is no second arm to fall back to, so a host that cannot run the forced-64 KiB oracle must fail loudly rather than pass vacuously" — the same loud-failure GOAL, reached by the opposite mechanism. The defect was the RECORD: two commit bodies assert a uniformity that does not exist, and history is not rewritten in this project — this card is the durable correction a reader of either commit body (or of `git log`'s uniform "task #1085 interlock" summary) needs.
   - **Verification:** `grep -c "executed\|real_page"` → `lazy_initial_commit_forced_page.rs`: 0, `decomp_hooks_forced_page.rs`: 12, `segment_state_reconciliation_oracle.rs`: 12; the two sentences quoted from `git show d58bd67` / `git show a988e51` bodies verbatim; `git show d58bd67 -- tests/lazy_initial_commit_forced_page.rs` (the file's 16-line delta, comment/message-only).

51. **`aligned-vmem`'s `#[cfg(unix)]` code was never compiled by the standard LOCAL verification matrix on the campaign's Windows host** (filed round 8, task #904, finding UC5 of `docs/reviews/2026-08-13-aligned-vmem-round8-closing-review.md`) — **CLOSED** by task #1059, option (a): a permanent cross-target gate in `npm run check`. **Reopened and re-closed by task #1071 — see the CORRECTION block below.**

   - **The decision.** Three options were on the table: (a) permanently add cross-target rows to `scripts/check-all.mjs`'s aligned-vmem package-gates group; (b) keep the round-8 stopgap (each future round remembering to run the one-off `--target` command per this card's old "Next trigger"); (c) do nothing, since CI covers it. (a) was chosen: (b) had already shown it rots — it depends on every future round re-reading this card, exactly the failure mode the open-items index exists to prevent, and between rounds the gate is simply absent; (c) leaves the repo's own mandatory pre-push gate (CLAUDE.md: "run `npm run check` before pushing, every time") blind to an entire cfg half of the crate. Concretely, the group gained two steps after its five host clippy rows: `cargo check -p aligned-vmem --target x86_64-unknown-linux-gnu --features "lazy-commit huge-pages fault-injection bench-internals" --all-targets` and the clippy equivalent with `-- -D warnings` — the exact commands this card's old "Current-number-or-verdict" had verified, feature-matching the group's existing `lazy-commit huge-pages fault-injection bench-internals` row (and CI's native-linux row for the same combination). `--all-targets` kept load-bearing per the old card's own note (without it, `tests/` is not built and Unix-only tests go unchecked).
   - **Why the LOCAL gate was the right place — CI was never the gap.** `ci.yml`'s `aligned-vmem-gates` job runs on `ubuntu-latest`, so every `#[cfg(unix)]`/`#[cfg(all(unix, not(miri)))]` path already compiles and clippies natively in CI on every PR, no `--target` flag needed. The gap was that `npm run check` runs on the Windows dev host, where a plain `cargo check`/`cargo clippy` compiles `#[cfg(unix)]` code to nothing — a fully green local gate said nothing about it. This was not hypothetical and not ancient history: it was independently rediscovered on 2026-08-17 by task #1055 (commit `a4b8e50`'s three-axis verification pass; recorded in the session checkpoint `docs/checkpoints/2026-08-17-1439.md`), which found genuinely missing imports in `crates/aligned-vmem/src/os/unix.rs` and `crates/aligned-vmem/src/os/miri.rs` that every host-target row had silently skipped — caught only by explicit `--target x86_64-unknown-linux-gnu` and `RUSTFLAGS="--cfg miri"` cross-checks.
   - **Scope of the closure.** The new rows cover the `cfg(unix)` half on the local gate. The `cfg(miri)` half stays with CI's `RUSTFLAGS="--cfg miri --cfg aligned_vmem_mock"` row (excluded locally along with mock under the same keep-it-fast rule), and runtime EXECUTION of unix code remains CI's job — the local rows are compile/lint-only, the same contract CI's native clippy rows already have. The i686 cross-targets remain local exclusions for speed.
   - **Measured wall-clock cost.** Isolated, first-touch: check 6 s / clippy 3 s wall (cargo-reported 5.08 s / 2.65 s). Warm inside the full suite (target + feature set already built): 0.72 s + 0.64 s (cargo-reported, from the gate log). Full-gate totals before/after the edit: 818 s → 820 s (+2 s, within run-to-run noise for a ~13.7 min gate). No new toolchain component: `x86_64-unknown-linux-gnu` was already installed via rustup on this host.
   - **Verification.** `npm run check` run twice — before the edit (baseline) and after — both ending in the literal log line "[check-all] ALL GREEN — safe to push"; the after-run's banner reads "running 32 step(s) ... aligned-vmem x11 ..." and both new steps have their own "[check-all] OK:" lines in the log (`target/check-logs/baseline.log` / `target/check-logs/after.log`).

   - **CORRECTION (task #1071, 2026-08-17): the closure above did not survive its first real test, and the "so the `#[cfg(unix)]` surface is compiled and linted on the Windows dev host every run" claim in it was too strong.** The push that followed task #1070 landed CI red on five jobs although `npm run check` had printed ALL GREEN immediately before it; the two compile breakages were fixed in commit `7326b39`, but the GATE that let them through had three holes, each confirmed independently: (1) **feature coverage** — the two cross-target rows #1059 added ran ONE hardcoded set, `lazy-commit huge-pages fault-injection bench-internals`, with `huge-pages` ALWAYS on, so the exact defect class of #1070's Breakage A (unix-side imports whose only use sites require `bench-internals` AND (`huge-pages` OR 32-bit) — unused, i.e. a `-D warnings` error, exactly when `bench-internals` is on WITHOUT `huge-pages` on 64-bit unix) was unreachable by construction: the gate added to catch unix drift could not catch the unix drift that actually shipped. (2) **mock exclusion** — the `--cfg aligned_vmem_mock` arm had been excluded "to keep the local check fast" with no measurement behind the exclusion, which is why Breakage B (the whole mock arm failing to compile — E0433 plus six unused imports) was invisible locally by design. (3) **cache-trust** — every cargo step's OK was a cache verdict, not proof of work: on a tree whose bytes differ but whose mtimes do not advance, cargo replays a fresh-looking cache and the step prints only `Finished` (the failing push's gate log shows exactly this for the cross-target clippy row: `Finished ... in 0.74s`, then `[check-all] OK`), and `touch` — the obvious cheap invalidation — was already known (task #1070) to be INSUFFICIENT for the cargo-test profile. **Task #1071's re-closure, each part verified by an injected-defect counterfactual (defect in → gate red with the quoted line → defect reverted → gate green):** a cross-target `bench-internals`-only clippy row (the minimal feature-lattice addition — see the task #1071 commit for the pairwise-interaction argument); a mock clippy row mirroring CI's (~2.5 s measured — the "for speed" exclusion retired with a number in hand; the mock TEST row stays excluded because it fails on a clean tree at HEAD: `decommit`'s `debug_assert` vs `tests/mock.rs`'s silent-skip expectation, a pre-existing crate issue recorded in the task #1071 commit); and two `cargo clean -p aligned-vmem` invalidation steps ahead of the group — the host form PLUS a `--target x86_64-unknown-linux-gnu` form, because the host form was measured to leave the cross-target dir untouched — together with an `expectWork` guard on every aligned-vmem cargo row that fails the gate unless the row's own output contains a real `Checking|Compiling|Documenting aligned-vmem` line, so a green vmem row provably executed against the bytes on disk. **Honest boundary after #1071:** the i686/i686-musl pointer-width surfaces, the `--cfg miri` compile check, and mock-backend TEST execution remain CI-only; the six workspace-level clippy rows and four root-crate test rows still trust cargo's incremental cache (forcing root rebuilds every run was judged disproportionate, on a measured scale factor: this host's cold-vs-warm full-gate delta is ~276 s — that is what per-run root invalidation would re-add — against a measured ~2.7 s ripple for the two root-facing matrix rows that DO run after the vmem cleans; the incident class was vmem-local).
   **Update (task #1072, 2026-08-18):** the mock-test-row blocker named above ("it fails on a clean tree at HEAD: `decommit`'s `debug_assert` vs `tests/mock.rs`'s silent-skip expectation") is resolved — the contradiction was adjudicated in favour of the task-#1051 tripwire: debug builds panic, release builds keep the documented silent skip, the silent-skip tests are profile-gated, the eager `decommit`'s rustdoc and the crate README no longer claim an unconditional silent no-op, and `smoke.rs`'s unix-only mirror got the same split (it was the same contradiction at the real-syscall layer, invisible on the Windows dev host). A clean tree at HEAD now passes the mock test row; adding that row to `scripts/check-all.mjs` remains the planned follow-up (deliberately out of scope for #1072).

   **Update (task #1082, 2026-08-18): that planned follow-up is DONE — the mock test row now runs in `scripts/check-all.mjs`, and item 51 has no outstanding action left.** The row mirrors ci.yml's `aligned-vmem-gates` row exactly (`cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals"` under `RUSTFLAGS=--cfg aligned_vmem_mock`, with `expectWork: 'aligned-vmem'`), placed after the `--all-features` test row to mirror CI's order. Measured cost: ~3.8 s after `cargo clean -p aligned-vmem` invalidation / ~1.5 s warm (10 tests across 5 suites, all green on a clean tree) — the same order as the mock clippy row task #1071 re-admitted (~2.5 s), so the "for speed" exclusion is retired with a number in hand for the TEST row too; the full-gate delta is the orchestrator's to measure against its own run (task #1071's +47 s/+5.4% cross-clippy addition is the reference shape, not a number to reproduce). Counterfactually proven non-vacuous: a one-line mock-recording regression (`mock::Call::Reserve { size: size + 1 }` at the recording site in `api/reserve.rs`'s `try_reserve_aligned`) turned the row red — `test result: FAILED. 12 passed; 2 failed`, panics at `tests/mock.rs:26:5` and `:57:5` (`assertion failed: matches!(calls[0], Call::Reserve { size, align, .. } if size == 2 * MIB && align == 2 * MIB)`) — then revert → green, `git diff` on `crates/` empty. This is the ONLY local row that EXECUTES the mock backend: the mock clippy row compiles the arm without running it, and every real-backend row compiles a different cfg arm entirely, so before this row the class it catches (mock-arm runtime regressions) was locally invisible by construction — which is why the follow-up was closed on the merits rather than deferred as a new active item: the blocker was already gone (#1072), the cost is measured and small, and the row is proven to catch its class.

54. **[T, INFO] Tautological tests and small untested corners** (Filed 2026-08-14, task #934/C-9, combining findings V-29 and V-31 of `docs/reviews/2026-08-14-aligned-vmem-pre-release-review.md`) — **CLOSED** by task #1058.

   - **V-29, genuinely fixed by deletion:** removed `min_page_equals_page` from `crates/aligned-vmem/tests/min_page.rs`. It asserted `MIN_PAGE == PAGE` where the definition is literally `pub const MIN_PAGE: usize = PAGE;` (`crates/aligned-vmem/src/min_page.rs:8`) — an equality the compiler guarantees, so the test could never fail. Deleted rather than rewritten because every candidate replacement assertion is likewise already compiler-enforced (the alias itself is type-checked; the concrete value is pinned by the untouched sibling `min_page_is_4kib`; const-context usability is guaranteed by `pub const`) — a rewrite would be busywork, exactly the failure mode the task brief warned against.
   - **V-31, two genuine gaps, filled:**
     - `ReservationParts`'s derived `PartialEq`/`Eq` had no struct-level test anywhere — the existing `reservation_parts_prevents_parameter_swap` and `reservation_parts_new_roundtrips_through_release` (`crates/aligned-vmem/tests/smoke.rs`) compare individual fields, never the whole struct. New `crates/aligned-vmem/tests/reservation_parts.rs`: two parts built from identical `(ptr, len, align)` compare equal; parts differing in exactly one field (each of ptr/len/align in turn) compare unequal; plus a `require_eq::<ReservationParts>()` marker check pinning the derive's `Eq` half, which `assert_eq!` alone does not exercise.
     - `leak_zeroed_pages` was tested only with a rounding size (`3 * PAGE + 7`). New `leak_zeroed_pages_exact_multiple_needs_no_rounding` in `crates/aligned-vmem/tests/smoke.rs`, next to the existing `leak_zeroed_pages_is_zeroed_and_static`: an exact `4 * PAGE` size (internal round-up is a no-op), asserting PAGE alignment, all-zero over the full size, and writability.
   - **V-31, three stale-card non-gaps, no code needed:** (1) `release`'s null-pointer early return is already covered by `release_null_is_noop_and_not_recorded` (`crates/aligned-vmem/tests/mock.rs`, task #949/T-4); (2) the deprecated `Reservation::is_empty` no longer exists — deleted by task #947/A-3 — so "untested" is moot (the surviving `LazyReservation::is_empty` in `crates/aligned-vmem/src/lazy_reservation.rs` is a different, newer, non-deprecated method, out of this item's scope); (3) the `try_reserve_aligned` `size + align` overflow case is already covered by `try_reserve_overflow_is_invalid_argument_on_all_platforms` (`crates/aligned-vmem/tests/smoke.rs`, task #922/V-11). The card's "untested corners" list predated those tests and was simply stale.
   - **Verification:** `cargo test -p aligned-vmem --all-features` green (both new tests pass, `min_page_equals_page` removed, nothing regressed); `npm run check` ALL GREEN.

41+61. **`aligned-vmem` had NO `cargo miri test -p aligned-vmem` step anywhere in CI** (item 41: the missing step; item 61: the same gap phrased as runtime-semantics concern — one fix, closed together) — **CLOSED** by task #1057.

   - **What was added:** a new dedicated job `aligned-vmem-miri` in `.github/workflows/ci.yml` (placed directly after `aligned-vmem-gates`), modeled on `numa-shim-macos-miri`'s per-PR small-crate pattern with three deliberate differences: `ubuntu-latest` runner (the miri fallback backend `src/os/miri.rs` is platform-independent std::alloc — no macOS-specific cfg-collision reason to pay for a macOS runner); `MIRIFLAGS: -Zmiri-ignore-leaks` (routes around the ONE intentional leak, `tests/smoke.rs`'s `leak_zeroed_pages_is_zeroed_and_static`, where the function under test `core::mem::forget`-leaks a process-lifetime sidecar by design — NOT `-Zmiri-disable-isolation`, which solves a different problem); per-PR cadence, not weekly (suite is small; ~45 s default / ~2 min all-features locally). Steps: `cargo miri test -p aligned-vmem` (default) + `cargo miri test -p aligned-vmem --all-features` — no narrower single-feature runs, since every feature's cfg-gated code is compiled and exercised under `--all-features`.
   - **What local verification found BEFORE committing (the important part):** the known leak was the ONLY blocker item 41's history recorded, but running the suite under miri on the committer's host (and, as the CI predictor, under `--target x86_64-unknown-linux-gnu`) found NINE more failing tests — all one mechanism: they assert real-backend observables (nonzero over-reserve head offset; bench-internals counter deltas) that `src/os/miri.rs`'s minimal std::alloc backend structurally cannot produce (it returns `(base, base, size)` — never over-reserves — and never increments any counter; the counter increment sites live only in the real unix/windows backends, whose storage re-exports in `src/bench_internals/mod.rs` are already `not(miri)`-gated). Five fail on linux/miri (smoke.rs `decommit_recommit_roundtrip_on_over_reserved_span`; bench_internals_counters.rs `bench_internals_counters_existence_and_reset`; huge_pages.rs `reserve_aligned_huge_exact_size_for_2mib_align` + both `reserve_aligned_huge_rejects_non_huge_page_aligned_{size,align}` — the hugetlb-alignment guards live in the real unix backend only); six on windows/miri (same smoke test; same counters test; huge_pages.rs both `..._still_two_call_path` tests; lazy_commit.rs `windows_lazy_reserve_saves_commit_charge` and `safe_decommit_over_never_committed_tail_succeeds`' counter block). Zero UB reports in any run. All nine post-date the crate's last miri-green run (2026-08-09, task #776) and had never been executed under miri — exactly the coverage gap this item existed to surface. With owner authorization, each gained the repo's established `not(miri)` exclusion (mirroring in-file precedent like smoke.rs's Darwin-family gates), so NO real-OS coverage was lost — Windows/Linux/macOS runs still execute every one of them.
   - **Verified after the exclusions, before committing:** all four local miri runs green — `MIRIFLAGS=-Zmiri-ignore-leaks cargo +nightly miri test -p aligned-vmem` (default and `--all-features`, on windows-msvc) plus the same two under `--target x86_64-unknown-linux-gnu` (what the ubuntu runner will execute). The intentional-leak test passes under the flag; no test was skipped or weakened beyond the miri-only cfg exclusions.
   - **Consequence:** items 41 and 61 both close; the `aligned-vmem-gates` compile-only `--cfg miri` check stays (it catches cfg/type drift cheaply); future miri-blocking test regressions in this crate now go red per-PR instead of unnoticed.

69. **Flaky test: `safe_decommit_over_never_committed_tail_succeeds` intermittently read `WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS` as 0 instead of 1 under full-suite parallel load** — **CLOSED** by task #1063.

   - **Discovery:** task #1056 (2026-08-17), during zero-trust verification of an unrelated doc-only fix (item 49 formatting) — the first `npm run check` run FAILED on this test with zero other changes in flight besides a markdown edit; the test binary was not even rebuilt, ruling out the edit as a cause.
   - **Root cause:** every counter-touching test in `crates/aligned-vmem/tests/lazy_commit.rs` wraps its body in `let _guard = serial_guard();` EXCEPT `windows_virtualfree_release_failures_accessor_exists`, which called `aligned_vmem::reset_bench_internals_counters()` unguarded. Under parallel test execution, that unguarded reset could land inside `safe_decommit_over_never_committed_tail_succeeds`'s own `serial_guard()`-protected read→decommit→read window — `serial_guard()` only serializes callers who ALSO take it, so an unguarded resetter is invisible to it — zeroing `WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS` mid-window and making the delta read 0 instead of 1. Confirmed load/timing-dependent: the failure disappeared on an isolated re-run and passed 15/15 in a tight rerun loop.
   - **Fix:** added `let _guard = serial_guard();` as the first line of `windows_virtualfree_release_failures_accessor_exists`'s body (`crates/aligned-vmem/tests/lazy_commit.rs`), matching every other counter-touching test in the file. One-line fix.
   - **Evidence:** `npm run check` run during task #1056 (`crush` session `t54-a-item49`), failing step `test (aligned-vmem --all-features)`, assertion `VirtualFree(MEM_DECOMMIT) attempts left: 0, right: 1` at `crates/aligned-vmem/tests/lazy_commit.rs:687`.

68. **Name asymmetry in `Reservation` decommit capability API** (`Reservation::decommit_reclaims_and_zeroes()`, associated `const fn`, compile-time capability query, vs. `Reservation::can_decommit_reclaim_and_zero()`, instance method combining compile-time capability with runtime `is_huge()`) — **CLOSED** by task #1052, no code change (option (c): asymmetry accepted as-is).

   - **Decision:** owner accepted the recommendation of an independent review (`@fh`, task #1052): the two names are each grammatically correct in their own construction, not accidentally inconsistent. `decommit_reclaims_and_zeroes()` is a third-person factual predicate ("decommit *reclaims and zeroes* [memory]"); `can_decommit_reclaim_and_zero()` follows Rust's standard `can_*` + bare-infinitive modal-capability naming pattern ("can [it] *reclaim and zero*"). Both proposed renamings (dropping both `s`es from the associated fn, or adding both to the instance method) would force one of the two names into a grammatically incorrect form purely to gain visual symmetry between two unrelated grammatical constructions.
   - **Discoverability already addressed structurally, without a rename:** the two methods are declared adjacently in `reservation.rs`, cross-linked via intra-doc references in both directions, and the instance method's body literally reads `Self::decommit_reclaims_and_zeroes() && !self.is_huge()` — visibly showing the relationship in the one place a reader would look.
   - **Rejected alternatives:** (a) rename the associated fn to `decommit_reclaim_and_zero` (drop both `s`es) — rejected, produces a grammatically incoherent imperative-looking name. (b) rename the instance method to `can_decommit_reclaims_and_zeroes` (add both `s`es) — rejected, ungrammatical after `can_`. A `#[deprecated]`-alias third option (keep both names) had already been rejected earlier in this card's history as permanent clarity debt.
   - **Bonus argument surfaced during closure:** the minor spelling difference is informative, not noise — it signals the two are genuinely different questions (one compile-time-only, one compile-time + `is_huge()` at runtime), where a forced-symmetric `X()`/`can_X()` pair would have wrongly implied the second is a trivial wrapper of the first.
   Full history: see this file's git history for the pre-closure card (task #934 filed it; tasks #1041/#1045/#1052 revised/corrected it before final closure).

46. **`numa-shim`'s public `reserve_on_node` signature returns `aligned_vmem::Reservation`, coupling the crate's own semver to `aligned-vmem 0.2`** — **CLOSED** by task #1053 (option (a): coupling accepted and documented; `pub use aligned_vmem::Reservation;` added to `numa-shim`, gated `#[cfg(feature = "vmem-integration")]`).

   - **Decision:** owner accepted the recommendation of an independent review (`@fh`, task #1053): `numa-shim` and `aligned-vmem` are sibling crates in the same workspace with a shared release process, so a coordinated semver-major bump across both is a cost the release process already absorbs, not an added one. The rejected alternative (a `numa-shim`-owned newtype around `Reservation`) either needs full API-forwarding boilerplate that drifts on every `aligned-vmem` change, or an `into_inner()` escape hatch that re-exposes the same coupled type anyway — reproducing the exact problem it was meant to remove.
   - **What changed:** `crates/numa-shim/src/lib.rs`'s `reserve_on_node` doc comment gained a "Semver coupling with `aligned-vmem`" section stating the coupling is intentional and why; a new `#[cfg(feature = "vmem-integration")] pub use aligned_vmem::Reservation;` re-export makes the coupling visible in `numa-shim`'s own public API surface (callers can now name the type as `numa_shim::Reservation` without a direct `aligned-vmem` dependency of their own) instead of leaving it implicit. No behavioral/functional code changed.
   - **Consequence:** unblocks task #657 (numa-shim's first crates.io publish gate), which was waiting on this decision.
   Full history: see this file's git history for the pre-closure card (filed task #778/F4, round-closing review, audit §A3).

66. **`Reservation` carried no committed-length state (R6-1 / R7-2, the second of R7's two conditional-NO-GO conditions).** — **CLOSED** by task #1051, commit `0c1e6c4`; the surface it shipped initially leaked the watermark back out through `as_reservation()` and was re-sealed by task #1104 (publication-audit finding H1 — see the correction bullet below).

   - **What this card asked for, and why none of its five options were taken.** The card listed: (a) a `committed_len` field on `Reservation`; (b) a separate `LazyReservation` type; (c) returning `(Reservation, usize)`; (d) explicitly ACCEPTING the caller-tracked contract; (e) excluding `lazy-commit` from the supported release profile. I first recommended (d) plus a capability query, on the ground that a field cannot be kept truthful. The owner rejected (d): we are publishing a library, and shipping sharp primitives with the invariants left to prose is the C++ path — consumers should get tools, not homework. That objection was correct, and my argument had been aimed at the wrong target: it was an argument against BOLTING A FIELD ONTO `Reservation`, not against moving the bookkeeping into the crate at all.
   - **Why the field genuinely could not work** (the part of the analysis that survives, and the reason (a) stays rejected): `Reservation`'s mutating methods take `&self`, not `&mut self`, so they cannot update a plain field; the free primitives take a bare `*mut u8` and never see the handle at all; the crate's own only consumer uses exactly those free primitives (`src/alloc_core/os.rs`); and `into_full_parts`/`from_raw_parts` would need a seventh field supplied by the caller, i.e. the field would be only as true as the caller — today's contract with more surface.
   - **What was built instead — option (b), sharpened.** `LazyReservation` owns the `Reservation` AND a watermark. `ensure_committed(len)` is idempotent and monotone (a call at or below the watermark issues no syscall), which is what removes the caller's "did I already commit this?" bookkeeping; `shrink_committed` rounds UP so a page holding bytes the caller asked to keep is never released; `into_reservation()` is the explicit door out. Mutators take `&mut self` — not hygiene, but the crate finally stating a requirement it always had: the watermark is racy and concurrent committers must serialise, which nothing under the raw primitives ever said.
   - **The boundary that keeps this a tool and not an allocator.** Exactly one `usize` is tracked; arbitrary committed/uncommitted HOLES are deliberately not representable. Governing rule, reusable for future proposals: the crate takes on state ONLY when the OS does not cheaply expose it AND the caller would otherwise have to invent it. "How much is committed" passes (Windows knows but charges a `VirtualQuery`; Unix has no such notion); "is THIS page committed" does not, and is not offered.
   - **`lazy_commit_is_honored()`** (const fn) completes the family `decommit_reclaims_and_zeroes()` / `is_huge()`: where a platform difference exists, expose it as something to branch on. `LazyReservation::new` derives the initial watermark FROM that query rather than from the caller's `initial_commit`, which is what makes the two incapable of disagreeing.
   - **Honest limit, documented on the type:** `as_ptr()` still hands out the raw pointer and passing it to the raw primitives makes the watermark stale. No API over raw memory can prevent that. What changed is the DEFAULT — the tracked path is what you get without asking.

   - **Correction, task #1104 (finding H1, `docs/reviews/2026-08-18-aligned-vmem-publication-readiness-audit.md`): the surface shipped in `0c1e6c4` contradicted this card's own rationale.** `LazyReservation::as_reservation(&self) -> &Reservation` — documented as "read-only queries" — handed back exactly the `&self`-mutator access the bullet above says had to be closed: from 100% safe code, `r.as_reservation().decommit(0, page_size())` changed OS commit state under the watermark while `committed_len()` kept promising the prefix writable (on Windows a caller that range-checks against the watermark before writing gets `STATUS_ACCESS_VIOLATION`); the reverse direction — committing past the watermark via `commit_range`/`recommit` through the borrow — was equally available. Its only caller in the entire repository was one redundant `len()` assertion in `tests/lazy_reservation.rs` (the direct `len()` proxy sits on the line above it). Counterfactuals run personally on this tree: a temporary test calling `r.as_reservation().decommit(0, ps)` and asserting the watermark unchanged PASSED pre-fix (the bypass reproduced); after the deletion the same file failed to compile with `error[E0599]: no method named 'as_reservation' found for struct 'LazyReservation'`; the temporary test was then deleted. The accessor is DELETED, not narrowed to a read-only view type: the only queries any caller needed (`len`/`as_ptr`/`align`) were already proxied directly on `LazyReservation`, and a view type is one `impl Deref<Target = Reservation>` away from resurrecting every mutator — it would move the hole, not close it. Regression oracle: `tests/lazy_reservation_no_borrowed_reservation.rs` (text guard in the `granted_huge_reader_enumeration.rs` style) fails on any `as_reservation` token, any `&Reservation`/`&mut Reservation` on a code line in `src/`, any `Deref`/`AsRef`/`Borrow` route to `Reservation`, or any change to `LazyReservation`'s pinned public method set — verified to fire by re-adding the accessor, watching it FAIL, and watching it PASS again after the restore.

   - **Verification:** new `tests/lazy_reservation.rs`, 10 tests, every platform arm asserting rather than skipping. Two counterfactuals run personally: rounding DOWN instead of up made `ensure_committed_rounds_up_to_a_page` FAIL; advancing the watermark WITHOUT issuing the commit killed the binary with `STATUS_ACCESS_VIOLATION` (0xc0000005), proving the writability test touches real OS-committed memory. Both reverted, suite re-run green.
   - **Consequence for the release:** this was the SECOND of R7's two conditional-NO-GO conditions (the first, R7-1, was closed by code in `13723d7`). Both are now closed.

50-U10. **`aligned-vmem` — the U10 half of item 50 ("Windows `bench-internals` reserve-path counters have zero test coverage") rested on a FALSE premise and is closed.** (Filed round 8, task #903, finding U10 of `docs/reviews/2026-08-13-aligned-vmem-round8-review.md`; re-flagged as stale by R7-9 and closed by task #1045.)

   - **Root cause:** item 50's U10 half asserted the counters are "exercised by NOTHING in `crates/aligned-vmem/tests/` — confirmed via `grep -rn \"windows_reserve_commit\" crates/aligned-vmem/tests/` returning no output". That grep does NOT return no output. Three test functions in two files exercise the counters, verified by re-running the grep and mapping every hit back to its enclosing `fn`:
     - `crates/aligned-vmem/tests/bench_internals_counters.rs::bench_internals_counters_existence_and_reset` — asserts `windows_reserve_commit_calls() >= 1` after a live reservation (Windows-gated), then asserts all three counters read `0` after `reset_bench_internals_counters()`.
     - `crates/aligned-vmem/tests/huge_pages.rs::reserve_aligned_huge_2mib_still_two_call_path_unprivileged` — asserts `single_calls + two_call_pairs == 1`, i.e. exactly one dispatch path was taken.
     - `crates/aligned-vmem/tests/huge_pages.rs::reserve_aligned_huge_4mib_still_two_call_path` — asserts `single_calls == 0` AND `two_call_pairs == 1`.
   - **Why this closes U10 rather than merely correcting it:** U10's stated "Next trigger" was a direct assertion that a given shape advances one counter and not the other. `reserve_aligned_huge_4mib_still_two_call_path` IS that assertion, in both directions, and it is discriminating — it fails if the dispatch condition regresses either way. The oracle U10 asked for already exists.
   - **What this closure does NOT claim:** the finer rustdoc claim that a large-page retry "issues a second syscall but is still counted as 1" remains unasserted by any test; it is a syscall-count claim, and no test counts syscalls. Only the DISPATCH claim is covered. Recorded so this closure is not later read as broader than it is.
   - **A correction inside this correction (task #1045):** the first draft of this closure entry cited four test functions, of which TWO do not exist under the names given (`reserve_aligned_huge_2mib_fast_path_or_two_call`, `reset_works`) and a third (`lazy_commit.rs::windows_lazy_reserve_saves_commit_charge`) exists but contains no reference to these counters at all — `grep -rn "windows_reserve_commit" crates/aligned-vmem/tests/lazy_commit.rs` is empty. That is the same failure mode as the claim being fixed: replacing a mis-citation with a new mis-citation (cf. task #900/U2). The inventory above was re-derived mechanically from the grep, not restated from a report.
   - **Files changed:** `docs/CORRECTNESS_OPEN_ITEMS.md` (item 50's U10 half removed from the open card, this closure entry added); `crates/aligned-vmem/src/lib.rs` (`WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS`'s rustdoc lost its stale "third best-effort retry" clause — the two-call path never requests `MEM_LARGE_PAGES`, per the code comment's own task #921/V-7 attribution). Post-split update (task #1082, 2026-08-18): that rustdoc now lives with the counter in `crates/aligned-vmem/src/bench_internals/windows.rs`, after task #1055 (commit `a4b8e50`) split the former monolith; this Files-changed line records the pre-split edit as made.

1. **Flaky test — `canary_survives_promotion_and_free_leaves_no_leak`**
   (`tests/r14_4_promotion_free_correctness.rs`) — **RESOLVED** by an urgent
   CI-fix task (2026-07-26), responding to `origin/main` CI run `30217256247`
   / job `89833506941` failing on the `test (--features "hardened
   medium-classes")` step with `error: 1 target failed: --test
   r14_4_promotion_free_correctness`.

   - **Root cause, confirmed:** `SEGMENTS_RESERVED_TOTAL`/
     `SEGMENTS_RELEASED_TOTAL` (`src/alloc_core/os.rs:52,57`) are
     process-wide `static AtomicU64`s. Both `#[test]` functions in this file
     (`canary_survives_promotion_and_free_leaves_no_leak` and
     `repeated_promote_and_free_does_not_leak_unboundedly`) read `a.stats()`
     — which loads these same global atomics — take a before/after
     snapshot, and assert a leak-free delta. `cargo test` runs test
     functions concurrently on multiple OS threads within one process by
     default; the two tests in this file (or any other test in the same
     binary) could reserve/release a segment on the shared counters between
     one test's own snapshots, polluting its delta with unrelated activity
     — exactly the historically observed "failed 1 of 3 runs" signature.
   - **Fix:** added a file-scoped `static TEST_LOCK: Mutex<()>` + `serial()`
     helper (the SAME established pattern already used in
     `tests/directory_authoritative_miss.rs`, `tests/alloc_zeroed_fresh_large_skip.rs`,
     `tests/r13_3_magazine_virgin_hit_skips_zero.rs`,
     `tests/r21_2_opt_h_stage1_precondition_probe.rs` for tests that read
     process-wide stats/diagnostic counters), and bound `let _guard =
     serial();` at the top of BOTH test functions in the file (both read
     the same global counters, so both needed serialization, not just the
     one named in the CI failure). No assertion logic was changed — the
     `released_delta <= reserved_delta` leak-bound check is untouched.
   - **Verification:** 4 full runs of the exact CI command (`cargo test
     --features "hardened medium-classes" --no-fail-fast`, matching R22-1's
     CI row exactly — 223 test binaries each run) — all clean, 0 failures.
     Additionally ~190 direct repeated invocations of the specific compiled
     test binary (`--test-threads=4/8/16`, mimicking CI-like concurrency)
     plus several `cargo test --test r14_4_promotion_free_correctness`
     invocations — 0 failures out of roughly 200+ total runs, against the
     historical ~1-in-3 failure rate. `cargo fmt --check` clean on the
     changed file.
   - **Files changed (test/implementation only):**
     `tests/r14_4_promotion_free_correctness.rs`; this index entry itself
     is the second file touched in the same commit (`bc4aacf`).
   - **Scope of what this fix actually proves:** this fix resolved the
     test-ISOLATION RACE only — it did not touch, and did not strengthen,
     the test's own leak-bound assertion (`released_delta <=
     reserved_delta`). That assertion is a DOUBLE-RELEASE guard (released
     count never exceeding reserved count), not a proof of no leak: if a
     grow reserved a segment and never released it, `reserved_delta=1,
     released_delta=0` satisfies `0 <= 1` trivially, so a genuine
     never-released segment would not be caught by this test. This
     semantic gap pre-dates `bc4aacf` and was correctly left untouched by
     it — fixing the test-isolation race was that commit's actual, correct
     scope. See open item 4 above ("Open items" §`[T]`) for a tracked
     follow-up on strengthening leak detection itself.

3. **Flaky test — `shadow_path_activation_oracle_fast_and_slow_both_reachable`
   scheduler-sensitive percentage thresholds (BOTH regimes).** **RESOLVED**
   in TWO steps, both 2026-08-16 — do not read this card as landing in a
   single commit:
   (a) the root-cause fix, commit `8d68715` (task #1030): the `SERIAL`
   guard plus exact-equality assertions;
   (b) a portability follow-up, the commit carrying this entry (task
   #1033, finding F5 of
   `docs/reviews/2026-08-16-aligned-vmem-r6-wave-review.md`): (a)'s
   exact equalities were only valid on strong-CAS targets, and were
   replaced by two-sided bounds.

   - **Root cause, confirmed:** process-global `DBG_RING_PUSH_SHADOW_FAST`/`_SLOW`
     counters are shared across all tests in the binary. libtest runs this
     file's three `#[test]` functions concurrently by default. Without a
     serial guard, concurrent sibling tests can pollute the counters between
     a test's `before`/`after` snapshots, causing spurious percentage
     assertion failures. The original failure pattern (slow_delta itself
     was structurally correct at 2000 ≈ ROUNDS, but fast_delta grew to 256
     under load) proves this: the adversarial mechanism worked, but the
     DELTA measurement was corrupted by concurrent noise.
   - **Fix (a), commit `8d68715`:** introduced a `static SERIAL: Mutex<()>`
     guard (following the pattern from `crates/aligned-vmem/tests/smoke.rs`) that
     all three `#[test]` functions in this file hold for their entire
     bodies. With the serial guard, each test has exclusive access to the
     global counters, eliminating the concurrent noise source. Because the
     noise source was gone, the assertions were then TIGHTENED rather than
     relaxed: lower bounds became exact equalities (`assert_eq!(total,
     ROUNDS)`), and the percentage thresholds moved from ≥95% to ≥99% fast
     path in the favorable regime and from a percentage to a
     `slow_delta >= ROUNDS - 10` count check in the adversarial one.
     Guard effectiveness was demonstrated by removing the guard: `cargo
     test --all-features --test remote_ring_shadow_head` then reported
     2258 against an expected 2000, i.e. one test's counter delta absorbed
     roughly 258 pushes issued by a concurrently running sibling. (The
     original card named a specific direction for that cross-test
     pollution; it is not recoverable from the recorded evidence, which
     shows only the polluted total, so the direction is deliberately left
     unstated here rather than guessed.)
   - **Fix (b), the commit carrying this entry:** (a)'s exact equalities
     silently assumed `compare_exchange_weak` never fails spuriously.
     `RemoteFreeRing::push` (`src/alloc_core/remote_free_ring.rs`) is a
     `loop { load tail; full_check(t); compare_exchange_weak(..) }`, and
     `full_check` bumps exactly one shadow counter PER CALL — so on an
     LL/SC target (aarch64/ARM) a spurious CAS failure retries the loop
     and double-counts one logical push. Exact equality therefore held
     only on strong-CAS targets, where `compare_exchange_weak` lowers to
     `lock cmpxchg`. Replaced with two-sided bounds
     (`total >= ROUNDS && total <= ROUNDS + CAS_RETRY_SLACK`,
     `CAS_RETRY_SLACK = 8`), which keeps everything (a) was for — the
     original failure was a ~256-push distortion by a sibling test, not a
     handful of CAS retries — while staying valid on every architecture.
     Latent, not live: both tests require `feature = "bench-internals"`,
     which is in neither `production` nor `experimental`, and every
     aarch64/macOS CI row uses only those bundles, so the tests do not
     compile there today. The failure would surface the moment an
     aarch64 row gains `bench-internals`, or on a local
     `cargo test --all-features` on Apple Silicon.
   - **Counterfactual:** removing the adversarial loop's
     `dbg_advance_head_only` call (so the shadow never goes stale) makes
     the test fail, confirming the oracle still detects a non-reached
     adversarial condition. Re-verified after fix (b) landed; note the
     assertion message changed between (a) and (b), so a quoted failure
     string is only meaningful against the matching revision.
   - **Verification:** `cargo test --all-features --test
     remote_ring_shadow_head`, 5 consecutive clean runs, plus
     `cargo clippy --all-features --all-targets -- -D warnings` clean —
     re-run for (b), not inherited from (a).
   - **Files changed:** `tests/remote_ring_shadow_head.rs` in both
     commits, plus this index entry.

2. **Clippy dead-code — `--features "hardened medium-classes"` was not
   clippy-clean (11 errors)** — **RESOLVED** by R23-5 (task #374). All 11
   were genuine `#[cfg(...)]` predicate mismatches (an item gated one way,
   its only consumer gated a DIFFERENT way, so under the specific
   intersection `hardened medium-classes` the consumer compiled out but the
   item did not) — confirmed exhaustively per item via `grep` across
   `src/`, `tests/`, `benches/`, `crates/` before touching anything; NONE
   were genuine orphans, so nothing was deleted.

   - **Items 1, 2, 4 — independent single-item mismatches:**
     - `src/alloc_core/alloc_core.rs:54` (unused import `SMALL_CLASS_COUNT`):
       both of the import's only two usages
       (`alloc_core.rs:711`/`directory_miss_streak` field,
       `alloc_core.rs:978`/its initializer) are
       `#[cfg(feature = "alloc-segment-directory")]`-gated, but the `use`
       itself was not. Fix: split the import so `SMALL_CLASS_COUNT` gets its
       own `#[cfg(feature = "alloc-segment-directory")]` line, matching its
       usages; `AllocKind`/`SizeClasses` (used unconditionally elsewhere)
       stay ungated.
     - `src/alloc_core/alloc_core_large.rs:448` and
       `src/alloc_core/alloc_core_small.rs:1941` (`let mut seg = ...` "does
       not need to be mutable"): both `seg` bindings are reassigned ONLY
       inside a `#[cfg(feature = "alloc-decommit")]` pool-drain-and-retry
       block a few lines below; with `alloc-decommit` off (as under
       `hardened medium-classes`) the binding is genuinely never mutated.
       Fix: `#[allow(unused_mut)]` on each binding, following the identical
       established pattern already at
       `src/registry/heap_core_ownership.rs:167` for the same
       feature-conditional-mutation shape.
   - **Items 3, 5, 6 — one unified root cause (`small_cur`), as suspected in
     the task brief:** `AllocCore::small_cur()` (`alloc_core.rs`, was gated
     `#[cfg(feature = "alloc-xthread")]`) has exactly one caller in the
     entire crate — `heap_core_xthread.rs::drain_heap_overflow`, which reads
     it ONLY inside its own `#[cfg(feature = "alloc-decommit")]` block
     (feeding `dec_live_and_maybe_decommit`, which itself requires that
     feature). `alloc-xthread` without `alloc-decommit` (exactly `hardened
     medium-classes`: `hardened = ["fastbin"]` →
     `["alloc-global","alloc-xthread"]`, neither of which pulls in
     `alloc-decommit`) left the method callable-but-uncalled. The two local
     `let small_cur = self.small_cur;` bindings
     (`alloc_core_small.rs:893`, `alloc_core_small_reclaim.rs:506`) are the
     SAME pattern one level down: each is read only inside its own sibling
     `#[cfg(feature = "alloc-decommit")]` block a few lines later. Fix:
     tightened `small_cur()`'s gate to
     `#[cfg(all(feature = "alloc-xthread", feature = "alloc-decommit"))]`
     (its true minimal predicate, matching its one caller), and gated both
     local bindings `#[cfg(feature = "alloc-decommit")]` directly (matching
     their one reader each). Verified two OTHER `let small_cur = ...`
     bindings at `alloc_core_small.rs:1132` and `:2545` were NOT in the
     11-error list and left untouched — clippy did not flag them (their
     enclosing functions/blocks have their own gating that made them a
     non-issue under this combo), confirming the fix was scoped to exactly
     the 3 flagged sites, not a mechanical crate-wide rename.
   - **Items 7-9 — one unified root cause (`sidecar.rs`), as suspected in
     the task brief:** `reserve_zeroed_with` has exactly one caller,
     `os.rs::reserve_directory_sidecar`, gated
     `#[cfg(feature = "alloc-segment-directory")]`. `deref`/`deref_mut` each
     have TWO independent consumer groups — `alloc_core_small.rs`'s
     `directory`/`directory_mut`/`maybe_materialize_directory` +
     `alloc_core_core_diag.rs`'s `dbg_rebuild_directory` (all inside
     `#[cfg(feature = "alloc-segment-directory")]`), and
     `large_cache_extended.rs`'s `deref_large_cache_extension[_mut]`
     forwarders (the whole module gated
     `#[cfg(feature = "large-cache-extended")]`) — either feature alone
     keeps them used. Under `hardened medium-classes`, `alloc-segment-directory`
     is off AND `large-cache-extended = ["alloc-decommit"]` is transitively
     off too (via `alloc-decommit`), so all three functions had zero live
     callers. Fix: followed the EXISTING convention already used one
     function above in the same file (`reserve`'s
     `#[cfg_attr(not(feature = "large-cache-extended"), allow(dead_code))]`,
     predating this task) rather than a hard `#[cfg]` on the function
     itself (keeps these generic `pub(crate) fn`s type-checking under
     `cargo-hack`-style per-feature builds) —
     `#[cfg_attr(not(feature = "alloc-segment-directory"), allow(dead_code))]`
     on `reserve_zeroed_with`, and
     `#[cfg_attr(not(any(feature = "alloc-segment-directory", feature = "large-cache-extended")), allow(dead_code))]`
     on `deref`/`deref_mut` (the `any(...)` reflecting their two independent
     consumer groups, neither of which alone is necessary).
   - **Items 10-11 — two independent single-item mismatches, as suspected:**
     - `src/registry/heap_core_xthread.rs:586`
       (`const EMPTIED_BASES_CAP: usize = 64;`, itself ungated): every
       actual usage (the `emptied_bases`/`emptied_count` declarations and
       both `if emptied_count < EMPTIED_BASES_CAP` comparisons) is already
       `#[cfg(feature = "alloc-decommit")]`-gated; only the constant
       declaration itself lacked the gate. Fix: added
       `#[cfg(feature = "alloc-decommit")]` to the `const` line, matching
       its usages.
     - `src/registry/heap_registry.rs:523` (`struct ConflictRollback`, and
       its `impl Drop`): constructed exactly once, inside
       `claim_with_config`'s config-mismatch branch — and
       `claim_with_config` itself is `#[cfg(feature = "alloc-decommit")]`-gated
       ("Only present under `alloc-decommit`", per its own doc comment).
       Fix: added `#[cfg(feature = "alloc-decommit")]` to both the struct
       and its `impl Drop`.
   - **One additional latent issue found and fixed in the same task (not
     among the original 11, but the same predicate-mismatch class, and
     newly exposed by fixing the 11 above — the lib now compiles under this
     combo, so `--all-targets` reaches this test target for the first
     time):** `tests/regression_batch_flush.rs`'s `DECOMMIT_COUNTER_SERIAL`/
     `SerialGuard` (a `TEST_LOCK`-style serialization guard) and its
     `use std::sync::atomic::{AtomicBool, Ordering}` import were declared
     unconditionally, but every actual use is inside
     `#[cfg(feature = "alloc-decommit")]`-gated test functions. Fixed the
     same way: gated the static/struct/impls/import on
     `#[cfg(feature = "alloc-decommit")]`.
   - **No deletions.** Every one of the 11 (plus the 1 latent test-file
     issue) was confirmed genuinely used under some other feature
     combination already in this project's CI matrix before any fix was
     applied — verified by `grep`ing every call site across the whole repo
     (not just under `hardened medium-classes`).
   - **Verification:**
     `cargo clippy --all-targets --features "hardened medium-classes" -- -D warnings`
     — 0 errors, 0 warnings (down from the stable 11). No new warning
     surfaced as a side effect of any individual fix (re-ran the full
     command after each fix). `cargo test` green across all of: `""`
     (default), `production`, `--all-features`, `hardened medium-classes`,
     `production alloc-stats`, `pinning` (the full
     `scripts/check-all.mjs` test-step feature matrix) — 0 failures in
     every combination. `cargo fmt --all -- --check` clean.
   - **CI:** added a 4th step to the `clippy` job in `.github/workflows/ci.yml`
     (`clippy (--features "hardened medium-classes")`, alongside the
     existing `clippy ()` / `clippy (--features experimental)` /
     `clippy (--all-features)` steps in that same job) so this combination's
     `-D warnings` gate now runs per-PR, not just `cargo test` (closed
     R22-1's deliberately-left-open gap).
   - **Files changed:** `src/alloc_core/alloc_core.rs`,
     `src/alloc_core/alloc_core_large.rs`, `src/alloc_core/alloc_core_small.rs`,
     `src/alloc_core/alloc_core_small_reclaim.rs`, `src/alloc_core/sidecar.rs`,
     `src/registry/heap_core_xthread.rs`, `src/registry/heap_registry.rs`,
     `tests/regression_batch_flush.rs`, `.github/workflows/ci.yml`, and this
     index.

3. **Deferred decision — `aligned-vmem`'s `mock` Cargo-feature-unification
   hazard was resolved with a doc-only fix, explicitly deferring a
   stronger `--cfg`-flag conversion; the SAME finding recurs in
   `numa-shim` and the deferral is load-bearing for that crate's own
   upcoming round.** — **CLOSED** (updated 2026-08-09, task #778/F5 —
   round-closing review of the numa-shim round). Filed 2026-08-09,
   task #776/F13, round-closing review of the aligned-vmem round.
   **RE-OPENED** 2026-08-14 (task #934/C-9) — see the `[A]` tier's item 42; the deadline this deferral was conditioned on has fired.

   - **Closure narrative:** `numa-shim`'s round reached its own §C10 finding
     in task #726 (commit `53b3ca2`) and applied EXACTLY the policy this
     item's "Next trigger" prescribed — a doc-only fix
     (`crates/numa-shim/Cargo.toml`'s `mock = []` feature comment, the `mock`
     module's own rustdoc, and a new `README.md` section), citing task
     #715's reasoning — but the review found this card was never updated
     in that commit, the same "update the card in the SAME commit"
     violation this file's own convention exists to catch (see item 41's
     analogous correction above). Both crates now carry the SAME recorded
     policy for the identical finding shape, so this item is closed
     rather than left open with a stale forward-reference.
   - **Current-number-or-verdict:** `aligned-vmem`'s `mock` feature
     (`crates/aligned-vmem/Cargo.toml`) AND `numa-shim`'s `mock` feature
     (`crates/numa-shim/Cargo.toml`) are both Cargo features (not `--cfg`
     flags), each documented with a Cargo-feature-unification warning in
     three places (`Cargo.toml`, the `mock` module's own doc, `README.md`).
     Task #715 (commit `e5f6700`) explicitly evaluated and DEFERRED the
     stronger fix for `aligned-vmem` — converting `mock` from a Cargo
     feature to a `--cfg`-style RUSTFLAGS flag, matching this repo's own
     `cfg(loom)`/`cfg(kani)` precedent (cfg flags do not unify across a
     build the way Cargo features do) — reasoning that neither crate has
     real external consumers before its first publish (`aligned-vmem`:
     task #658; `numa-shim`: task #657), so the doc-only fix closes the
     realistic near-term risk at much lower cost than a mechanical
     rewrite of the whole test-invocation surface and CI matrix for
     EITHER crate. Task #726 (commit `53b3ca2`) applied the identical
     reasoning to `numa-shim`.
   - **Evidence:** `crates/aligned-vmem/Cargo.toml`'s `mock = []` feature comment
     (the "CARGO FEATURE-UNIFICATION HAZARD" block) and
     `crates/numa-shim/Cargo.toml`'s `mock = []` feature comment both state
     the deferral explicitly: "Revisit if/when this crate gains external
     consumers and the hazard is reported for real." (Cited by feature
     name, not line range — round-5 closing review QC6 found a line-range
     citation into this exact block go stale within the same round that
     wrote it.)
   - **Revisit condition (both crates jointly):** if EITHER crate gains a
     real external consumer that reports this hazard for real, re-open
     this item and revisit BOTH crates' resolution together — do not let
     one crate silently drift to a `--cfg`-flag conversion while the
     other stays doc-only for the same finding shape.

4. **Two flaky coarse-wall-clock tests surfaced by `npm run check`'s
   `--all-features` step** — **RESOLVED** by R23-6 (task #375). One
   independent read-only review first corrected the originally-proposed fix
   (a `TEST_LOCK`-style mutex): a mutex only serializes test FUNCTIONS
   within ONE test binary/process, but the actual flakiness source is CPU
   contention from MULTIPLE test binaries (separate OS processes) running
   concurrently under `npm run check`'s `--all-features` step, plus the CI
   runner's own background load — a mutex inside one binary cannot
   serialize against a different process. That correction was confirmed
   independently before this task began and is reflected in the fix below
   (no `TEST_LOCK` was added to either file).

   - **`tests/regression_segment_table_tombstone_rebuild.rs::backshift_no_latency_spike_at_threshold_boundary`
     — got a deterministic replacement.** The test's (b) claim ("no single
     `unregister`/`recycle` does `O(HASH_CAPACITY)` work") maps exactly onto
     `SegmentTable::hash_remove`'s backward-shift scan-step count (the
     `j = (j+1) & mask` walk across both its find-the-slot and
     shift-the-cluster phases). Added `HASH_REMOVE_MAX_SCAN_STEPS`
     (`src/alloc_core/segment_table.rs`) — a process-wide high-water-mark
     `AtomicU64`, `alloc-stats`-gated increment (same convention as
     `OPT_H_ATTEMPTS`/`HARDENED_LARGE_NOOP_COUNT`), reset hook
     `reset_hash_remove_max_scan_steps`, and `AllocCore` accessors
     `dbg_hash_remove_max_scan_steps`/`dbg_reset_hash_remove_max_scan_steps`
     (`src/alloc_core/alloc_core_core_diag.rs`) — deliberately a MAX not a
     sum, matching the original test's own "no single call is an outlier"
     framing rather than conflating many small deletes with one large one.
     New test `backshift_max_scan_steps_bounded_at_threshold_boundary`
     (`#[cfg(feature = "alloc-stats")]`, same file) drives the identical
     `W = 600`-distinct-bases wave-then-drain shape as the original and
     asserts the high-water mark stays `<= 4 * W` (`HASH_CAPACITY = 8192`
     would be ~13.6x that bound) — a deterministic per-run assertion, zero
     timing, zero retries. (R24-1/task #379 wording-precision note: the
     MEASUREMENT is exact per-run, but the `4 * W = 2400` threshold is a
     regression bound calibrated to this wave's `W = 600`, not a proven
     O(cluster) worst-case for arbitrary configurations — it reliably catches
     a full O(`HASH_CAPACITY`) regression but could miss a smaller
     pathological cluster under 2400 steps.) The original wall-clock test is
     KEPT (not deleted, for manual/`--ignored` diagnostic value) but marked
     `#[ignore = "..."]`
     with a message pointing at the deterministic replacement and
     `npm run iai`.
   - **`tests/dealloc_sublinear.rs::own_thread_free_is_subquadratic` —
     no clean deterministic replacement exists; demoted to non-blocking.**
     Investigated seriously (per this task's explicit instruction not to
     default to demotion): the guard this test protects
     (`AllocCore::dealloc_small`'s M2 double-free check,
     `src/alloc_core/alloc_core_small.rs`) is, by design, an UNCONDITIONAL
     O(1) `AllocBitmap::is_free` bit test with NO loop — Phase 13.4a already
     replaced the O(free-list-length) `free_list_contains` walk this test
     guards against with exactly that O(1) bitmap test. A call-count counter
     ("how many times was the guard tested") would read identically (= N
     after N frees) under BOTH the correct O(1) implementation and the
     regressed O(N²) walk it guards against — the walk's CALL COUNT never
     changed across that regression, only its internal LENGTH did, and there
     is no length-dependent loop left in production code to instrument. The
     only way to get a counter would be adding one to code that would first
     need to reintroduce the very walk being guarded against — not a
     diagnostic-only addition. Per this task's constraint ("if a new counter
     requires touching a genuinely hot path... stop and explain the
     tradeoff"), this test is `#[ignore]`d instead, with a message pointing
     at manual `--ignored` runs and `npm run iai` /
     `benches/perf_gate_iai.rs`'s `small_churn_16b`-family arms as the
     deterministic Ir-based judges for this same free-path cost.
   - **Mechanism confirmed:** `scripts/check-all.mjs` runs `cargo test
     --features <combo>` for each of its feature-matrix entries and fails
     the whole gate on ANY test failure (including any `#[ignore]`d-off
     test simply not running) — `#[ignore]` is exactly the mechanism
     `cargo test` (and therefore this repo's CI/`check-all.mjs`) already
     uses to exclude a test from the blocking pass/fail gate while keeping
     it runnable via `cargo test -- --ignored`, so no `check-all.mjs`/CI
     workflow change was needed.
   - **Non-vacuity — mutation counterfactual (the new deterministic test):**
     temporarily forced `hash_remove`'s phase-1 find loop to burn
     `HASH_CAPACITY - 1` extra counter increments before matching (simulating
     the pre-N3 O(HASH_CAPACITY) tombstone-scan regression class directly,
     without touching pointer/unsafe logic) —
     `backshift_max_scan_steps_bounded_at_threshold_boundary` FAILED
     immediately (`max_steps = 8191` against the `2400` bound, with a
     message correctly naming the O(HASH_CAPACITY) regression class);
     reverted, and the test passed again. Confirms the new test is
     non-vacuous — it fails without the property it's checking for holding.
     No counterfactual was performed for `own_thread_free_is_subquadratic`
     (it was demoted, not replaced) — its own pre-existing counterfactual
     documentation (module doc comment, "author-verified" restoring the old
     `free_list_contains` walk trips the assertion) is unchanged and still
     applies to manual/`--ignored` runs.
   - **Verification:** `cargo test --features production` (223 binaries),
     `cargo test --features "production alloc-stats"` (223 binaries, exit
     0 — this is the combo that actually compiles and runs the new
     deterministic test), and `cargo test --all-features` all green, 0
     failures. Both previously-flaky tests confirmed `... ignored, <reason>`
     under every combo they're compiled under; the new deterministic test
     confirmed `... ok` under `production alloc-stats` and `--all-features`,
     and confirmed ABSENT (not vacuously passing) under plain `production`
     (no `alloc-stats`). `cargo clippy --all-targets -- -D warnings` clean
     across all three CI feature-matrix entries (`""`, `experimental`,
     `--all-features`). `cargo fmt --all -- --check` clean on all touched
     files.
   - **Files changed:** `src/alloc_core/segment_table.rs`,
     `src/alloc_core/alloc_core_core_diag.rs`,
     `tests/regression_segment_table_tombstone_rebuild.rs`,
     `tests/dealloc_sublinear.rs`, and this index.

5. **`dealloc_batch_small` doc comment claimed the LAST `TCACHE_CAP` freed
   blocks stay magazine-warm; the implementation keeps the FIRST.** —
   **RESOLVED** by R24-7 (task #385), a doc-only policy decision (no `src/`
   behavior change, no numbers measured).

   - **First observed:** independent read-only review of Round 23
     (`docs/reviews/2026-07-27-r23-readonly-review.md` §5.3).
   - **The gap:** `src/registry/heap_core_dealloc_batch.rs`'s
     `dealloc_batch_small` "Trade-off" doc comment (from the original R11-4
     commit `ff9ad7a`) claimed the LAST `TCACHE_CAP` blocks stay
     magazine-warm. The implementation iterates `for &p in blocks` in slice
     order and fills the magazine until `count == TCACHE_CAP`, then routes
     every further ACCEPTED block to `flush_class` — so with an empty
     magazine the FIRST `TCACHE_CAP` accepted blocks stay warm, the opposite
     of the claim.
   - **Decision (option (a) of the R24-7 brief): correct the documentation
     to describe the actual first-warm behavior; do NOT switch to a
     rolling-buffer last-warm algorithm.** `git blame` shows the "last warm"
     text was in the original R11-4 commit, unedited since, with no recorded
     rationale — an aspirational doc error matching scalar temporal-locality
     intuition, never verified against the always-first-warm implementation;
     there is no design reason "last" was specifically chosen that "first"
     would defeat. A last-warm rolling buffer would add, per overflow block,
     a `clear_magazine` RMW on a hot L1 bitmap line plus rotation/index
     arithmetic plus an extra stage write — strictly MORE per-block work
     than the current overflow arm (which only writes to `stage`), i.e. the
     SAME cost category two adjacent Round-24 tasks measured as NO-GO
     regressions in this exact code region: R24-3 (task #381,
     +37 Ir/overflow-event) and R24-4 (task #382, +14 Ir/block). The benefit
     (locality for "free a large batch then immediately realloc same
     class") is contested by the doc's own use-case argument AND has no
     in-tree consumer (R23-7: the batch API ships experimental with no
     production caller), so even a zero-cost switch would realize no
     production benefit today. Under that prior, prototyping the rolling
     buffer would very likely reproduce the R24-3/R24-4 regression class.
   - **The corrected doc comment's secondary claim is unaffected and still
     holds:** a small batch (`N <= TCACHE_CAP`) is byte-for-byte as warm as
     the scalar loop under EITHER first- or last-warm policy (all `N`
     accepted blocks fit the magazine).
   - **No in-context Ir measurement was run,** because option (b) was not
     pursued: the brief's own recommendation frames (a) as the default and
     (b) as the higher-bar prove-it-first path, and the structural prior
     (two NO-GOs in the same cost category + no consumer + the doc's own
     argument against the benefit) made the measurement very likely to only
     confirm a regression. The mandatory-if-pursued measurement gate
     therefore did not apply.
   - **Files changed:** `src/registry/heap_core_dealloc_batch.rs` (doc
     comment only) and this index. Zero `src/` behavior change; `git diff
     HEAD -- src/` shows only the doc-comment edit. No version bumps.

30. **`canary_survives_promotion_and_free_leaves_no_leak`'s leak-bound
   assertion proved no double-release, not no leak.** — **RESOLVED** by
   R28-2 (task #431), a test-only strengthening (no `src/` behavior change).

   - **The gap (recap):** the pre-existing `released_delta <=
     reserved_delta` assertion in
     `tests/r14_4_promotion_free_correctness.rs` is satisfied trivially by
     `reserved_delta=1, released_delta=0`, so a grow that reserves a segment
     and never releases it would pass silently — the assertion is a
     double-release/corruption guard, not a leak proof.
   - **Observable used — no new hook needed.** Investigated existing
     `SegmentTable`/diagnostic surface before adding anything: `HeapCore`
     already exposes `dbg_contains_base` (`&mut self`, gated
     `alloc-global + alloc-xthread`, `src/registry/heap_core_diag.rs:482`)
     and `dbg_live_count_for` (`&self`, gated `alloc-decommit`,
     `heap_core_diag.rs:317`), both safe `pub fn`s already appropriately
     gated per the benchmark-hook rule (not new — pre-existing, ungated
     wider than needed). Both gates are satisfied by plain `production`
     (`alloc-xthread` and `alloc-decommit` are both in the `production`
     feature list), so no new hook and no `bench-internals` dependency was
     required. To reach a `*mut HeapCore` for the CURRENT thread's own
     `SeferAlloc` from a `tests/` integration test (`SeferAlloc` itself
     exposes no direct `HeapCore` accessor), reused the SAME established
     save/poison/restore pattern `tests/dealloc_only_no_bind_torn.rs`
     already uses: `sefer_alloc::global::tls_heap::dbg_mark_local_torn_for_test()`
     (snapshot + poison `LOCAL`) immediately followed by
     `dbg_restore_local_for_test(saved)` (undo the poison), yielding the
     saved pointer — binding is per-THREAD (TLS), not per-`SeferAlloc`-
     instance, so this is exactly the same `HeapCore` the test's own
     `a.alloc`/`a.dealloc` calls already routed through.
   - **The new assertion:** resolves `grown`'s segment base
     (`dbg_segment_base_of_ptr`) and calls the production teardown-trim
     primitive `SeferAlloc::dbg_trim_current_thread()` (pre-existing,
     `src/global/sefer_alloc.rs:423` — flushes every tcache class, drains
     the empty-small-segment hysteresis pool, evicts the large_cache) BOTH
     immediately before taking a `live_count` baseline AND immediately after
     freeing `grown`, so both snapshots are read in the same converged,
     magazine/pool/cache-free regime (the double trim matters: a freed
     block routinely sits in the per-thread magazine rather than being
     reconciled into `live_count` immediately — see the gap found in
     development, below). After freeing and trimming, asserts `grown_base`
     is in exactly one of two sanctioned states: (a) fully unregistered
     (`dbg_contains_base == false` — the Large-segment-free path always
     calls `table.unregister` before returning, cache-admitted or not), or
     (b) still registered but with `live_count` decreased by EXACTLY one
     relative to the pre-free baseline (covers the `!HAS_PROMOTION`
     medium-ladder case, where segments are shared across size classes via
     a single per-thread `small_cur` bump cursor and routinely host other
     live blocks). Any other outcome (segment still registered with an
     unchanged or increased live_count) fails with a message naming the
     leak.
   - **Design iteration during development (kept in the report per the
     task's non-vacuity requirement, not just the final counterfactual):**
     two earlier designs were tried and rejected by ACTUAL feature-combo
     test runs, not just review — (1) a bare `dbg_contains_base(grown_base)
     == false` assumption failed under `production medium-classes
     exact-span-large` (`HAS_PROMOTION == false`) because medium-class
     segments are shared across size classes (`AllocCore::carve_block`'s
     single per-thread `small_cur`), so the segment legitimately stays
     registered with other live blocks; (2) an absolute
     `live_count_after_free == Some(0)` assumption also failed under the
     same combo (`live_count` went `Some(2)` before and after — unrelated
     co-tenant blocks were still magazine-buffered, not yet reconciled),
     which led to discovering the magazine-residency gap (`dealloc` does
     not call `dec_live` for a block that lands in the tcache — see
     `HeapCore::dbg_is_free_for`'s doc comment) and the final double-trim,
     before/after-delta design above.
   - **Non-vacuity — mutation counterfactual, run TWICE (once for the
     Large-promoted path, once for the medium-ladder path, since they are
     structurally different code paths):**
     - **Large path** (`production medium-classes`, `HAS_PROMOTION ==
       true`): commented out the `self.table.unregister(base)` call in the
       cache-admitted leg of `AllocCore::dealloc`'s Large branch
       (`src/alloc_core/alloc_core.rs:1451`), simulating a grow that
       deposits a segment into `large_cache` but never removes it from the
       table. New assertion FAILED immediately: `LEAK: grown_base (...) is
       still registered in the segment table ... live_count went from None
       to None`. Reverted; `git diff` confirmed byte-identical to the
       original; test passed again.
     - **Medium-ladder path** (`production medium-classes exact-span-large`,
       `HAS_PROMOTION == false`): commented out the
       `dec_live_batch_and_maybe_decommit`-driven block inside `flush_run`
       (`src/alloc_core/alloc_core_small_magazine.rs:682-693`, guarded with
       `#[cfg(any())]` for a clean single-site disable), simulating a leak
       where a block returned to the magazine-flush path never reconciles
       its live_count. New assertion FAILED with `live_count went from
       Some(2) to Some(2)` — exactly the "no change at all" signature the
       assertion's own doc comment predicts for this failure mode. Reverted;
       `git diff` confirmed byte-identical to the original; test passed
       again. (An earlier, less isolated counterfactual attempt at the same
       Large-branch call site under this combo produced a
       `STATUS_ACCESS_VIOLATION` crash instead of a clean assertion failure
       — because skipping `unregister` while `large_cache_slot_set` still
       ran created a genuinely double-owned segment that
       `dbg_trim_current_thread`'s `evict_all` then double-freed; this was
       diagnostic noise from an overly-blunt counterfactual site, not a
       defect in the new assertion, so the `flush_run` site above was used
       instead for a clean, isolated result.)
   - **CI-compatibility gap found and fixed during zero-trust review (before
     commit):** the strengthened block's own two accessors need
     `alloc-decommit` (`dbg_live_count_for`) and `alloc-xthread`
     (`dbg_contains_base`), a strictly NARROWER feature set than this file's
     own top-level `#![cfg(all(feature = "alloc-global", feature =
     "medium-classes"))]` gate — and `.github/workflows/ci.yml` runs a
     dedicated `test (--features "hardened medium-classes")` step
     (`hardened = ["fastbin"]` = `alloc-global + alloc-xthread`, WITHOUT
     `alloc-decommit`) that exercises this exact file. The as-delegated diff
     compiled clean only under the two combos it was directly tested against
     (`production medium-classes[, exact-span-large]`, both of which include
     `alloc-decommit` via `production`) and failed to compile under `hardened
     medium-classes` with two `E0599: no method named dbg_live_count_for`
     errors — confirmed via `cargo test --no-run --features "hardened
     medium-classes" --test r14_4_promotion_free_correctness` BEFORE the fix.
     Fixed by narrowing the new block's own gate to `#[cfg(all(feature =
     "alloc-decommit", feature = "alloc-xthread"))]` (a `let (heap,
     grown_base, live_count_before_free) = { ... };` tuple-block before the
     unconditional `a.dealloc` call, and a second `#[cfg(...)]` block after
     it for the assertion itself — the actual `a.dealloc(grown, ..)` call the
     pre-existing `released_delta <= reserved_delta` assertion needs stays
     UNCONDITIONAL either way) rather than widening the file's own top-level
     gate, which would have silently dropped this test from the `hardened
     medium-classes` CI row's coverage entirely. Re-verified after the fix:
     `cargo test --no-run --features "hardened medium-classes" --test
     r14_4_promotion_free_correctness` compiles clean and the test still
     passes (exercising only the original double-release guard, as before
     this task); both counterfactuals above were RE-RUN against the
     restructured code (not just the pre-restructure version) and still fail
     correctly.
   - **Verification:** `cargo test --release --features "production
     medium-classes" --test r14_4_promotion_free_correctness` and `cargo
     test --release --features "production medium-classes exact-span-large"
     --test r14_4_promotion_free_correctness` both green (2 passed, 0
     failed) after the final design landed. Repeat-run flake check: 35
     `cargo test` invocations plus 120 direct repeated binary invocations
     (60 per feature-combo binary, `--test-threads=4`) — 1 anomalous failure
     out of ~155 total runs, attributed to this session's heavy concurrent
     multi-agent build contention on the shared `target` directory
     (repeatedly observed "Blocking waiting for file lock on build
     directory" messages throughout), not reproduced in any of the
     subsequent 120 direct-binary runs. Full-suite regression check:
     `cargo test --release --features production` (226 `test result: ok`
     blocks, 0 `FAILED`) and `cargo test --release --features "production
     medium-classes"` (226 `test result: ok` blocks, 0 `FAILED`) both clean.
     `cargo fmt --check` clean on the changed file.
   - **Files changed:** `tests/r14_4_promotion_free_correctness.rs` and this
     index. No `src/` changes (the two counterfactual breaks used for
     non-vacuity verification were both reverted before this commit — `git
     diff` on `src/` is empty). No version bumps.

   - **R29-1 correction (2026-07-29, task #432) — REOPENED then RE-RESOLVED
     with a real root cause.** The R28-2 entry above recorded "1 anomalous
     failure out of ~155 total runs, attributed to this session's heavy
     concurrent multi-agent build contention on the shared `target`
     directory" and marked the item RESOLVED on that attribution. An
     independent readonly review
     (`docs/reviews/2026-07-29-r28-readonly-review.md` §"P0 — the R28-2
     anomalous failure is not explained") flagged that build-lock contention
     explains DELAY, not a COMPLETED assertion failure, and that the original
     root cause was therefore unproven. R29-1 investigated and confirmed the
     review was right to flag it: the anomaly is REAL (reproduced), but its
     root cause is a **test-logic bug (classification (a) from the task's
     taxonomy), NOT an allocator correctness concern, NOT infrastructure**.
     - **Reproduction:** the test binary was built from a PRIVATE isolated
       target dir (`target-r29-1-isolated/`, since cleaned up) so shared
       build-lock contention was structurally eliminated from the loop —
       the same isolation technique R26-1 used. A 2000-run sweep of the
       `production medium-classes` combo (the Large-promotion path,
       `HAS_PROMOTION == true`) using a HYBRID binary that kept the ORIGINAL
       windowed assertion (`released_delta <= reserved_delta`) but added
       failure-path-only diagnostics reproduced **6 failures out of 2000
       (0.30%)**, all with an IDENTICAL trajectory — evidence captured in
       `docs/_raw_r29_1_repro_captured.log` (150 lines, 6 full
       stdout/stderr dumps). The `production medium-classes exact-span-large`
       combo (`HAS_PROMOTION == false`, the medium-ladder path) showed **0
       failures in 600 runs** — the bug is specific to the Large-promotion
       path.
     - **Classification (a) — proven, not inferred.** Every one of the 6
       captured failures shows: (1) the R28-2 per-base leak proof at line 284
       PASSED (it ran before the failing line-319 guard and did not fire) —
       `grown`'s own segment was correctly freed
       (`still_registered=false`, `live_count_before_free=None`,
       `live_count_after_trim_recheck=None`); (2) the GLOBAL cumulative
       invariant held at failure time (`reserved_total=4 >
       released_total=2`) — no double-release. The failing trajectory was
       always: `reserved before=3 after_promote=4 after_free=4 | released
       before=0 after_promote=1 after_free=2` — i.e. the promotion grow
       released `p`'s now-empty OLD segment (reserved during heap/TLS init
       or by the sibling test via the persistent TLS heap binding, BEFORE
       this test's `stats_before` snapshot) INSIDE the window, while only 1
       segment (grown's Large) was reserved INSIDE the window. The windowed
       `released_delta <= reserved_delta` guard's premise ("every in-window
       release has a matching in-window reserve") is INVALID for
       process-wide cumulative counters read over an arbitrary snapshot
       window — a segment reserved before the window can be released inside
       it. This is the SAME mechanism the earlier `TEST_LOCK` fix (item 1
       above) partially addressed (the concurrency race between the two
       test FUNCTIONS) but did NOT fully close: the `TEST_LOCK` serializes
       against the sibling test function's concurrent activity, but NOT
       against segments left in the persistent TLS heap by PRIOR test
       invocations on the same thread, whose later release crosses the
       window. NO `src/` allocator code is implicated — the R28-2 per-base
       proof (the real leak detector) correctly passed in all 6 failures.
     - **Fix applied:** replaced the unsound WINDOWED assertion
       (`released_delta <= reserved_delta` since `stats_before`) with the
       sound GLOBAL cumulative invariant
       (`segments_released_total <= segments_reserved_total`, no windowing)
       — which is window-independent and exactly captures the guard's stated
       intent ("a double-release would indicate corruption": only a genuine
       double-release of the same OS reservation can push global released
       past global reserved). Leak detection was never this counter's job
       (the R28-2 entry's own "Scope" note at lines 121-132 already said so)
       — it is the per-base proof's job, which is reliable and
       segment-specific. The windowed deltas are retained as diagnostic
       CONTEXT only (printed on the failure path, never asserted). Failure-
       path-only diagnostics (zero pass-path cost, so they do not perturb
       the timing of the race they diagnose) were added on every assertion
       path in the test: the trajectory across all three snapshots, plus the
       cfg-gated per-base `still_registered`/`live_count` state, so a future
       CI failure is self-diagnosing from logs alone.
     - **Fix verified:** a 2000-run sweep of the same `production
       medium-classes` combo with the global-invariant fix showed **0
       failures out of 2000** (evidence: `docs/_raw_r29_1_confirm_captured.log`,
       5 lines), against the pre-fix 6/2000. All three CI-relevant feature
       combos compile clean, including the cfg-narrowing-sensitive `hardened
       medium-classes` combo (= `fastbin` + `medium-classes` = `alloc-global
       + alloc-xthread` WITHOUT `alloc-decommit`, where the per-base
       diagnostic block's `#[cfg(all(feature = "alloc-decommit", feature =
       "alloc-xthread"))]` gate correctly compiles out — the same
       R28-2-documented cfg-narrowing gap, re-verified not reintroduced).
     - **Corrected status:** the R28-2 "1 anomalous failure attributed to
       build contention" hypothesis is **REFUTED** — the anomaly is a real
       ~0.3% false-positive rate of an unsound windowed assertion form, now
       fixed. The item is **RESOLVED** on the corrected root cause
       (classification (a), test-logic window-asymmetry bug, fixed and
       verified at 0/2000). This is NOT a still-live allocator correctness
       concern and does NOT block anything.

31. **CI clippy `--all-targets` red on all five rows — pre-existing
   example/test lint+compile errors** — **RESOLVED** by R33-1 (task #506,
   commit `e526517befbf5a0cd0ca1a7ee62f9d84ffe509ee`). Five distinct failures, all pre-existing on `main`
   (four inherited from Round-31 example files, one from Round-32 task
   #502). The brief enumerated only two and prescribed "one line of
   doc-indent + adding the missing `fn main`"; re-running ALL five ci.yml
   clippy rows (as the brief instructed) revealed three further latent
   failures masked by cargo's fail-fast target scheduling — all five were
   necessary for the DONE-WHEN criterion (all five clippy rows green):

   - **E0601** in `examples/r31_10_trim_cost_gate.rs:326` — the example was
     auto-discovered (no `[[example]]` Cargo.toml entry) but gated
     `#![cfg(all(feature = "alloc-global", feature = "alloc-decommit"))]`,
     so under any feature set lacking both, the cfg stripped the entire
     crate body including `fn main`. (The brief's "add the missing
     `fn main`" framing was a misdiagnosis — `fn main` already existed at
     line 313; the root cause is the missing registration.) **Fix:**
     registered it in `Cargo.toml` with
     `required-features = ["alloc-global", "alloc-decommit"]`, mirroring
     its sibling `r31_10_trim_rss_gate` (already correctly registered,
     never failed).
   - **`clippy::doc_lazy_continuation`** in
     `examples/_shared/r31_3_large_cache_extended_narrow_ab_workload.rs:257`
     — a `/// block.` continuation line under a `/// -` list item. **Fix:**
     indented the line 2 spaces (clippy's own suggestion).
   - **E0599** in `examples/r31_8_large_cache_scan_isolation_off.rs:41,43`
     — calls `dbg_large_cache_hits` (`#[cfg(feature = "alloc-decommit")]`,
     `src/alloc_core/alloc_core_large_cache.rs:751`) but its
     `required-features` listed only `["alloc-core"]`. **Fix:** added
     `"alloc-decommit"` to both `r31_8_large_cache_scan_isolation_off` and
     `..._on` (which share the workload via `include!`).
   - **E0432/E0599** in `examples/r31_3_large_cache_extended_narrow_on.rs:39,43`
     — uses `LargeCacheConfig` + `SeferAlloc::with_config` (both
     `alloc-decommit`-gated) but `required-features` listed only
     `["alloc-global"]`. **Fix:** added `"alloc-decommit"`. (The `_off`
     variant uses `SeferAlloc::new()` only and was never affected.)
   - **`clippy::int_plus_one`** in `tests/remote_ring_shadow_head.rs:165`
     (Round-32 task #502, commit `d38bf73`) — `fast_after >= fast_before + 1`
     → `fast_after > fast_before` (semantically identical, clippy's
     suggestion). NOT inherited from Round 31 — the one Round-32-origin
     failure of the five.

   - **Verification:** all five ci.yml clippy rows pass locally with
     `-D warnings` (`cargo clippy --all-targets` for default /
     `--features experimental` / `--all-features` /
     `--features "hardened medium-classes"` / `--features "production"` —
     each verified rc=0); `cargo fmt --all -- --check` clean;
     `cargo test --features production` green. No runtime behavior changed
     (four `Cargo.toml` example registrations + one doc-indent + one
     clippy-suggested test rewrite).
   - **Files changed:** `Cargo.toml` (4 example `required-features`
     registrations), `examples/_shared/r31_3_large_cache_extended_narrow_ab_workload.rs`,
     `tests/remote_ring_shadow_head.rs`, this index entry.
   - **Commit prefix:** `fix(ci)` per the R30-12 taxonomy — no shipping or
     opt-in algorithm code changed, no production default changed; all edits
     are CI-clippy-red fixes (example registrations, a doc-indent, a
     clippy-suggested test rewrite).
   - **Open follow-up kept:** item 11's `npm run check` coverage-gap half
     remains OPEN above (the bugs are fixed but the question of why the
     local gate did not catch them is not).

32. **F10 shadow-head ordering gap — finding F-1**
   (`docs/reviews/2026-08-04-release-stabilization-audit.md`, finding F-1
   [medium]) — **RESOLVED** by R34-6 (task #525). The F10 shadow-head fast
   path in `RemoteFreeRing::full_check`
   (`src/alloc_core/remote_free_ring.rs`) replaced every push's pre-F10
   `head.load(Acquire)` with a `cached_head.load(Relaxed)` on the
   producer's own cache line. The module doc's value-domain proof
   (`cached_head <= head` always, so the fast path can only under-estimate
   occupancy) was correct, but the ordering role the removed load played
   was never addressed: under the abstract memory model, a producer P that
   takes only the fast path carries no happens-before chain to the
   consumer's `slot.store(EMPTY)`, so the consumer's clear and P's
   `slot.store(offset)` into a recycled slot are unordered. NOT a data
   race (both atomic on the same `AtomicU32`) — a potential
   lost-update/liveness defect, confirmed NOT realizable on any hardware
   Rust targets (x86-TSO, ARMv8, RISC-V RVWMO, POWER cumulativity).

   - **Resolution (variant a — promote ordering):** the two `cached_head`
     accesses in `full_check` were promoted from `Relaxed` to
     `Acquire`/`Release`, restoring the exact happens-before edge the
     removed `head.load(Acquire)` supplied, on the same producer-owned
     cache line.
   - **Cost measurement:** byte-for-byte identical assembly on x86-64
     (verified via `objdump` — both `Acquire` load and `Release` store
     compile to the same `mov` as `Relaxed`); wall-clock A/B (5 runs each)
     showed fully overlapping ranges (Relaxed: 5.65–6.10 µs, A/R:
     5.75–6.54 µs; `benches/r34_6_remote_ring_cached_head_ordering_gate.rs`).
   - **Also:** the staleness precondition (~2³² consumer advances) was
     explicitly labeled as an ASSUMPTION (not a theorem) in the module
     doc, per the second independent review's request.
   - **Commit prefix:** `fix(perf)` per the R30-12 taxonomy — shipping
     code changed to close a latent ordering/correctness defect, no
     speedup claimed, no observable behavior change on real hardware.

33. **F-5 release-surviving panic sites vs. "NEVER panics" doc claim**
    (`docs/reviews/2026-08-04-release-stabilization-audit.md`, finding F-5
    [low]) — **RESOLVED** by R34-16 (task #535). The module doc in
    `src/global/sefer_alloc.rs` claimed "Every entry point here returns null
    on failure and NEVER panics," but five release-surviving (not
    `debug_assert!`) invariant checks are reachable from the `GlobalAlloc`
    impl under `production`:
    (1) `alloc_core/alloc_core.rs:2158` `assert!` in
    `realloc_inplace_fast_path_known_base`;
    (2) `alloc_core/alloc_core_large_cache.rs:147` `.expect` in
    `large_cache_slot_take` (base);
    (3) `:160` `.expect` (extension);
    (4) `:166` `unreachable!` (take, extension disabled);
    (5) `:321` `unreachable!` (set, extension disabled).

    - **Resolution (variant b — doc accuracy, no behavior change):** the
      audit could not construct a reachable violation of any of the five
      ("cannot happen" invariant checks), and the codebase's own
      `AllocCore::reclaim_offset` already documents the tradeoff between a
      graceful no-op and a defence-in-depth abort. The five were left as
      release panics deliberately: each guards allocator metadata whose
      silent corruption would be strictly worse than an immediate abort, so
      a future bug that broke one trips loudly at the point of corruption
      instead of continuing with inconsistent state. Softening variants (a)
      were rejected per-site — sites 4 cannot be softened at all
      (`large_cache_slot_take` returns a value `CachedLarge`, no no-op
      return possible; it is `#[cfg(not(large-cache-extended))]`-unreachable
      by construction), and sites 2–3 are take-side `.expect`s whose callers
      prove occupancy via an ARRAY read (`large_cache_slot_get` /
      `oldest_occupied_slot`), NOT the R32-12 bitmask, so a bitmask/array
      desync cannot reach them — softening them would only mask the very
      desync they guard against.
    - **Doc fix:** `sefer_alloc.rs`'s "No-panic" section rewritten to (1)
      keep the accurate failure-path bullets, (2) enumerate the five
      tripwires as "abort by design" defence-in-depth, and (3) state
      explicitly that a panic escaping `GlobalAlloc` aborts via
      `#[rustc_nounwind]` (not UB), independent of any downstream
      `panic = "abort"` setting.
    - **Pinning test:** `tests/no_panic_doc_accuracy.rs` pins the five by
      their distinctive panic-message strings (exactly once each) AND pins
      the doc's qualifying language (`rustc_nounwind`, `invariant tripwire`,
      absence of the old overclaim).
    - **Commit prefix:** `docs(global)` per the R30-12 taxonomy — module-doc
      accuracy fix, no shipping code changed (the only non-doc additions are
      the regression test and this index entry).

34. **F-6 `HeapCore` by-value construction stack-pressure pin**
    (`docs/reviews/2026-08-04-release-stabilization-audit.md`, finding F-6
    [low]) — **RESOLVED** by R34-18 (task #537). `HeapCore` is constructed BY
    VALUE on the frame that triggers a thread's FIRST allocation
    (`HeapRegistry::claim`'s `HeapCore::new(idx) → write(hc)` in both `claim`
    and `claim_with_config`, and the process-global fallback's
    `MaybeUninit<HeapCore>` path in `global/fallback.rs`). Rust does not
    guarantee return-value/move elision, so a debug build (or any backend that
    materialises the temporary) can place one ~7 KiB copy on a small-stack
    thread's first-allocation frame — a realistic stack-overflow risk for
    embedded-class 16–64 KiB stacks. The audit's ~7 KiB figure was INFERRED
    from in-tree `-Zprint-type-sizes` field-offset notes, never measured
    (`size_of::<HeapCore>()` existed nowhere in `src/` or `tests/`).

    - **Resolution:** `size_of::<HeapCore>()` measured directly via a
      compile-error array-length probe under `--features production` (the same
      technique as the `SegmentHeader == 144` pin) = **7576 bytes** (breakdown:
      `core: AllocCore` = 864 B, `tcache: Tcache` = 6664 B — the dominant
      per-class magazine cache — plus `id`/handles ≈ 48 B). A compile-time
      `const _: () = assert!(size_of::<HeapCore>() <= 8192)` pin added in
      `src/registry/heap_core.rs` right after the struct definition, mirroring
      the established `SegmentHeader` pin pattern. Budget = 8192 (8 KiB, half
      of a 16 KiB embedded stack) leaves ~8 % headroom (616 B): minor field
      additions don't trip it, material bloat (a new array/sub-struct, or
      `Tcache` growing another class) fails the build and forces a deliberate
      budget bump. The pin is an unconditional `<=` (not exact `==`): it must
      hold across every feature composition (the struct has `#[cfg]`-gated
      fields; `production` is the maximum at 7576 B, every smaller composition
      is strictly below). A runtime `#[test]`
      (`tests/r34_18_heap_core_stack_pressure_pin.rs`) mirrors the pin and adds
      a non-vacuous lower bound (suspicious-shrink guard). This is the ONLY
      unbounded-growth stack-pressure surface in the tree (no recursion, no
      recursive drop glue, no larger stack buffer than `emptied_bases:
      [*mut u8; 64]` = 512 B, cold path), so this single pin guards the whole
      category.
    - **Point 2 (in-place `HeapCore::new_in_place` initializer) — SKIPPED:**
      evaluated and rejected as not-cheap. The dominant 6664 B component is
      `Tcache::new()`, which itself returns by value; eliminating the 7576 B
      temporary requires cascading in-place init into `Tcache` too (a
      multi-struct refactor across the `registry` + `tcache` modules), changes
      the fallible-`Option` error-handling shape at all three call sites, and
      touches `UnsafeCell` write safety invariants. The compile-time pin is
      the auditor's primary deliverable (F-6's own "Suggested direction") and
      closes the category; the in-place rewrite is not warranted for a [low]
      finding whose risk is already bounded by the pin.
    - **Commit prefix:** `fix(perf)` per the R30-12 taxonomy — structural
      layout pin, no runtime behavior change, no speedup claimed.

15. **G2 — no loom model exercises the F10 fast path over a recycled slot**
    (`docs/reviews/2026-08-04-release-stabilization-audit.md`, finding G2
    [medium]) — **RESOLVED** by R34-19 (task #538). The two existing shadow
    loom models in `tests/loom_remote_ring.rs` neither reached the F-1
    interleaving: `RingModelShadow` (CAP=4) joined producers before draining
    (no wrap → no slot reuse); `RingModelShadow1` (CAP=1) forced the slow
    path exclusively. The one thing F10 actually changed — a producer proving
    room from the shadow alone and reserving a slot the consumer just cleared
    — was modelled by nothing.

    - **Resolution:** added `RingModelShadow2` (CAP=2, post-R34-6
      `Acquire`/`Release` cached_head orderings) and
      `shadow_fast_path_recycled_slot_concurrent_drain_never_loses_or_duplicates`:
      one producer pushes 4 offsets (bounded retry) + one consumer drains
      concurrently, `preemption_bound = 2`. CAP=2 + 4 pushes forces slot
      reuse; the interleaving where the consumer drains between pushes 2
      and 3 makes the producer's push 3 take the slow path (refreshing
      `cached_head`) and push 4 take the FAST PATH into the just-drained
      slot — the exact F-1 shape, reached in 2 preemptions.
    - **Honest limitation (stated in the test's doc comment):** this model
      is a **regression-pin, not an ordering proof**. Loom's store history
      is append-only per atomic, so it cannot surface F-1's modification-
      order freedom; even if the ordering bug were present, loom would
      very likely not detect it. What the model pins: value-domain
      invariants (exactly-once delivery, no overflow into occupied slot,
      no deadlock/panic) hold under slot reuse + concurrent drain. The
      ordering question itself was resolved in R34-6 (item 32 above,
      renumbered from a collision by task #623/M2).
    - **Counterfactual verification (non-vacuity):** replacing
      `full_check`'s body with `Ok(())` (always admit) causes the test to
      FAIL — in the zero-preemption interleaving where all 4 pushes
      execute before any drain, pushes 3+4 overwrite pushes 1+2 in the
      same 2 slots, and offset 10 is reclaimed 0 times despite landing
      (`assertion left == right failed: offset 10 landed but was
      reclaimed 0 times`). Without this check the model could be vacuous
      (R33-3 / task #508 lesson).
    - **Commit prefix:** `test(loom)` — explicitly OUTSIDE the R30-12
      five-slot taxonomy, which governs runtime/opt-in/measurement/docs
      code changes; this is pure verification-coverage addition with zero
      shipping code changed.

35. **F-2 provenance-asymmetry hypothesis — RESOLVED-NEGATIVE**
    (`docs/reviews/2026-08-04-release-stabilization-audit.md`, finding F-2
    [low]; open item 15) — **RESOLVED** by R34-5 (task #524), following the
    item's own decision rule. The item's blocking question was: does the
    concurrent multi-producer SMALL-block `RemoteFreeRing` push/drain path
    (`Node::atomic_u32_at`, backing `head`/`tail`/`cached_head`/`slots`) flag
    under Stacked Borrows the way `Node::atomic_ptr_ref` was fixed for in
    task #142 — the one piece of evidence the repo's tooling could not
    supply until a concurrent small-ring miri test existed (audit G1).

    - **Trigger test added:** R34-5 (task #524, commit `fd54ddc`, plus
      `b47a261`/`91ff1dd` fixing two local miri/tsan wrapper scripts that had
      silently omitted the `internals` feature) added
      `tests/regression_xthread_small_ring_miri.rs`
      (`xthread_small_ring_two_producers_push_owner_drains`): 2 spawned
      producer threads concurrently free small blocks from the SAME segment
      (both CAS-reserving into the same per-segment `RemoteFreeRing`) while
      the owner concurrently allocates, then force-drains via
      `dbg_drain_all_rings` — the exact "≥2 concurrent remote small-block
      ring pushes" shape audit finding G1 said was missing.
    - **Wired into `miri-plain` under plain (Stacked Borrows) miri, not Tree
      Borrows — confirmed by reading, not assumed:** `grep -n MIRIFLAGS
      .github/workflows/ci.yml` shows the `miri-plain` job's `env` block
      (`.github/workflows/ci.yml:860`) sets
      `MIRIFLAGS: "-Zmiri-disable-isolation -Zmiri-preemption-rate=0.5"` —
      no `-Zmiri-tree-borrows` anywhere in that value or job, so this job
      runs under miri's default provenance/aliasing model, Stacked Borrows,
      exactly the model the item's decision rule names as its trigger
      condition. The job's `run:` step (`:900-903`) lists
      `--test regression_xthread_small_ring_miri` alongside the two
      pre-existing large-block plain-miri tests, confirmed via `git show
      fd54ddc --stat` (touches `.github/workflows/ci.yml`,
      `scripts/miri.mjs`, `docs/ARCHITECTURE.md`, and the new test file).
    - **Trigger condition checked independently, not taken on the commit
      message's word:** re-ran the exact test locally —
      `MIRIFLAGS="-Zmiri-disable-isolation -Zmiri-preemption-rate=0.5" cargo
      +nightly miri test --features "alloc-global alloc-xthread internals"
      --test regression_xthread_small_ring_miri` — result: `test result: ok.
      1 passed; 0 failed` (~67s locally), with only the expected/documented
      integer-to-pointer-cast warnings (the same exposed-provenance
      re-derivation warnings the other `miri-plain` tests already produce,
      not errors). This matches task #524's own commit message verification
      log ("1 passed (~49s), only the expected integer-to-pointer-cast
      warnings") and independently confirms it under a fresh local run
      rather than trusting the commit's self-report.
    - **Verdict:** the trigger did NOT fire — the concurrent small-block ring
      test does not flag under Stacked Borrows. Per the item's own decision
      rule ("only if that test flags under Stacked Borrows should the
      `atomic_ptr_ref` treatment be applied to `atomic_u32_at`"), the
      `expose_provenance`/`with_exposed_provenance_mut` treatment is **NOT
      required** for `atomic_u32_at`/`atomic_u64_at`/`atomic_u8_at`. The
      original provenance-asymmetry hypothesis (task-#142's fix applied to
      `atomic_ptr_ref` only, not the ring's other atomic accessors) is
      closed **resolved-negative**: this repo's tooling can now answer the
      question audit finding G1 said was unanswerable, and the answer is
      "no asymmetry-driven miri failure is reachable" — not "asymmetry
      confirmed harmless by inspection alone," a materially stronger claim
      than the item started with.
    - **Scope of what this resolution does and does not prove:** miri's
      Stacked Borrows model not flagging a 2-producer/1-consumer
      interleaving over ~49-67s of runtime is evidence the asymmetry is not
      an easily-triggered UB source under that model and that workload
      shape — it is not an exhaustive proof over all interleavings/thread
      counts (miri explores the interleavings its scheduler happens to
      generate, not all of them) and says nothing about Tree Borrows (which
      the item's own text already argued is structurally immune here via
      `Cell` permission on raw-pointer-derived `&AtomicU32`, a separate,
      independent argument this resolution does not depend on).
    - **Files changed (doc-only):** this index entry (the corresponding open
      item, now item 35 above after task #623/M2's collision renumbering,
      was replaced with a one-line "Recently resolved" pointer per
      CLAUDE.md's R34-24 current-state-card structural rule; this closure
      narrative added here). No source, test, or CI file changed by this
      task — #524 already landed the test and CI wiring in a prior commit.
    - **Commit prefix:** `docs` — pure documentation update (closing a
      stale open-item card to reflect an already-landed, already-verified
      resolution); no shipping or opt-in code changed, no measurement run
      newly performed that a report's verdict rests on (the miri re-run
      here reproduces #524's own already-published result, it does not
      establish a new one).

36. **H8 — `dbb4016`'s `fix(perf):` prefix considered for a reword to
    `feat(api):`, DECIDED against a rebase, prefix left as-is** (task #578,
    `docs/reviews/2026-08-05-sol-remediation-readonly-review.md` finding H8)
    — **RESOLVED, no code change.** Sol-F1's commit (`9296adb`, post-G1-
    rebase SHA `dbb4016`, "AllocCore::dbg_* inherent methods now genuinely
    require `internals`") used `fix(perf):`. The review flagged this as
    inapt for a pure visibility/cfg-gating change (no algorithm changed,
    only which callers can reach existing code) and pointed to the
    identical-class predecessor `27879af` (R34-3, gating the module PATHS
    behind `internals`), which used `feat(api):` — arguably the closer
    match, since CLAUDE.md's R30-12 taxonomy has no dedicated slot for
    "API-surface visibility change."
    - **Decision:** left `dbb4016` as-is — an accepted historical
      imprecision, not reworded. Two considered options were (a) a small
      rebase to reword just `dbb4016`, or (b) accept the existing prefix
      and use correct judgment for any NEW commits in the same class. (b)
      was chosen per the task's own explicit default guidance ("default to
      (b) unless a rebase is already happening anyway for some other
      reason in this batch") — no other rebase was in flight this round,
      and this is the exact non-retroactive posture CLAUDE.md's own R30-12
      section already states for this rule ("no historical commit message
      is retagged or amended by this rule; it governs new commits going
      forward only" — the same posture the raw-log-truncation and
      immutable-source-identity rules elsewhere in CLAUDE.md also take).
      H2 (task #572), the directly-analogous follow-up commit extending
      this exact same gating work to 6 more files, independently used
      `fix(perf):` as well (`25d6ac4d23b4859b726724424e5912dc54fe0bf0`) and
      passed `verify-commit-prefixes.mjs` — establishing `fix(perf):` as
      the now-repeated, lint-accepted precedent for "narrow an existing
      diagnostic hook's reachability without changing its behavior,"
      rather than treating `dbb4016` as an isolated one-off mistake to
      correct. A rebase deep enough to reword `dbb4016` would also need to
      touch every commit stacked on top of it since (including H2, H3, H4,
      H5, H7 above) — disproportionate risk for a P4 wording nit, per the
      same cost/benefit reasoning G1's rebase (task #555) already weighed
      once this session for a higher-severity (P2) case.
    - **Files changed:** none (this index entry only) — a documented
      decision, not a rebase or a reword.

37. **Flaky test — `repeated_same_segment_frees_are_observed_as_tier1_hits`**
    (`tests/segment_table_contains_base_tier1_counters.rs`) — **RESOLVED**
    by wave 3's own `npm run check --all-features` gate run (2026-08-05,
    same session as H1-H8, tasks #571-578).

    - **Root cause, confirmed:** `CONTAINS_BASE_TIER1_HITS`/
      `CONTAINS_BASE_TIER1_MISSES` (`src/alloc_core/alloc_core.rs`) are
      process-wide `static AtomicU64`s. Both `#[test]` functions in this
      file read them via a before/after delta; `cargo test` runs the two
      tests in this file in parallel by default, so the OTHER test's
      `contains_base`/`dbg_hash_contains_only` traffic could land inside
      one test's own delta window — exactly the SAME failure class as item
      1 above (`canary_survives_promotion_and_free_leaves_no_leak`), a
      different process-wide counter pair, same root cause. Observed
      failure: `hits_delta=31 misses_delta=2` against an expected `N=32`
      for `repeated_same_segment_frees_are_observed_as_tier1_hits`.
      Confirmed as a parallelism artifact, not a real regression: passed
      clean under `cargo test --test
      segment_table_contains_base_tier1_counters --all-features --
      --test-threads=1`; confirmed the file predates this session
      (`git log -- tests/segment_table_contains_base_tier1_counters.rs`
      last touched by Round 34's `7aeee2d`, an unrelated rustfmt-drift
      commit) — this is a pre-existing flake this wave's own full-matrix
      run happened to surface, not something wave 1/2/3's own changes
      introduced.
    - **Fix:** added the SAME established `static TEST_LOCK: Mutex<()>` +
      per-test `let _guard = TEST_LOCK.lock().unwrap();` pattern item 1
      above already used (also matching
      `tests/directory_authoritative_miss.rs`,
      `tests/alloc_zeroed_fresh_large_skip.rs`,
      `tests/r13_3_magazine_virgin_hit_skips_zero.rs`,
      `tests/r21_2_opt_h_stage1_precondition_probe.rs`). No assertion logic
      changed.
    - **Verification:** 5 full `cargo test --test
      segment_table_contains_base_tier1_counters --all-features` reruns
      (default multi-threaded scheduling) after the fix — all clean, 0
      failures. `cargo fmt --all -- --check` clean.
    - **Files changed:** `tests/segment_table_contains_base_tier1_counters.rs`
      (serialization only); this index entry.

38. **Flaky test — `ac1_trim_empties_pool_and_evicts_large_cache`**
    (`tests/r31_10_trim_current_thread_api.rs`) — **RESOLVED** by wave 4's
    own post-landing `npm run check --all-features` gate run (2026-08-05,
    same session as I1-I10, tasks #579-588; found in a background rerun
    launched after `782b92e` landed, task #589).

    - **Root cause, confirmed:** `segments_released_total`
      (`SeferAlloc::stats()`) is a process-wide counter shared across every
      `SeferAlloc`/thread in the process. `cargo test` runs the six
      `#[test]` functions in this file in parallel by default; every one of
      them calls `trim_current_thread()` at least once (which can release a
      cached span and bump this counter), while
      `ac1_trim_empties_pool_and_evicts_large_cache` and
      `ac3_trim_does_not_affect_other_thread_heap` each read a before/after
      delta on it — the SAME failure class as items 1 and 25 above, a
      different process-wide counter, same root cause. Observed failure:
      `released_before=1, released_after_cache=2` — a sibling test's
      `trim_current_thread()` call landed in the narrow window between
      `ac1`'s two counter reads. Confirmed the file predates this session
      (`git log -- tests/r31_10_trim_current_thread_api.rs` last touched by
      Round 34's `7aeee2d`, an unrelated rustfmt-drift commit; the test
      itself dates to Round 31, task #474) — a pre-existing flake this
      wave's own full-matrix rerun happened to surface, not a regression
      from I1/F1 (the `HeapCore` stack-pressure budget change) or I5/F5
      (gating `SeferAlloc::dbg_trim_current_thread`), neither of which
      touches `segments_released_total` accounting or this test's own
      logic.
    - **Fix:** added the SAME established `static TEST_LOCK: Mutex<()>` +
      per-test `let _guard = TEST_LOCK.lock().unwrap();` pattern items 1
      and 25 above already used. Applied to ALL SIX tests in the file, not
      just the two that read the delta — every other test's
      `trim_current_thread()` call is itself a source of interference for
      those two. No assertion logic changed.
    - **Verification:** 5 full `cargo test --all-features --test
      r31_10_trim_current_thread_api` reruns (default multi-threaded
      scheduling) after the fix — all clean, 0 failures. Also verified
      clean under `cargo test --features "production internals" --test
      r31_10_trim_current_thread_api`. `cargo fmt --check` clean on the
      changed file.
    - **Files changed:** `tests/r31_10_trim_current_thread_api.rs`
      (serialization only); this index entry.

39. **Flaky test — `oom_injection_flag_is_clean_after_test`**
    (`tests/regression_free_path_chunk_oom_graceful.rs`) — **RESOLVED**
    by the first full remote CI run over the pushed backlog (2026-08-05,
    CI run `31045983765` on landing SHA `42d4206`, task #621, found during
    the map-verification pass of this session's release-readiness work).

    - **Root cause, confirmed:** `DBG_INJECT_CHUNK_OOM` is a process-wide
      `internals`-gated `AtomicBool`. This file has two `#[test]` fns whose
      correctness relies on sequential execution — the module doc says so
      explicitly (`oom_injection_flag_is_clean_after_test` is designed to
      run AFTER `chunk_oom_on_free_path_returns_gracefully_not_abort`,
      verifying its `OomInjectionGuard` cleared the flag on drop) — but
      `cargo test` runs the two in parallel by default with nothing
      serializing them. A race window between
      `dbg_set_inject_chunk_oom(true)` in the main test and the guard's
      `Drop` clearing it back to `false` let the second test observe the
      flag stuck `true`. Same failure class as items 1/25/26 above
      (process-wide diagnostic flag/counter, multiple tests in one file,
      no serialization), a different flag, third recurrence this session.
    - **Fix:** added the SAME established `static TEST_LOCK: Mutex<()>` +
      per-test `let _lock_guard = TEST_LOCK.lock().unwrap();` pattern
      items 1/25/26 already used (renamed to `_lock_guard` in this file
      specifically to avoid shadowing the pre-existing `let _guard =
      OomInjectionGuard;` binding in the main test — both guards are held
      simultaneously for correctness, shadowing would only have been
      confusing, not incorrect, since Rust drops shadowed bindings at
      scope end in reverse declaration order). No assertion logic changed.
    - **Verification:** 5 full `cargo test --features "production
      alloc-stats bench-internals internals" --test
      regression_free_path_chunk_oom_graceful` reruns (default
      multi-threaded scheduling) after the fix — all clean, 0 failures.
      `cargo fmt --check` clean.
    - **Files changed:** `tests/regression_free_path_chunk_oom_graceful.rs`
      (serialization only); this index entry.

40. **CI-coverage gap — `cargo test -p racy-ptr-cell` ran in ZERO CI
    configurations** (`.github/workflows/ci.yml`'s `test-workspace` job)
    — **FLAGGED AND RESOLVED IN THE SAME ROUND** (2026-08-09, task #774
    filing this entry per this file's own "file in the same commit that
    flags it" rule; found by the racy-ptr-cell round-closing review,
    `docs/reviews/2026-08-09-racy-ptr-cell-round-closing-review.md` §F1;
    closed by task #773 immediately prior in the same round, commit
    `a5e8e42`).

    - **Root cause, confirmed:** the crate's only two prior CI
      invocations were `cargo build -p racy-ptr-cell --no-default-features
      --target thumbv7em-none-eabi` (compiles no test target at all) and
      `cargo test --release -p racy-ptr-cell --test loom_racy_ptr_cell`
      under `RUSTFLAGS="--cfg loom"` (excludes `tests/cell_unit.rs` twice
      over — by `--test` target selection and by the file's own
      `#![cfg(not(loom))]` gate). All 7 of `cell_unit.rs`'s tests,
      including 4 added by the racy-ptr-cell `rust-intel` remediation
      round (tasks #700/#706-710) — `panicking_init_rolls_back_and_subsequent_call_succeeds`,
      `init_returning_the_sentinel_address_panics`,
      `align_of_one_payload_panics_at_construction`,
      `dbg_rollback_reenterable_happy_path_and_not_applicable_arm` — had
      never executed in CI. Same gap class as the identical
      `tagged-index-stack` gap task #639/#772 (F4/F5) closed one round
      earlier.
    - **Fix:** added `cargo test -p racy-ptr-cell --no-fail-fast` and
      `cargo test -p racy-ptr-cell --release --no-fail-fast` to
      `test-workspace`, next to the existing bare-metal build. The
      `--release` step specifically matters: `init_returning_the_sentinel_address_panics`
      regression-guards an `assert!` promoted from `debug_assert!` (task
      #707) — a debug-only run would stay green even if that promotion
      were silently reverted.
    - **Verification:** counterfactual — reverted the `assert!` to
      `debug_assert!` locally, confirmed the new debug step stays green
      (expected: `debug_assert!` still fires in debug) and the new
      `--release` step fails with the expected panic-message mismatch;
      reverted cleanly (`git diff` empty on `src/lib.rs`).
    - **Related, also resolved same round (not a separate item):** the
      round-closing review's F5 also flagged `CHANGELOG.md`'s "`#709` is
      **Not miri-verified**" caveat as unfiled; task #774 closed the
      underlying gap for `tests/cell_unit.rs` directly (applied `#709`'s
      own `expose_provenance`/`with_exposed_provenance_mut` fix to an
      identical provenance-losing pattern the same round's `#706`
      introduced there — see finding F2 in the same review) rather than
      filing it as a standing open item. `#709`'s own fix, in
      `tests/loom_racy_ptr_cell.rs`, remains structurally unverifiable by
      miri (loom's green-thread scheduling simulation is incompatible with
      miri's execution model) — this is an intrinsic tooling limitation,
      not an open action item.
    - **Files changed:** `.github/workflows/ci.yml` (task #773); this
      index entry.

41. **SyncRegion one-shot convenience methods missing reentrancy cross-references**
   (`crates/sefer-region/src/sync_region.rs`, methods `clear` and `get_cloned`) — **RESOLVED**
   by the release-prep review's finding F5 closure (2026-08-09). The round-2 closing review
   (`docs/reviews/2026-08-08-sefer-region-round2-closing-review.md`, finding F) flagged that
   of the seven one-shot convenience methods (`insert`, `remove`, `contains`, `len`,
   `is_empty`, `clear`, `get_cloned`), only `remove` explicitly cross-references the type-level
   `## Reentrancy` section. This is a documentation gap: `clear` runs every `T::Drop` under
   the write lock, and `get_cloned` runs `T::clone` under the read lock — the two methods that
   actually execute user code under the lock — yet neither points to the deadlock hazard section.

   - **Root cause, confirmed:** the type-level `## Reentrancy` section at
     `crates/sefer-region/src/sync_region.rs:45-58` does document the hazard thoroughly
     (it explicitly names `clear` and `get_cloned` on lines 47-48), but the per-method
     rustdoc pages for those two methods have no inline reference to it. A reader who
     arrives at `SyncRegion::clear`'s or `SyncRegion::get_cloned`'s rustdoc via a search
     engine sees no deadlock warning.
   - **Fix:** added `see the [reentrancy section](Self#reentrancy)` links to the doc
     comments for both `SyncRegion::clear` and `SyncRegion::get_cloned`, mirroring the
     existing pattern in `SyncRegion::remove`'s own doc.
   - **Verification:** `cargo doc -p sefer-region --all-features --no-deps` generates
     without broken-intra-doc-link warnings; the new links resolve correctly on the
     rendered docs.
   - **Files changed:** `crates/sefer-region/src/sync_region.rs`, `docs/CORRECTNESS_OPEN_ITEMS.md`.
    only; the BSD half of item 43 stays OPEN in the main index above — no
    BSD CI runner exists for this crate).

    - **Confirmation:** commit `1dbd6b4` was pushed and CI run `31692217669`
      (job `94421845398`, `test macos (production)`, image
      `macos-26-arm64`) ran green, executing
      `apple_silicon_page_size_is_16_kib` (`crates/aligned-vmem/tests/smoke.rs`) —
      `ok`. `page_size()` returned exactly `16384` on real aarch64 Darwin
      hardware, confirming the `_SC_PAGESIZE = 29` cfg-table entry
      (`crates/aligned-vmem/src/lib.rs` at closure time — `crates/aligned-vmem/src/os/unix.rs` since task #1055's split, per post-split note task #1082; task #714) is the correct constant for the
      macOS family, not merely a value that compiles.
    - **Task/round:** task #888 (round 7, finding T1 of
      `docs/reviews/2026-08-13-aligned-vmem-round7-review.md`), following
      the exact action item's own "Next trigger" text (filed round-closing
      review, task #776/F13): "if `apple_silicon_page_size_is_16_kib`
      passes, move the macOS half of this item to 'Recently resolved' with
      the run's citation."
    - **Remaining scope:** FreeBSD, NetBSD/OpenBSD, and DragonFly remain
      unverified on real hardware — tracked as the BSD half of item 43 in
      the main index above, unchanged by this closure.

44. **`aligned-vmem`'s hand-written `mmap` FFI declaration has an ABI shape mismatch risk on 32-bit Unix targets — the `offset` parameter was hardcoded as `i64`, assuming a 64-bit POSIX `off_t`, which is not guaranteed on 32-bit Unix platforms (e.g. glibc i686 and traditional 32-bit ARM default to a 32-bit off_t without `_FILE_OFFSET_BITS=64`).** — **CLOSED** (task #914, correcting H2C1 docs half of `docs/reviews/2026-08-13-aligned-vmem-round10-closing-review.md`).

    The independent review (task #911, finding M2) correctly identified this as a calling-convention/shape compatibility problem, not just a value-range issue — a wrong-width trailing parameter can misalign subsequent stack/register argument-passing on some 32-bit calling conventions (e.g. ARM EABI requires 8-byte alignment for 64-bit parameters, which can insert padding a 32-bit-off_t callee doesn't expect, causing it to read padding bytes as the offset value — garbage, not the intended zero).

    Task #911's initial fix used a `compile_error!` that rejected ALL `unix + 32-bit-pointer-width` combinations. The round-10 closing review (finding H2C1) identified this as a publish-blocking regression: it broke Tier 1 `i686-unknown-linux-gnu` and Tier 2 `armv7-unknown-linux-gnueabihf`, both of which have a 32-bit off_t and were previously supported. Task #914 corrected this before 0.2.0 shipped.

    - **Final fix (task #914):** replaced the `compile_error!` with a per-target `OffT` type alias that correctly types the `mmap` FFI's `offset` parameter per-target:
      - `OffT = i32` for 32-bit Linux/Android (matching their actual default 32-bit off_t width)
      - `OffT = i64` for everything else, including 32-bit BSD/Darwin (which have a 64-bit off_t natively)

      The `mmap` extern's `offset` parameter is now typed as `OffT` instead of `i64`. This allows `i686-unknown-linux-gnu` and `armv7-unknown-linux-gnueabihf` to build correctly again with the CORRECT ABI, rather than being rejected outright. The original `compile_error!` (task #911) was removed entirely, not narrowed.
    - **Evidence:** task #914 commit, per-target `OffT` type alias definitions and the `mmap`/`munmap`/`madvise`/`sysconf` extern block in `crates/aligned-vmem/src/lib.rs` (at fix time; `crates/aligned-vmem/src/os/unix.rs` since task #1055's split — post-split note task #1082), plus verification on current targets. Historical note: the original compile_error!-based fix (task #911) was caught as a publish-blocking regression by the round-10 closing review (H2C1) and corrected in the same round (task #914) before 0.2.0 shipped.

45. **`aligned-vmem` Linux HugeTLB path leaks entire pinned huge-page mapping when system's default huge-page size is not 2 MiB.** — **CLOSED** (task #909, finding H1 of `docs/reviews/2026-08-13-aligned-vmem-independent-review.md`).

    Before this fix, `libc_mmap` (`crates/aligned-vmem/src/lib.rs`, `#[cfg(all(unix, not(miri)))]` — both pre-split locators; `crates/aligned-vmem/src/os/unix.rs` since task #1055's split, post-split note task #1082) requested huge pages via plain `MAP_HUGETLB` with NO size-encoding flag (no `MAP_HUGE_2MB`/`MAP_HUGE_1GB` etc.). On Linux, that means the kernel uses the SYSTEM'S CONFIGURED DEFAULT huge-page size — which is set via the `default_hugepagesz=` kernel boot parameter and can be 1 GiB on HPC/database-tuned hosts, NOT universally 2 MiB. But `LINUX_HUGE_PAGE_SIZE` was hard-coded to 2 MiB, and `unix_reserve`'s guard validated callers' `size`/`align` against that 2 MiB constant before allowing a huge-page reservation attempt. `try_reserve_aligned_exact` then returned `reservation_len = size` (the caller's REQUESTED size) as if that's the real OS reservation length — but if the kernel actually used a 1 GiB huge page (because that's the configured default), the true mapping is 1 GiB, not 2 MiB. Later, `release_reservation` called `munmap(reservation, reservation_len)` using that wrong, too-small length. Linux's HugeTLB `munmap` requires the length argument to be a multiple of the ACTUAL underlying huge-page size the mapping used. A 2 MiB length is not a multiple of 1 GiB, so this munmap call failed with EINVAL — and `libc_munmap` silently discards the return value by design. Result: the entire 1 GiB virtual-memory mapping AND its pinned physical huge page leaked permanently and were never released. Repeating this exhausts the system's HugeTLB pool. This finding corrects/falsifies the prior "durable premise" established by task #714 and reconfirmed by later reviews (`docs/reviews/2026-08-07-aligned-vmem-rust-intel-audit.md` and `docs/reviews/2026-08-09-aligned-vmem-round-closing-review.md`) that "the default is always 2 MiB on mainstream x86_64/aarch64 Linux" — that premise was wrong, because `default_hugepagesz=` is independently configurable per-host regardless of architecture.

    - **Fix:** added `MAP_HUGE_2MB` constant (`21 << 26 = 0x54000000`, taken from Linux kernel `include/uapi/linux/mman.h`), OR'd it into the `mmap` flags alongside `MAP_HUGETLB`, and updated `LINUX_HUGE_PAGE_SIZE`'s doc to state that the crate NOW explicitly pins the huge-page request to 2 MiB rather than relying on — and being wrong about — the system default being "always 2 MiB." This makes the fix fail-closed: if 2 MiB huge pages specifically aren't configured/available on the host, the mmap call fails cleanly (returns null → the crate's normal OOM/error path), rather than silently succeeding with a mismatched size and leaking on munmap.
    - **Trade-off (functional narrowing):** before this fix, `reserve_aligned_huge(1 GiB, 1 GiB)` on a `default_hugepagesz=1G` host with a provisioned 1 GiB pool worked correctly — the crate got a genuine 1 GiB huge page (`is_huge() == true`), no leak. With the explicit `MAP_HUGE_2MB` request, the crate can no longer obtain huge pages of any size OTHER than 2 MiB, on any host, ever. This is a fail-closed functional narrowing (leak prevention wins, but the general "any huge-page size" case is gone unless a future round queries `/proc/meminfo`'s `Hugepagesize:` or records the kernel-rounded length instead of assuming 2 MiB).
    - **Kernel-version caveat:** the `MAP_HUGE_*` size encoding (`MAP_HUGE_SHIFT`/`MAP_HUGE_2MB` etc.) was introduced in Linux 3.8 (2013); on an older kernel the bits above `MAP_HUGE_SHIFT` are not interpreted by the `MAP_HUGETLB` path, silently reverting to "use the system default huge-page size" — i.e. H1's exact bug returns, with no diagnostic, on a pre-3.8 kernel. Practically irrelevant for a 2026 crate, but recorded for completeness.
    - **Evidence:** task #909 (this task), finding H1 of `docs/reviews/2026-08-13-aligned-vmem-independent-review.md`. Historical context: the falsified "default is always 2 MiB" premise was established by task #714 and reconfirmed by `docs/reviews/2026-08-07-aligned-vmem-rust-intel-audit.md` and `docs/reviews/2026-08-09-aligned-vmem-round-closing-review.md` — this item records the correction.

56. **[T, LOW] `scripts/vmem-doc-drift-guard.mjs` false-positives on `from_raw_parts`'s "insufficient whenever the reservation was over-reserved" sentence** — **CLOSED** (2026-08-16, personal follow-up during round-3 close-out, per this item's own "Next trigger").

    The guard's `SCOPE` regex used `\bwhen\b`, which requires a word boundary immediately after "when" — so it does not match "whenever" (no boundary exists between "n" and "e" mid-word). `from_raw_parts`'s rustdoc uses "insufficient whenever the reservation was over-reserved", a correctly-conditional sentence in plain English, but the literal regex missed it and the guard convicted it as an unconditional drift.

    - **Fix:** widened the regex from `\bwhen\b` to `\bwhen(ever)?\b` in `scripts/vmem-doc-drift-guard.mjs`'s `SCOPE` alternation — exactly the one-word scope-list addition this item's own "Next trigger" specified, no `lib.rs` prose touched.
    - **Verification:** `node scripts/vmem-doc-drift-guard.mjs` now exits 0 (`OK: no unconditional over-reserve/trim statements found`); `HARD_FAIL` (`unconditional`) is untouched, so a genuinely unconditional sentence containing "whenever" would still be convicted — the widening only affects the SCOPE alternative, not the hard-fail path. `npm run check`'s `vmem-doc-drift-guard` step, previously the sole failing step, now passes.
    - **Note for whoever re-cites this item:** the two sentences' line numbers had already drifted from this item's own citation (`lib.rs:1017,1042` at filing time, `:1032,:1057` after round 3's doc edits moved them) before this closure — recorded here as a live instance of the exact staleness class this campaign's own conventions exist to catch, caught only because the guard itself re-ran and re-reported current line numbers rather than the citation being trusted at face value.
    - **Evidence:** `scripts/vmem-doc-drift-guard.mjs`'s `SCOPE` regex (the one-line diff); `crates/aligned-vmem/src/lib.rs` (the two `from_raw_parts` sentences, unchanged; post-split note task #1082: they now live in `crates/aligned-vmem/src/reservation_parts.rs`, still unchanged in wording); `docs/CORRECTNESS_OPEN_ITEMS.md` (this closure).

42a. **`aligned-vmem`'s `mock` Cargo-feature-unification hazard (item 42's aligned-vmem half)** — **CLOSED** (2026-08-16, task #962, per the maintainer decision recorded in this session: "делаем 2" — convert, do not just document the risk).

    Cargo unifies features across a build's WHOLE dependency graph, not per edge: `mock` REPLACES the real syscall backend with a thread-local recording stub, so if ANY crate anywhere downstream of a build (including a sibling workspace member's own `[dev-dependencies]`) enabled `aligned-vmem/mock`, every OTHER consumer in that SAME build would silently get the mock backend too — no compile error, no warning. First flagged task #715 (rust-intel audit MEDIUM §C10), deferred at the time with an explicit "free only until 0.2.0's first publish" deadline (task #658) recorded in `Cargo.toml`'s own feature comment. That deadline arrived with 0.2.0 queued for publish (this item moved to `[A]`/URGENT, task #934/C-9, for exactly that reason).

    - **Fix:** converted `mock` from a Cargo feature to the build-time `--cfg aligned_vmem_mock` flag (enabled via `RUSTFLAGS="--cfg aligned_vmem_mock"`), matching this repo's own `cfg(loom)`/`cfg(kani)` precedent — cfg flags are passed only explicitly per build invocation and do not unify across the dependency graph, closing the hazard structurally rather than by documentation/convention alone. Confirmed before converting that the hazard had zero present cost (grep across the whole workspace: `aligned-vmem/mock` was not actually enabled anywhere), so the conversion carried no breaking-change cost despite touching 13 files (`crates/aligned-vmem/{Cargo.toml,README.md,src/{lib,mock,fault_injection}.rs,tests/{mock,mock_reentrancy,fault_injection,lazy_commit,smoke}.rs}`, `.github/workflows/ci.yml`, root `Cargo.toml`, `crates/numa-shim/Cargo.toml` — the last two comment-only cross-reference updates).
    - **CI:** every step that previously relied on `--all-features` to reach mock coverage (which it can no longer do — mock is not a Cargo feature) got a dedicated `RUSTFLAGS: "--cfg aligned_vmem_mock"` step instead: `aligned-vmem-gates` (clippy + test, plus the `--cfg miri` row now also carrying `--cfg aligned_vmem_mock` to preserve its previously-incidental mock+miri type coverage), `test-windows`, `test-macos`, `test-workspace`. The feature-powerset job's comment corrected (5 features now, not 6 — mock sits entirely outside cargo-hack's feature space).
    - **Verification:** a clean `cargo build -p aligned-vmem` produces zero `unexpected_cfgs` warnings (the new `[lints.rust] unexpected_cfgs` check-cfg declaration in `crates/aligned-vmem/Cargo.toml` covers the flag); `cargo test` WITH vs WITHOUT `RUSTFLAGS="--cfg aligned_vmem_mock"` flips `mock.rs` (0↔12), `mock_reentrancy.rs` (0↔2), `fault_injection.rs` (5↔0), and `lazy_commit.rs` (12↔11) exactly as intended; clippy clean on every arm; `cargo doc --all-features --no-deps` clean; `cargo test -p numa-shim --features mock` still green (28/28, confirming numa-shim's own SEPARATE `mock` feature — deliberately left unconverted, see item 42's remaining card above — was not disturbed); `ci.yml` re-parses as valid YAML; the full `npm run check` workspace gate is ALL GREEN.
    - **Not closed by this task:** `numa-shim`'s own `mock` Cargo feature carries the identical hazard and remains unconverted, deliberately — its own first publish (task #657) has not happened yet, so its "free to convert" window has not closed. See item 42's remaining card (renumbered 42, scope narrowed) for that half.
    - **Evidence:** commit (task #962, this session); `crates/aligned-vmem/Cargo.toml`'s `[lints.rust] unexpected_cfgs` declaration and the removed `mock = []`; `.github/workflows/ci.yml`'s new `RUSTFLAGS: "--cfg aligned_vmem_mock"` steps.

57. **`scripts/bench-table.mjs` has been unable to build `benches/global_alloc.rs` since task #583, ~11 days before discovery.** — **CLOSED** (2026-08-16, found and fixed by a user report of `npm run bench:table` failing).

    `scripts/bench-table.mjs` hardcodes `FEATURES = 'production'` (set when the script was created, commit `73a6b2b`, 2026-07-07). Task #583 (commit `7a9b7c7`, 2026-08-05, "fix(perf): close F5 — gate `SeferAlloc::dbg_trim_current_thread` behind `internals`") added `internals`/`bench-internals` to `benches/global_alloc.rs`'s own `[[bench]] required-features` in the root `Cargo.toml` (the bench file calls `AllocCore::dbg_layout_class_for` and `SeferAlloc::dbg_trim_current_thread`, both gated behind those features), but never updated `bench-table.mjs`'s `FEATURES` constant to match. `production` (the workspace's default-bundle feature alias) does not and should not include `internals`/`bench-internals` — those are diagnostic-only, opt-in surfaces, correctly excluded from the shipping default. Confirmed pre-existing and unrelated to any session's own work: reproduced the identical error (`error: target 'global_alloc' ... requires the features: 'alloc-global', 'internals', 'bench-internals'`) against a detached worktree at `d1de3bc`, the base commit for an entire multi-round campaign that had no reason to touch this script.

    - **Fix:** `scripts/bench-table.mjs`'s `FEATURES` constant changed from `'production'` to `'production internals bench-internals'`. Both added features are measurement-only surface gates (`internals = []` in `Cargo.toml`, zero runtime-behavior change) — adding them does not change what the bench measures, only what diagnostic API the bench file is allowed to call, matching the already-documented idiom `cargo test --features "production internals"` / `"production bench-internals"` used elsewhere in this repo (`Cargo.toml`'s own `internals` feature comment, `benches/perf_gate_iai.rs`).
    - **Verification:** `cargo bench --features "production internals bench-internals" --bench global_alloc --no-run` compiles cleanly; `npm run bench:table` runs to completion end-to-end (`[bench-table] PASS — 159 bench id(s) parsed, all 51 expected ids present`).
    - **Why this stayed undiscovered for ~11 days:** `npm run bench:table` is not part of `npm run check` (the mandatory pre-push gate) or CI — it is a manually-invoked reporting script, run only when a human explicitly asks for comparative wall-clock numbers. Nobody happened to run it between task #583 landing and this report.
    - **Evidence:** `git log -S'const FEATURES' -- scripts/bench-table.mjs` (created `73a6b2b`, last touched for unrelated label/guard reasons at `0e29fc2`/`b23d7c5`); `git log -1 -S'"internals", "bench-internals"' -- Cargo.toml` → `7a9b7c7`; reproduction against a `d1de3bc` detached worktree.

49. **`aligned-vmem` has ten FFI call sites relying on the edition-2021 implicit `unsafe fn` body instead of an explicit `unsafe {}` block with its own `// SAFETY:` comment — none unsound today, but edition 2024 makes `unsafe_op_in_unsafe_fn` a hard error at all ten.** — **CLOSED** (task #997, P3-8 pass 2 of the 0.2.0 pre-release audit/closing-review campaign).

    Fully resolved across two passes of the same campaign. Pass 1 (P3-8): `libc_munmap` and miri's `release_reservation` wrapped in explicit `unsafe {}` with per-operation `// SAFETY:` comments; the third original site, `libc_madvise_hugepage`, was deleted entirely by a separate finding (II-5) that removed the whole function (the sole remaining `MADV_HUGEPAGE` call was ineffective) — a deleted function has no migration debt. Pass 2 (closing-review, task #997): the remaining six sites this item's own measurement command names were wrapped the same way — the Windows `release_reservation`'s `winapi_virtual_release` call (see `fn release_reservation`), `winapi_virtual_reserve`'s `VirtualAlloc` call (see `fn winapi_virtual_reserve`), `winapi_virtual_decommit`'s `VirtualFree` call (see `fn winapi_virtual_decommit`), `winapi_virtual_release`'s own `VirtualFree` call (see `fn winapi_virtual_release`), the Unix `release_reservation`'s `libc_munmap` call (see `fn release_reservation`), and `libc_madvise`'s `madvise` call (see `fn libc_madvise`). Also fixed two NEW sites introduced by a different round after this item was first filed: `benches/vmem_bench.rs`'s `fault_pages` helper (`base.add(offset)` and `ptr::write_volatile(...)`, added by task #985/II-19), which had its own `unsafe_op_in_unsafe_fn` debt from day one. Re-measured with this item's own prescribed command (`RUSTFLAGS="-W unsafe_op_in_unsafe_fn" cargo clippy -p aligned-vmem --all-targets --features "lazy-commit huge-pages fault-injection bench-internals"`, both on the default target and `--target x86_64-unknown-linux-gnu` for the Unix-only sites) — zero warnings on both, confirmed independently before closing this card.

    - **Evidence:** `docs/reviews/2026-08-16-aligned-vmem-fxx-prerelease-audit.md` findings P3-8 (pass 1 + pass 2 fixes) and II-5 (the `libc_madvise_hugepage` removal); direct fixes in `crates/aligned-vmem/src/lib.rs` at the six call sites cited above and `crates/aligned-vmem/benches/vmem_bench.rs`'s `fault_pages` (post-split note task #1082: those call sites now live in `crates/aligned-vmem/src/os/*.rs` after task #1055's module split).

52. **[T, INFO] `decommit_lazy` leaves free BSD reclaim on the table** — **CLOSED** (filed 2026-08-14, task #934/C-9, from `docs/reviews/2026-08-14-aligned-vmem-pre-release-review.md` finding V-4).

    Resolved by the `madv_free_advice()` function now having BSD-specific arms (FreeBSD/DragonFly: `MADV_FREE_BSD_5`; NetBSD/OpenBSD: `MADV_FREE_BSD_6`) instead of falling back to `MADV_DONTNEED`. The function now dispatches to the real `MADV_FREE` constants for all four BSDs, making `decommit_lazy` genuinely lazy on those platforms (not a `MADV_DONTNEED` fallback). Item 48 (the underlying decommit/Darwin gap) remains open as a separate issue; item 53 (the `from_raw_parts` interaction) is also now closed.

    - **Evidence:** see `fn madv_free_advice` — the current implementation with BSD arms.

53. **[T, INFO] `Reservation::from_raw_parts` hard-codes `granted_huge: false`, creating a fail-open hazard when callers follow documented decommit advice** — **CLOSED** (filed 2026-08-14, task #934/C-9, sub-observation about item 48 from `docs/reviews/2026-08-14-aligned-vmem-pre-release-review.md`).

    Resolved by `from_raw_parts` now taking an explicit `granted_huge: bool` parameter (breaking change) instead of hard-coding `false`. Callers can now accurately reconstruct a `Reservation` with the correct `granted_huge` flag when adopting a foreign reservation, eliminating the fail-open hazard. This was a deliberate API change in the 0.2.0 pre-release cycle to address the contract gap identified in this item. Item 48 (the underlying decommit/Darwin gap) remains open as a separate issue.

    - **Evidence:** see `pub unsafe fn from_raw_parts` signature (`granted_huge: bool` parameter) and its use in the `Reservation` constructor (`granted_huge` field).

64. **Follow-up from commit 66b8508 (task #1030): `npm run check` lacks a `cargo test -p aligned-vmem` row with DEFAULT features, though ci.yml has a separate job that tests workspace members with default features.** — **CLOSED** (filed 2026-08-16, R7-8 finding class third occurrence; this is the same gap task #1024 closed with commit 66b8508, resurfaced as items 64/65 in task #1034).

    The local gate now matches ci.yml's runtime coverage: added `cargo test -p aligned-vmem` (plain default features, no `--all-features`) to `scripts/check-all.mjs`. The pre-existing clippy rows (`--all-targets`) validated compile-time only; this new step catches runtime failures (OOM, panic) that clippy cannot see. Placement: immediately after the `test (aligned-vmem --all-features)` step, maintaining the existing grouped aligned-vmem block.

    - **Files changed:** `scripts/check-all.mjs` (added one new step; updated header comment to reflect aligned-vmem block is now 9 steps: 5 clippy + 2 test + 1 doc + 1 optional semver).
    - **Verification:** `cargo test -p aligned-vmem` (default features) runs green locally (53 tests, 0 failures); `node --check scripts/check-all.mjs` passes (script is valid JS); `cargo test --all-features --test ci_clippy_matrix_consistency` passes (PER_PR_ROWS unchanged — aligned-vmem rows are a separate group, not part of the pinned matrix).
    - **Evidence:** this session's `scripts/check-all.mjs` diff; the added step's name `test (aligned-vmem default)`; local verification output below.

65. **CI-coverage gap: `aligned-vmem-gates` job added three steps (cargo doc with RUSTDOCFLAGS="-D warnings", cargo publish --dry-run, cargo semver-checks check-release) that are NOT covered by `npm run check`.** — **CLOSED** (filed 2026-08-16, task #1039 coverage gap; same class as task #1024's `aligned-vmem package gates` gap).

    The local gate now reproduces two of the three CI steps (the third, `cargo publish --dry-run`, was deliberately excluded per this item's own "Next trigger" instruction because it contacts crates.io on every local run). Added:
    1. `RUSTDOCFLAGS="-D warnings" cargo doc -p aligned-vmem --all-features --no-deps` — implemented as a `cargo doc` step with `env: { RUSTDOCFLAGS: '-D warnings' }` to avoid Windows shell quoting issues.
    2. `cargo semver-checks check-release --package aligned-vmem` — implemented as an optional wrapper script (`scripts/aligned-vmem-semver-check-optional.mjs`) that (a) checks if `cargo-semver-checks` is installed via `cargo semver-checks --version`, and (b) if missing, prints a clear diagnostic ("tool not installed; skipping — this is expected and NOT a failure") and exits 0; if present, runs the actual check and propagates its exit code. This distinction makes "tool missing" (configuration absence) visible and distinct from "tool present but broke" (actual semver violation), avoiding the forbidden `|| true` silent-swallow pattern.

    - **Files changed:** `scripts/check-all.mjs` (added two new steps; made the runner loop actually PASS `step.env`; corrected the header's step numbering); `scripts/aligned-vmem-semver-check-optional.mjs` (new optional wrapper script); `scripts/lib.mjs` (no change needed).
    - **A defect this closure INTRODUCED and then had to fix — recorded here rather than only in the commit body (exactly the F7/R22-3 class this index exists to prevent):** the first version of the doc step declared `env: { RUSTDOCFLAGS: '-D warnings' }` on the step object while `check-all.mjs`'s runner loop still called `run(step.cmd, step.args, { cwd: REPO_ROOT })` — it never read `step.env`. `run()` itself was never at fault (`scripts/lib.mjs:52` forwards `opts` straight to `spawn`, so it has always supported `env`); the loop simply dropped the field. The result was a step NAMED "warnings-as-errors" that could not fail — worse than an absent step, because it reads as coverage. Fixed by merging `step.env` OVER `process.env`, not replacing it: `spawn`'s `env` option replaces the environment wholesale, which would strip `PATH` and break every subsequent `cargo` lookup. Proven by counterfactual rather than by inspection — the same broken intra-doc link yields `exit 0` without the pass-through and `exit 101` with it.
    - **A second stale-doc defect found in the same file while closing this item:** the header comment's step numbering placed the aligned-vmem group AFTER the two remaining `PER_PR_ROWS` rows ("12-13 = PER_PR_ROWS, 14 = aligned-vmem"). The runtime array has had the opposite order since commit 66b8508 introduced the group. Re-derived from the array itself and corrected to `12-20` (aligned-vmem, 9 steps) / `21-22` (PER_PR_ROWS), renumbering the seven entries after them; the sibling stale figure in `docs/perf/OPEN_ITEMS.md` item 50 ("steps 14-19") was corrected in the same pass.
    - **Verification:** `RUSTDOCFLAGS="-D warnings" cargo doc -p aligned-vmem --all-features --no-deps` runs green locally; `node scripts/aligned-vmem-semver-check-optional.mjs` runs green locally (found cargo-semver-checks 0.50.0, semver check reported `0 checks: 0 pass, 254 skip` — every lint is SKIPPED because 0.1.0→0.2.0 is a major bump for a 0.x crate, so this step cannot fail until 0.2.0 is itself the baseline); `node --check scripts/check-all.mjs` passes; full `npm run check` from the primary checkout reports `ALL GREEN`.
    - **Evidence:** this session's `scripts/check-all.mjs` diff; the new `scripts/aligned-vmem-semver-check-optional.mjs` script with explicit "tool missing vs. tool present but broke" diagnostic distinction; local verification output below.

71. **`scripts/vmem-doc-drift-guard.mjs` went false-green when commit `a4b8e50` (task #1055) split `crates/aligned-vmem/src/lib.rs` into modules-per-file — its scan list was frozen at the pre-split three files (`src/lib.rs`, `Cargo.toml`, `README.md`), so every rustdoc that moved into `src/api/*.rs`, `src/os/*.rs`, `src/reservation*.rs`, `bench_internals/*.rs` left the guard's jurisdiction, and a live violation of its own rule shipped under the green (task #1069's `reserve_aligned_huge.rs` pool-cost note: an unqualified "deliberately not trimmed away / mapping kept whole" sentence).** — **CLOSED** (2026-08-18, task #1078; third guard of the campaign to lose contact with its subject, after task #1071's cargo cache replay and task #1073's foreign-worktree test binary).

    Fix: the guard now walks ALL of `crates/aligned-vmem/src/**` recursively (36 .rs files at closure time; an empty walk is a hard error, and the OK/FAIL lines report the scanned-file count so a future silent scope loss is visible in the output itself), plus the unchanged `Cargo.toml`/`README.md`. Deliberately NOT scanned: `tests/`, `benches/`, `examples/` (internal prose, never rendered as crate documentation) and `CHANGELOG.md` (a historical record of what was true at each version). The stale self-citation ("reserve_aligned's own rustdoc at lib.rs:741-749") now names its post-split location, `src/api/reserve.rs:15-33`. The one convicted sentence was fixed by adding its true path condition ("when huge pages are granted via this over-reserve path") — no suppression list added, and the predicate (TRIGGER/HARD_FAIL/SCOPE) is untouched.

    - **Verification:** positive counterfactual — the widened guard FAILS (exit 1) on the pre-prose-fix tree with exactly one finding, the task #1069 sentence (both quoted in the task #1078 commit body); negative counterfactual — a scaffold holding only `src/api/reserve.rs` plus one injected unconditional sentence fails on exactly the injected line, proving the walk reaches the canonical must-not-flag file and passes all its real sentences. Full `npm run check` green.
    - **Evidence:** the task #1078 commit (guard diff, one-sentence prose diff, this entry).

75. **`scripts/verify-vmem-page-constant-call-sites.mjs`'s tree scan was host-dependent: a readdir walk with a hand-maintained SKIP_DIRS that consulted no `.gitignore`, so gitignored scratch copies (on the reporting host: `tmp/asm_check/main.rs`, `tmp/heap_core_size_probe.rs`, `tmp/sefer_backup.rs` — a 1058-line stale copy of a source file) were scanned alongside the real sources, making both the verdict (a stale copy of an old `alloc_core.rs` could flip the guard RED with long-fixed call sites, or dilute it GREEN) and the summary's "scanned N file(s)" count host-dependent and not reproducible from a clean clone.** — **CLOSED** (2026-08-18, task #1088, finding L7; fixed in the same task that filed it).

    Fix: the scan set is now the TRACKED tree — `git ls-files -- '*.rs'` (fails loudly if git is unavailable rather than silently falling back to a walk, which would resurrect the host-dependence; a tracked-but-missing-on-disk file is skipped with a loud WARNING), and the `src/` production-provenance index is derived from the SAME list filtered to `src/`, so the two can never disagree about what the production tree is. The summary line now prints both the scanned count and the tracked count. Accepted trade-off: a brand-new file is not scanned until tracked (its guard coverage starts at `git add`). Precedent for a guard shelling out to git: `scripts/verify-commit-prefixes.mjs`. **SUPERSEDED 2026-08-18 (task #1099, finding I3) — that accepted trade-off no longer holds, and this paragraph now describes the guard only as it stood between L7 and I3:** the tracked-only scan set turned out to have MOVED the blind spot rather than closed it — a not-yet-`git add`ed file carrying a violating call site made the PRE-PUSH gate report ALL GREEN — so the scan set became `git ls-files --cached` ∪ `git ls-files --others --exclude-standard`. That closes both holes at once: gitignored scratch stays excluded and clean-clone reproducibility is preserved (on a fresh checkout the untracked query is empty, so the count reduces to the tracked count by construction), while coverage for a new file now starts at file creation instead of at `git add`. See the task #1099/I3 commit for the before/after counterfactual.

    - **Verification (before/after on the same working tree):** OLD code, clean tree: `scanned 555 file(s)`; OLD code + one untracked gitignored `tmp/peek.rs` (content-free) added: `scanned 556 file(s)` — the pollution reproduced on this host (the reporting host's 558 = its 555 tracked + its 3 tmp files); NEW code with `tmp/peek.rs` still present: `scanned 555 file(s) (555 tracked .rs in the index...)` = `git ls-files -- '*.rs' | wc -l` exactly, every counter unchanged (213 call sites, 52 candidate args, both per-flow partitions identical), ALL GREEN. Demo file removed afterwards; clean-tree re-run identical.
    - **INFO 14 recorded here (same defect class — scan-count provenance), decision: no amendment, non-retroactive:** `9681a21`'s body cites "scanned 554 file(s)". Measured against the commit itself (`git ls-tree -r --name-only 9681a21 | grep -c '\.rs$'` = 554), the figure coincides exactly with that commit's tracked `.rs` count, so a clean clone at that commit reproduces it today; HEAD's 555 differs by exactly the one `.rs` file committed after it (`tests/decomp_hooks_forced_page.rs`). What the commit does NOT record is the scan-set identity (clean vs dirty working tree at measurement time) — under the old walker the same printed figure could equally have come from a dirty tree (e.g. 551 tracked + 3 untracked), and that ambiguity is unresolvable post hoc. The verdict is unaffected (the counter-partition fix is arithmetically self-consistent and re-verified green on the current tree). The commit is NOT amended (history non-retroactive); this card plus the tracked-tree fix close the class going forward.
    - **Evidence:** task #1088's diff to `scripts/verify-vmem-page-constant-call-sites.mjs`; `git ls-tree -r --name-only 9681a21 | grep -c '\.rs$'`; `git diff --name-status 9681a21..HEAD -- '*.rs'`.
