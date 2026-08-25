# Twenty-first independent pre-publication audit — `numa-shim` @ `5ff29cb`

**Author:** `@oh` (Opus, effort=high).
**Reported:** 2026-08-25, Europe/Berlin.
**Revision reviewed:** `5ff29cbeaecfa86fbbfc736bfad34016fb0d9c10` (`main`, confirmed via `git log -1`).
**Previous bases:** `623062d059b391a7becb5ed509df1c8e73741eaa` — the single revision audited *concurrently* by both the nineteenth review (`@oh`, item 111) and the twentieth (Sol-codex "прогон 17", item 112). Remediation waves under review: `#1336`/`#1337`/`#1338`/`#1339`/`#1340` (nineteenth) and `#1341`/`#1342` (twentieth).
**Mode:** READ-ONLY, STATIC. No sub-agents. No file in the repository edited, no `git` write command run. **Nothing was built or executed** — no `cargo build`/`check`/`test`/`clippy`/`rustdoc`, no Miri, no benchmark, no `cargo publish --dry-run`. Every conclusion below is source reading, verified against the Linux kernel's `mm/mempolicy.c` / `include/uapi/linux/mempolicy.h` / `arch/sparc/include/uapi/asm/fcntl.h` contracts, against Docker's default seccomp profile, and against the sibling `aligned-vmem` source in this workspace. The only working-tree write is this report.

**Concurrency note.** A twenty-second review (Sol-codex "прогон 18", `docs/reviews/2026-08-25-091156-...`, item 113) landed on this same revision while this audit was already running. I read the crate from scratch before consulting it. Two of my findings (P2-2, P3-1) converge with its F2/F1 — independent confirmation, not transcription. **Four of my findings are not in it** (P2-1, P3-2, P3-3, P3-4), and its F3 is not in mine as a blocker.

---

## Verdict

**NO-GO — for the fourth consecutive review, nothing that ships is wrong.**

I found **no P1**, and **no memory-safety, provenance, UB, ownership, or FFI-contract defect anywhere in the shipped code**. Both remediation waves land, and I re-verified all twelve fixes individually against source rather than trusting the index cards. `cpumap::nth_token`'s deletion leaves zero dangling references anywhere in the working tree (the only surviving hits are in `.claude/worktrees/*` scratch checkouts, not the crate); `mock::CURRENT_NODE_SLOT`'s `Cell<u32>` swap is sound and complete — `grep -n "borrow" crates/numa-shim/src/lib.rs` returns hits only for `CALLS` and `POLICY_FAILURE_SLOT`, both non-`Copy` and legitimately `RefCell`.

The block is on **two P2s**, both in the verification/documentation layer, and both are **incomplete halves of the two P2 remediations this very wave landed**:

1. the nineteenth review's P2-1 fix (`#1336`) made the *positive* policy-oracle test skip gracefully in a seccomp container — and left the *negative control* forty lines below hard-panicking in exactly the same container, behind a comment that misdescribes what that test does. `cargo test -p numa-shim --features vmem-integration` is **still hard-RED on containerized Linux**, which is the precise symptom P2-1 was filed to remove;
2. the nineteenth review's P2-2 fix (`#1337`) made `tests/smoke.rs` arch-aware — and the *same wave* (`#1340`/`#1341`) created `tests/readme_examples.rs`, which reintroduces the identical arch-blind hard-fail, propagated from the README's own headline example and `NodeId::new`'s rustdoc. This is the **fourth** dimension-by-dimension recurrence of the same predicate defect.

Plus four P3s, of which one (P3-2) is a *new* defect introduced by a docs-honesty fix in this wave: `#1340`'s replacement race-window comment contradicts itself inside one paragraph.

If the owner prefers, each P2 can be explicitly risk-accepted with a recorded note in `docs/correctness-open-items/`, in which case the **code-level verdict is GO**.

---

## Limitations and method

