# Tenth independent pre-publication review — `numa-shim` @ `472fc98`

**Author:** `@oh` (Opus, effort=high). **Reported:** 2026-08-23 18:32:20 Europe/Berlin.
**Revision reviewed:** `472fc981746891c3c131b6619c3fe5856b8040ae` (`main`), working tree
clean under `crates/numa-shim/`.
**Mode:** READ-ONLY, STATIC. No sub-agents. No file edited, no git write command run.
**Nothing was executed** — no `cargo test`, `cargo check`, `cargo build`, `clippy`,
`cargo doc`, `cargo fmt`, `cargo package`, Miri, or benchmark, per the brief. Every
finding below is a source/history read. Where a finding would ordinarily be settled by
running something, I say so explicitly and mark it UNVERIFIED-BY-EXECUTION.

**Scope:** this is the follow-up audit item 100's "Next trigger" calls for (task #1272),
after tasks #1261–#1271 landed. It re-verifies F1–F8 against the source directly rather
than against the fixing tasks' own claims, independently sanity-checks the two
owner-decision writeups, reviews task #1266's new public API, and hunts for defects
nobody has filed yet.

**Filename ASCII-only**, matching the convention the fifth and eighth audits adopted for
`scripts/verify-commit-prefixes.mjs` compatibility.

---

## 0. Verdict

**NO-GO for the next crates.io publish of `numa-shim` — and, unlike the known
exceptions, for a reason that is NOT yet tracked anywhere.**

