# Eleventh independent pre-publication review — `numa-shim` @ `1043b0e`

**Author:** `@oh` (Opus, effort=high). **Reported:** 2026-08-23 19:22:30 Europe/Berlin.
**Revision reviewed:** `1043b0e37e403d807b8852b1aa58615edd148bcc` (`main`, local),
working tree clean under `crates/numa-shim/`.
**Mode:** READ-ONLY, STATIC. No sub-agents. No file edited, no git write command run.
**Nothing was built or executed** — no `cargo test`, `cargo check`, `cargo build`,
`clippy`, `cargo doc`, `cargo fmt`, `cargo package`, Miri, or benchmark, per the brief.
Every source-level finding below is a read, not a run; where a finding would ordinarily
be settled by running something I mark it **UNVERIFIED-BY-EXECUTION**.

**One exception to "static":** I made three read-only crates.io registry API queries
(`GET /api/v1/crates/{numa-shim,aligned-vmem,bench-scale-tool}/versions`). These are
network reads, not builds, and they let me *independently confirm* two premises that the
ninth and tenth reviews could only take on trust — see §6. Their results are quoted
verbatim.

**Scope:** this is the review task #1280 calls for, after tasks #1274–#1279 landed
against the tenth review's N1–N12
(`docs/reviews/2026-08-23-183220-numa-shim-publication-readiness-review-oh.md`, item
101). It re-verifies each of those six tasks' claimed fixes **against current source
rather than against their own commit messages**, hunts specifically for the
fix-introduces-a-regression pattern this campaign has now hit twice, checks what N1–N12
left open, and looks for defects nobody has filed.

**Finding IDs:** the ninth audit used `F1–F8`, the tenth used `N1–N12`. This one uses
**`E1–E11`** ("eleventh"). Severity uses this campaign's P0–P3 scale.

**Filename ASCII-only**, matching the convention the fifth and eighth audits adopted for
`scripts/verify-commit-prefixes.mjs` compatibility.

---

## 0. Verdict

**CONDITIONAL GO.**

The tenth review's NO-GO rested on **N1 (P0)** — three unrecorded semver-breaking changes
to published 0.1.0's `mock` surface. **N1 is genuinely closed**, and I was able to
corroborate its factual premise harder than the tenth review could: crates.io's own API
confirms `numa-shim 0.1.0` was published **2026-06-29T17:36:48.594937Z** with the feature
map `default: []`, `mock: []`, `vmem-integration: ["dep:aligned-vmem"]` — so `mock` **is**
published API, and all three changes are breaks against a real published surface. The
CHANGELOG now records them in a `### Removed` block.

**No P0 and no P1 remains.** I found **no UB, no soundness hole, and no correctness
defect** in the Linux FFI, the Windows FFI, the cpumap parser, the topology cache, the
`NodeResolution` API, or the six landed fixes. The N4 reservation leak is properly closed
and the whole `reserve_aligned_numa` ownership chain is now complete across every early
return.

The GO is **conditional on three things**, none of which is a code defect and none of
which the six tasks were asked to do:

1. **C1 — nothing in this campaign has been pushed, and no CI run exists for any of it.**
   `git rev-parse origin/main` → `87424a4`; `HEAD` → `1043b0e`; **36 commits between**.
   Every "gates green" claim in the six fix commits' bodies is a local claim from a
   Windows dev host. Critically, the *two rustdoc doc-lint rows and the plain-Linux test
   row that task #1276 added have never executed* — the exact configurations they exist to
   guard (a real ubuntu runner compiling `pub mod linux` + `tests/node_resolution_linux.rs`,
   and `RUSTDOCFLAGS="-D warnings"` across three feature sets) have no receipt. See **E11**.
