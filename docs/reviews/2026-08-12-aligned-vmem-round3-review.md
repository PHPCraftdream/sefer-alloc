# `aligned-vmem` — round-3 review (fresh pass over the post-#851–#857 tree)

**Date:** 2026-08-12
**Scope:** `crates/vmem/` in full — `src/{lib,error,mock,fault_injection}.rs`, `tests/` (7 files:
`smoke.rs`, `mock.rs`, `huge_pages.rs`, `lazy_commit.rs`, `min_page.rs`, `vmemerror_io_bridge.rs`,
`fault_injection.rs` — there is no eighth), `benches/vmem_bench.rs`,
`examples/v20_849_unix_exact_reserve_hit_rate.rs`, `Cargo.toml`, `README.md`; plus the
`aligned-vmem`-touching parts of `.github/workflows/ci.yml`, the root `Cargo.toml`,
`src/alloc_core/alloc_core_core_diag.rs`, `docs/CORRECTNESS_OPEN_ITEMS.md` and `CHANGELOG.md`.
**Reviewed tree:** local `main` @ `b20b4a58fce7b60c2da29f968330072abade55aa`
(task #855's merge). `git status --short -- crates/vmem .github docs/reviews` shows no
modifications in scope; the only entries are the two prior review documents, which are
**untracked** (see F11).
**Toolchain:** `cargo`/`rustc` stable as installed on this host; Windows 10 Pro x86_64, 4 KiB page.
**Nature:** read-only. Nothing was modified other than the creation of this document. No
`git add` / `git commit`. Every command quoted below was actually run on this host, and every
`file:line` citation was read in the current tree before being written down.

**Relationship to the prior two rounds.** This pass does not re-report V1–V21
(`docs/reviews/2026-08-12-aligned-vmem-code-quality-review.md`) or W1–W16 + P-A/P-B/P-C
(`docs/reviews/2026-08-12-aligned-vmem-post-campaign-closing-review.md`). It spot-checks a
selection of their claimed fixes (see "Checked and explicitly NOT findings") and reports only what
is new — mostly residue of round 2's own fixes. Findings are numbered `F1…F11` to stay unambiguous
against the `V`- and `W`-series.

**Platform honesty up front.** This host is Windows/x86-64 with a 4 KiB page. F1 is
**reasoned from the source of the test and of `decommit_lazy`**, not executed on a 16 KiB-page
host (none is available here). F7 is reasoned from the Win32 `VirtualAlloc` allocation-granularity
contract. Everything else was read directly, and the "verified green" table below was executed.

---

## Verdict up front

**The crate itself is in good shape. Two rounds of review are visible in it and nothing new was
found in the `unsafe` code, the atomics, or the reservation lifecycle.** No soundness hole, no
panic-safety gap, no race, no platform divergence beyond what is already documented. The
performance null result from rounds 1 and 2 (syscall-bound, no CPU hot path) re-confirms; I found
no new concrete opportunity and am not re-litigating it.

**What is new is almost entirely round-2 residue, and it clusters in two places.** First,
round 2's own fixes were applied to the *call sites* but not to the *assertions/docs one line
away*: `tests/mock.rs`'s W9 fix changed the two `decommit` calls to `page_size()` and left the
assertion below them comparing against `PAGE` (F1) — the same defect W9 named, in the same
function, still page-size-dependent. Second, W4's counter split was applied inside
`crates/vmem/src/lib.rs` and nowhere else: the *root crate's* forwarding accessor, which is the
surface the R32-13 gate actually reads, still carries verbatim the "Each call issues exactly 2
syscalls" claim W4 was written to remove (F3), and the root crate exposes no accessor for the new
split at all.

**One real CI gap survives round 2 by name.** W14 identified three blind spots — `cfg(miri)`,
macOS, and Windows. Task #856 closed the first two; the Windows one is still open. `test-windows`
(`.github/workflows/ci.yml:738-766`) runs the root crate only, so `crates/vmem/tests/*` never
executes on Windows — including `lazy_commit.rs`'s `lazy_reserve_small_align_still_reserves_full_span`,
which is the #848 regression test and is **structurally incapable of failing on Linux or macOS**,
the only two platforms CI runs it on (F2).

**Context that changes how the above should be read: none of this has ever run in CI.** Local
`main` is **25 commits ahead of `origin/main`** (`origin/main` = `d2fec28`, an ancestor;
`git rev-list --count HEAD..origin/main` = 0), and every commit of both aligned-vmem rounds —
`76ac08f` (task #842) through `b20b4a5` (task #855) — is in that unpushed range. The
`aligned-vmem-gates` job (including its new miri compile gate), the macOS row, and the
feature-powerset sweep have therefore never executed on a runner. F1 in particular is a
prediction about the first push, not an observation of a red run.

**Publish posture (task #658).** Nothing here is a soundness or API-shape blocker. The three
worth settling before `cargo publish` are F1 (it will red the macOS row), F4/F5 (two shipping doc
claims — one rustdoc, one README table row — that are factually wrong about the current code),
and F3 (a measurement-oracle claim that would make a future perf round wrong by up to 2×). None
requires a breaking change. The remaining public-surface decisions round 2 left open (`W7`'s
`RawReservation` scope, `W12`'s missing `ReservationParts::new`, `W13`'s absent `try_decommit`)
are all *additive* to revisit after publish and are correctly recorded as such in the source; I
re-read all three and have nothing to add.

---

## What was verified green (so the negatives below are read in context)

| command | result |
|---|---|
| `cargo test -p aligned-vmem --all-features` | **green** — 18 smoke + 3 vmemerror_io_bridge + the rest; 0 failed, 0 doctests |
| `cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings` | **green**, exit 0 |
| `cargo clippy -p aligned-vmem --all-targets -- -D warnings` (default row) | **green**, exit 0 |
| `cargo fmt -p aligned-vmem --check` | **green**, no output |
| `RUSTFLAGS="--cfg miri" cargo check -p aligned-vmem --all-features` | **green** — W1's fix holds |
| `git rev-list --count HEAD..origin/main` | `0`; `git log --oneline origin/main..HEAD` = 25 commits |

---

# Findings

## Category 1 — round-2 fix residue

### F1 — MEDIUM — W9's fix changed the two `decommit` calls in `tests/mock.rs` to `page_size()` but left the assertion one line below comparing the recorded offset against `PAGE`; the test is still page-size-dependent and will fail on the macOS runner that task #856 added in the same round

`crates/vmem/tests/mock.rs:21-22` (the fixed call sites) against `crates/vmem/tests/mock.rs:35`
(the unfixed assertion), and `crates/vmem/src/lib.rs:988-998` (`decommit_lazy` records the
caller's `start` verbatim).

Current state of `records_reserve_and_decommit`:

```rust
// mock.rs:20-23
unsafe {
    decommit(base, 0, page_size());
    decommit_lazy(base, page_size(), 2 * page_size());
}
…
// mock.rs:35
assert!(matches!(calls[2], Call::DecommitLazy { start, .. } if start == PAGE));
```

`decommit_lazy` (`lib.rs:988-998`) validates against `page_size()` and then records
`Call::DecommitLazy { base, start, end }` with the caller's own `start`. So the recorded `start`
is `page_size()`, and the assertion is effectively `page_size() == PAGE`, i.e. `== 4096`.

**Failure scenario (concrete):** on any host where `sysconf(_SC_PAGESIZE) != 4096` — Apple
Silicon macOS (16 KiB), 64 KiB-page aarch64 Linux — `calls[2]` is
`DecommitLazy { start: 16384, … }`, the `matches!` guard is false, and the test panics. This is
precisely the failure mode W9 described ("will fail on any 16 KiB- or 64 KiB-page host"), moved
from the call site to the assertion rather than removed.

**Why it matters now rather than hypothetically:** task #856 added
`cargo test -p aligned-vmem --all-features --no-fail-fast` to the `test-macos` job
(`.github/workflows/ci.yml:787-790`, `runs-on: macos-latest`). GitHub-hosted `macos-latest` images
are Apple Silicon (arm64), where `page_size()` is 16384. The macOS row added to close W9's own
blind spot is therefore the row this residue reds. Not yet observed, because none of this is
pushed (see F11) — but it is the first thing the new row will report.

**Fix:** `if start == page_size()` (two tokens), matching the call above it. Note `PAGE` stays
imported and used by the other tests in the file, so the import does not go dead.

**Verified by reading, not executed:** on this Windows host `page_size() == PAGE == 4096`, so the
whole file passes locally (it did, in the green table above). No 16 KiB-page host is available
here; the reasoning needs none — `PAGE` is a compile-time `4096` (`lib.rs:147`) and the recorded
value is `page_size()`'s runtime result.

### F2 — MEDIUM — W14's third CI gap (Windows) is still open: `crates/vmem/tests/*` runs on no Windows job, so #848's single-call fast path and its own regression test have never executed on the only platform where either exists

`.github/workflows/ci.yml:738-766` (`test-windows`, three `cargo test --features …` steps, all
root-crate-scoped — no `-p aligned-vmem` anywhere in the job), against
`crates/vmem/src/lib.rs:1435-1478` (the Windows single-call path) and
`crates/vmem/tests/lazy_commit.rs:70-116` (its regression test).

Round 2's W14 named three blind spots. Task #856 closed two — the miri compile gate
(`ci.yml:156`) and the macOS row (`ci.yml:790`) — and left the third. Every job that runs
`cargo test -p aligned-vmem` today is Linux or macOS: `aligned-vmem-gates` (`:138`
`runs-on: ubuntu-latest`, `:152`), `test-workspace` (`:814` `ubuntu-latest`, `:820`/`:862`/`:882`),
`test-macos` (`:790`).

**Failure scenario (concrete):** `lazy_reserve_small_align_still_reserves_full_span`
(`lazy_commit.rs:70-116`) is the regression test for the `commit_len == size` guard at
`lib.rs:1435`. On Unix that guard does not exist — `reserve_aligned_lazy_raw`
(`lib.rs:2012-2018`) simply forwards to `reserve_aligned_raw`, ignoring `initial_commit`
entirely — and under miri likewise (`lib.rs:2342-2349`). So on Linux and macOS the test asserts
that an eager full-span reservation is a full-span reservation: it cannot fail regardless of what
`win_reserve_commit` does. If someone deleted `&& commit_len == size` from `lib.rs:1435`
tomorrow, every CI job would stay green and Windows consumers would get a reservation silently
truncated to `initial_commit` bytes — the exact bug the test's own comment says was reproduced
during #848's zero-trust review.

The same shape applies to the whole file set: `huge_pages.rs`'s
`reserve_aligned_huge_ordinary_page_sized_request_succeeds`, `mock.rs`'s Windows recording
behaviour, and `smoke.rs`'s `decommit`/`recommit` round-trips against real `MEM_DECOMMIT` all run
only on non-Windows CI. The root crate's Windows rows do exercise `win_reserve_commit`, but only
at `align = 4 MiB > 64 KiB`, i.e. exclusively the two-call path.

**Fix:** one line in `test-windows` — `- run: cargo test -p aligned-vmem --all-features
--no-fail-fast` — mirroring `ci.yml:790` exactly.

### F3 — MEDIUM — W4's counter split stopped at `crates/vmem/src/lib.rs`: the root crate's forwarding accessor still carries verbatim the "Each call issues exactly 2 syscalls" claim W4 existed to remove, and no accessor for the split counters was ever surfaced to the consumer that uses them as an oracle

`src/alloc_core/alloc_core_core_diag.rs:129-141` and `:143-147`; `crates/vmem/Cargo.toml:98-109`
(specifically `:101-102` and `:104`); root `Cargo.toml:564-566` and `:2267-2268`; against
`crates/vmem/src/lib.rs:207-228` (the corrected statics) and `:248-278` (the accessors).

Task #853 correctly split `WINDOWS_RESERVE_COMMIT_CALLS` into
`WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` / `…_TWO_CALL_PAIRS`, corrected their rustdoc, and corrected
the module-level design comment (`lib.rs:160-184`). Four sites outside that file did not follow:

1. **`src/alloc_core/alloc_core_core_diag.rs:129-135`** — `dbg_windows_reserve_commit_calls`'s
   rustdoc still says, verbatim: *"process-wide count of `aligned_vmem::win_reserve_commit` calls
   … **Each call issues exactly 2 syscalls** (`VirtualAlloc(MEM_RESERVE)` + `VirtualAlloc(MEM_COMMIT)`)
   … since Windows has **no fast/slow-path split** to measure today (F11 step 3 territory)."*
   Both clauses are false since #848. This is the surface that matters: the R32-13 gate binary
   (`examples/r32_13_windows_reserve_commit_decomposition_gate.rs:123`, `:135`, `:156`) reads
   `HeapCore::dbg_windows_reserve_commit_calls()`, which forwards to
   `windows_reserve_commit_calls()` (`lib.rs:255-258`) — the *sum* of both paths. So W4's own
   stated failure scenario ("a future round reads it as its path-activation oracle and computes
   syscalls as `2 × calls`") is still fully live, just one indirection further out than round 2
   looked.
2. **`src/alloc_core/alloc_core_core_diag.rs:143-147`** — `dbg_reset_vmem_bench_internals_counters`
   says "reset all **three** `aligned_vmem` bench-internals counters" and lists three; there are
   now four (`lib.rs:280-294`).
3. **`crates/vmem/Cargo.toml:101-102`** — the `bench-internals` feature's own doc still describes
   "a Windows-side count of `win_reserve_commit` calls (**2 syscalls each: one `MEM_RESERVE` + one
   `MEM_COMMIT`**)". Same stale unit claim, inside the reviewed crate.
4. **Root `Cargo.toml:564-566` and `:2267-2268`** name `WINDOWS_RESERVE_COMMIT_CALLS`, a static
   that no longer exists.

Separately, and the substantive half: **no root-crate accessor exists for the split.**
`alloc_core_core_diag.rs` exposes `dbg_unix_exact_reserve_attempts` / `_hits` /
`dbg_windows_reserve_commit_calls` / `dbg_reset_vmem_bench_internals_counters` and nothing else,
so a consumer cannot read `windows_reserve_commit_single_calls()` /
`…_two_call_pairs()` (`lib.rs:266-278`) at all. The instrument W4 asked for exists in the library
and is unreachable from the gate that needs it.

One more, smaller: `crates/vmem/Cargo.toml:104` says the counters mirror sefer-alloc's convention
of "`#[doc(hidden)]` **accessors**". Task #853 hid the four **statics** (`lib.rs:195`, `:204`,
`:214`, `:227`) — which is what W11 asked for — and left the accessors documented (`:233`, `:242`,
`:253`, `:264`, `:274`, `:288` all carry `#[cfg_attr(docsrs, doc(cfg(...)))]`, none carries
`#[doc(hidden)]`). The comment now describes the inverse of what shipped.

**Fix:** correct the four doc sites; add `dbg_windows_reserve_commit_single_calls` /
`…_two_call_pairs` forwarders beside the existing one (additive, `#[doc(hidden)]`,
`bench-internals`-gated) so the split is usable where it was needed.

### F4 — LOW — `reserve_aligned`'s rustdoc says Windows "**unconditionally** over-reserves `size + align`"; that has been false since #848 for `align <= 64 KiB`, and the crate's own module comment 550 lines above says the opposite

`crates/vmem/src/lib.rs:726-729`, against `:174-177` and `:1435-1478`.

```rust
// lib.rs:723-731 (reserve_aligned's rustdoc)
/// … On a miss (wrong alignment), over-reserves `size + align`
/// bytes and keeps the full mapping. On Windows,
/// unconditionally over-reserves `size + align` bytes and keeps the full
/// mapping.
```

The module-level `bench-internals` comment at `:174-177` describes the real behaviour correctly
("one syscall (the fast path for `align <= 64 KiB`, over-reserving **nothing** — base == region)
or two syscalls (the traditional path for larger alignments, over-reserving `size + align`)"), and
the code at `:1435` / `:1478` returns `Ok((base, base, commit_len, …))` on the fast path — no
over-reserve at all.

**Failure scenario:** a Windows consumer sizing its VA budget from the published rustdoc
over-provisions by `align` bytes per reservation for every `align <= 64 KiB` call, or (the more
likely direction) reads `reservation_len` expecting `size + align` and finds `size`. Nothing
breaks; the documented resource model is simply wrong for one of the two Windows paths.

The same over-generalisation is in the **crates.io package description**,
`crates/vmem/Cargo.toml:7`: *"exact-size mmap fast path on Unix; on fast-path miss **or Windows**,
over-reserve size+align and keep the full mapping"*. Task #854 rewrote that sentence for the Unix
no-trim change (W5) without accounting for #848's Windows change, which had landed six commits
earlier. This is the **fourth** documented drift of this one sentence; W5 counted three
(`#640`, `#650`, `#842`) and recommended a `grep -n 'trim' crates/vmem/` guard. A guard on
"over-reserve" would have caught this one too.

### F5 — LOW — the README API table gives `into_parts()` the wrong return type and label, and has no row for the method that actually returns `ReservationParts`

`crates/vmem/README.md:42-44`, against `crates/vmem/src/lib.rs:510-515` and `:529-544`.

```markdown
| `Reservation::into_parts() -> ReservationParts` | Take the raw reservation, suppress `Drop`, for self-hosted release (typed form). |
| `release(ptr, len, align)` (unsafe) | … (legacy tuple form). |
| `release_parts(ReservationParts)` (unsafe) | Release a reservation taken via `into_reservation_parts`, exactly once (typed form). |
```

`into_parts` returns `(*mut u8, usize, usize)` (`lib.rs:511`); `into_reservation_parts` returns
`ReservationParts` (`lib.rs:530`). The table names the tuple method, gives it the struct's return
type, calls it "(typed form)", and then — one row down — refers a reader to
`into_reservation_parts`, which appears nowhere else in the table. The README's own runnable
example three sections above (`README.md:32`) destructures the 3-tuple, contradicting its own
table.

This row was added by W10's fix (task #855). Non-blocking, but it is the crate's front page and
the mistake points a reader at exactly the API-shape confusion V8/W12 were about.

**Fix:** `| Reservation::into_parts() -> (*mut u8, usize, usize) | … (legacy tuple form). |` plus a
new row for `Reservation::into_reservation_parts() -> ReservationParts`.

### F6 — LOW — `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` is incremented on the huge-pages retry branch, where **two** `VirtualAlloc` calls were issued, while its own rustdoc guarantees "exactly 1 syscall"; the sibling counter's doc discloses its own retry, this one does not

`crates/vmem/src/lib.rs:1463-1465` (the increment) against `:207-212` (the doc), and
`:1544-1547` + `:217-225` (the sibling that does disclose it).

The single-call path's rustdoc says: *"Each call issues exactly 1 syscall
(`VirtualAlloc(MEM_RESERVE | MEM_COMMIT)`)"*. The retry branch is reached when the first,
large-page-flagged `VirtualAlloc` returns NULL; it issues a second, plain `VirtualAlloc`
(`:1455-1460`) and, on success, increments the same counter (`:1463-1464`) before returning.
Two syscalls, one increment, under a doc that promises one syscall per increment.

`WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS`'s doc handles the analogous case explicitly — *"plus a
possible third best-effort retry on a `huge-pages` commit failure — not counted here, see that
call site"* (`:222-224`) — so the asymmetry is a gap in #853's own split, not a pre-existing
convention.

**Failure scenario:** narrow. Requires `bench-internals` + `huge-pages` + a Windows host whose
large-page request fails (i.e. essentially every Windows host without `SeLockMemoryPrivilege`),
and only affects a syscall-count derivation, not behaviour. Rated LOW for that reason, but it is
the same class of defect as F3 and W4: a counter whose documented unit does not match what it
counts.

### F7 — LOW — the Windows single-call path's alignment guarantee rests on a hardcoded `WIN_ALLOCATION_GRANULARITY = 65536` that is never validated against the value the crate already queries from the OS

`crates/vmem/src/lib.rs:1710` (the constant), `:1435` (the guard that consumes it), against
`:1680` (`SystemInfo::dw_allocation_granularity`, declared and never read) and `:348-355`
(`query_os_page_size`, which already calls `GetSystemInfo` and reads only `dw_page_size`).

Before #848 nothing depended on the allocation granularity — round 1's V21 noted
`dw_allocation_granularity` as merely unread. After #848 the public alignment contract on the
fast path *is* "`VirtualAlloc(NULL, …)` returns a base aligned to at least 64 KiB", asserted by a
literal rather than by the OS's own answer. The crate is one struct field away from checking it
and does not.

**Failure scenario:** on a hypothetical Windows target whose `dwAllocationGranularity` were below
64 KiB, `reserve_aligned(size, 65536)` would take the single-call path and could return a base
that is not 64 KiB-aligned — a silent violation of the crate's central documented guarantee, with
no error and no assertion. REASONED-FROM-SPEC: every Windows target this crate supports reports
64 KiB, so this is not a live bug; it is an unchecked load-bearing assumption that became
load-bearing only in the last round. A `debug_assert!(info.dw_allocation_granularity as usize >=
WIN_ALLOCATION_GRANULARITY)` alongside the existing `GetSystemInfo` call, or deriving the constant
from it, closes it for nothing.

## Category 2 — documentation consistency (all INFO)

### F8 — INFO — `Reservation::is_huge`'s rustdoc lost a paragraph and swallowed a sentence into the wrong list item during the round-2 merge, and carries two stray backslash escapes the identical text elsewhere does not

`crates/vmem/src/lib.rs:472-494`, against the identical prose on the `granted_huge` field at
`:388-406`.

Two things, both artefacts of the hand-resolved doc-comment conflict / `doc_lazy_continuation`
fix in the #852→#853 merges:

1. `:488-489` — *"For `align > 64 KiB` the two-call path is used, which does not support
   `MEM_LARGE_PAGES` at all."* is indented four spaces, making it a lazy continuation of
   **list item 3** ("The calling process has `SeLockMemoryPrivilege`"), which it has nothing to do
   with. The field doc at `:403-405` renders the same sentence as its own paragraph, correctly.
   The field doc also has an *"If any of these conditions fail, the function falls back to
   ordinary pages and this flag is `false`"* paragraph (`:403-404`) that `is_huge`'s copy does not
   have at all — the reader of the *public method* gets the strictly weaker explanation.
2. `:478` — `/// This is the \"best-effort\" observable:`. These are literal backslashes in a
   `///` comment (confirmed by `grep -n '\\"' crates/vmem/src/lib.rs`, which matches only this
   line). CommonMark treats `\"` as an escaped `"` so the rendered page is fine; the source is
   inconsistent with `:393`, which writes the same phrase as `"best-effort"` unescaped. Cosmetic
   only — listed because it is a marker of a machine-generated edit that also produced item 1.

### F9 — INFO — the Windows **two-call** path derives `granted_huge` from the requested flag rather than an observed grant — the half of W2's fix that only landed on Unix

`crates/vmem/src/lib.rs:1565` (`Ok((base, region, over, extra_commit_flags != 0))`), against
`:1849` / `:1961` (the Unix sites, which now thread `HUGE_SUPPORTED && huge`, `:2077-2084`).

W2's fix introduced `HUGE_SUPPORTED` so Unix reports the grant, not the request. The Windows
two-call path was not given the equivalent treatment: `extra_commit_flags != 0` is a property of
the *request*. The crate's own docs (`:1283-1284`, `:404-405`, `:1646-1647`) state that the
two-call path "does not support `MEM_LARGE_PAGES` at all", so if that commit ever returned
non-NULL with the flag set, `is_huge()` would report `true` on a path documented as always
`false`. Per the Win32 contract (V3/W3's finding: `MEM_LARGE_PAGES` requires a combined
`MEM_RESERVE | MEM_COMMIT` call) that branch is unreachable, which is why this is INFO and not a
bug — but the code and the doc disagree about which one is authoritative, and the Unix side now
expresses the same idea with an explicit constant.

Adjacent, same shape and same rating: `Reservation::from_raw_parts` hard-codes
`granted_huge: false` (`lib.rs:650`, "Caller cannot know; conservatively assume false"), and
`is_huge`'s rustdoc (`:472-494`) does not mention that an adopted reservation always reports
`false` regardless of how it was created.

### F10 — INFO — `mock::Call::Release`'s variant doc still says the call comes from `into_parts` + manual release; V9 made `Drop` record it too

`crates/vmem/src/mock.rs:85`, against `crates/vmem/src/lib.rs:655-668` (`Drop` records
`Call::Release`) and `:869-873` (`release` records it).

`/// [`crate::release`] (from `into_parts` + manual release).` The RAII path now records the same
variant — that was V9's whole point, and `tests/mock.rs:46-62`
(`drop_records_release`) pins it. The variant's own doc is the one place a consumer reads to learn
what produces a `Release` entry, and it names only half the producers.

## Category 3 — process / paper trail

### F11 — INFO — neither aligned-vmem round has been pushed; the round-2 campaign has no CHANGELOG entry (its own CHANGELOG text says so), and both review documents are untracked

Verified: `git rev-parse origin/main` = `d2fec289526e2208cff9ce21ad617a3e343a1194`;
`git merge-base --is-ancestor origin/main HEAD` succeeds; `git rev-list --count HEAD..origin/main`
= `0`; `git log --oneline origin/main..HEAD` lists 25 commits, from `05ce375` up to `b20b4a5`,
containing every commit of both rounds (`76ac08f` #842 … `b20b4a5` #855, including `24737e0` #856
and `19ea694` #857). `gh run list` shows the most recent workflow runs against `d2fec28`,
`e4f98d3` and `bce871e` — none of the campaign's SHAs.

Three consequences worth recording, none of which is a code defect:

1. **No CI evidence exists for either round.** Every "green" claim in rounds 1 and 2, and in this
   one, is a local claim. The `aligned-vmem-gates` job's miri compile gate (`ci.yml:156`), the
   macOS row (`:790`) and the powerset sweep (`:2001-2002`) have never run on a runner. Per
   CLAUDE.md's "Before every push … Then confirm CI went green — do not assume it", this is the
   step that has not happened yet, and F1 is the concrete thing waiting on the other side of it.
2. **The #851–#857 round has no CHANGELOG entry.** `CHANGELOG.md:275` states it explicitly:
   *"that follow-up round (tasks #851-857) is tracked separately and will get its own CHANGELOG
   entry once complete."* `grep -n '#851\|#855\|#856\|#857' CHANGELOG.md` returns no bullets for
   any of them. This is W16's finding recurring one round later; nothing in
   `docs/perf/OPEN_ITEMS.md` or `docs/CORRECTNESS_OPEN_ITEMS.md` tracks it, so a fresh session
   inherits no memory of it — the exact scenario CLAUDE.md's "Round start: check BOTH open-items
   indexes" convention exists to prevent.
3. **Both prior review documents are untracked** (`git status --short -- docs/reviews` shows
   `?? docs/reviews/2026-08-12-aligned-vmem-code-quality-review.md` and
   `?? …-post-campaign-closing-review.md`). Every commit message in both rounds cites them by
   path; those citations are currently unresolvable from any commit.

Minor and adjacent: `docs/CORRECTNESS_OPEN_ITEMS.md:1837` says task #851's fix landed in
"(commit pending)". It landed as `78ecc81`.

---

## Checked and explicitly NOT findings

Spot-checks of rounds 1 and 2 that I verified still hold in the current tree (not an exhaustive
re-derivation of all 37 findings — a selection weighted toward the ones a later commit could
plausibly have undone):

* **W1 (HIGH, the `cfg(miri)` compile break) is genuinely fixed and now guarded.**
  `RUSTFLAGS="--cfg miri" cargo check -p aligned-vmem --all-features` is green on this host, and
  the CI guard exists at `ci.yml:156`. `lib.rs:2342-2349` and `:2352-2360` both destructure
  3-tuples now.
* **W2 (`granted_huge` reporting the request off Linux) is fixed for Unix.** `HUGE_SUPPORTED`
  (`lib.rs:2077-2084`) is `true` only under `all(target_os = "linux", feature = "huge-pages")`,
  and both Unix return sites use `HUGE_SUPPORTED && huge` (`:1849`, `:1961`). The Windows two-call
  half is F9 above.
* **W4's in-crate half is done.** The two split statics exist (`lib.rs:215`, `:228`) with corrected
  rustdoc, the module design comment (`:160-184`) describes both Windows paths, and the
  ~33%/4.6 µs claim at `:1413-1417` is now explicitly labelled "an unverified hypothesis, not a
  validated benchmark result". The out-of-crate half is F3.
* **W5's trim/over-reserve pass held for the Unix sentence.** `grep -n 'trim' crates/vmem/` finds
  no surviving "over-reserve + trim" claim; `lib.rs:1877-1882` documents the no-trim decision, and
  `reserve_aligned`'s rustdoc carries the measured VA cost (`:733-735`). The residual Windows
  over-generalisation is F4.
* **W6 (orphaned doc comments) is fixed.** `validate_size_align` (`lib.rs:745-752`) now carries
  only its own one-line doc; `finish_reservation` (`:784-798`) has the doc that used to sit on
  `RawReservation`; `try_reserve_aligned` (`:821-826`) has its own correct copy.
* **W7/W12/W13 are recorded-as-decided, exactly as round 2 asked.** `RawReservation`'s doc
  (`lib.rs:765-772`) says plainly it is "call-site convenience only … does NOT eliminate the
  transposition risk entirely"; `ReservationParts`'s doc (`:684-689`) states the missing
  constructor and why the self-hosted-metadata use case is not yet served; `decommit`/
  `decommit_lazy` (`:922-926`, `:978-981`) each carry a "**No fallible form:**" paragraph.
* **W8 (stale `page_size()` rustdoc) is fixed.** `lib.rs:304-310` now says "gets a crate-level
  silent skip … Even at the OS level, madvise(2) rejects the entire call (all-or-nothing)", and no
  "silently do partial work" wording survives anywhere in `src/`.
* **W10's README alignment-contract bullets are correct.** `README.md:99-100` states the real
  `page_size()`-vs-`PAGE` asymmetry and `:102-105` gives the rationale. The API-table half is F5.
* **W11 shipped as asked.** All four `bench-internals` statics carry `#[doc(hidden)]`
  (`lib.rs:195`, `:204`, `:214`, `:227`). (They remain `pub`, so a downstream consumer can still
  `store()` into them — `#[doc(hidden)]` was W11's own proposed remedy for the *semver* half, and
  the mutability half is unchanged by design. Not re-opened.)
* **W14's first two gaps are closed.** miri compile gate at `ci.yml:156`; macOS row at `:790`;
  aligned-vmem powerset at `:2001-2002` (weekly/`workflow_dispatch`, alongside the root crate's).
  The third is F2.
* **W15's paper trail exists.** `docs/perf/R_V20_849_UNIX_EXACT_RESERVE_HIT_RATE.md`,
  `docs/perf/_raw_r_v20_849_unix_exact_reserve_hit_rate.log`, the `…_summary.csv`, and
  `docs/perf/OPEN_ITEMS.md:1130` (item 46, the bare-metal remeasure card) are all present.
* **`docs/CORRECTNESS_OPEN_ITEMS.md` item 41's Status card was updated as W1 required**
  (`:1802-1846`): sub-item 3 marked CLOSED by #851, sub-item 1 (the intentional
  `leak_zeroed_pages` leak) correctly still the sole remaining runtime blocker, and the
  compile-only gate recorded in the Next-trigger block.
* **V5's two `from_raw_parts` test leaks stay fixed and non-vacuous.** `smoke.rs:309-333` and
  `:341-361` both `catch_unwind` → `release` → `resume_unwind` the *original* payload, so the two
  distinct `#[should_panic(expected = …)]` strings still discriminate.
* **V1's no-trim fix is intact.** `lib.rs:1877-1899` returns `(base, region_ptr, over)`;
  `release_reservation` (`:1965-1969`) `munmap`s exactly `(reservation, reservation_len)`. No
  `munmap` on a computed sub-offset survives anywhere in `unix_reserve`.
* **P-A's free guard landed correctly.** `lib.rs:1945` is
  `if align > page_size() && !region_addr.is_multiple_of(align)`, with the invariant documented at
  `:1940-1944`. The skip is sound: a kernel `mmap` result is page-aligned, so for
  `align <= page_size()` the second conjunct is provably false.
* **P-B landed.** `decommit` (`:949`) and `decommit_lazy` (`:989`) each hoist one
  `let ps = page_size();`.
* **`fault_injection`'s atomics are unchanged since #718/#775 and still correct** —
  `Release`/`Acquire` pair at `:108`/`:139`, `fetch_update` with the lazy-`then` underflow note at
  `:125-133`, and the third disarm-vs-rearm race still declared out of scope in the module doc
  (`:47-57`). `tests/fault_injection.rs` serialises on a `Mutex` (`:34`) so the process-global
  hooks are not raced by libtest's thread pool.
* **`error.rs`'s `From<VmemError> for std::io::Error` and V17's de-duplication both hold**
  (`error.rs:138-148`, one `#[cfg(not(miri))] last_os_error_code` at `:150-155`), and all three
  arms are covered by `tests/vmemerror_io_bridge.rs` with real assertions on `raw_os_error()` /
  `kind()` / message.
* **`mock::Call`'s V6 constructors are complete** — all 8 variants (`mock.rs:136-191`), exercised
  from the integration-test crate at `tests/mock.rs:133-157`. `fail_next_reserve`'s V10 doc
  correction holds (`mock.rs:219-221`, `os_refusal_unknown_code`).
* **Not re-raised from round 1, still open, deliberately:** `benches/vmem_bench.rs`'s asymmetric
  `black_box` usage (`:54-57` wraps the `reserve_aligned` call and its arguments; `:69` does not),
  noted by V18 as a sub-item and never assigned a task. It affects only cross-arm comparability of
  absolute ns/op within the bench, which nothing publishes. Stylistic; INFO at most; recorded here
  so it is not re-discovered as new in a round 4.
* **Performance: null result re-confirmed, nothing new.** Every public entry point is one syscall
  deep; `page_size()` is a single relaxed load after the first call (`lib.rs:313-316`);
  `align_up_addr` is two arithmetic ops (`:2385-2388`); the counters are compiled out by default.
  The only levers remain syscall count and address space, both already settled by
  `docs/perf/R_V20_849_UNIX_EXACT_RESERVE_HIT_RATE.md` (P-A: leave the dispatch alone) and #848
  (Windows single call). I found no new concrete opportunity and am not manufacturing one.

---

## Recommended order

1. **F1** — two tokens in `crates/vmem/tests/mock.rs:35` (`PAGE` → `page_size()`). Do this before
   the first push, or the macOS row task #856 added is the thing that reds.
2. **F2** — one line in `test-windows` (`cargo test -p aligned-vmem --all-features
   --no-fail-fast`), mirroring `ci.yml:790`. This is the row that makes the #848 regression test
   non-vacuous for the first time.
3. **F3** — correct the four stale counter-doc sites (`alloc_core_core_diag.rs:129-135` first, it
   is the live oracle) and add the two missing `dbg_*` forwarders for the split counters.
4. **F4, F5** — the two shipping doc claims that are factually wrong about the current code:
   `reserve_aligned`'s "unconditionally over-reserves" (plus `Cargo.toml:7`'s crates.io
   description), and the README's `into_parts` row. Both are minutes; both are publish-facing.
5. **F11 (2)** — write the `#851-#857` CHANGELOG entry the CHANGELOG itself says is owed, and
   commit the two review documents (and this one) so the commit messages that cite them resolve.
   Then push and confirm CI green on the landing SHA, per CLAUDE.md.
6. **F6, F7** — one counter doc/increment placement, one `debug_assert` against
   `dw_allocation_granularity`. Cheap; neither blocks anything.
7. **F8, F9, F10** — three small doc corrections (`is_huge`'s list continuation + missing
   paragraph + escapes, the Windows two-call `granted_huge` derivation note, `Call::Release`'s
   variant doc). Batchable in one pass.

Nothing in this list is a breaking change, and nothing here reopens a round-1 or round-2 finding.
If only two things are done before the 0.2.0 publish, make them **F1** and **F2** — together they
are what turns the previous round's CI work from "added" into "actually exercising the code it was
added for".