The two exceptions the brief pre-authorises (the version bump itself, F1/task #1262;
and NUMA gate phases 2 and 4, F8/task #1270) are real but already owned. I am not
adding to them. My NO-GO rests on something else:

> **N1 (P0): the tree carries three semver-BREAKING changes to `numa-shim` 0.1.0's
> already-published public surface, all made under an explicitly-stated "before this
> crate's first publish" premise that was false at the time, none of them recorded in
> `CHANGELOG.md`, and task #1263 — the task that corrected exactly that false premise —
> corrected it for only one of the five decisions the same commit made.**

This is not a style point. It changes F1's *content*, which is still an open owner
decision (task #1262): the next release cannot be `0.1.1`, and the changelog it ships
with is currently missing its entire breaking-change section. Publishing as-is would
put a silently-breaking `0.1.x` on crates.io.

Secondary blockers, all new:

- **N2 (P1)** — `numa-shim`'s real **Linux** backend is exercised by **no per-PR CI job
  at all**, and task #1266's Linux-only module + its test file are compiled by **no CI
  job at all**, on any platform. The one variant `NodeResolution` exists to expose
  (`FellBackToZero`) has zero coverage anywhere.
- **N3 (P1)** — `numa-shim` has **zero rustdoc CI coverage**, while declaring a
  `[package.metadata.docs.rs] features` list narrower than `--all-features`; its
  crate-level rustdoc very likely emits unresolved-intra-doc-link warnings in the
  DEFAULT feature set that `cargo add numa-shim` produces.
- **N4 (P2)** — task #1269's own F7 "hardening" introduced a reservation **leak** on
  the error path it added.
- **N12 (P2)** — F8's one PASSING phase (Phase 1) was measured at `356fb44`, before
  `+256` lines of change to the very file the gate exists to protect. It is stale for
  `472fc98` even setting phases 2/4 aside.

Everything else I found is P3/INFO.

**Code-level soundness:** I found **no UB and no new soundness hole** in the Linux FFI,
the Windows FFI, the cpumap parser, the topology cache, or the new API. N4 is a
resource leak on a practically-unreachable branch, not unsoundness. The crate's actual
allocator-facing behaviour looks correct to me on a careful read.

**Per-finding disposition of F1–F8** (detail in §2):

| Finding | Owning task | State at `472fc98` | My verdict |
|---|---|---|---|
| F1 (P0) version/changelog/root-pin | #1262 | **not started** (deliberate) | OPEN — and now *understated*, see N1/N10 |
| F2 (P1) `mock` non-additive feature | #1264 | writeup landed, decision pending | OPEN by design; writeup is sound |
| F3 (P1) safety-doc divergence | #1265 | fixed in 2 of 4 places | **PARTIAL** — see N7 |
| F4 (P1) node ≥ 64 silent `Some(0)` | #1266 | additive API landed | **PARTIAL** — see §4 / N8 |
| F5 (P2) `#[doc(hidden)] pub mod` semver | #1267 | writeup landed, decision pending | OPEN by design; writeup is sound, premise verified |
| F6 (P2) README `vmem-integration` example | #1268 | fixed | **CLOSED** (unverified by compilation) |
| F7 (P2/P3) Windows hardening | #1269 | 3 of 3 addressed | **REGRESSED** — see N4, N5 |
| F8 (P1) NUMA release gate | #1270 | Phase 1 PASS, 3 partial/outstanding | OPEN — and Phase 1 is now stale, see N12 |

---

## 1. What I read

Full source of `crates/numa-shim/` (`src/lib.rs` 1550 lines, all five test files, the
bench, `Cargo.toml`, `README.md`, `CHANGELOG.md`, both licence files listed); the audit
report; item 100's card including the task-#1267 addendum; item 42's card in
`ACTIVE.md`; perf item 59; `docs/NUMA_GATE_RUN_2026-08-23_task1270.md`; the numa-shim
and adjacent jobs in `.github/workflows/ci.yml`; root `Cargo.toml`'s numa pin and
`numa-aware-mock` wiring; root `README.md`'s unsafe inventory; `Cargo.lock`; and git
history — `845560f` (the pre-publish tree), `dbfeca3`, `c5e013b`, `53b3ca2`, `fd2a3bb`,
`ec7d162`, `6075c59`, `5b842c7`, `b801b06`, `bf7e1cb`, `3a83ee8`, `e4107cf`, `50b166c`,
`5ffdc72`, `cc2fed4`.

---

## 2. F1–F8, re-verified against source

### F1 — version / changelog / root pin: OPEN, deliberately, and materially incomplete

State confirmed unchanged: `crates/numa-shim/Cargo.toml:3` is `version = "0.1.0"`;
`crates/numa-shim/CHANGELOG.md:7` is `## Unreleased`; root `Cargo.toml:914` still pins
`numa-shim = { path = "crates/numa-shim", version = "0.1", ... }`. Correct to leave —
this repo does not bump versions without an explicit request.

But F1's own checklist, as recorded in item 100 and in the audit, names four sites
(crate manifest, root pin, dated changelog section, tag). It misses two more that a
publish will expose:

- **`crates/numa-shim/README.md` pins `0.1` in three places** — lines 33 and 36 in the
  Usage `[dependencies]` block, line 64 in the `vmem-integration` block. The README is
  the crates.io landing page; shipping `0.2.0` with a README telling readers to write
  `numa-shim = "0.1"` is a self-contradiction visible on the front page.
- **The breaking-change content of the release** — N1 below. `0.1.1` is not an
  available choice.

### F2 — `mock` remains a non-additive Cargo feature: OPEN by design, writeup sound

Verified `mock = []` still present (`Cargo.toml:64`), still backed by a long hazard
comment, and still reachable graph-wide. The concrete in-repo demonstration item 42
cites is real and I re-confirmed it: root `Cargo.toml:721`
`numa-aware-mock = ["numa-aware", "numa-shim/mock"]` is a plain root feature, so every
`--all-features` build of the root crate — including `npm run check`'s own
`cargo test --all-features` step — silently runs the recording backend instead of the
platform backend. That is exactly the failure mode the feature-unification hazard
describes, occurring inside this repository today.

No new mechanism found. See §3 for my check of item 42's numbers.

### F3 — safety docs vs. real surface: PARTIAL

`ec7d162` did two things, both correct as far as they go:

- `README.md:116-121`'s `# Safety` block now matches the real contract (mapped range,
  page-granularity caveat, short-circuit exemption). Good — this was the sharper half
  of F3.
- `src/lib.rs:46-48`'s comment now reads "The public API is safe EXCEPT `bind_range`, a
  `pub unsafe fn` carrying its own documented `# Safety` contract".

Three residuals survive — see **N7**. The most consequential: the fix landed in a `//`
line comment, but the audit's complaint was about "чтение crate-level rustdoc", and the
crate-level rustdoc (`//!`, lines 1–43) still says nothing about `bind_range` being
`unsafe`; neither does the crates.io `description`, which still advertises
"forbid(unsafe_code)-friendly for consumers" without qualification. Those two are the
surfaces an external reader actually meets.

### F4 — node ≥ 64 silent `Some(0)`: PARTIAL (see §4 for the full API review)

The additive `NodeResolution` + `current_node_resolution()` landed and is well-built.
It does not, however, close the specific hazard F4 named. Two gaps, detail in §4:
`bind_range`'s node ≥ 64 skip still has **no** caller-detectable signal at all, and
`FellBackToZero` conflates "this kernel has no NUMA sysfs, so node 0 is genuinely
right" with "topology exists but this CPU's node could not be determined, so node 0 may
be wrong" — the exact distinction F4 asked for.

### F5 — `#[doc(hidden)] pub mod` semver policy: OPEN by design, writeup sound, premise verified

See §3. The addendum's central factual claim checks out, and its reasoning about why
option (a) is asymmetric and option (b) is compiler-blocked is correct on the source I
read.

One thing the addendum gets right that is easy to miss and worth restating: task #1266
**added a second** doc-hidden public module (`pub mod linux`) *after* the audit
reported, so the decision now covers strictly more surface than F5 originally named.

### F6 — README `vmem-integration` example: CLOSED

`6075c59` added an explicit `aligned-vmem = "0.2"` line to the example's `[dependencies]`
block with a comment explaining exactly why the feature alone is not enough, and
switched the example to `use aligned_vmem::{page_size, PAGE};` with a note preferring
`page_size()`. I verified both names are genuinely public in `aligned-vmem` 0.2
(`crates/aligned-vmem/src/lib.rs:201-202` re-export `PAGE` and `page_size`) and that
`reserve_on_node(ps * 16, PAGE.max(ps), node)` satisfies the documented contract for
both the 4 KiB and 16 KiB page granules.

Caveat, stated because nothing enforces it: the README's ` ```rust ` fences are not
compiled by anything — `src/lib.rs` does not `include_str!` the README, and no test
extracts them. F6's fix is therefore a *reasoned* fix, not a *verified* one. That is
acceptable (this repo bans doctests by convention), but "the example compiles" remains
an unchecked claim.

### F7 — Windows hardening: addressed on all three points, but REGRESSED

All three sub-points were touched:

1. `raw_u + align - 1` → `raw_u.checked_add(align - 1)?` (`src/lib.rs:1300`).
2. `VirtualFree`'s result is now compared to 0 (`:1332`).
3. `committed` is now compared to `base` via `debug_assert_eq!` (`:1351`).

But (1) introduced a leak (**N4**) and (2) does not actually do anything, while its
comment claims it does (**N5**). (3) is a reasonable disposition — a `debug_assert!`
plus a stated Win32-contract argument — though it is worth being explicit that in
`--release` (the only build shape that matters for a published crate) nothing is
checked; the `// SAFETY` block's "verified by debug_assert above" is true only in debug
builds.

### F8 — NUMA release gate: OPEN, and Phase 1 is now stale

`docs/NUMA_GATE_RUN_2026-08-23_task1270.md` is an honest, well-written report. It does
not overclaim: Phase 3 is explicitly labelled PARTIAL/host-level-only, Phases 2 and 4
explicitly DID NOT RUN, and the verdict says outright "do NOT cut a 0.x.y release
touching `crates/numa-shim/**` on the strength of this run alone." It also surfaces a
genuine documentation drift (the gate doc's own invocation compiles 0 tests because
`internals` is missing from the feature list) — good catch, correctly flagged rather
than silently fixed. Both cited raw logs are committed (`git ls-files` confirms
`docs/perf/_raw_numa_gate_p1_2026-08-23.log` and
`_raw_numa_gate_p3_partial_2026-08-23.log`), satisfying the raw-log citation rule.

The problem is **N12**: its measurement identity is "base commit `356fb44`", and
`git diff --stat 356fb44 HEAD -- crates/numa-shim/` reports `src/lib.rs | 256 ++++...`
plus two new test files. Phase 1's PASS therefore describes a tree that no longer
exists.

---

## 3. Independent check of the two owner-decision writeups

I checked the load-bearing factual claims rather than the recommendations (which are
owner calls, not mine).

### Item 100's F5 addendum — "neither doc-hidden module is in published 0.1.0"

**CORROBORATED, and more robustly than the addendum itself argues.**

Direct checks I ran (read-only git):

- `git show 845560f:crates/numa/src/lib.rs` — the pre-publish tree. Its complete public
  surface is: `NO_NODE`, `pub mod mock` (with `MockCall`, `CALLS`, `CURRENT_NODE_SLOT`,
  `drain`, `set_current_node`), `current_node`, `bind_range`, `reserve_on_node`. Zero
  occurrences of `doc(hidden)`; zero occurrences of `pub mod cpumap`. Confirmed.
- `git log -S "pub mod cpumap" -- crates/` → first introduced by `c5e013b`,
  **2026-08-09** (task #721).
- `git log -S "dbg_node_resolution_for_cpu" -- crates/` → first introduced by `3a83ee8`,
  **2026-08-23** (task #1266).

The addendum's argument depends on `845560f` being the exact published tree. Mine does
not need that assumption: both modules were introduced *six weeks and two months*
respectively after **any** 2026-06-29 publish date, so the conclusion holds for any
plausible published tree. The addendum's conclusion is right; its evidence is narrower
than it needed to be.

The one claim I could **not** re-verify in read-only mode is the underlying publish
event itself (2026-06-29 17:36:48 UTC), which rests on task #1263's crates.io API query.
Everything in this report that depends on it depends only on "0.1.0 was published at
some point in late June 2026", which the CHANGELOG, the `homepage` field still pointing
at the pre-rename `crates/numa` path in the 0.1.0 manifest, and the docs.rs/crates.io
badges in the root README all independently corroborate.

Two smaller claims in the same addendum, both spot-checked and correct:

- "`cpumap`'s parser cannot live in a dev-only crate because production calls it" —
  correct: `crate::cpumap::format_sysfs_path` at `src/lib.rs:867` and
  `crate::cpumap::parse_contains_cpu` at `:888`, both inside `topology()` /
  `cpu_to_numa_node_checked`, i.e. on the real `current_node()` path.
- "`linux::dbg_node_resolution_for_cpu` forwards to `pub(crate)` internals no external
  crate can reach" — correct: `platform::cpu_to_numa_node_checked` is `pub(crate)`
  (`:884`), and `mod platform` is private.

### Item 42's F2 recommendation (task #1264)

**Central claims all check out.** Spot-checked:

- `numa-aware-mock = ["numa-aware", "numa-shim/mock"]` at root `Cargo.toml:721` — yes,
  and it is a *feature*, not a dev-dependency, so `--all-features` reaches it. The
  card's "this repo itself demonstrates how easily that happens" is literally true.
- "27 `feature = "mock"` cfg sites across 5 files" —
  `grep -rn 'feature = "mock"' crates/numa-shim/ | wc -l` → **27**. Exact.
- "8 root test files, 6 of them `use numa_shim::mock;`" — `grep -rl 'numa-aware-mock'
  tests/` → 8 files; `grep -rl 'numa_shim::mock' tests/` → 6 files. Exact.
- "`[[bench]] numa_bench`'s `required-features = ["mock"]` must be resolved because a
  bench cannot require a cfg flag" — correct; `Cargo.toml:76-79`.
- "the next release is already 0.2.0-shaped and already breaking" — **correct, and
  understated**: the card grounds this only in the `aligned-vmem` 0.1→0.2 return-type
  move. N1 below shows the tree is already breaking on three further, unrecorded axes.
  This *strengthens* the card's own recommendation (option (a) at the 0.2.0 boundary),
  since the marginal semver cost of removing `mock` is now provably zero.

I have no correction to offer to either writeup. Both are honest about being
recommendations rather than decisions, which is the right posture.

---

## 4. Review of task #1266's `NodeResolution` API

### Soundness: clean

`current_node_resolution()` is a safe `fn` with no pointer arguments, `#[must_use]`, and
no `unsafe` in its own body. Its Linux arm calls `sched_getcpu` through the same
already-audited wrapper `current_node_impl` uses, then delegates to the same
`cpu_to_numa_node_checked` the existing path uses — it shares the real implementation
rather than duplicating it on the one platform where the logic is non-trivial. The
`linux::dbg_node_resolution_for_cpu` forwarder takes a `u32`, not a pointer, so the
CLAUDE.md R25-1 `dbg_*`-hook *safety* rule (raw-pointer hooks must be `unsafe fn`) does
not apply. No soundness concern.

### "Does not change any existing function's behavior": TRUE

I checked this specifically, since it is the additive claim's load-bearing part.
`current_node()`'s body is byte-for-byte what it was: the mock arm still records
`MockCall::CurrentNode(n)` and remaps `NO_NODE` → `None`; the real arm still calls
`platform::current_node_impl()`. `bind_range` and `reserve_on_node` are untouched. Every
platform module gained a new `current_node_resolution_impl` alongside its existing
`current_node_impl` without modifying the latter. Confirmed additive.

### API design: good, with four criticisms

**(a) It only partially achieves what it claims.** The doc says the enum lets callers
distinguish "genuinely resolved" from "silently fell back to node 0". On Linux it does —
but `FellBackToZero`'s own doc enumerates three causes and treats them identically:

1. sysfs unreadable (topology exists, answer possibly wrong);
2. CPU's real node ≥ 64 (topology exists, answer **is** wrong — F4's actual hazard);
3. kernel has no NUMA sysfs at all (topology absent, node 0 is genuinely correct).

Case 3 is *not* a fallback in any meaningful sense; cases 1–2 are. A caller on an
ordinary `CONFIG_NUMA=n` desktop gets the same "warning" signal as a caller on a
128-node machine whose binding is silently wrong. This is exactly the conflation F4
complained about (`«caller не получает отличия между single-node fallback и ошибкой
определения»`), moved one level down rather than removed. The distinction is cheaply
available — `topology()` already knows whether *any* node's `len > 0` — so a fourth
variant (or a companion "was topology readable at all" accessor) would close it. The
enum is `#[non_exhaustive]`, so adding one later is non-breaking; but the *decision*
belongs in the release that freezes the enum.

**(b) It addresses node DETECTION, not the SKIPPED BINDING.** F4's remediation text asks
for "API, позволяющий caller обнаружить пропуск binding". `bind_range(ptr, len, 100)`
still returns `()` and silently no-ops (`src/lib.rs:1030`, `if node == NO_NODE || node
>= 64 { return; }`), with no counter, no return value, no diagnostic. A caller that
correctly obtains `Resolved(100)` from some other source and passes it to `bind_range`
gets exactly the pre-#1266 silence. The commit subject's honest "(owner decision on
default behavior still open)" acknowledges *something* is open; neither the commit body
(which is empty — see N11) nor the CHANGELOG entry states which part.

**(c) `FellBackToZero` is unreachable under `mock` and untested everywhere.** The mock
arm maps `NO_NODE` → `Unavailable` and everything else → `Resolved(n)`. There is no way
to script `FellBackToZero`. So the crate's own "so CI can assert the wrapping logic on
any target" feature cannot exercise the single state the new API exists to expose. Its
only real oracle is `tests/node_resolution_linux.rs`, which — per **N2** — runs in no
CI job. Net: `FellBackToZero` has zero automated coverage of any kind. This is the same
shape of defect task #722 fixed for `current_node` (a state that consumers must handle
but the mock could not produce), reintroduced one function over.