- One agent, no sub-agents. Source mode read-only; the only write is this report.
- Full crate re-read from scratch with no assumption that prior remediation worked: `src/lib.rs` (all 2326 lines, in three passes), all 10 integration test files, `benches/numa_bench.rs`, `Cargo.toml`, `README.md`, `CHANGELOG.md` (all 367 lines), and every `numa-shim-*` job in `.github/workflows/ci.yml`.
- Cross-checked against `crates/aligned-vmem/src/api/reserve.rs` (`reserve_aligned -> Option<Reservation>` vs `try_reserve_aligned -> Result<..>` — load-bearing for finding P3-3), `src/reservation.rs` (`from_raw_parts` preconditions), `src/page.rs`/`src/page_size.rs`, and `src/os/miri.rs` (so the new `readme_examples.rs` under `cargo miri test` is genuinely reachable, not a latent CI red).
- Kernel-contract claims re-derived from `mm/mempolicy.c` (`kernel_get_mempolicy`, `do_get_mempolicy`, `copy_nodes_to_user`, `get_policy_nodemask`, `mpol_set_nodemask`, `mpol_new_preferred`), `include/uapi/linux/mempolicy.h`, `arch/{x86,arm64}/Kconfig`, `arch/sparc/include/uapi/asm/fcntl.h`, and Docker's `profiles/seccomp/default.json`.
- Reviewed by `rust-intel` categories: unsafe/FFI, errno timing, strict provenance, cleanup/ownership on every error exit, cfg matrix, test-oracle validity, documented-promise conformance. Async/crypto/network are absent and inapplicable.
- Runtime statements below are static-analysis results, not new executions.

---

## What changed since the last two reports

| Task | Check | Assessment |
|---|---|---|
| `#1336` — preflight errno classification | `policy_oracle_linux.rs:413-465`: `EPERM`/`ENOSYS` → loud `eprintln!` skip, fatal under `NUMA_SHIM_REQUIRE_ORACLE=1`, everything else panics fail-closed. The reserve arm's own allowlist is byte-for-byte unchanged | **Correct in itself, but does not achieve its stated goal — P2-1** |
| `#1337` — arch-aware `smoke.rs` predicate | All four sites re-derived cell-by-cell across mock / real-Linux-x86_64 / real-Linux-armv7 / real-Linux-under-miri / Windows / macOS / other-unix. Every cell matches production dispatch exactly; the two-arm collapse in `_rejects_node_beyond_nodemask_range` (already `#[cfg(target_os="linux")]`-scoped) is correct | **Closed correctly** |
| `#1338` — policy-oracle CI sentinels | `ci.yml:2722-2724`: `tee` + two `grep -F` on the exact test names | **Closed correctly** |
| `#1339` — `O_CLOEXEC` arch scope | `src/lib.rs:1589-1598`, cfg-split. Verified sparc/sparc64 are the only real rustc Linux targets with a divergent value | **Closed on the wrong axis — P3-1** |
| `#1339` — CHANGELOG structure | One `### Changed` under `## Unreleased` (`grep -n "^### "` confirms `:57` only); `FellBackToZero`/`bind_range` prose carries reconciling notes | **Closed correctly** |
| `#1340` — race-window comment | Both twins rewritten | **The replacement contradicts itself — P3-2** |
| `#1340` — `OnceLock` stack depth | `src/lib.rs:1497-1515`. Arithmetic re-derived: 8192 + 4096 + 64 = 12,352 ✓, ~20 KiB worst case ✓ | **Closed correctly** |
| `#1340` — README compile oracle + fence retag | `tests/readme_examples.rs` present and genuinely runs; `README.md:172` is now ` ```text ` | **Closed, but the oracle carries P2-2 and P3-4** |
| `#1341` — two rustdoc examples | `src/lib.rs:145-157` (both arms now `Reservation` via `.expect`) and `:775-779` (`.ok().or_else(...)`) — both type-correct; three new tests carry them verbatim | **Closed in `src/lib.rs`; the same broken snippet survives in `CHANGELOG.md` — P3-3** |
| `#1342` — `nth_token` deletion | Zero references crate-wide; `tests/cpumap_parser.rs` is 15 tests as claimed | **Closed correctly** |
| `#1342` — `Cell<u32>` swap | `:397`/`:425`/`:430` use `Cell::new`/`.set()`/`.get()`; no `.borrow*()` reaches this slot anywhere | **Closed correctly** |
| `#1342` — `Cargo.toml` cfg comment | `:65-75` now states the correct distinction (no transitive feature unification; a global `RUSTFLAGS` *does* reach the whole target graph) | **Closed correctly** |

---

## Findings blocking `GO`

### P2-1 — the nineteenth review's P2-1 is half-closed: the negative control still hard-panics in the exact container `#1336` was written for, behind a comment that misstates what the test does

**Sites:** `crates/numa-shim/tests/policy_oracle_linux.rs:639-642` (the bare `.expect`), against `:626-631` (its justifying comment) and `:198-209` / `:399-412` (the `#1336` reasoning that refutes it).