2. **C2 — F8's Phase 1 re-run is already stale again** (**E1**): task #1279 measured
   `c427dd6`, and `41af0fd` (#1275) and `813d9e7` (#1277) both changed
   `crates/numa-shim/src/lib.rs` afterwards. Its own report predicted this. Phase 1 must
   be re-run at the **final pre-tag revision, after the F1 version-bump commit** — never
   before it.
3. **C3 — one CI line** (**E2**): the real Linux `reserve_on_node` path (mmap +
   `mbind(2)` on a fresh reservation — the crate's headline capability) is executed by
   **no CI job on any platform**. N2's fix added the plain-default ubuntu test row but not
   the `--features vmem-integration` row that Windows and macOS both already have.

**Still owner-gated, unchanged and correctly so:** F1 (version choice — must be `0.2.0`,
plus the root pin *and* three README pins, **E10**), F8 phases 2/4 (infrastructure), and
the two recorded-but-undecided owner calls F2/item 42 (`mock` feature) and F5
(`#[doc(hidden)] pub mod` semver policy). The CHANGELOG's own
`### Owner decisions pending` heading says "Resolve this heading before cutting the
release section" — that instruction is still live, and it is a decision, not a defect.

**Everything else I found is P3/INFO.**

### Disposition of N1–N12 at `1043b0e`

| Finding | Owning task | State in source | Verdict |
|---|---|---|---|
| N1 (P0) unrecorded breaking changes | #1274 | `### Removed` block + item 42 correction-scope note + metadata bullet | **CLOSED** (premise independently corroborated, §6) |
| N2 (P1) no Linux backend CI | #1276 | plain `cargo test -p numa-shim` added on ubuntu | **PARTIAL** → **E2** |
| N3 (P1) no rustdoc CI / broken default links | #1276 + #1277 | 2 doc-lint rows added; 3 links de-linked | **CLOSED**, unverified by execution (**E11**) |
| N4 (P2) reservation leak | #1275 | `let ... else` releases before `return None` | **CLOSED** |
| N5 (P3) empty `if` + false comment | #1275 | `let _ = unsafe { VirtualFree(..) };` + honest comment | **CLOSED** |
| N6 (P3) resolution not recorded under mock | #1277 | new `MockCall::CurrentNodeResolution` + 2 tests | **CLOSED**, new doc inconsistency → **E3** |
| N7 (P3) three F3 residuals | #1277 | `//!` `## Safety` section; description qualified; "confined to platform modules" corrected | **CLOSED** for all 3; a 4th surface remains → **E4** |
| N8 (P3) F4 residual unrecorded | #1274 | scope note on the CHANGELOG entry | **CLOSED as a record** (the hazard itself stays open by design) |
| N9 (P3) item 100 card stale | #1278 | item 100 refreshed | **CLOSED then immediately RE-OPENED** → **E6** |
| N10 (P3) duplicate CHANGELOG heading | #1274 | merged into one section, both bullets kept | **CLOSED** |
| N11 (INFO) smaller items | — (unowned) | untouched | **OPEN** → **E9** |
| N12 (P2) stale Phase 1 PASS | #1279 | re-run at `c427dd6`, 31/0 | **CLOSED-THEN-STALE** → **E1** |

---

## 1. What I read

Full current source of `crates/numa-shim/` (`src/lib.rs`, 1,597 lines, read in two
passes; all five test files; `benches/numa_bench.rs`; `Cargo.toml`; `README.md`;
`CHANGELOG.md`). The full diffs of `d68f6fc` (#1274), `41af0fd` (#1275), `719192c`
(#1276), `813d9e7` (#1277), `b503140` (#1278), `663394a` (#1279), plus `git show --stat`
for each to establish exactly which files each touched. `docs/reviews/2026-08-23-183220-…-oh.md`
(the tenth review) end to end. Items 100 and 101 in
`docs/correctness-open-items/TRACKED_publish_readiness.md`; item 42's new correction-scope
note in `ACTIVE.md`; the item-N → file lookup table in `docs/CORRECTNESS_OPEN_ITEMS.md`.
`docs/NUMA_GATE_RUN_2026-08-23_task1279_phase1_rerun.md`;
`docs/NUMA_RELEASE_GATE.md`'s "When to run" clause. All five `numa-shim-*` jobs plus the
`msrv` job in `.github/workflows/ci.yml`; `release.yml`'s changelog-consolidation and
CI-green guards. Root `Cargo.toml`'s numa pin and `numa-aware-mock` wiring; root
`README.md`'s crate-inventory section; every `MockCall` / `numa_shim::mock` /
`current_node_resolution` reference across the repository's own `tests/`, `src/`,
`benches/`, `examples/`. Three crates.io registry queries (§6).

---

## 2. Re-verification of tasks #1274–#1279 against source

Each fix was checked in the file, not in its commit message.

### #1274 (N1, N8-changelog, N10) — VERIFIED

`crates/numa-shim/CHANGELOG.md` now carries a `### Removed` section (`:106-140`) whose
lead-in states the false-premise history explicitly, names 0.1.0's 2026-06-29 publish
date, records that #1263's correction covered only decision 5 of 5, and states the
consequence ("cannot be `0.1.1`") while correctly declining to make the version decision.
All three breaks are listed with their commits (`dbfeca3`, `53b3ca2` ×2) and their exact
downstream failure modes. The two `### Owner decisions pending` headings are merged into
one section at `:15` with both bullets preserved (N10 ✓ — I re-grepped the file; there is
exactly one such heading). The metadata drift (`categories` dropped `"no-std::no-alloc"`,
`homepage` moved) is a bullet under `### Changed` (`:99-104`). The N8 scope note is
appended to the `NodeResolution` entry (`:84-89`) and is honest about both residuals
(binding-side untouched; `FellBackToZero` conflation). `ACTIVE.md` item 42 gained the
dated CORRECTION SCOPE NOTE naming all three axes.

One structural nit, not a defect: two of the three items in `### Removed` are
`#[non_exhaustive]` *additions*, not removals. The section's lead-in explains it is a
breaking-change group, so a reader is not misled; `### Removed` is simply the closest
Keep-a-Changelog slot.

### #1275 (N4, N5) — VERIFIED, and the ownership chain is now complete

`src/lib.rs:1340-1345`:

```rust
let Some(rounded) = raw_u.checked_add(align - 1) else {
    // SAFETY: `raw` came from the MEM_RESERVE above and was never
    // handed out; releasing before returning None cannot double-free.
    unsafe { VirtualFree(raw, 0, MEM_RELEASE) };
    return None;
};
```

`MEM_RELEASE` with `dwSize == 0` on the exact base returned by the reserve call is the
correct Win32 form. I then walked **every** exit from `reserve_aligned_numa` after `raw`
becomes live, which is the actual question N4 raised:

1. `size`/`align` contract violation and `size.checked_add(align)?` — both return *before*
   any reservation exists. Nothing to free. ✓
2. `raw.is_null()` — the reserve itself failed. Nothing to free. ✓
3. `checked_add` overflow — **now releases** (the N4 fix). ✓
4. `committed.is_null()` — releases (pre-existing, `:1391`). ✓
5. `from_raw_parts` — ownership transfers to the `Reservation`. ✓

The only remaining unwind path is the `debug_assert_eq!` at `:1398`, which leaks on panic
in debug builds only; it asserts a Win32 contract that cannot fail, and a panicking
`debug_assert` is a bug report, not an error path. Not worth changing.

N5: the empty `if` is gone, replaced by `let _ = unsafe { VirtualFree(raw, 0, MEM_RELEASE) };`
under a comment (`:1374-1391`) that now says the double failure is *unreportable* through
an `Option`-returning signature and that **no diagnostics counter exists in this crate** —
which is true, and is the honest version of the sentence #1269 got wrong.

Cosmetic inconsistency, stated only for completeness: the N4 site uses a bare
`unsafe { … };` statement while the sibling uses `let _ = unsafe { … };`. Both are correct
(`VirtualFree` returns a non-`must_use` `i32`).

### #1276 (N2-CI, N3-CI) — VERIFIED, one gap left (E2)

`.github/workflows/ci.yml:2635` adds `- run: cargo test -p numa-shim` as the **first**
step of the `numa-shim-mock` job (ubuntu), ahead of the two `mock` rows. This is the row
that matters: it is the only job anywhere that compiles `pub mod linux` and
`tests/node_resolution_linux.rs` (both gated
`all(target_os = "linux", not(miri), not(feature = "mock"))`), and the only per-PR job
that executes `sched_getcpu` + the sysfs cpumap reader + real `mbind(2)`.

Two knock-on closures I checked and confirm:

- `tests/node_resolution_linux.rs:3-6`'s header — *"exercised by CI's plain
  `cargo test -p numa-shim` on ubuntu-latest"* — was a false claim about a step that did
  not exist. It is now **true**.
- `NodeResolution::FellBackToZero`, which the tenth review showed had *zero* automated
  coverage of any kind (unreachable under `mock`; its only oracle in a file no job built),
  now has a real oracle that runs. I also checked that oracle is not host-dependent:
  `dbg_node_resolution_for_cpu(1_000_000)` returns `FellBackToZero` both when sysfs
  exists (word-count guard rejects CPU 1,000,000) and when it is absent (`len[i] == 0` for
  all 64 nodes) — so it passes on a single-node GitHub runner and on a NUMA host alike.
  Good test design.

The doc-lint rows (`:2655` and `:2656-2662`) mirror the `aligned-vmem` pattern correctly,
including the important detail: the docs.rs feature list is **derived** via
`cargo metadata … | jq …` with a `test -n "$FEATURES"` guard rather than hand-copied —
exactly what CLAUDE.md's doc-lint rule requires, and the drift surface it explicitly
rejects.

Gap: no `cargo test -p numa-shim --features vmem-integration` row on ubuntu. → **E2**.

### #1277 (N3-docs, N6, N7) — VERIFIED, with one new doc inconsistency (E3)

- **N3 (docs half).** The three intra-doc links to the feature-gated `reserve_on_node`
  from always-compiled documentation are converted to plain code font: `:40` (feature
  table), `:44` (platform matrix header), `:436` (inside `bind_range`'s own docs). Links
  to unconditional items (`current_node`, `bind_range`) are kept live. I then traced
  **every** remaining intra-doc link in the file by hand against all three CI-built
  configurations (default, `vmem-integration`, `--all-features`) and found none that
  should fail to resolve: every `reserve_on_node` / `Reservation` / `aligned_vmem::*` link
  now lives inside `#[cfg(feature = "vmem-integration")]` items, every `mock::*` link
  inside the `mock`-gated module, and the `[`CurrentNode`]: MockCall::CurrentNode` /
  `[`BindRange`]` / `[`ReserveOnNode`]` reference definitions all resolve in-scope.
  **UNVERIFIED-BY-EXECUTION**, but I could not construct a failing case on paper.
- **N7 (1).** `src/lib.rs:28-34` is a new `## Safety` section in the **crate-level `//!`
  rustdoc** — i.e. on the docs.rs landing page, which was N7's whole point. ✓
- **N7 (2).** `Cargo.toml:7`'s description now ends *"forbid(unsafe_code)-friendly except
  the one unsafe fn, bind_range."* ✓
- **N7 (3).** `src/lib.rs:53-61`'s comment no longer claims "all other unsafe is confined
  to platform modules"; it now names `bind_range_impl_linux`, `libc_mbind`, and the
  `extern "C" { fn syscall(...) }` block as crate-root residents. I verified all three
  are indeed outside every `mod platform` (`:1062`, `:1137`, `:1127`). ✓
- **N6.** `MockCall::CurrentNodeResolution(NodeResolution)` added (`:170`);
  `current_node_resolution()`'s mock arm now records (`:357`); two new tests in
  `tests/node_resolution.rs` (`:50-80`) assert both the `Resolved` and the `Unavailable`
  recording with `assert_eq!` equality oracles. The old comment's justification — that
  recording "would break existing tests' expectations" — was indeed false, and I
  re-verified the counterfactual myself rather than trusting the commit: `mock_dispatch.rs`
  never calls `current_node_resolution()`, and `node_resolution.rs`'s three pre-existing
  tests never drain after it. ✓ → but see **E3**.

### #1278 (N9) — VERIFIED as done, but already stale again (E6)

`TRACKED_publish_readiness.md:396` was rewritten to a real current-state disposition for
all eight F-findings. The work was done correctly. The problem is that it was written
*while* #1275/#1277/#1279 were in flight and describes them as such — and all three landed
within minutes. → **E6**.

### #1279 (N12) — VERIFIED as run, but superseded (E1)

`docs/NUMA_GATE_RUN_2026-08-23_task1279_phase1_rerun.md` is an honest report: it names its
measurement identity as a full SHA (`c427dd6`, satisfying CLAUDE.md's immutable-identity
rule), states its host, cites a committed raw log, explains the 28→31 delta by exactly
`node_resolution.rs`'s three new tests, and explicitly declines to make the phases-2/4
owner decision. It also *pre-declares its own expiry*: "task #1275 … is NOT in this
measured tree; … a future re-run will be owed if #1275 lands before the release cut."
It landed. → **E1**.

---

## 3. Regression hunt — did any of the six fixes break something?

The brief names this as a live pattern (task #1269's F7 hardening leaked a reservation).
I hunted for it deliberately rather than assuming.

**#1275's fix: clean.** Walked all five exits from `reserve_aligned_numa` (§2 above). The
ownership discipline is now uniform. `let ... else` requires the divergent block to
`return`, which it does. No new early return was introduced.

**#1277's new enum variant: no compile break anywhere.** `MockCall` gained a variant. I
swept **every** consumer in the repository for an exhaustive `match` that a new variant
would break:

- `crates/numa-shim/tests/mock_dispatch.rs` — `assert_eq!` on singleton vectors and
  `matches!` with `..`; no exhaustive match. ✓
- `crates/numa-shim/benches/numa_bench.rs` — never names `MockCall`. ✓
- root `tests/numa_cache_invalidation.rs:207` — `.filter(|c| matches!(c, mock::MockCall::CurrentNode(_)))`,
  a filter, not an exhaustive match. ✓
- the other five root test files that `use numa_shim::mock` never name `MockCall`. ✓

Because `MockCall` is `#[non_exhaustive]`, downstream crates were already forced into
wildcard arms, so the variant is additive for external consumers too.

**#1277's new *recording*: no behavioural side effect on existing assertions.** The root
crate never calls `current_node_resolution()` (verified by grep across `src/`, `tests/`,
`benches/`, `examples/`), so no root test's call-log count changes. Within `numa-shim`,
`node_resolution.rs`'s three pre-existing tests never drain after the call, and each test
runs on its own thread against a `thread_local!` log, so there is no cross-test bleed.
`record()` remains reentrancy-safe (`try_with` + `try_borrow_mut`) and `CALLS_CAP`-bounded,
so the extra recording cannot reintroduce the #726 unbounded-growth hazard. ✓

**#1276's new CI rows: no red-by-construction step that I can find.** The plain
`cargo test -p numa-shim` row compiles two previously-never-compiled units
(`pub mod linux`, `tests/node_resolution_linux.rs`) — the highest-risk part of the whole
wave, since a compile error there would turn `main` red on a row that has never run. I
read both closely: `dbg_node_resolution_for_cpu` calls only `pub(crate)
super::platform::cpu_to_numa_node_checked`, which exists under the identical
`all(target_os = "linux", not(miri))` cfg (with `not(feature = "mock")` a strict subset),
and the test's assertion is host-independent as shown above. I found no cfg mismatch.
**UNVERIFIED-BY-EXECUTION** — this is precisely what C1 exists to settle.

**Net: one new *documentation* inconsistency introduced (E3), no new functional defect.**
The fix-breaks-something pattern did not recur at the code level this round.

---

## 4. What N1–N12 left open or closed only partially

- **N2 → E2.** The Linux `reserve_on_node` path is still executed nowhere.
- **N9 → E6.** Item 101's card was never refreshed; item 100's refresh is stale again.
- **N11 → E9.** Never owned by a task; nothing in the wave touched any of it.
- **N12 → E1.** Closed and re-opened by two commits that landed after the measurement.
- **N6/N7 → E3/E4.** Closed as specified, but each leaves an adjacent surface: the mock
  log's own recording convention (E3) and the README (E4).
- **N8** is closed *as a record only*, correctly — F4's binding-side hazard
  (`bind_range` silently no-oping on `node >= 64` with no signal) and `FellBackToZero`'s
  three-way conflation are unchanged in code. That is the intended disposition, and the
  CHANGELOG now says so out loud, which is what N8 asked for. The tenth review's §4(a)/(d)
  design criticisms — a fourth variant to separate "no topology at all" from "topology
  exists, answer may be wrong", and defining `current_node_impl` in terms of
  `current_node_resolution_impl` so the published mapping table is true by construction —
  remain unactioned. `NodeResolution` is `#[non_exhaustive]`, so the fourth variant stays
  additive later; the parallel-implementation point does not expire so cheaply, since the
  mapping table is about to become a published guarantee.

---

## 5. New findings

### E1 (P2) — F8's Phase 1 re-run is already stale at `HEAD`; N12 is not closed

`docs/NUMA_GATE_RUN_2026-08-23_task1279_phase1_rerun.md:8` cites `c427dd6`. Two commits
touching `crates/numa-shim/src/lib.rs` landed after it: `41af0fd` (#1275, the Windows
reserve path — the exact file region the gate's Windows phases exist to protect) and
`813d9e7` (#1277, the mock recording path plus docs). `docs/NUMA_RELEASE_GATE.md`'s
"When to run" clause triggers on any diff touching `crates/numa-shim/**`.

This is mechanically demonstrable from the report's own numbers, not just from the SHA:
it records `node_resolution 3/3` within a 31-test total. At `HEAD`, `tests/node_resolution.rs`
holds **five** tests (#1277 added two), so the same invocation now yields **33**, not 31.
A stale count is exactly the signal the report itself used to prove #1270's run was stale.

**This is a scheduling problem, not a defect.** The F1 version-bump commit has not happened
yet and will itself touch `crates/numa-shim/Cargo.toml` + `CHANGELOG.md`, re-triggering
the gate again. **Fix: adopt an ordering rule — Phase 1 runs LAST, on the final pre-tag
revision, after the version bump.** Running it any earlier guarantees another stale PASS.

### E2 (P2) — the real Linux `reserve_on_node` path is executed by no CI job on any platform

`numa-shim-mock` (ubuntu) runs `cargo test -p numa-shim` (default — no `vmem-integration`,
so `reserve_on_node` does not exist), `--features mock`, and `--features "mock vmem-integration"`
(mock backend — `platform::reserve_on_node_impl` is bypassed entirely). Windows
(`:2681`) and macOS (`:2704`) both have a `--features vmem-integration` row. **Linux does
not.**

Consequence: `platform::reserve_on_node_impl` for Linux (`src/lib.rs:804-820` —
`aligned_vmem::reserve_aligned` followed by `bind_range_impl_linux`, i.e. a real
`mbind(2)` against a real fresh `mmap` before first fault) is **compiled** (by the
`--all-features` clippy row) but **never executed** anywhere. `tests/smoke.rs`'s
`reserve_on_node_returns_valid_span` and `reserve_on_node_large_align_round_trip` run
against the real backend on Windows and macOS only. The weekly `numa-real-kernel` job
reaches Linux mbind through the *root* crate's tests, not through numa-shim's own suite
or this function.

That is the crate's headline capability on its headline platform, unexercised, in a
release that advertises it. The tenth review's own N2 remediation text said "(and
optionally `--features vmem-integration`)" — the optional half turns out to be the
load-bearing one. **Fix: one line**, `- run: cargo test -p numa-shim --features vmem-integration`
in the `numa-shim-mock` job.

*Related, lower:* the ubuntu clippy row is `--all-features` (mock on), and the Windows
default-feature clippy row cannot see `target_os = "linux"` code, so `pub mod linux` is
now **rustc**-checked but never **clippy**-checked. Mentioned for completeness only.

### E3 (P3) — #1277 left the two "current node" mock variants recording on opposite sides of the mapping, and a comment that says the opposite

Two facts that are individually fine and jointly contradictory:

- `current_node()` records the **raw** slot value *before* remapping:
  `src/lib.rs:402-403` is `let n = mock::current_node_slot(); mock::record(MockCall::CurrentNode(n));`
  and only then maps `NO_NODE → None`. `tests/mock_dispatch.rs:56-60` asserts exactly this
  and documents it — *"The call is still recorded with the raw sentinel value — only the
  PUBLIC return value is remapped."*
- `current_node_resolution()` records the **mapped** outcome: `src/lib.rs:342-357` computes
  `resolution` first and records that.

Two doc defects follow:

1. **`src/lib.rs:140`** — `MockCall::CurrentNode`'s doc says *"the inner value is what was
   returned."* False on the `NO_NODE` path, where the recorded value is `NO_NODE` and the
   returned value is `None`. The crate's own test asserts the contradiction. (Pre-existing,
   but see 2.)
2. **`src/lib.rs:355-356`** — #1277's new comment says the resolution recording is
   *"mirroring `CurrentNode`'s 'inner value is what was returned' convention."* It is not
   mirroring it; it is doing the opposite (post-map vs pre-map). A reader who takes this
   sentence at face value learns a rule about the mock log that the code does not follow —
   and the sentence now *cites* defect 1 as precedent, which is what promotes a dormant
   wrong doc into an actively propagating one.

Exactly the doc-vs-code divergence class F3/N7 were raised about, reintroduced one function
over. **Fix is documentation only** (changing `CurrentNode`'s recording would break the
contract `mock_dispatch.rs` deliberately asserts): one clause in each doc naming which side
of the mapping that variant records, and delete the false "mirroring" claim.

### E4 (P3) — the README, the crates.io landing page, never mentions the new public API

`crates/numa-shim/README.md`'s `## Public API` block (`:106-127`) lists `NO_NODE`,
`current_node`, `bind_range`, `reserve_on_node`. It does **not** list `NodeResolution` or
`current_node_resolution()` — permanent public API added by #1266, given a prominent
`### Added` entry in the CHANGELOG, and the entire subject of F4's remediation. The
platform matrix at `:9-14` likewise has no column or note for it.

None of #1274–#1279 touched `README.md` (verified by `git show --stat` on all six). N7's
argument was precisely that a fix landing in a `//` comment misses "the surfaces an
external reader actually meets"; #1277 fixed two of those three surfaces (crate rustdoc,
crates.io description) and the README is the third. A reader who arrives from crates.io
and never opens docs.rs cannot discover that the fallback-detection API exists.

### E5 (P3) — dangling Markdown reference link on the README landing page

`README.md:59`: ``Enables [`reserve_on_node`], which reserves aligned anonymous virtual memory``.
There are **no link-reference definitions anywhere in the file** (verified: zero lines
matching `^\[.*\]:`). On crates.io and GitHub this renders literally as `[reserve_on_node]`,
brackets included. Purely cosmetic — but it is on the landing page, and the fix is to drop
the brackets or make it an inline docs.rs link.

### E6 (P3) — item 101's Status card is stale in exactly the way N9 flagged for item 100, and #1278's refresh of item 100 is itself already stale

Both halves verified in `docs/correctness-open-items/TRACKED_publish_readiness.md`:

- **`:424`** still reads *"**Status:** OPEN — none of N1-N12 actioned by this filing task"*,
  and **`:449`** still gives *"**Next trigger:** N1-N12 landing (tasks #1274-#1280)"* — after
  ten of the twelve findings have landed. `git show --stat` on all six fix commits confirms
  **none** of them touched item 101's card.
- **`:396`** (item 100, the card #1278 refreshed) describes F3's residuals as *"being fixed
  by #1277 in parallel with this refresh"*, F7 as *"being re-fixed by #1275, in parallel"*,
  and F8's Phase-1 re-run as *"pending #1279, in parallel"*. All three landed; two of them
  within minutes of the refresh commit.

CLAUDE.md's R34-24 rule makes the Status card the **first visible block** and the
current-state contract for round-start reading, precisely because the in-session TaskList
does not survive a session boundary. A fresh session reading these two cards today would
conclude that nothing from the tenth review has been actioned and that three tasks are
still in flight. The campaign has now produced this same staleness twice consecutively,
which suggests the fix is structural, not another one-off refresh: **the card refresh
belongs in the last commit of a wave, not in a parallel commit written mid-wave.**

### E7 (P3) — two changes to the published `mock` surface made by this wave are not in the CHANGELOG

`### Added` covers `NodeResolution` / `current_node_resolution()` but not:

- **`mock::MockCall::CurrentNodeResolution`** — a new public variant. Additive (the enum is
  `#[non_exhaustive]`), so non-breaking, but it is new public API inside the `mock` surface
  that crates.io confirms 0.1.0 also published.
- **the behaviour change** that `current_node_resolution()` now writes to the mock call log
  at all.

Neither is dangerous. But N1's entire lesson was "unrecorded changes to the published mock
surface", so recording these two costs one bullet and closes the loop the same wave opened.

### E8 (P3) — the MSRV gate never checks `numa-shim`'s default (non-`mock`) configuration

`.github/workflows/ci.yml`'s `msrv` job (pinned 1.88) runs a workspace-root
`cargo check --all-features` + `cargo test --no-run --all-features`, then **explicit
`-p sefer-region` and `-p aligned-vmem` rows** — there is no `-p numa-shim` row.

Root `--all-features` reaches numa-shim only through `numa-aware-mock` →
`numa-shim/mock` (root `Cargo.toml:721`) plus the root pin's own
`features = ["vmem-integration"]` (`:914`). So on the pinned 1.88 toolchain the
`cfg(not(feature = "mock"))` arms of all four public functions and the entire
`pub mod linux` module are **never compiled**, and numa-shim's own `tests/`/`benches/`
targets (and the `bench-scale-tool` dev-dependency's own MSRV) are never built at 1.88 at
all. `rust-version = "1.88"` is a published contract with no gate behind the exact
configuration `cargo add numa-shim` produces.

This is precisely the class that item 88 / task #1173 closed for `aligned-vmem`, with the
two explicit rows that sit immediately above in the same job — nobody has filed the
`numa-shim` equivalent. Risk today is low (I read those arms and see no post-1.88 API;
`usize::is_multiple_of` is 1.87), so this is P3, not a blocker. **Fix mirrors the existing
rows:** `cargo check -p numa-shim` and `cargo test -p numa-shim --no-run --all-features`.

### E9 (INFO) — N11's residuals, none owned by any task

- Root `README.md:580` still calls `numa-shim` **"~300 lines"**; `src/lib.rs` is **1,597**.
  The same sentence calls `aligned-vmem` "~400 lines", also long stale. This sentence's
  whole rhetorical purpose is "an auditor can read this in isolation", so the numbers are
  load-bearing for the claim.
- `NodeResolution::Resolved(u32)` still carries no `#[non_exhaustive]` and **no recorded
  decision either way**, while its `MockCall` siblings document theirs at length. The enum
  is about to be frozen by a publish; the decision belongs in that release.
- `linux::dbg_node_resolution_for_cpu` remains present in a default-feature build with no
  `bench-internals`-style gate (CLAUDE.md R25-1 rule (2)). A convention point, not a
  defect — the hook takes a `u32`, not a raw pointer, so the *safety* half of R25-1 does
  not apply — but it feeds directly into the still-open F5 decision, since a publish
  freezes it.
- Windows/macOS/miri/fallback `current_node_impl` and `current_node_resolution_impl` are
  still copy-paste parallels, so the rustdoc's published three-row correspondence table is
  an unenforced claim (tenth review §4(d)).
- `src/lib.rs:1398`'s over-long `debug_assert_eq!` line survives. #1277's commit body claims
  `fmt` clean; **UNVERIFIED-BY-EXECUTION**, and C1 will settle it.

### E10 (INFO) — F1's tracked checklist still omits the README's three `0.1` pins

`crates/numa-shim/README.md:33`, `:36` (Usage block) and `:64` (`vmem-integration` block)
all say `numa-shim = "0.1"` / `version = "0.1"`. Item 100's F1 line names
`Cargo.toml:3`, `CHANGELOG.md:7`, root `Cargo.toml:914`, and `release.yml`'s guard — the
README pins were raised only in the tenth review's §2 prose and were never carried into
the tracked checklist. Whoever executes #1262 from the card will miss them, and they are
on the crates.io landing page, contradicting the version being shipped. Fold them into
#1262's checklist now, while the omission is visible.

### E11 (INFO / process, and the reason the GO is conditional) — no commit in this campaign has been pushed and no CI run exists for any of it

`git rev-parse origin/main` → `87424a4` (task #1260, aligned-vmem's release stamp);
`HEAD` → `1043b0e`; **36 commits between**, including every one of #1273–#1279.

Consequences worth stating plainly:

- Every "gates green" line in the six fix commits' bodies is a **local** claim from a
  single Windows dev host. CLAUDE.md is explicit that an agent's statement is a claim, not
  a receipt — and this campaign has twice found a claimed-good fix that regressed
  something.
- The three CI steps added by #1276 have **never executed**. Two of them
  (`RUSTDOCFLAGS="-D warnings" cargo doc` × 2) exist specifically to catch a class of
  defect that no local Windows run reproduces, and the third compiles two units that had
  never been compiled anywhere. Their first execution is also their first test.
- Nothing here is a *silent* hazard: `release.yml` carries a "main CI workflow must be
  green for this commit" guard (`:311`+) that structurally blocks `cargo publish` on a red
  or absent CI run, and CLAUDE.md's own pre-push section requires confirming green on the
  **landing SHA read from the remote**, not the local HEAD.

So the correct disposition is not "NO-GO" — it is "the source-level work is done; the
verification step this repo already mandates has not happened yet."

---

## 6. What I checked and found clean

Stated so this report does not read as if only defects were looked for.

- **Publish preconditions — now positively verified, not assumed.** Three read-only
  crates.io API queries:
  - `numa-shim` — one version, **0.1.0**, created **2026-06-29T17:36:48.594937Z**, not
    yanked, features `default: []`, `mock: []`, `vmem-integration: ["dep:aligned-vmem"]`.
    This **independently confirms** the premise the ninth/tenth reviews took on trust from
    task #1263's query, and it settles a claim N1 depends on: `mock` **is** part of
    0.1.0's published surface, so all three recorded breaks are breaks against real
    published API. The tenth review explicitly flagged this as the one claim it could not
    re-verify.
  - `aligned-vmem` — **0.2.0** created 2026-08-23T15:13:51Z, **not yanked** (and 0.1.0
    before it). N11's flagged publish precondition — that `aligned-vmem = "0.2"` must
    resolve from the registry once `cargo publish` strips the `path` — is **SATISFIED**.
  - `bench-scale-tool` — **0.1.0** published 2026-07-14, not yanked. The dev-dependency
    resolves. ✓
- **Windows reserve/commit/release ownership** — complete across all five exits (§2), sound
  arithmetic (`over = size + align`, `base + size <= raw + over` by construction), single
  release of the whole span via `Reservation`'s `Drop`.
- **`MockCall` consumers** — swept repository-wide; no exhaustive match anywhere, so the
  new variant breaks nothing (§3).
- **`current_node`/`bind_range`/`reserve_on_node` additivity** — re-checked at `1043b0e`:
  still byte-for-byte unchanged by this wave. Only `current_node_resolution`'s mock arm
  moved.
- **`record()` reentrancy and the `CALLS_CAP` bound** — unchanged and still correct under
  the new call site; the extra recording cannot reintroduce #726's unbounded growth.
- **`node_resolution_linux.rs`'s oracle is host-independent** — passes on a single-node
  runner and on a NUMA host, by two different code paths (§2). This matters because it is
  about to run on CI for the first time.
- **Linux FFI / cpumap parser / topology cache** — re-read; nothing in this wave touched
  them, and I re-confirmed the properties the tenth review cleared: `read_cpumap_into`
  closes `fd` exactly once on all three exits and fails closed on truncation;
  `format_sysfs_path`'s worst case is 29 + 10 + 8 = 47 bytes into a `[u8; 64]`;
  `maxnode = 65` is the correct compensation for the kernel's `get_nodes()` decrement;
  the `OnceLock` cache is allocation-free by construction.
- **cfg matrix** — the four `mod platform` blocks remain mutually exclusive; the new
  `pub mod linux`'s `not(feature = "mock")` is a strict subset of `platform`'s own cfg, so
  `cpu_to_numa_node_checked` is always in scope where the forwarder is compiled.
- **CI doc-lint rows follow the rule correctly** — feature list derived from
  `cargo metadata`, not hand-copied, with a non-empty guard; both the `--all-features` and
  the docs.rs-exact rows present, neither subsuming the other. This is a textbook
  application of CLAUDE.md's doc-lint rule.
- **Item 101 is present in `docs/CORRECTNESS_OPEN_ITEMS.md`'s item-N → file lookup table**
  (`:376`), so the two-hop citation path the index's own structure rule requires resolves.
- **The #1279 gate report's discipline** — full-SHA measurement identity, committed raw
  log, honest about what did not run, declines to make the owner's decision. It is stale
  (E1) but it is well-built.

---

## 7. Recommended order before publish

1. **C1 / E11 — push, then confirm CI green on the landing SHA read from the remote**
   (`git fetch && git rev-parse origin/main`). This validates #1276's three new steps, all
   six fixes' green claims, and E9's rustfmt question in one go. Do this **before**
   anything below, because a red row here changes the rest of the plan.
2. **E2 — add `cargo test -p numa-shim --features vmem-integration` to `numa-shim-mock`.**
   One line; it is the only way the crate's headline Linux capability gets executed at all.
3. **E3, E4, E5, E7 — the documentation set.** All small, all in the class this crate has
   repeatedly proven prone to: mock-log recording convention (E3), README public-API block
   and platform matrix (E4), the dangling reference link (E5), the two missing CHANGELOG
   bullets (E7).
4. **E6 — refresh item 101's Status card and re-refresh item 100's**, and make the refresh
   the *last* commit of the wave rather than a parallel one. Consider recording that
   ordering as the convention, since this is the second consecutive occurrence.
5. **E8 — two MSRV rows for `-p numa-shim`**, mirroring the `sefer-region`/`aligned-vmem`
   rows already in that job.
6. **E9, E10 — the INFO set**, including folding the README's three `0.1` pins into
   #1262's checklist *before* F1 is executed.
7. **F2 (item 42) and F5 — the two owner decisions.** Both writeups are ready and both were
   independently sanity-checked by the tenth review. N1 strengthens item 42's own
   recommendation (a): 0.2.0 is already breaking on three axes, so the marginal semver cost
   of also removing `mock` is zero. The CHANGELOG's `### Owner decisions pending` heading
   must be resolved before the release section is consolidated — that is its own stated
   instruction.
8. **F1 — the version decision and bump: `0.2.0`, not `0.1.1`.** Sites: crate manifest,
   root `Cargo.toml:914`'s `version = "0.1"` pin (which will fail resolution otherwise),
   README `:33`/`:36`/`:64`, the dated CHANGELOG header, the tag.
9. **E1 / F8 Phase 1 — re-run LAST, on the final pre-tag revision, after step 8.** Any
   earlier and it is stale by construction. Expect 33 tests under `--features mock`, not
   31.
10. **F8 phases 2/4 — the owner's explicit call**, recorded in the release notes:
    ship with them outstanding, or obtain them first. `docs/NUMA_RELEASE_GATE.md` requires
    one of the two, and this environment can provide neither.
11. **Then publish.** The `aligned-vmem 0.2` and `bench-scale-tool 0.1` registry
    preconditions are already verified satisfied (§6).

---

**Summary verdict: CONDITIONAL GO.** The tenth review's P0 is genuinely closed and its
premise is now confirmed against crates.io rather than inferred. Nine of twelve N-findings
are fully closed; N2 and N12 are partial and N11 is untouched, and all three residuals are
one-line-to-one-paragraph fixes. **No P0, no P1, no UB, no soundness hole, and no
correctness defect remains in the shipped code** — the six fixes did not, this time,
introduce a functional regression, though #1277 did leave one documentation inconsistency
(E3) of exactly the kind this crate keeps producing. What stands between this tree and a
publish is not code: it is (C1) a CI run that has never happened for any of 36 unpushed
commits, (C2) a release gate that must be re-taken after the version bump, (C3) one
missing CI line, and the owner's own decisions on F1, F2, F5, and F8's outstanding phases.