**(d) The documented mapping table is an unenforced claim.** The rustdoc publishes a
three-row correspondence between `current_node_resolution()` and `current_node()`. On
Windows/macOS/miri/fallback the two functions are *parallel implementations*, not one
derived from the other — `platform::current_node_impl` and
`platform::current_node_resolution_impl` in the Windows module (`:1134` and `:1166`) are
near-identical copies of the `GetCurrentProcessorNumberEx` + `GetNumaProcessorNodeEx` +
`ok == 0 || node == u16::MAX` sequence. A future fix to one (a new sentinel, a new
error code) can miss the other, and nothing — no test, no type — would notice. Defining
`current_node_impl` in terms of `current_node_resolution_impl` on every platform would
make the published table true by construction. Cheap, and worth doing before the table
becomes a published guarantee.

Smaller notes: `Resolved(u32)` carries no `#[non_exhaustive]`, unlike the sibling
`MockCall` variants whose analogous decision the crate documents at length — no
justification is recorded either way. And the doc gives no warning that
`current_node()` and `current_node_resolution()` called back-to-back can disagree if the
thread migrates between CPUs.

---

## 5. New findings

Severity uses this campaign's P0–P3 scale. Every finding names file+line and is a source
read; none is execution-verified.

### N1 (P0) — Three unrecorded semver-breaking changes to published 0.1.0's `mock` surface, made under a false "before first publish" premise that task #1263 corrected for only one of five decisions