`#1336` correctly reasoned, in its own new comment, that:

> Docker's default seccomp profile gates `get_mempolicy`, `mbind` and `set_mempolicy` as ONE `CAP_SYS_NICE` group and denies all three with EPERM

I re-derived that against `moby/profiles/seccomp/default.json` — the three names are one entry, `includes.caps: ["CAP_SYS_NICE"]`, and the profile's default action is `SCMP_ACT_ERRNO` with `EPERM`. Correct.

But the negative control **calls `get_mempolicy(2)` itself** and classifies nothing:

```rust
let (mode, nodemask) = unsafe {
    get_mempolicy_addr(probe)
        .expect("get_mempolicy(MPOL_F_ADDR) on a live reservation must succeed")
};
```

and its own comment asserts the opposite of the syscall it is about to issue:

> this negative control has NO `ReserveNumaError::Os` skip arm to tighten — **it never touches the NUMA policy machinery** (plain `aligned_vmem` reservation, no mbind), so there is no environment-vs-implementation errno classification to apply. Its failure mode is already loud (`.expect` below), which is the correct posture here

`get_mempolicy(2)` **is** the NUMA policy machinery, and it is in the same denial group the paragraph above it names.

Concrete failure: `docker run rust:latest` on any `CONFIG_NUMA=y` host (i.e. essentially every distro kernel; `/sys/devices/system/node/node0/cpumap` is present through the read-only sysfs bind mount) →

1. `current_node()` → `Some(0)` (sysfs readable, `sched_getcpu` unrestricted),
2. `aligned_vmem::try_reserve_aligned` → `Ok`,
3. `get_mempolicy_addr(probe)` → `EPERM`,
4. `.expect` → **panic** → suite RED.

So `cargo test -p numa-shim --features vmem-integration` on containerized Linux is **still hard-red after `#1336`** — the failure simply moved from `reserve_preferred_on_node_installs_mpol_preferred_on_the_usable_span` to `plain_unbound_reservation_is_not_reported_as_preferred_for_our_node`. The nineteenth review's own stated GO condition ("The Linux policy oracle no longer produces a **false red** on a sandbox that denies the NUMA policy syscalls") is not met.

The nineteenth review saw the site and scoped it out as "a *pre-existing, explicitly reasoned* decision recorded in the task `#1318` comment." That scoping is no longer defensible at this revision: `#1336` **added** the reasoning that makes the `#1318` comment factually wrong, and did not carry it forty lines down.

**Fix:** give `get_mempolicy_addr`'s call site the same errno classification the preflight now has — `EPERM`/`ENOSYS` ⇒ loud `eprintln!` skip (fatal under `NUMA_SHIM_REQUIRE_ORACLE=1`), everything else ⇒ panic. A negative control genuinely "only proves anything if it RUNS" — which is an argument for making its *inability to run* loud and green-skipped, not for converting an environment denial into a test failure. Alternatively, hoist the preflight into a shared helper both tests call first.

---

### P2-2 — `tests/readme_examples.rs`, created by this wave, reintroduces the arch-blind hard-fail `#1337` just removed from `smoke.rs` — and it is inherited from the README's own headline example

**Sites:** `crates/numa-shim/tests/readme_examples.rs:115` and `:198`; source snippets at `crates/numa-shim/README.md:116` and `crates/numa-shim/src/lib.rs:154`; the acknowledgment at `readme_examples.rs:66-80`.

Both the README's first `vmem-integration` example and `NodeId::new`'s "Ergonomic path from detection" snippet are:

```rust
Some(node) => reserve_preferred_on_node(..., NodeId::new(node).expect("never NO_NODE"))
    .expect("NUMA-preferred reservation failed"),
```

On a real Linux target outside x86_64/aarch64 — `s390x-unknown-linux-gnu`, `powerpc64le-unknown-linux-gnu`, `riscv64gc-unknown-linux-gnu`, all real rustc targets with NUMA-capable kernels — detection is **architecture-independent** (`mod platform` at `src/lib.rs:1324` is gated `all(target_os = "linux", not(miri))`, no arch clause), so `current_node()` genuinely returns `Some(n)`; `reserve_preferred_on_node` then returns `Err(UnsupportedArchitecture)` from `src/lib.rs:1420-1424`, before any argument or node validation. `.expect` panics.

Three separate problems, in increasing order of importance:

1. `tests/readme_examples.rs` hard-fails there — the exact defect class `#1337` closed in `smoke.rs` days earlier, recreated in a file added by the same wave. The nineteenth review's GO condition ("`cargo test -p numa-shim --features vmem-integration` is green on **every Linux target the crate's platform matrix claims**") is *still* not met, just from a different file.
2. `src/lib.rs:145-157` is a **public rustdoc example** that panics on a platform row `src/lib.rs:56`'s own matrix documents as supported. `#1341` fixed this example's *type* errors and left its *behavioral* error.
3. `README.md:110-123` is the crate's single most-copied artifact. A downstream user on ppc64le who pastes it verbatim gets a panic instead of the demonstrated behavior — the same class as `#1268` (F6), the incident `tests/readme_examples.rs` exists to prevent.

`readme_examples.rs:66-73` names the panic path explicitly ("no CI row builds that combination... it is named here so it stays a known property rather than a surprise"). That is honest, and it is the right note to write — but a comment in a test file is not a risk acceptance for a *published example*, and the note is not recorded in either open-items index under the item-111 card that owns the arch axis.

**Fix:** bring the two "simple" forms in line with the idiom the crate has already established and verified two paragraphs away — `src/lib.rs:775-779`'s `.ok().or_else(|| aligned_vmem::reserve_aligned(size, align))`, and the README's own second block — or branch explicitly on `ReserveNumaError::UnsupportedArchitecture`. Then update the oracle copies to match. This is a strictly smaller change than `#1337` was.

---

## Non-blocking findings

### P3-1 — `O_CLOEXEC` is *disabled* on sparc/sparc64 while the correct value sits in the comment directly above it

**Site:** `src/lib.rs:1589-1598`.

```rust
#[cfg(any(target_arch = "sparc", target_arch = "sparc64"))]
const O_CLOEXEC: core::ffi::c_int = 0;
```

…under a doc comment that states, verbatim: "sparc's UAPI defines `O_CLOEXEC` as `0x400000`". I confirmed that value against `arch/sparc/include/uapi/asm/fcntl.h`. `#1339`'s stated rationale — "instead of passing an unrelated bit to `open(2)`" — is sound when the correct value is unknown; here it is known, written down, and declined. The result is that `#1327`'s fork+exec fd-leak hardening is *knowingly inert* on a real rustc target, when a one-line change makes it correct at exactly the same verification confidence (structural cfg matching; no sparc toolchain is needed for either arm).

This is the "fix that papers over the symptom rather than the root cause" pattern: the root cause (an arch-specific UAPI constant applied arch-blindly) is fixed by *using the right constant per arch*, not by removing the flag.

**Fix:** `const O_CLOEXEC: core::ffi::c_int = 0x400000;` in the sparc arm.