**What 0.1.0 published.** From `git show 845560f:crates/numa/src/lib.rs`:

```rust
    #[derive(Debug, Clone, PartialEq, Eq)]        // no #[non_exhaustive]
    pub enum MockCall {
        CurrentNode(u32),
        BindRange { base: usize, len: usize, node: u32 },      // no #[non_exhaustive]
        ReserveOnNode { size: usize, align: usize, node: u32 }, // no #[non_exhaustive]
    }

    std::thread_local! {
        pub static CALLS: RefCell<Vec<MockCall>> = const { RefCell::new(Vec::new()) };
        pub static CURRENT_NODE_SLOT: RefCell<u32> = const { RefCell::new(0) };
    }
```

**What `472fc98` ships** (`src/lib.rs:123-179`): `#[non_exhaustive]` on the enum, on
`BindRange`, and on `ReserveOnNode`; both thread-locals narrowed to `pub(crate)`.

**Each is breaking for a `--features mock` consumer of 0.1.0:**

- `pub` → `pub(crate)` on `CALLS`/`CURRENT_NODE_SLOT` is an item *removal*.
- Enum-level `#[non_exhaustive]` breaks any downstream exhaustive `match` on `MockCall`.
- Variant-level `#[non_exhaustive]` breaks struct-literal construction and
  exhaustive field patterns.

**The last one is self-proven inside this repository.** `53b3ca2`'s own commit body
records: *"tests/mock_dispatch.rs's own BindRange struct-literal CONSTRUCTION
(bind_range_records_args) and ReserveOnNode field-pattern match
(reserve_on_node_chains_and_records) both failed to compile immediately after the change
(integration tests compile as a separate crate, so the same enforcement downstream
consumers would hit applies here too)"*. That is a demonstration that a downstream crate
breaks, written down by the change that caused it.

**The premise.** `53b3ca2`'s opening line: *"rust-intel audit, all decided now, **before
this crate's first crates.io publish (task #657)** -- retrofitting any of these later is
itself a breaking change"*. The publish had happened six weeks earlier. Timeline:

| Date | Commit | Change | Semver effect vs. published 0.1.0 |
|---|---|---|---|
| 2026-06-29 | — | **0.1.0 published** | baseline |
| 2026-07-19 | `dbfeca3` | enum-level `#[non_exhaustive]` on `MockCall` | **breaking** |
| 2026-08-09 | `53b3ca2` (#726) | `CALLS`/`CURRENT_NODE_SLOT` → `pub(crate)` | **breaking** |
| 2026-08-09 | `53b3ca2` (#726) | `#[non_exhaustive]` on both struct variants | **breaking** |
| 2026-08-23 | `50b166c` (#1263) | corrected the premise — for decision (5), the `mock` **feature**, only | — |

`50b166c` touched exactly two files (`crates/numa-shim/Cargo.toml`,
`docs/correctness-open-items/ACTIVE.md`). It did not touch `CHANGELOG.md`, and it did
not revisit decisions (1)–(4) of the very commit whose premise it was correcting. That
is the miss: the correction was scoped to the *finding that prompted it* rather than to
the *commit that shared its premise*.

**Consequences for the release, concretely:**

1. The next version **cannot be `0.1.1`**. Under Cargo/SemVer 0.x rules, breaking
   changes require the minor bump — `0.2.0`. F1/task #1262 is an open owner decision
   today; this removes one of its options.
2. `CHANGELOG.md`'s `## Unreleased` section has `### Fixed`, `### Added`, `### Changed`
   and two `### Owner decisions pending` blocks — and **no `### Removed` and no
   breaking-change callout**. A reader upgrading `0.1.0 → 0.2.0` is told about the
   `aligned-vmem` return-type move and nothing else.
3. Item 42's own recommendation gets *stronger*, not weaker: if 0.2.0 is already
   breaking on three additional axes, the marginal cost of also removing `mock` (option
   (a)) is provably zero.

**Recommended fix:** add a `### Removed` / breaking-changes block to `CHANGELOG.md`'s
Unreleased section naming all three, and record in item 42 (or item 100) that #1263's
premise correction has an unswept remainder. Also worth checking whether any *other*
decision made in this crate between 2026-06-29 and 2026-08-23 carried the same "not yet
published" reasoning — I checked `53b3ca2`, `dbfeca3`, `fd2a3bb`, `c5e013b`; I did not
sweep the whole range exhaustively.

*Non-breaking metadata drift found alongside, worth one changelog line each:* the
`categories` list dropped `"no-std::no-alloc"` (correct — the crate uses
`std::thread_local!` and `std::sync::OnceLock`) and `homepage` moved from
`.../crates/numa` to `.../crates/numa-shim`. Neither is in the changelog.

### N2 (P1) — numa-shim's real Linux backend has no per-PR CI coverage, and task #1266's Linux module + test are compiled by no CI job at all

Reading every `numa-shim` step in `.github/workflows/ci.yml`:

| Job | Runner | Steps |
|---|---|---|
| `numa-shim-mock` (`:2611`) | ubuntu | `test --features mock`; `test --features "mock vmem-integration"`; `clippy --all-features --all-targets` |
| `numa-shim-windows` (`:2634`) | windows | `test` (default); `test --features vmem-integration`; `test --features mock`; `test --features "mock vmem-integration"`; clippy ×2 |
| `numa-shim-macos` (`:2663`) | macos | `test`; `test --features vmem-integration`; `test --features mock` |
| `numa-shim-macos-miri` (`:2677`) | macos | `miri test` ×3 |
| `numa-real-kernel` (`:2712`) | ubuntu, **weekly/dispatch only** | root-crate targets only (`--test numa_alloc` / `numa_segment_id` / `numa_seam`); no `-p numa-shim` step |

Two consequences:

**(a) No job ever runs `cargo test -p numa-shim` on Linux without `mock`.** The only
ubuntu job runs `mock` in both test steps, and its clippy row uses `--all-features`
(which turns `mock` on). So `smoke.rs`'s `current_node_returns_valid_or_none` and
`bind_range_on_owned_memory_does_not_panic` — the only tests that exercise
`sched_getcpu` + the sysfs reader + the real `mbind(2)` syscall — run on Windows and
macOS, where all three are no-ops or absent, and never on Linux. For a crate whose
headline is `mbind(2)` via raw `syscall(2)`, its flagship path has no per-PR test. The
weekly `numa-real-kernel` job does reach that code, but through the *root* crate's
tests, not numa-shim's own suite.

**(b) `pub mod linux` and `tests/node_resolution_linux.rs` are compiled by nothing.**
Both are gated `all(target_os = "linux", not(miri), not(feature = "mock"))`. Linux ⇒ only
the ubuntu jobs; `not(mock)` ⇒ none of them. The Windows/macOS jobs fail the `target_os`
arm. So neither the module nor the test has ever been type-checked in CI on any
platform, and — given this repo's dev host is Windows — plausibly never anywhere.

`tests/node_resolution_linux.rs:4-6` states the opposite in its own header:

> *"This file is gated on Linux (non-mock, non-miri) and is exercised by CI's plain
> `cargo test -p numa-shim` on ubuntu-latest."*

There is no such step. This is a false claim in a test file's own documentation, of
exactly the class F3 was raised about.

**Combined with §4(c):** `NodeResolution::FellBackToZero` cannot be produced under
`mock`, and its only real test never runs. The new API's most important state has zero
coverage.

**Fix is one line:** add `- run: cargo test -p numa-shim` (and optionally
`--features vmem-integration`) to the `numa-shim-mock` job — or better, rename that job,
since it is no longer mock-only. Cheap, and it closes both (a) and (b).

*Related, lower:* `numa-shim` is not in the weekly `feature-powerset` job (which covers
the root crate at `:2949` and `aligned-vmem` at `:2979`). With 3 features that is 8
combinations; the hand-written rows above already cover 5 of them, so the marginal value
is small — mentioned for completeness, not as a blocker.

### N3 (P1) — Zero rustdoc CI coverage, a narrower-than-`--all-features` docs.rs feature list, and probable broken intra-doc links in the default feature set

`crates/numa-shim/Cargo.toml:27-28` declares:

```toml
[package.metadata.docs.rs]
features = ["vmem-integration"]
```

CLAUDE.md's own doc-lint rule (the "fifth instance of the meta-pattern", task #1142)
requires that any crate with a `package.metadata.docs.rs.features` list narrower than
`--all-features` carry a CI doc-lint row building **exactly that list** with
`RUSTDOCFLAGS="-D warnings"`, *in addition to* an `--all-features` row.

`numa-shim` has **neither**. Grepping every `cargo doc` invocation in `ci.yml`:

- `:65` — `RUSTDOCFLAGS="-D warnings" cargo doc -p sefer-region --all-features --no-deps`
- `:329` — `RUSTDOCFLAGS="-D warnings" cargo doc -p aligned-vmem --all-features --no-deps`
- `:354` — `RUSTDOCFLAGS="-D warnings" cargo doc -p aligned-vmem --no-deps --features "$FEATURES"` (the docs.rs list, derived from `cargo metadata`)
- `:2871` — `cargo doc --no-deps --all-features` in the `docs` job, with
  `RUSTDOCFLAGS: "-D warnings"` deliberately **removed**, and with no `-p`, so it
  documents the root package only; `--no-deps` excludes `numa-shim` even though
  `--all-features` compiles it.

So no rustdoc warning from `numa-shim` can fail any job, and none can even be *emitted*.
The rule is non-retroactive and `numa-shim`'s docs.rs metadata predates it, so this is
not a rule violation per se — but this crate is about to publish, and its docs.rs render
is the landing page.

**And there is very likely something to find.** The crate-level rustdoc is unconditional
(lines 1–43), but references `reserve_on_node` — an item that only exists under
`vmem-integration` — with intra-doc-link syntax in at least three places:

- `:32` — `` | `vmem-integration` | Enables [`reserve_on_node`], which uses … `` (in the
  feature-flag table)
- `:40` — `` | Windows … | [`reserve_on_node`] (feature) | `` (in the platform matrix; the
  same link also appears in the Linux/macOS/miri/other rows)
- `:400` — `` use [`reserve_on_node`] (with the `vmem-integration` feature) `` in
  `bind_range`'s own docs, which is also unconditional

Under **default features** — the configuration `cargo add numa-shim` produces —
`reserve_on_node` does not exist, so `rustdoc::broken_intra_doc_links` (warn-by-default)
should fire on each. **UNVERIFIED-BY-EXECUTION**: I did not run `cargo doc`. But the
mechanism is identical to the `aligned-vmem` case CLAUDE.md's rule was written from, and
no gate anywhere in this repo would have caught it.

Under the docs.rs set (`vmem-integration`, no `mock`) I traced every intra-doc link in
`src/lib.rs` by hand and found none that fails to resolve — `Reservation` resolves via
the `pub use aligned_vmem::Reservation` re-export, `aligned_vmem::*` links resolve
because the crate is a real dependency in that configuration, and every link inside
`pub mod mock` is cfg'd out along with the module. So the docs.rs render itself looks
clean; it is the default-feature render that is suspect.

**Recommended:** add two `cargo doc` rows for `numa-shim` mirroring the `aligned-vmem`
pattern at `:329`/`:354` (`--all-features`, and the docs.rs list derived from
`cargo metadata` rather than hand-copied), and decide how the unconditional crate-level
doc should reference a feature-gated item — the usual fix is `` `reserve_on_node` `` in
plain code font in the always-compiled table, with the live link kept inside the gated
item's own docs.

### N4 (P2) — Task #1269's F7 fix leaks the `MEM_RESERVE`d region on the error path it introduced

`crates/numa-shim/src/lib.rs:1282-1302`:

```rust
        let raw = unsafe { VirtualAllocExNuma(GetCurrentProcess(), null_mut(), over,
                                              MEM_RESERVE, PAGE_READWRITE, node) };
        if raw.is_null() {
            return None;
        }
        let raw_u = raw as usize;
        // Checked alignment arithmetic: …
        let rounded = raw_u.checked_add(align - 1)?;   // <-- returns None; `raw` is never freed
        let base_u = rounded & !(align - 1);
```

The `?` returns `None` **after** a successful `MEM_RESERVE` and **without** calling
`VirtualFree(raw, 0, MEM_RELEASE)`. The reservation leaks for the process lifetime, and
the caller gets an indistinguishable `None`.

The inconsistency is self-evident three statements later: the sibling commit-failure
branch at `:1328-1346` — added by the *same task* — carefully releases `raw` before
returning `None`, and its comment explains at length why an unreleased reservation
matters. The new arithmetic branch does the exact thing that comment warns about.

**Reachability:** requires `raw + align - 1` to overflow `usize`, i.e. a Win32
reservation within `align` bytes of `usize::MAX`. On 64-bit Windows, user-mode VA tops
out far below that, so this is practically unreachable — which is precisely why the
pre-#1269 code used unchecked arithmetic. The finding is not "this happens"; it is that
a hardening change added a *new* early-return without extending the ownership discipline
the function already had, converting a theoretical wrap into a theoretical leak. This is
the boundary/error-path-resource-lifecycle class rust-intel names.

**Fix:** two lines —

```rust
        let Some(rounded) = raw_u.checked_add(align - 1) else {
            // SAFETY: `raw` came from the MEM_RESERVE above and was never handed out.
            unsafe { VirtualFree(raw, 0, MEM_RELEASE) };
            return None;
        };
```

### N5 (P3) — The same F7 fix's error-path comment claims a counter that does not exist, and its `if` body is empty

`src/lib.rs:1332-1344`:

```rust
            if unsafe { VirtualFree(raw, 0, MEM_RELEASE) } == 0 {
                // Double-failure: … (Matched to this file's style: other "nothing more
                // we can do" cleanup paths release silently; this one is at least
                // counted, not silently ignored.)
                // Note: No logging here — …
            }
```

Nothing is counted. The block contains only comments; there is no counter, no flag, no
diagnostic. Functionally this is identical to the pre-#1269 `let _ = VirtualFree(...)`,
so F7's actual complaint (*"не имея способа сообщить возможный leak"* — no way to report
the possible leak) is **not** addressed, only commented about. The phrase "at least
counted" is a false statement in a comment, in a crate where a previous fixup
(`b801b06`) already had to remove a different false claim from this same comment block.

Two acceptable resolutions: (i) delete the `if` and restore `let _ = …` with a comment
that honestly says the failure is unreportable through an `Option`-returning signature;
or (ii) actually add the counter (a `static AtomicUsize` behind a diagnostic accessor)
so the comment becomes true. Do not leave an empty conditional whose comment asserts an
effect.

*Also, UNVERIFIED-BY-EXECUTION:* an `if cond { }` whose block contains only comments is
an empty block to the AST, which is what `clippy::needless_if` (complexity, default-on)
targets. This code is compiled on the `numa-shim-windows` clippy rows with
`-D warnings`. I did not run clippy and cannot say whether the lint fires here; worth
checking, because if it does, `main` is red for that job.

### N6 (P3) — `current_node_resolution()` is not recorded under `mock`, contradicting the `mock` module's own contract, and the stated reason for that is factually wrong

`src/lib.rs:70-72` (the `mock` module's rustdoc): *"Records **every** invocation into a
thread-local buffer so unit tests can assert the wrapping logic is correct on any
target"*.

`src/lib.rs:311-322`:

```rust
        let n = mock::current_node_slot();
        // Note: we do NOT record this call — keep it simple, mirroring the
        // mapping without adding to the mock log. Recording would change
        // mock log contents and break existing tests' expectations.
```

Two problems. First, `current_node_resolution` is a public NUMA function that reads the
mock's scripted state and returns without recording — so the module doc's "every
invocation" is now false, and a test cannot assert that a consumer called it.

Second, the justification is wrong on the facts. Every existing test opens with
`fresh_drain()`; `mock_dispatch.rs`'s two equality-oracle assertions
(`assert_eq!(calls, vec![MockCall::CurrentNode(n)])`) are in tests that never call
`current_node_resolution()`; and `node_resolution.rs`'s three tests never inspect the
log at all. Adding a `MockCall::CurrentNodeResolution(..)` variant would be additive
(the enum is `#[non_exhaustive]`) and would break nothing. So the reason given for a
deliberate contract deviation does not hold — either record it, or amend the module doc
to say which functions are recorded.

### N7 (P3) — Three F3 residuals: the two most-read surfaces still carry the unqualified claim, and the corrected sentence is still partly false

1. **Crate-level rustdoc (docs.rs landing page) says nothing.** The fix landed at
   `src/lib.rs:46-48`, which are `//` line comments, not `//!` doc comments — invisible
   in rendered docs. Lines 1–43 (the actual crate rustdoc) never mention that
   `bind_range` is `unsafe`. The audit's F3 was explicitly about
   *"чтение crate-level rustdoc"*.
2. **The crates.io description still says it unqualified.** `Cargo.toml:7` ends
   `"… forbid(unsafe_code)-friendly for consumers."` A consumer with
   `#![forbid(unsafe_code)]` literally cannot call `bind_range`. The claim is true for
   `current_node`/`reserve_on_node` and false for the third of three functions; the
   description is the first thing every crates.io visitor reads and was not touched by
   `ec7d162`.
3. **The corrected sentence is still factually wrong in its second half.** It says
   *"all other unsafe is confined to platform modules"*. It is not:
   `unsafe fn bind_range_impl_linux` (`:1025`), `unsafe fn libc_mbind` (`:1100`), and
   the `extern "C" { fn syscall(...) }` block (`:1090`) all live at the crate root,
   outside every `mod platform`. The comment block was rewritten by the fix and this
   half was carried over unchanged.

### N8 (P3) — F4's node ≥ 64 hazard still has no caller-detectable signal, and `FellBackToZero` conflates two materially different situations

Detail in §4(a)/(b). Filed as its own finding because F4 is currently recorded as
"actioned by #1266" and the residual is not written down anywhere: neither the
CHANGELOG entry (`crates/numa-shim/CHANGELOG.md:63-72`, which describes the enum without
noting what it does not cover) nor item 100 (whose Status card, per N9, has not been
refreshed at all) records that F4's binding-side ask is untouched.

### N9 (P3) — Item 100's Status card is stale: it still reads as if nothing has landed

`docs/correctness-open-items/TRACKED_publish_readiness.md:396-408` still says
*"**Status:** OPEN — none of F1-F8 or the perf observations actioned by this filing
task"*, *"**Current-number-or-verdict:** NO-GO stands at `87424a4`"* with all eight
findings described in the present tense, and *"**Next trigger:** F1-F8 landing (tasks
#1262/#1264-#1270)"* — after #1263, #1265, #1266, #1268, #1269, #1270 and both writeup
tasks have landed. The only update is the F5 addendum appended at line 411, *below* the
card, so the card's first visible block — the one CLAUDE.md's R34-24 rule makes
load-bearing for round-start reading — is wrong.

This campaign already has the fix as an established pattern: `54f69e9` ("refresh item
97's Status card now that F2-F4 landed"), `4850234` (item 98), `2ebc3c5` (item 99). Item
100 is the one that did not get its refresh commit.

### N10 (P3) — CHANGELOG structure: duplicate heading, and (with N1) a missing section

`crates/numa-shim/CHANGELOG.md` has `### Owner decisions pending` **twice** inside the
same `## Unreleased` section — once at line 15 (the F2/`mock` decision) and once at line
83 (the F5/doc-hidden decision). Two identically-named sibling headings in one release
block are easy to lose when the section is consolidated under a dated version header at
release time, which is exactly the operation that is pending. Merge them into one
section with two bullets. (Combine with N1's missing `### Removed`.)

### N11 (INFO) — Smaller items, listed for completeness

- **`dbg_node_resolution_for_cpu` is reachable in a default-feature build.** CLAUDE.md's
  R25-1 rule (2) says a hook with no production caller *"MUST default to gating behind
  the `bench-internals` feature, not production-composition features."* This crate has no
  `bench-internals` feature, and the hook is gated only on target/`not(mock)`, so on
  Linux it is present in the exact configuration `cargo add numa-shim` produces. It is
  safe and harmless, so this is a convention point, not a defect — but it reinforces
  item 100's F5 addendum: whatever policy the owner picks, this hook is currently on
  track to be frozen into the published surface of a default build.
- **Windows `current_node_impl` / `current_node_resolution_impl` are copy-paste
  parallels** (`:1134`, `:1166`) — see §4(d).
- **`#1266`'s main commit `3a83ee8` has an empty body** (subject line only, then trailer).
  Every other commit in this wave carries a substantive body; for a commit that adds a
  permanent public API and leaves a decision open, the reasoning is now recoverable only
  from the code.
- **Root `README.md:579` still calls `numa-shim` "~300 lines"**; `src/lib.rs` is 1550.
- **`src/lib.rs:1351` is a 151-character line** (the `debug_assert_eq!` added by #1269)
  in a repo with no `rustfmt.toml`, i.e. `max_width = 100`, and `cargo fmt --all --
  --check` runs both in CI (`ci.yml:51`) and in `npm run check`. My reading of rustfmt's
  behaviour is that it gives up and leaves the macro verbatim when even the vertical
  form cannot fit the unsplittable string literal, so this most likely does *not* fail
  the gate — but I did not run it. **UNVERIFIED-BY-EXECUTION**; cheap to confirm.
- **Publish precondition, not a defect:** `numa-shim`'s manifest requires
  `aligned-vmem = "0.2"` from the registry once the `path` is stripped at publish time.
  `crates/aligned-vmem/CHANGELOG.md:7` carries a dated `## 0.2.0 - 2026-08-23` header
  (stamped by `87424a4`/task #1260), which implies it shipped; I could not re-query
  crates.io in read-only mode. Confirm before running `cargo publish` for `numa-shim`.
  The `bench-scale-tool = "0.1"` dev-dependency resolves from the registry per
  `Cargo.lock:64-67` — fine.

### N12 (P2) — F8's Phase 1 PASS was measured on a superseded tree

`docs/NUMA_GATE_RUN_2026-08-23_task1270.md:44` records its measurement identity as
"base commit 356fb44". Confirmed: `bf7e1cb`'s parent is `356fb44`, and
`git diff --stat 356fb44 HEAD -- crates/numa-shim/` reports `src/lib.rs | 256 +++…`,
`Cargo.toml | 14`, `README.md | 24`, `CHANGELOG.md | 35`, plus the two new test files.
The report's own numbers corroborate the staleness independently: it counts "28 passed
across 4 test binaries", whereas `472fc98` under `--features mock` has five test
binaries (`node_resolution.rs`'s three tests did not exist yet).

`docs/NUMA_RELEASE_GATE.md` requires the gate when the diff touches
`crates/numa-shim/**`. Since the Phase 1 run, `#1266` added a public API and `#1269`
changed the Windows reserve path — both in that tree. Re-running Phase 1 costs one
command and would newly cover `node_resolution.rs`; there is no reason to carry a stale
PASS into the release record. Phases 2 and 4 remain outstanding for the already-recorded
infrastructure reasons.

---

## 6. What I checked and found clean

Stated so this report does not read as if only defects were looked for.

- **Linux raw FFI.** `sched_getcpu`/`open`/`read`/`close` signatures against their POSIX
  prototypes; every `unsafe` block carries a site-specific `// SAFETY:`; `read_cpumap_into`
  loops to EOF, bounds every write to `out[total..]`, closes `fd` on all three exits
  (overflow, read error, success) exactly once, and fails closed on truncation. The
  `maxnode = 65` for a 64-bit nodemask is correct and correctly explained (the kernel's
  `get_nodes()` decrement quirk); node 63 is addressable.
- **cpumap parser.** Most-significant-word-first indexing, `target_word >= word_count`
  guard, `>8`-digit rejection, empty/invalid-token rejection — all fail closed, none can
  panic or index out of bounds. `format_sysfs_path`'s `[u8; 10]` digit buffer covers the
  full `u32` range and the 64-byte output buffer has 17 bytes of slack at `u32::MAX`.
  `tests/cpumap_parser.rs`'s 17 tests are real behavioural oracles with negative
  controls, not smoke tests.
- **Topology cache.** Allocation-free by construction (`[[u8; 1024]; 64]` + `[usize; 64]`
  inside a `OnceLock`), so the `get_or_init` reentrancy hazard task #777 fixed is
  structurally gone, not merely guarded. The torn-snapshot hotplug window is documented
  as its own property.
- **Windows FFI.** `PROCESSOR_NUMBER` mirror pinned by const-eval size/align/offset
  assertions; `ok == 0 || node == u16::MAX` correctly separates call failure from the
  `MAXUSHORT` non-existent-processor sentinel; reserve/commit/release ownership is
  sound (`over = size + align` is a `PAGE` multiple, `base + size <= raw + over` holds by
  construction, `Drop` releases the whole span once) — apart from N4's early return.
- **`from_raw_parts` call.** Its six arguments match `aligned-vmem` 0.2's signature
  (`crates/aligned-vmem/src/reservation.rs:1378-1385`), and each documented precondition
  is discharged by the surrounding code.
- **cfg matrix.** The four `mod platform` blocks are mutually exclusive (the macOS block
  carries the `not(miri)` guard whose absence once caused E0428); `mock`'s
  `allow(dead_code)` cfg_attrs are applied at every site that needs them; `#[doc(hidden)]`
  correctly exempts both hidden modules from `#![deny(missing_docs)]`.
- **`mock` recording.** `try_with` + `try_borrow_mut` makes `record()` reentrancy-safe
  under a sefer-alloc-as-global scenario, dropping only the log entry and never the
  returned value; `CALLS_CAP` bounds the log and `drain()` documents the truncation.
- **Additivity of #1266.** `current_node`, `bind_range`, `reserve_on_node` are
  byte-for-byte unchanged (§4).
- **Docs.rs feature list.** The deliberate explicit list (rather than
  `all-features = true`, which would render `mock` as reference API) is the right call
  and is well justified in the manifest comment.
- **Raw-log citation discipline.** Both logs cited by the F8 gate report are committed.
- **Perf item 59.** All five observations filed, each marked unmeasured, with P3
  correctly cross-referenced to correctness item 45 rather than duplicated.

---

## 7. Recommended order before publish

1. **N1** — decide the version (`0.2.0`, not `0.1.1`) and write the missing
   breaking-change section; note the unswept remainder of #1263's premise correction in
   item 42 or item 100. This is F1's real content and must precede everything else.
2. **N4** — release `raw` on the `checked_add` failure path. Two lines.
3. **N5** — make the double-failure branch either honest or effective; check whether
   `clippy::needless_if` fires on it.
4. **N2** — add the missing `cargo test -p numa-shim` step on ubuntu; this is the
   cheapest large coverage win in this report and unblocks any claim about the Linux
   backend or about `FellBackToZero`.
5. **N3** — add the two `cargo doc` rows (all-features + docs.rs list derived from
   `cargo metadata`) and fix whatever the default-feature render reports.
6. **F2 / F5** — the two owner decisions. Both writeups are ready; N1 strengthens the
   case for item 42's own recommendation (option (a)).
7. **N6, N7, N8, N10** — doc/contract corrections; each is small and each is in the
   class the audit already found this crate prone to.
8. **N9** — refresh item 100's Status card, matching the pattern items 97/98/99 already
   established.
9. **N12 + F8** — re-run Phase 1 on the landing revision; decide explicitly (with the
   owner) whether to ship with Phases 2 and 4 outstanding plus a release-note record, or
   to obtain them first. `docs/NUMA_RELEASE_GATE.md` requires one of the two.
10. **Then** re-check the `aligned-vmem 0.2` registry precondition and publish.

**Summary verdict: NO-GO.** Not primarily for the two pre-authorised exceptions, but for
N1 — an unrecorded breaking-change set that makes the currently-open version decision
wrong if taken as `0.1.1`, and makes the shipped changelog incomplete either way. N2,
N3, N4 and N12 are the supporting blockers. The crate's *code* is in good shape: I found
no UB, no unsoundness, and no correctness defect in the allocator-facing paths.