*(Independently found as F1 of the concurrent twenty-second review, item 113; task #1345 is already filed.)*

### P3-2 — `#1340`'s corrected race-window comment contradicts itself, and its replacement mitigation claim is false on the file's own contents

**Sites:** `tests/smoke.rs:514-533` (Linux twin) and `:476-488` (Windows twin).

The corrected Linux comment says, in consecutive sentences:

> …such mappings **are not hypothetical**: libtest spawns a fresh thread per test, and each thread stack is a multi-MiB anonymous mapping that fits this hole. The oracle's real protection is only the sub-millisecond window and **this binary's lack of concurrent mmap activity inside it** — a risk that silently grows **if this file gains more mmap-performing tests**.

Three problems in one paragraph:

1. Sentence 1 asserts concurrent mappings exist in this binary; sentence 2 asserts they do not. Both cannot be the mitigation record.
2. `smoke.rs` **already contains** another mmap-performing test — `reserve_preferred_on_node_returns_valid_span` (`:173`) reserves `page * 4` and drops it — so "if this file gains more" is written as though the count were zero. It is one, plus every libtest worker thread's stack.
3. The Windows twin (`:476-488`) says "the absence of concurrent mapping creation inside it in this test binary" with no libtest-thread caveat at all, so the two twins now disagree with each other as well.

The nineteenth review's P3-2 was precisely "the documented mitigation is not the mitigation." The replacement text has the *mechanism* right (top-down `arch_get_unmapped_area_topdown` placement makes probe #4 the preferential collision target — I re-derived this and it is correct) and the *mitigation* wrong in a new way. The assertions themselves are untouched and correct; this is documentation accuracy only, and no failure has been observed.

**Fix:** state the actual protection — the window is sub-millisecond *and* the freed hole is 8 MiB while the only concurrent mappers in this binary are libtest thread stacks and one 4-page reservation, so a collision requires an unlucky interleave rather than being structurally excluded. Or implement one of the nineteenth review's three structural options (before/after `/proc/self/maps` diff; drop the two extremal probes; serialize behind a process-global mutex).

### P3-3 — `CHANGELOG.md` still carries the exact non-compiling snippet the twentieth review's P2 (`#1341`) was filed to remove

**Sites:** `crates/numa-shim/CHANGELOG.md:112` and `:120`.

```
`reserve_preferred_on_node(size, align, node).or_else(|_| aligned_vmem::reserve_aligned(size, align))`.
```

`Result::or_else`'s closure must return a `Result`; `aligned_vmem::reserve_aligned` returns `Option<Reservation>` (verified: `crates/aligned-vmem/src/api/reserve.rs:63`). This is byte-for-byte the defect `#1341` fixed at `src/lib.rs:775-779` — the twentieth review's only blocking finding — surviving in the release-notes document a 0.1.0 → next upgrader actually reads. `:120` then points readers at it ("or the documented `.or_else` composition").

The section now contradicts itself: `#1341`'s own entry at `:139`, twenty-seven lines below, quotes the **correct** form while explaining why the broken one was impossible. `CHANGELOG.md` is packaged into the `.crate` tarball and rendered on GitHub; the fix is a two-token edit (`.or_else(|_| …)` → `.ok().or_else(|| …)`) plus the same on `:120`'s prose.

### P3-4 — `tests/readme_examples.rs` is the crate's newest top-level-`cfg`-gated test file and the only *new* one without a green-and-dead sentinel

**Sites:** `crates/numa-shim/tests/readme_examples.rs:82` (`#![cfg(feature = "vmem-integration")]`); `.github/workflows/ci.yml` (no row greps any of its four test names — verified by grep across `.github/`, `scripts/`, `package.json`).

Of the five top-level-`cfg`-gated test files, three carry sentinels (`mock_dispatch.rs`, `node_resolution.rs`, and — since `#1338`, three days ago — `policy_oracle_linux.rs`); two do not: `node_resolution_linux.rs` (pre-existing) and `readme_examples.rs` (new this wave). If the feature is ever renamed or the gate typo'd, the file compiles to **zero tests**, every row exits 0, and the *entire* recurrence guard for the nineteenth review's P3-4 *and* the twentieth review's F1 disappears with no signal — the exact task-#1101 / task-#1070-"Breakage B" hazard `#1338` had just finished closing for its sibling file.

The residual risk is genuinely lower than the policy oracle's (one `cfg` condition, not five), which is why this is P3 and not P2.

**Fix:** one `tee` + one `grep -F "test readme_vmem_integration_example_compiles_and_runs ... ok"` on any row that builds the feature (`ci.yml:2722` is the natural home; the macOS mock+vmem row at `:2864` is the second-best).

---

## Additional observations (informational, no action required for `GO`)

- **The positive `smoke.rs` tests inherit the environment gap `#1329`/F1.1 closed for the oracle.** `smoke.rs:245` and `:371` `.expect("NUMA-preferred reservation failed")` with no `MPOL_F_MEMS_ALLOWED` preflight. On the topology shapes `policy_oracle_linux.rs:283-299` itself documents as real — a memoryless (CPU-only) node, or a cpuset splitting the CPU and memory masks — `current_node()` resolves node *N*, `mbind` returns the *documented* `EINVAL`, and these two tests go red while the oracle correctly skips. Same root as P2-2 (positive tests `.expect` without the environment guards the crate's own code documents), different axis.
- **`reserve_preferred_on_node_commits_only_the_requested_span_not_the_whole_over_reservation` (`smoke.rs:652`) is backend-blind.** It carries `#[cfg(all(windows, feature = "vmem-integration"))]` with no `not(numa_shim_mock)`, so on the `numa-shim-windows` mock+vmem row it exercises `aligned_vmem`'s reservation instead of `reserve_aligned_numa` — the `#724` regression it exists to guard — and prints `... ok` either way. Not a false red and the real-backend row also runs it, so the guard is intact; the *attribution* in that configuration is not. It also lacks the `require_oracle()` escalation its two sibling positive tests have (moot: the env var is never set on Windows).
- **`README.md`'s platform table omits the "Linux other arch" row** that `src/lib.rs:56`'s matrix carries. The README covers the *reservation* half in prose at `:243-245`, but a README-only reader concludes that **detection** is unavailable on e.g. ppc64le Linux, which is wrong. `Cargo.toml:21-32` declares these sites "kept in sync deliberately" for the Windows row; the arch row is the one that drifted.
- **`README.md`'s "## Public API" block omits `NodeId::get`** (`src/lib.rs:176`), and renders the type as `pub struct NodeId(u32);`, which reads as a public tuple field. Cosmetic.
- **`CHANGELOG.md:22-24`'s Phase-1 record ("31/0 at `c427dd6`") is stale** relative to today's suite — a mock + `vmem-integration` run now compiles roughly 50 tests. It is honest as a *historical* record and explicitly states "the final pre-tag re-run is still owed"; it is the one item in this file that must be **executed** rather than edited before a tag. The nineteenth review's P3-5 point 3 asked for "refresh or re-run"; `#1339` did neither and did not record a deferral.
- **`docs.rs` doc-lint coverage is complete for this crate.** `package.metadata.docs.rs.features = ["vmem-integration"]` is exactly `--all-features` here (the only feature), and `ci.yml:2752-2777` runs all three rows (`--all-features`, the `cargo metadata`-derived docs.rs set, and bare defaults). CLAUDE.md's task-#1142 rule is satisfied, including its "derive, don't hand-transcribe" clause.
- **CI backend/feature/OS coverage is otherwise complete**: real-Linux (with and without `vmem-integration`, the latter with `NUMA_SHIM_REQUIRE_ORACLE=1` and two sentinels), Linux mock ±feature, Windows real ±feature, Windows mock ±feature, macOS real ±feature, macOS mock ±feature (three sentinels), macOS+miri real ±feature, macOS+miri mock. **Two cells remain uncovered**: (a) mock + `vmem-integration` under miri (already recorded in `#1325`'s CHANGELOG entry), and (b) any non-x86_64/aarch64 Linux target at all — which is what makes P2-2 invisible to CI. A `cross test --target powerpc64le-unknown-linux-gnu -p numa-shim --features vmem-integration` row would make both P2-2 and `#1337`'s new arm self-verifying; a plain `cargo check` row will not (both are runtime assertions).
- **MSRV job (`ci.yml:1916-1917`) never exercises the mock cfg.** Currently clean — everything used (`const {}` 1.79, `is_multiple_of` 1.87, `addr`/`with_addr` 1.84, `offset_of!` 1.77, `io::Error::other` 1.74) is within 1.88.
- **`aligned-vmem` is still `0.2.0` locally**, so `version = "0.2", path = "../aligned-vmem"` will resolve to the published registry copy at `cargo publish` time. No drift.
- The `0.2.0` version bump remains owner-deferred and is **not** raised as a finding.

### Re-verified clean this pass (no finding)

The Linux two-stage reserve→`mbind`→release-on-failure path and its capture-errno-**before**-cleanup ordering (`src/lib.rs:1411-1417`); the `maxnode = 65` compensation for `get_nodes()`'s internal `--maxnode` (⇒ `BITS_TO_LONGS(64) = 1` ⇒ exactly the 8-byte `u64` nodemask); the Windows `MEM_RESERVE`/`MEM_COMMIT`/`MEM_RELEASE` ownership on **all three** error exits (`checked_add` overflow `:2050`, commit failure `:2087`, `committed != base` `:2117`), each releasing exactly once with no handle ever handed out, and the `committed != base` check unconditional rather than `debug_assert`; `Reservation::from_raw_parts`'s preconditions re-derived arithmetically against what the Windows path supplies (`base_addr ≥ raw_addr`; `base_addr + size ≤ raw_addr + over - 1 < raw_addr + over` since `over = size + align`; `over` a `PAGE` multiple because `size` is and `align` is a power of two ≥ `PAGE`); the strict-provenance `addr()`/`with_addr()`/`cast()` round-trip; the `ProcessorNumber` layout const-asserts (size 4 / align 2 / offsets 0,2,3); `MAXNODE = 1024` vs `copy_nodes_to_user`'s `ALIGN(1023,64)/8 = 128` bytes exactly matching `[u64; 16]`, including the `nr_node_ids`-clamped short copy plus `clear_user` tail and the `copy > PAGE_SIZE` guard; `kernel_get_mempolicy`'s `maxnode < nr_node_ids ⇒ EINVAL` precheck being genuinely satisfied (`CONFIG_NODES_SHIFT range 1 10` on both x86_64 and arm64 ⇒ `nr_node_ids ≤ 1024`); `MPOL_F_MEMS_ALLOWED`'s NULL-`policy`/NULL-`addr` convention (`if (policy && put_user(...))`; `flags & (MPOL_F_NODE|MPOL_F_ADDR) ⇒ EINVAL`, neither combined); `get_policy_nodemask` returning exactly `{node}` for a flagless `MPOL_PREFERRED` (so the equality assert is neither vacuous nor over-strict, and `mpol_set_nodemask`'s intersection with `cpuset_current_mems_allowed` is what the preflight makes safe to assume); the `rsplit` linearization's byte-for-byte equivalence on empty / whitespace-only / trailing-comma / leading-comma / double-comma / word-boundary inputs; `ReverseIndex`'s fail-closed validate-then-commit, first-mapping-wins, and `node ≤ 63 ⇒ u8` fit; `format_sysfs_path`'s 29 + 10 + 8 = 47 ≤ 64 bound; the four `mod platform` cfg blocks' mutual exclusivity including the macOS×miri cell; `should_retry_eintr`'s `ErrorKind::Interrupted` equivalence to `EINTR` on the real `last_os_error()` path, with the streak reset on forward progress and no fd leak on either retry site; `read_cpumap_into`'s fail-closed treatment of a file at or beyond the buffer size; the crate-doc's `unsafe`-inventory claim, checked exhaustively against every `unsafe` token in the file (all 26 sites are inside a `mod platform` block or one of the three named crate-root Linux mbind helpers, exactly as documented); the 32-bit-Windows `compile_error!`; and `#![deny(missing_docs)]` coverage of every public item across all three `#[doc(hidden)]` semver-exempt modules.

---

## Recommended fix order

1. **P2-1** — classify `get_mempolicy_addr`'s errno at its call site (or hoist the preflight into a shared helper), so a default-seccomp container stops going red and the file stops contradicting its own `ERRNO_EPERM` doc.
2. **P2-2** — make the README's first example and `NodeId::new`'s rustdoc snippet panic-free on `UnsupportedArchitecture`, using the `.ok().or_else(...)` idiom the crate already ships two sections away; update the two oracle copies to match.
3. **P3-1** — `O_CLOEXEC = 0x400000` on sparc/sparc64.
4. **P3-3** — `CHANGELOG.md:112`/`:120`: the same `.ok().or_else(...)` correction `#1341` already applied to `src/lib.rs`.
5. **P3-2** — resolve the race-window comment's self-contradiction in both twins.
6. **P3-4** — one grep sentinel for `tests/readme_examples.rs`.
7. Optionally, the informational items: the `smoke.rs` positive tests' missing mems-allowed guard, the backend-blind Windows commit test, the README platform table's missing "Linux other arch" row, and a `cross test` row for one non-x86_64 Linux target (which would make P2-2 and `#1337` both self-verifying).

## Conditions for `GO`

- `cargo test -p numa-shim --features vmem-integration` is green inside a default-seccomp container on a `CONFIG_NUMA` host — i.e. **both** policy-oracle tests skip loudly rather than one skipping and one panicking.
- No **public example** — README or rustdoc — panics on a platform row `src/lib.rs`'s own matrix documents as supported, and the compiled oracle enforces that rather than documenting the exception.
- Either the sparc/sparc64 `O_CLOEXEC` constant is the arch-correct `0x400000`, or the decision to disable the hardening is recorded as an explicit risk acceptance rather than as a fix.
- `CHANGELOG.md`'s `## Unreleased` section does not simultaneously publish a broken snippet and the note explaining why that snippet is broken.
- The owed pre-tag Phase-1 gate re-run is either executed against this revision or explicitly re-waived with a recorded note.
- After the fixes, the owner runs the normal dynamic verification matrix — this read-only review does not substitute for it.

Once those are closed, the production design is, in my assessment, publication-ready: a safe public API with no `pub unsafe fn`, typed failures on every path, fail-closed detection, RAII cleanup with errno captured before it, a deliberately narrow and honestly-stated platform contract, and `unsafe` confined to a small, individually-justified FFI seam that I could not fault at this revision. Every blocking finding in this report — for the fourth consecutive review — is in the verification or documentation layer, not in what ships.
