# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

Changes since the 0.1.0 publish of 2026-06-29. This section's heading is
deliberately version-neutral: the next version number is an owner decision
not yet made, and this repository does not bump versions without an explicit
request. At release time, consolidate this section under a dated
`## <version> - <date>` heading.

### NUMA gate verification caveat (owner risk acceptance, 2026-08-23)

- **This release's real-multi-socket and real-Linux-kernel NUMA binding
  behavior has NOT been independently verified** beyond mock-dispatch and
  single-node host-level testing. Of the four phases
  `docs/NUMA_RELEASE_GATE.md` requires before a `0.x.y` release touching
  this crate: Phase 1 (mock dispatch) passed (31/0 at `c427dd6`, task
  #1279; the final pre-tag re-run is still owed per the eleventh
  review's E1 ordering rule), Phase 3 (Windows virtual NUMA) is PARTIAL
  — host-level suites only, the in-guest Hyper-V procedure never ran —
  and Phases 2 (real Linux kernel / QEMU) and 4 (real 2-socket metal)
  did not run at all: the development environment has no Linux kernel
  access and no multi-socket cloud instance access.
- Per an explicit owner decision dated 2026-08-23 (task #1290), this
  release publishes anyway with Phases 2 and 4 outstanding — a knowing,
  recorded risk acceptance, NOT a judgment that those phases are
  unnecessary. Full record with the release-SHA placeholder:
  `docs/NUMA_GATE_2026-08-23_0.2.0_phase24_waiver.md`.
- **Users on genuine NUMA hardware should independently validate
  node-binding correctness before relying on it in production.**

### Owner decisions pending

- **`mock` feature's Cargo-unification hazard** (F2 of the 2026-08-23
  publication audit, `docs/reviews/2026-08-23-164206-numa-shim-publication-audit-Sol-codex.md`;
  `docs/correctness-open-items/ACTIVE.md` item 42; task #1264): DECISION MADE
  2026-08-23 (task #1288) — option (a): converted to the build-time `--cfg numa_shim_mock`
  flag, mirroring aligned-vmem's task #962. See the `### Removed` section below for the
  breaking-change entry and migration note.
- **Semver policy for the two `#[doc(hidden)]` test-only modules** —
  `pub mod cpumap` (parser helpers) and `pub mod linux`
  (`dbg_node_resolution_for_cpu`); audit finding F5, scope-expanded by a
  later zero-trust review to cover both. Recommendation recorded (option
  (c): commit both to the published surface at the next release, with
  `cpumap` promoted to documented API); **DECIDED** (task #1289,
  owner-confirmed): keep both structurally as-is (still `#[doc(hidden)]`,
  still `pub`) and declare both semver-exempt — see the Changed entry
  below. Neither module is in published 0.1.0 — the next publish is
  what freezes them as public API, so the decision must land in that
  release's scope. Full writeup: task #1267 addendum on item 100 in
  `docs/correctness-open-items/TRACKED_publish_readiness.md` (task #1267).

### Changed

- **BREAKING: `NodeId::new` rejects the `NO_NODE` sentinel — `NodeId` is valid-by-construction** — task #1309 (finding F4 of the fifteenth independent review, `docs/reviews/2026-08-24-170047-numa-shim-publication-audit-Sol-codex.md`). `NodeId::new(u32)` was an unchecked `const fn` that accepted ANY `u32` — including `NO_NODE` (`u32::MAX`), which its own doc comment said "must NOT be wrapped" — so the documented forbidden state was representable, and downstream behavior for it diverged by platform (Linux `InvalidNode`, Windows forwards the sentinel to the OS, other platforms `UnsupportedPlatform` before any node check). Now `NodeId::new(u32) -> Option<NodeId>` returns `None` for exactly `NO_NODE` and `Some(_)` for every other `u32` — deliberately NOT a general node-existence validator: platform-specific validity (Linux's 0..=63 nodemask, Windows OS forwarding) stays at `reserve_preferred_on_node`'s fallible checks. No `new_unchecked`/`unsafe` escape hatch was added; detection composes as `current_node().and_then(NodeId::new)`. `NodeId` was introduced in this same unreleased cycle (task #1306), so the net 0.1.0 -> next diff contains only the fallible signature.
- **BREAKING: `current_node()` is fail-closed on undetermined Linux topologies** — task #1308 (F1, the only P1, of the fifteenth independent review, `docs/reviews/2026-08-24-170047-numa-shim-publication-audit-Sol-codex.md`). Previously every undetermined case — sysfs unreadable, CPU on a node >= 64, no NUMA sysfs at all — collapsed into `Some(0)`, indistinguishable from a genuinely resolved node 0; combined with task #1306's strict `reserve_preferred_on_node`, a caller could successfully install a NUMA preference for the WRONG node. Now `cpu_to_numa_node` maps lookup failure to `NO_NODE` — both failure paths (`sched_getcpu(2)` failure and topology-lookup failure) converge on the same sentinel — and `current_node()` returns `None` for it; `Some(0)` occurs only for a CPU genuinely resolved to node 0. Granular failure reason: `current_node_resolution()`.
- **BREAKING: `NodeResolution::FellBackToZero` renamed to `NodeResolution::TopologyUnavailable`** — task #1308. The old name described a node-0 fallback that no longer exists once `current_node()` fails closed; the new name states what actually happens. Both `TopologyUnavailable` and `Unavailable` map to `None`; their distinction is diagnostic ("platform has a NUMA API and detection ran, but this CPU could not be resolved" vs "no NUMA API / the OS call itself failed"). The variant was introduced in this same unreleased cycle (task #1266), so the net 0.1.0 -> next diff contains only the new name.
- **Windows support is now documented as 64-bit only** — task #1313 (finding F11 of the fifteenth independent review, `docs/reviews/2026-08-24-170047-numa-shim-publication-audit-Sol-codex.md`). The crate's public platform matrix said just "Windows" while its own Windows FFI test layout was hard 64-bit (`MEMORY_BASIC_INFORMATION`'s `PartitionId` exists in `winnt.h` only under `#if defined(_WIN64)`) and CI never covered i686 — an unstated third policy the review named ("might work, but nobody promises"). Owner decision (asked and answered): `x86_64-pc-windows-msvc` and equivalent only; 32-bit Windows (`target_pointer_width = "32"`) is explicitly out of scope. Documentation-only — no `cfg(target_pointer_width)` code gate, no behavior change: `README.md`'s platform-support table, the crate-doc platform matrix in `src/lib.rs`, a policy note in `Cargo.toml`, and the `tests/smoke.rs` `PartitionId` comment all state the same policy.
- **The 64-bit-only Windows policy is now enforced at compile time, not just documented** — task #1321 (P2 of the sixteenth independent review, `docs/reviews/2026-08-24-204022-numa-shim-publication-audit-Sol-codex.md`). Task #1313 (F11) decided the policy — Windows supported on 64-bit targets only, 32-bit Windows out of scope — but left it documentation-only; the sixteenth review found that a user targeting e.g. `i686-pc-windows-msvc` got a crate that COMPILED while silently unsupported, the test FFI layout's `MEMORY_BASIC_INFORMATION` hardcoding a 64-bit pointer width and thereby giving false compatibility confidence. `src/lib.rs` now emits `compile_error!` under exactly `cfg(all(windows, target_pointer_width = "32"))`: a 32-bit Windows build fails loudly with a message naming the policy and pointing at the platform matrix and README, while 32-bit NON-Windows targets (e.g. `i686-unknown-linux-gnu`) are deliberately unaffected — the gate is Windows-specific, not pointer-width-specific. This is an enforcement of an ALREADY-DECIDED policy, not a new policy decision: no supported configuration's behavior changed, and the crate compiles identically on every 64-bit target. The three policy-statement sites (`README.md`'s platform table, the crate-doc platform matrix in `src/lib.rs`, and `Cargo.toml`'s header comment) were updated in the same change to state the gate exists; they remain in sync.

### Fixed

- **The `MPOL_F_MEMS_ALLOWED` preflight hard-panicked in exactly the seccomp-sandboxed environment the file's own `ERRNO_EPERM` allowlist documents — containerized Linux went hard RED where pre-#1329 it correctly skipped** — task #1336 (finding P2-1 of the nineteenth independent review, `docs/reviews/2026-08-25-022026-numa-shim-publication-audit-oh.md`). Task #1329's preflight (`get_mempolicy(2)` with `MPOL_F_MEMS_ALLOWED`, run BEFORE the positive oracle's reserve call) consumed its result through a bare `.expect` — no errno classification at all. Docker's default seccomp profile gates `get_mempolicy`, `mbind` and `set_mempolicy` as ONE group behind `CAP_SYS_NICE`, denying all three with `SCMP_ACT_ERRNO(EPERM)`; so inside a container `current_node()` succeeds (sysfs is readable), the preflight gets EPERM, and the `.expect` PANICS — while the reserve arm's `Some(ERRNO_EPERM) => skip` 40 lines below, written for exactly that environment, is unreachable (no realistic environment permits the preflight but denies mbind). The `.expect` message's own wording ("the policy oracle cannot run here at all") described a SKIP condition implemented as a PANIC. The preflight now classifies its errno with the same allowlist philosophy the reserve arm already implements: `EPERM` and `ENOSYS` (a seccomp policy can deny with either; the review names both) ⇒ loud `eprintln!` skip stating the sandbox denies the policy syscalls as a group (get_mempolicy/mbind/set_mempolicy); every other errno, and an `Os` error carrying no raw errno, still PANICS fail-closed. `NUMA_SHIM_REQUIRE_ORACLE=1` (task #1324) keeps the CI row maximally strict: under it the new skip is FATAL with a message explaining the host was declared oracle-required while the syscalls are denied — that env var is set only on the repo's own real-Linux CI row, which is not a container. The reserve arm's classification (ENOMEM/EPERM skip; EINVAL/EFAULT/ENOSYS/unknown panic) is byte-for-byte unchanged, as is the negative control's pre-existing, task-#1318-reasoned `.expect` (the review scopes P2-1 to the preflight only); `ERRNO_EPERM`'s and `ERRNO_ENOSYS`'s doc comments gained sentences recording that each constant now serves both sites with their differing classifications. Test-only change: one test file; no production code, no version bump.

- **`cpumap::parse_each_set_cpu` was O(words²) on the topology cold path — now a single linear O(words) pass** — task #1334 (finding F11 of the eighteenth independent review, `docs/reviews/2026-08-24-224323-numa-shim-publication-audit-Sol-codex.md`). The old body computed `word_count` and then, for each word `w`, called `nth_token(data, word_count-1-w, b',')` — and `nth_token` linearly rescans `data` from byte 0 on every call, so extracting all tokens cost O(word_count × scan_length) = O(words²) worst case. That sat on the first-ever `current_node()` call's cold path, compounding: `ReverseIndex::index_node` invokes `parse_each_set_cpu` twice per node (task #1310's validate-then-commit fail-closed scheme, deliberately untouched) and `topology()`'s `OnceLock` initializer runs it for up to 64 nodes. The rewrite replaces word counting + indexed backward token extraction with one `data.rsplit(|&b| b == b',')` pass: `rsplit` yields the comma-delimited words rightmost-first, which is exactly the w=0 (lowest CPU indices) through highest-word order the old `left_index = word_count-1-w` arithmetic produced, so each byte of `data` is now visited O(1) times per call. Behavior is byte-for-byte identical, verified by edge-case analysis against the old `nth_token`-based semantics on: empty input / whitespace-only (both fail closed with zero `on_cpu` calls — `rsplit` on an empty slice yields exactly one empty token, which `parse_hex_u32` rejects, matching the old path where `nth_token` returned the empty last token for `word_count == 1`); trailing comma (empty rightmost token fails first in both, zero callbacks); leading comma and double comma (fail closed identically, with the same partial `on_cpu` calls for tokens parsed before the malformed one in both versions); and the word-order boundary (leftmost text token covers the highest CPU indices — pinned by `tests/cpumap_parser.rs`'s `cpu_32_is_bit_0_of_the_leftmost_word` oracle, unchanged and green). The old code's `None => return false` arm on `nth_token` was provably dead (`left_index` never exceeds the comma count, so `nth_token` always returned `Some`), so its removal changes nothing. `nth_token` itself is RETAINED unmodified: after this rewrite it has no production caller, but it remains a `#[doc(hidden)]` SemVer-exempt parsing primitive directly exercised as its own oracle by `tests/cpumap_parser.rs` (`nth_token_splits_on_separator`, `nth_token_single_token_no_separator`) — deleting it would require editing test files held as the unchanged regression oracle for this task, and it is not dead code. Commit prefix `fix(perf)` per this repo's commit-subject taxonomy: shipping code changed to fix a complexity/efficiency defect, but NO wall-clock speedup is measured or claimed — the O(w²)→O(w) reduction is provable by construction (one pass, each byte visited once), and a real-parser before/after measurement would require temporarily retaining the old implementation solely for the measurement run, process overhead disproportionate to a P3 cold-path finding; this matches task #1310's own CHANGELOG posture for the strictly larger O(nodes×bytes)→O(1) lookup win ("No wall-clock performance claim (unmeasured); this records complexity/footprint facts only").

- **Documentation-honesty corrections — no behavior change anywhere** — task #1333 (findings F9 and F10 of the eighteenth independent review, `docs/reviews/2026-08-24-224323-numa-shim-publication-audit-Sol-codex.md`, plus the same review's "additional observations" note on dependency wording). Four doc/comment sites, all prose-only: (1) `tests/smoke.rs`'s Windows-oracle duplication comment no longer says the 32-bit Windows policy is "undecided" — it is decided (64-bit Windows only, task #1313/F11) and `compile_error!`-enforced (task #1321, sixteenth review P2); the original struct's comment now cites the enforcement too, and both structs stay structurally untouched (consolidation into a shared Windows-only test helper remains possible follow-up). (2) `current_node()`/`current_node_resolution()` docs no longer claim warm calls are "pure in-memory ... no syscalls at all" — the topology lookup is cached in-memory, but every call still samples the CPU via `sched_getcpu()`, a platform call on every invocation; the task #723 entry in "### Added" below was corrected with the same precision. (3) `benches/numa_bench.rs`'s module doc now states up front that `current_node/first_call`/`warm_call` measure mock dispatch/recording overhead only, not real sysfs cold-path or production warm-lookup cost (bench names kept for historical bench-id continuity; real-backend calibration stays with `docs/perf/OPEN_ITEMS.md` item 59). (4) "zero C library dependencies"/"100 % Rust"/"no C/C++ libraries" reworded to "zero third-party C/C++ dependencies (no libnuma, no hwloc)" across `src/lib.rs`'s crate doc, README.md's headline, and the Cargo.toml `description` (the crate calls system libc/Win32 directly via FFI — no third-party C library, but system libc IS an FFI dependency); `version` untouched.

- **`tests/node_resolution_linux.rs`'s mapping test compared two INDEPENDENTLY-SAMPLED CPU snapshots as if they were one — a false-failure race on real multi-NUMA hosts** — task #1332 (finding F5 of the eighteenth independent review, `docs/reviews/2026-08-24-224323-numa-shim-publication-audit-Sol-codex.md`). `current_node_agrees_with_resolution_mapping` called `current_node()` and then separately `current_node_resolution()`; each internally performs its own `sched_getcpu()` snapshot, and the OS scheduler is free to migrate the calling thread between the two calls — on a genuine multi-node machine two individually-CORRECT snapshots (`Some(0)` from the first call's CPU, `Resolved(1)` from the second's) would fail the test's `assert_eq!` purely because the test compared two different points in time as one. Today's single-node CI hides the race; a planned real-multi-NUMA CI gate would make it likely to actually fire. Rewritten as a single-snapshot oracle: the test takes ONE raw `sched_getcpu()` sample (FFI declared directly in the test file, the same self-contained pattern `tests/policy_oracle_linux.rs` uses for its raw `syscall(2)` declaration — no production-code forwarder added, `src/lib.rs` untouched) and, unless the sample is negative (loud `eprintln!` skip + return, matching the sibling files' skip-message style), feeds that SAME `cpu` value into the two existing doc-hidden test-only forwarders `linux::dbg_node_resolution_for_cpu` / `linux::dbg_current_node_for_cpu`, which never re-sample the CPU — no scheduler migration can occur between two calls that do not consult the scheduler at all, so the check is fully deterministic given the one sample. The documented mapping asserted is unchanged (Resolved(n) ↔ Some(n); everything else ↔ None, wildcard arm required by `#[non_exhaustive]`); the file's other two tests (fixed-literal `1_000_000` CPU indices, no live sampling — no race to begin with) and the test's NAME are unchanged (no CI sentinel greps for it). Test-only change — no production code touched, no version bump; the file is Linux-only (`cfg`-gated out on Windows/miri/mock hosts), so the rewritten test was cross-compile-verified on x86_64/aarch64 Linux and first executes on Linux CI.

- **Linux `current_node()`/`current_node_resolution()` took the `sched_getcpu()` snapshot BEFORE the one-time topology scan, widening the first call's scheduler-migration window for no benefit** — task #1331 (finding F3 of the eighteenth independent review, `docs/reviews/2026-08-24-224323-numa-shim-publication-audit-Sol-codex.md`). Both production entry points in the Linux `mod platform` called `sched_getcpu()` first and then `cpu_to_numa_node`/`cpu_to_numa_node_checked`, whose FIRST-ever call internally triggers `topology()`'s `OnceLock::get_or_init` — an up-to-64-file sequential sysfs scan. On that first call the CPU snapshot was therefore taken BEFORE a long initialization window during which the thread could migrate to a different CPU/node, so the returned node reflected where the thread WAS before init started, not necessarily where it was when the function returned. Both functions now call `topology()` first (completing initialization) and only then take the `sched_getcpu()` snapshot, resolving the CPU via `.lookup(cpu)` on the already-obtained topology reference instead of re-entering `cpu_to_numa_node`/`cpu_to_numa_node_checked` (which would consult the `OnceLock` a second time — idempotent and cheap, but redundant). This is a COLD-PATH-ONLY narrowing of an unavoidable snapshot-vs-migration window, NOT an elimination of it: on warm calls (topology already initialized) the window is small, essentially irreducible for any snapshot-style API, and byte-for-byte unchanged in behavior — same single `topology()` `OnceLock` check either way, so warm-path performance is unaffected. The review's optional extra-strictness suggestion (re-snapshot the CPU after the lookup and retry once if it changed) was deliberately NOT adopted — it would change warm-path behavior for a marginal gain outside this task's minimal-reorder scope. Return contracts are unchanged (`u32` with `NO_NODE` sentinel semantics / `NodeResolution` variants verbatim); `cpu_to_numa_node`/`cpu_to_numa_node_checked` and their `pub mod linux` test-forwarder callers (`dbg_current_node_for_cpu`, `dbg_node_resolution_for_cpu`) are untouched. The migration-window narrowing itself cannot be tested deterministically (no portable way to force a scheduler migration mid-scan); verification is code-reading plus cross-target compile/clippy plus the existing mock-backend test suite staying green.

- **The Linux policy oracle's errno classification was simultaneously over-strict and over-lenient, and its own `get_mempolicy_addr` probe was mis-sized for >64-possible-node hosts** — task #1329 (F1.1, F1.2, F6 of the eighteenth review, `docs/reviews/2026-08-24-224323-numa-shim-publication-audit-Sol-codex.md`). This SUPERSEDES PART of task #1318's classification as a REFINEMENT, not a reversal: #1318's core contribution (converting an unconditional skip into SOME classification) stands; #1329 corrects which errno land in which bucket and adds the missing preflight. F1.1 (over-strict): `mbind(2)` documents `EINVAL` also for a well-formed call naming no online/cpuset-allowed/memory-bearing node; `current_node()` proves only the sysfs CPU→node mapping; fixed via the `MPOL_F_MEMS_ALLOWED` preflight (flag `1<<2` verified against `include/uapi/linux/mempolicy.h`; mode `NULL`/ignored, `addr` `NULL` per the non-MPOL_F_ADDR convention; kernel returns `cpuset_current_mems_allowed`, which `guarantee_online_mems` maintains intersected with `N_MEMORY`, so a set bit is both cpuset-allowed AND memory-bearing); bit unset ⇒ loud skip before any reserve call. F1.2 (over-lenient): old catch-all `Some(errno) => skip` hid unknown errno green; now an explicit allowlist — `ENOMEM` (documented for this exact `flags=0` form) and `EPERM` (seccomp/docker container-deny, the F8.2 case) skip with per-errno reasons; `EINVAL`/`EFAULT`/`ENOSYS` (post-preflight) and every unknown errno panic, fail-closed. F6: `kernel_get_mempolicy` rejects `maxnode < nr_node_ids` with `EINVAL` before any copy-out (mm/mempolicy.c), so the old `maxnode=65` single-u64 probe failed on >64-possible-node hosts regardless of the target node; both probes now use `[u64; 16]` (1024 bits; `CONFIG_NODES_SHIFT` capped at 10 on x86_64 and arm64) with `maxnode = 1024`, whose `ALIGN(1023,64)/8 = 128`-byte copy-out exactly matches the array (clamped downward on smaller kernels); `node_bit_set` helper tests the target bit across words. Test-only: `tests/policy_oracle_linux.rs` + this CHANGELOG; `src/lib.rs`'s deliberate single-u64/nodes-0..63 production contract (task #1306) unchanged; task #1324's `require_oracle()` blocks and the negative control's logic untouched (its call site only mechanically updated for the array return type). Cross-compile-verified only (x86_64-unknown-linux-gnu + aarch64-unknown-linux-gnu: `cargo check --tests`, `clippy --all-targets -D warnings`, `fmt`); real execution owed to the real-Linux CI row, same limitation as tasks #1311/#1318/#1319/#1320.

- **EINTR retry check compared a hand-decoded raw errno instead of `std::io::ErrorKind::Interrupted` and lived untestably inside the Linux-only `platform` module; the sysfs cpumap `open(2)` lacked `O_CLOEXEC`** — task #1327 (P3-1 and P3-2 of the seventeenth independent review, `docs/reviews/2026-08-24-223343-numa-shim-publication-audit-oh.md`). P3-1: `should_retry_eintr` (task #1319's bounded-EINTR retry decision for the topology scan) matched `err.raw_os_error() == Some(4)` against a locally-defined `EINTR` constant, and both it and `EINTR_RETRY_LIMIT` sat inside `#[cfg(all(target_os = "linux", not(miri)))] mod platform` as private items — ZERO test coverage on ANY host (the module does not compile on this repo's Windows dev machine, and no Linux test reached the private fn either; the exact "accident of code organization, not a genuine platform requirement" reasoning the target-independent `cpumap` module's own doc comment records for its extraction). The check is now `err.kind() == std::io::ErrorKind::Interrupted` — std's own `decode_error_kind` maps `EINTR` to `Interrupted` on every Unix, portable by construction, no new dependency (the crate still deliberately avoids `libc`) — and the decision fn + limit moved to a new target-independent `#[doc(hidden)]` SemVer-exempt `numa_shim::eintr` module (task #1289 pattern, same as `cpumap`/`linux`; README's doc-hidden-module enumeration updated from two to three), exercised by the new `tests/eintr_retry.rs` on every host including Windows. **Retry BEHAVIOR is unchanged: same bound (`EINTR_RETRY_LIMIT = 16` consecutive interruptions), same reset-on-progress streak semantics — only the errno-vs-ErrorKind check and the function's location changed; on the real call path (`last_os_error()` on Linux) `EINTR` is the only errno that decodes to `Interrupted`, so the retry set is byte-for-byte identical to the raw-errno check.** The now-unused local `EINTR: i32 = 4` constant was deleted (its only use was the old check). P3-2: the cpumap `open(2)` call passed flags `0` (`O_RDONLY`, no close-on-exec), so a concurrent `fork()`+`exec()` during the topology initializer's one-time 64-node scan could leak the fd into the child; the call now passes `O_CLOEXEC` — defined locally as `0o2000000` per Linux `asm-generic/fcntl.h` (`#define O_CLOEXEC 02000000` octal), identical on x86_64 and aarch64, same local-constant precedent as `SYS_MBIND`/`MPOL_PREFERRED` (`O_RDONLY` is `0` on Linux, so the flags value is exactly read-only + close-on-exec). **Flag-bit-only change**: no other behavior difference; actual close-on-exec kernel behavior is not testable without a real fork+exec harness (out of scope) — the constant's value and kernel-header citation are the verification.
- **Three ungated intra-doc links to the `vmem-integration`-gated `reserve_preferred_on_node` broke the DEFAULT-feature rustdoc build with exactly 3 unresolved-link errors** — task #1326 (P2-3 of the seventeenth independent review, `docs/reviews/2026-08-24-223343-numa-shim-publication-audit-oh.md`; independently found as F8 of the eighteenth, `docs/reviews/2026-08-24-224323-numa-shim-publication-audit-Sol-codex.md`). `RUSTDOCFLAGS="-D warnings" cargo doc -p numa-shim --no-deps` (default features, no `--features` flag — the exact configuration `cargo add numa-shim` produces for a downstream consumer) failed at three sites: the crate-level platform matrix, `NodeId::new`'s doc comment, and `NodeResolution::TopologyUnavailable`'s doc comment. All three referenced `reserve_preferred_on_node` via markdown intra-doc links from doc comments compiled REGARDLESS of features, while the linked function itself is `#[cfg(feature = "vmem-integration")]`-gated — with the feature off, the function does not exist in the item tree rustdoc builds, so all three links were broken. CI's two numa-shim rustdoc rows both build with the feature ON (one `--all-features`; the other derives `package.metadata.docs.rs`'s pinned set, which for this crate IS `vmem-integration`), so neither caught it — the exact gate-blindness class CLAUDE.md's doc-lint rule (task #1142) describes, and a recurrence of the broken-link class task #1277 already fixed once for this crate with nothing added to prevent recurrence. Fixed by de-linking the three mentions to plain backticks (prose otherwise unchanged; the feature-gated occurrences inside the `Reservation` re-export's and the function's own doc comments were already correct and untouched), and by adding a THIRD rustdoc CI row for numa-shim with NO `--features` flag at all (true default features), next to the two existing rows in the numa-shim-mock job, mirroring the task #778 DEFAULT-features clippy row's rationale: default features is what `cargo add numa-shim` produces, and no feature-on row subsumes it. Doc-comment-only change in `src/lib.rs` (exactly 3 sites) plus one CI workflow row: zero behavior change, no version bump. Known separately-trackable gap, deliberately left out of scope per the seventeenth review's own framing: the `mock` module's intra-doc links remain unchecked by any rustdoc CI row (no row runs rustdoc under `--cfg numa_shim_mock`, since the mock never reaches docs.rs — it is a build-time cfg, not a Cargo feature); a one-line row (`RUSTDOCFLAGS="--cfg numa_shim_mock -D warnings" cargo doc -p numa-shim --all-features --no-deps`, verified green on the development host 2026-08-24) closes it whenever the owner wants.

- **`tests/smoke.rs`'s three platform branches were backend-blind — under `--cfg numa_shim_mock` + `vmem-integration` on macOS (or miri) they expected `UnsupportedPlatform`, which the mock never returns** — task #1325 (P2-2 of the seventeenth independent review, `docs/reviews/2026-08-24-223343-numa-shim-publication-audit-oh.md`; independently found as F2 of the eighteenth, `docs/reviews/2026-08-24-224323-numa-shim-publication-audit-Sol-codex.md`). The three `reserve_preferred_on_node` smoke tests gated their expectations on `cfg!(all(any(target_os = "linux", windows), not(miri)))` — TARGET only, never BACKEND — while the mock dispatch arm (task #1311/F6's deliberate Linux-shaped, no-platform-check contract) records the call, checks `node < 64`, and forwards to `aligned_vmem::try_reserve_aligned`, which succeeds on macOS: two round-trip tests would get `Ok` where they assert `Err(UnsupportedPlatform)`, and the zero-size test would get `InvalidArguments` where it asserts `UnsupportedPlatform` — three false-red tests in exactly the configuration `README.md` and `src/lib.rs` advertise ("assert the wrapping logic ... on any target including macOS and miri"). No CI row ran that combination (the macOS jobs pair the mock cfg WITHOUT `vmem-integration`; the Linux/Windows jobs pair it WITH the feature but on targets where the guard happened to take the supported branch anyway), which is why it was never observed. Fixed with the seventeenth review's mechanism — each branch becomes backend-aware, `cfg!(any(numa_shim_mock, all(any(target_os = "linux", windows), not(miri))))`, so under the mock ALL targets take the supported branch, matching what the mock actually does. The eighteenth review's alternative (compile-gate the real-platform tests with `not(numa_shim_mock)` and keep mock expectations entirely in `tests/mock_dispatch.rs`) was considered and rejected: a larger diff that would REMOVE the only coverage where the mock's reserve path drives a real `aligned_vmem` reservation on a third platform. No existing CI row's behavior changes (on Linux/Windows + mock the predicate already took the supported branch via the target clause; without the mock it degenerates to the old predicate exactly). A new `numa-shim-macos` CI step runs `RUSTFLAGS="--cfg numa_shim_mock" cargo test -p numa-shim --features vmem-integration` with grep sentinels naming the three tests — the first CI row crossing that combination, and the fix's first real EXECUTION proof (verification on the Windows dev host is code-reading only: Windows' `cfg!(windows)` took the supported branch both before and after the fix). The miri twin of that cell (mock + `vmem-integration` under miri) remains uncovered by any row. Test-only change — no production code touched (the mock arm's task #1311/F6 doc-honesty decision stands untouched), no version bump.

- **All four positive `current_node()`-gated tests skipped silently when the crate's OWN detection returned `None`, so a detection regression disabled them together while CI stayed green** — task #1324 (P2-1 of the seventeenth independent review, `docs/reviews/2026-08-24-223343-numa-shim-publication-audit-oh.md`). Task #1318 closed the `Err(ReserveNumaError::Os)` half of the vacuous-skip hazard; the OTHER skip arm in the same tests was left untouched: `let Some(node) = current_node() else { eprintln!("skip: …"); return; }` — identical and undifferentiated at four sites (`tests/policy_oracle_linux.rs` x2, `tests/smoke.rs` x2). That arm is not merely an environment concern: `current_node()` is this crate's own detection chain (`sched_getcpu` → sysfs cpumap reader → `ReverseIndex` lookup — rewritten by task #1310 and EINTR-hardened by task #1319 in the same wave), so a regression anywhere in it would silently disable the flagship policy oracle, its negative control, and both `reserve_preferred_on_node` round-trips together on the one CI row that runs the real Linux backend (`numa-shim-mock` job's `cargo test -p numa-shim --features vmem-integration`) — and CI stays green, because an internally-skipped test still prints `... ok` (the mock rows' `grep -F "test ... ok"` sentinel greps cannot catch this either). Fixed with the inverse of the root crate's `SEFER_NUMA_TEST=1` real-hardware gate (`tests/numa_alloc.rs`): a new `NUMA_SHIM_REQUIRE_ORACLE=1` env var, set ONLY on that one real-Linux CI row, under which each of the four skip arms PANICS first with a message naming the detection chain ("this CI row exists specifically to prove NUMA detection works; a None here means detection regressed, not that this host lacks NUMA hardware"); with the var unset (local/dev runs, mock rows, Windows/macOS jobs) the tolerant F8.2 skip is byte-for-byte unchanged. Test-only change — two test files plus one env block on one CI step; no production code touched, no version bump. Verification honesty: the None→panic arm is reachable only where `current_node()` can return `None` (the Linux sysfs chain) — on the Windows dev host `current_node()` resolves `Some(0)` and the arm is unreachable, so the panic direction is compile/cross-check-verified only and first executes on the Linux CI row; the complementary directions WERE run locally (env var set + working detection → still green on the real Windows backend; env var unset → mock suite green, 46 tests, unchanged).

- **Strict-provenance round-trip in the Linux policy-oracle test, plus a mock test leaving its scripted policy failure armed past its own end (the sixteenth review's two test-side P2s)** — task #1320 (`docs/reviews/2026-08-24-204022-numa-shim-publication-audit-Sol-codex.md`, its two test-side P2 findings). P2 (policy oracle): `tests/policy_oracle_linux.rs`'s `get_mempolicy_addr` helper took the probe address as `addr: usize` and cast it back to `*mut c_void` at the syscall site, while both callers produced that usize via `r.as_ptr().add(page) as usize` — the same integer pointer round-trip task #1313 had just eliminated from the Windows PRODUCTION path (`reserve_aligned_numa`'s `raw.addr()`/`with_addr` fix), written into a TEST in the same wave. Test-only (no production exposure), but inconsistent with the crate's own strict-provenance discipline: pointer-typed values should stay pointer-typed end-to-end (the strict-provenance APIs are stable since Rust 1.84; this crate's MSRV is 1.88). The helper now takes `addr: *mut core::ffi::c_void` directly (matching the C prototype's `void *addr`; the syscall site passes it through with no cast at all), and both call sites — the positive oracle and the negative control — pass `r.as_ptr().add(page).cast()` with no intermediate integer. Signature/plumbing change only, identical runtime behavior; the file remains cross-compile-verified on x86_64/aarch64 Linux and first executes in CI, per its own verification-status note. P2 (mock hygiene): `tests/mock_dispatch.rs`'s `policy_failure_script_for_other_node_does_not_fire` arms `mock::set_policy_failure(5, …)` and exercises only node 3, so node 5's failure is intentionally never consumed — and the test ended without `mock::clear_policy_failure()`, leaving the failure armed in the thread-local `POLICY_FAILURE_SLOT` after the test returned. The test harness reuses worker threads across tests, so a later test on the same thread touching node 5 could inherit the armed failure; this was masked only because neighboring tests defensively clear the slot at their OWN start — an implicit, fragile test-ordering dependency. The test now clears the slot explicitly before returning. A scan of the file's other `set_policy_failure` call sites found no further leaks: `scripted_policy_failure_returns_os_error_and_releases_exactly_once` and `policy_failure_script_is_one_shot` each arm a node whose failure is consumed by their own exercised failing call (the mock slot is one-shot), so neither leaves anything armed. Both fixes are test-only: no production code touched, no version bump, no assertions changed.
- **The Linux policy oracle skipped on ANY `ReserveNumaError::Os`, hiding implementation errors behind a green CI** — task #1318 (P1 — the review's only P1 — of the sixteenth independent review, `docs/reviews/2026-08-24-204022-numa-shim-publication-audit-Sol-codex.md`). In `crates/numa-shim/tests/policy_oracle_linux.rs`, the `Err(Os)` arm of `reserve_preferred_on_node_installs_mpol_preferred_on_the_usable_span` unconditionally `eprintln!`-skipped and returned — but `Os` carries not only legitimate environment refusals (a cgroup-restricted node) but also implementation errors: `EINVAL` (bad addr/len/maxnode marshalling in the crate's own `mbind` wrapper), `EFAULT` (bad pointer argument), `ENOSYS` (wrong syscall number for the arch). A regression making `mbind` always fail therefore left CI green via skip — defeating the very oracle task #1311 (F3) added to prove the flagship API works. The arm now classifies `raw_os_error()`: `EINVAL`/`EFAULT`/`ENOSYS` PANIC with the errno number and name, stating these indicate an implementation bug in `reserve_preferred_on_node`'s syscall marshalling, not an environment limitation; `None` (an `Os` error with no errno — itself suspicious) also panics; every other errno (`EPERM`, `ENOMEM`, cgroup refusals) keeps the F8.2 container-case skip, but the skip diagnostic now prints the errno number so a skipping CI log is diagnosable. Errno values are defined locally in the test file from asm-generic errno.h (the crate deliberately avoids the `libc` crate), following the file's existing `SYS_GET_MEMPOLICY`/`MPOL_F_ADDR`/`MPOL_PREFERRED` local-constant precedent. The negative control needed no classification change: it reserves through `aligned_vmem::try_reserve_aligned` directly (never the NUMA policy path) and already fails loud via `.expect` — noted in a comment. Test-only change: no production code touched, no version bump.
- **A single `EINTR` during the first topology load permanently disabled NUMA detection** — task #1319 (P2 of the sixteenth independent review, `docs/reviews/2026-08-24-204022-numa-shim-publication-audit-Sol-codex.md`). `read_cpumap_into` (the Linux `platform` module's sysfs cpumap reader) treated ANY `open(2)`/`read(2)` `-1` return as a hard failure with no errno check — so one signal arriving during the process's first `current_node()` call (whose `OnceLock` initializer reads up to 64 sysfs node files sequentially and caches the resulting reverse index for the process lifetime; the call sits on sefer-alloc's allocation hot path under `numa-aware`) abandoned a perfectly readable topology and cached the partial index forever: `current_node()` returned `None` for the rest of the process's life because of one stray, expected signal. Fixed by capturing errno IMMEDIATELY on a `-1` return (task #1306's errno-timing contract — before any cleanup FFI call such as `close` can overwrite it) and, specifically for `EINTR` (defined locally as `4` per Linux `asm-generic errno-base.h`, identical on x86_64/aarch64 — same local-constant precedent as `SYS_MBIND`/`MPOL_PREFERRED`, since the crate deliberately avoids the `libc` crate), re-issuing the identical call with a bounded streak counter: `EINTR_RETRY_LIMIT = 16` consecutive interruptions without forward progress. The bound exists so a pathological signal storm (a profiler or interval timer firing faster than the syscall can complete) cannot spin the `OnceLock` initializer forever — every thread hitting `current_node()` blocks on it — an unbounded retry would trade the permanent-`None` availability bug for a hang; the streak resets on any progress, so the limit applies to consecutive interruptions, not total retries. Retrying `read` at the same buffer offset is sound because POSIX guarantees an `EINTR`-failed read transferred zero bytes; retrying `open` is sound because `EINTR` means no fd was created (no leak). Every other errno (`EACCES`, `ENOENT`, ...) and retry-limit exhaustion keep the exact pre-fix close-fd-and-return-`None` fail-closed posture. No signature or `Option<usize>` contract change for non-EINTR cases, and the retry paths stay allocation-free (`last_os_error`/`raw_os_error` store the code inline), preserving task #777's allocation-free `OnceLock` initializer. The retry decision is factored into a pure helper (`should_retry_eintr`) shared by the open and read sites; no unit test was added because the module is cfg-gated to real Linux (uncompilable on this crate's Windows dev host and under the `numa_shim_mock` CI path) and the predicate is a single boolean expression — verification was line-by-line review plus cross-target `cargo check` (x86_64-unknown-linux-gnu and aarch64-unknown-linux-gnu) and the crate's full test suite.
- **`reserve_aligned_numa`'s aligned-base derivation used an integer pointer round-trip instead of strict-provenance-preserving pointer arithmetic** — task #1313 (finding F9's safe half, fifteenth independent review). `let raw_u = raw as usize; ... let base = base_u as *mut u8;` lost provenance information Rust's strict-provenance model wants preserved. Now uses `raw.addr()`/`raw.with_addr(base_addr)`, mirroring the sibling `aligned-vmem` crate's own equivalent fix (`crates/aligned-vmem/src/os/windows.rs`, task #717). Pure provenance mechanics: computed addresses are byte-identical to before, verified on this Windows host by the existing `VirtualQuery`-based over-reservation test (`reserve_preferred_on_node_commits_only_the_requested_span_not_the_whole_over_reservation`) plus both round-trip tests, all green. No wall-clock performance claim — strict-provenance conversion is a compile-time concept on current LLVM codegen; nothing was measured and none is claimed. The review's OTHER F9 half — a one-call `MEM_RESERVE | MEM_COMMIT` fast path when `align <= WIN_ALLOCATION_GRANULARITY` — is deliberately NOT implemented here (needs a real before/after measurement plus byte-for-byte behavior proof across the existing over-reservation tests first); filed as `docs/perf/OPEN_ITEMS.md` item 60.
- **`current_node()` silently collapsed an undeterminable node into `Some(0)`** — task #1308 (finding F1 of the fifteenth review): unreadable sysfs, a CPU on a node >= 64, or a kernel with no NUMA sysfs at all produced the same `Some(0)` as a genuinely node-0-resolved CPU, so a caller could install a NUMA preference for the wrong node with an `Ok` result (worse after task #1306 made `reserve_preferred_on_node` actually install `MPOL_PREFERRED` and check the result). Fixed by the fail-closed remap under `### Changed`.

- **`cpu_to_numa_node_checked` O(nodes × bytes) per-lookup re-parse and false per-node cpumap buffer rationale** — task #1310 (review findings F5 and F10 from `docs/reviews/2026-08-24-170047-numa-shim-publication-audit-Sol-codex.md`). F5: the old `NODE_CPUMAP_BUF_LEN = 1024` comment justified the buffer as covering "~3640 CPUs on a SINGLE node," but a Linux node cpumap is the GLOBAL cpumask `cpumask_of_node(node) & cpu_online_mask` — bit indices are global logical CPU IDs, so the buffer bounds global CPU-ID space, not per-node CPU count. The false rationale hid a real defect: on many-node/sparse-ID systems ALL nodes' cpumaps can simultaneously exceed a small buffer, silently dropping the whole topology into the node-0 fallback. F10: after the first call, each `cpu_to_numa_node_checked` lookup re-parsed up to ~64 KiB of cached raw text — O(nodes × cpumap-bytes) per lookup. The fix replaces the per-node raw-text cache (`[[u8; 1024]; 64]`, ~64.5 KiB) with an allocation-free reverse index `cpu -> node` (`[u8; 8192]`, 8 KiB) built ONCE inside the same `OnceLock` initializer by parsing each node's file exactly once through the single `parse_each_set_cpu` interpreter (no second divergent parsing path); lookup is now a single array probe. `MAX_INDEXED_CPUS = 8192` derives from the kernel's per-arch `NR_CPUS` ceiling (x86_64 caps at 8192, arm64 at 4096 — the archs this crate supports), covering every possible set bit on supported kernels; CPU IDs beyond it degrade exactly like the old oversized-file case (unmapped → `TopologyUnavailable` via `cpu_to_numa_node_checked`'s `None` — renamed from `FellBackToZero` by task #1308, same unreleased cycle). Observable behavior relative to the PRE-#1310 design is preserved: same `None`/`Unavailable`-vs-`Resolved` distinction and (pre-#1308) `Some(0)` conditions, same lowest-node-wins for overlapping masks (first-mapping-wins under the real caller's ascending scan), same fail-closed per-node handling of malformed/oversized files, same `current_node()`/`current_node_resolution()` signatures and semantics. One internal nuance: `parse_contains_cpu` (doc-hidden, semver-exempt) now fails closed when ANY token is malformed, not just the target word's. No wall-clock performance claim (unmeasured); this records complexity/footprint facts only, per this repo's commit-prefix/evidence discipline.
- **`mbind(2)` `maxnode` off-by-one silently dropped node 63's bit** from the
  nodemask — a real ABI divergence from `libnuma`'s binding behaviour
  (task #697).
- **cpumap reads longer than one 256-byte buffer were silently treated as
  complete** — a real bug on ~900+-CPU hosts, where `/sys/…/nodeN/cpumap`
  spans multiple words and the truncated parse mapped high CPU ids to the
  wrong node (task #720).
- **Windows `reserve_on_node` committed `size + align` instead of `size`**,
  doubling the commit charge of every NUMA-preferred reservation (task #724).
- **`mock::set_current_node(NO_NODE)` produced `Some(NO_NODE)` instead of
  `None`**, violating `current_node`'s own documented "returns `Option`,
  never the sentinel" guarantee — under the one feature that exists to let CI
  assert that wrapping logic (task #722).
- Several doc/code semantics divergences: `bind_range`'s `# Safety` contract
  now applies only when the `node`/`len` short-circuit does not fire (the
  function never touches `base` otherwise), and `current_node`'s
  Linux-unreachable-`None` claim was corrected to the actual collapse-into-
  `Some(0)` behaviour (tasks #722, #725).

### Added

- **Process-lifetime topology cache for `current_node()`** — the first Linux
  call performs up to 64 `open`/`read`/`close` sysfs triples (one per
  candidate node) to populate a `OnceLock` cache; every subsequent call's
  topology lookup is a pure in-memory bit-test — though the per-call
  `sched_getcpu()` CPU sample remains a platform call (task #723; precision
  corrected in task #1333, eighteenth review F10). The cache's
  initializer stores only static/stack data and performs no heap allocation,
  so calling `current_node()` from inside a `#[global_allocator]` that itself
  consults it cannot deadlock or re-enter `OnceLock::get_or_init` mid-init —
  the original heap-allocating initializer from #723 was replaced after that
  hazard was found (task #777).
- **`mock::set_policy_failure`/`clear_policy_failure` and `mock::MockCall::InstallPolicy`/`PolicyFailureRelease`** — task #1311 (finding F6 of the fifteenth independent review, `docs/reviews/2026-08-24-170047-numa-shim-publication-audit-Sol-codex.md`). Additive mock-only surface (compiled only under the `--cfg numa_shim_mock` build flag, never in default/docs.rs builds) that enables tests to script a simulated policy-installation failure separately from a reservation failure, proving the two-stage cleanup contract (reservation succeeded + policy failed; original errno preserved after cleanup; reservation released exactly once). The mock now records `InstallPolicy` on every call (success and failure), and `PolicyFailureRelease` strictly after the reservation's `Drop` runs on the failure path. Observable change: a SUCCESSFUL `reserve_preferred_on_node` under the mock now records TWO entries (`ReservePreferredOnNode` then `InstallPolicy { succeeded: true }`), so downstream mock-log assertions on the success path need updating. Doc-honesty note added: the mock approximates the real Linux backend's contract (single-`u64` nodemask, two-stage reserve-then-policy with release-on-policy-failure), not real Windows (which forwards any node id to the OS) or real macOS (which returns `UnsupportedPlatform` unconditionally before argument validation).
- **`mock::CALLS_CAP`** (public, 4096) caps the mock recording log so a
  never-drained call log cannot grow without bound; `drain()` documents that
  it returns a silently truncated oldest-first prefix past the cap (tasks
  #726, #778).
- **`NodeResolution` enum and `current_node_resolution()` function** —
  additive API that lets callers distinguish "genuinely resolved to this
  NUMA node via platform topology" from "silently fell back to node 0" on
  Linux (task #1266, audit finding F4). `current_node()` remains unchanged;
  use `current_node_resolution()` only when you need to detect the fallback
  case (e.g., for diagnostic logging or warnings that NUMA hints may not be
  effective). The Linux implementation distinguishes the three cases:
  `Resolved(n)` when the CPU is found in the cached sysfs cpumap,
  `FellBackToZero` when the CPU is not found (including nodes >= 64 or
  unreadable topology), and `Unavailable` when `sched_getcpu(2)` fails.

  Scope note (task #1274, finding N8 of the tenth review): this addition
  addresses DETECTION only — `bind_range` with `node >= 64` still silently
  no-ops with no caller-detectable signal (F4's binding-side ask remains
  open), and `FellBackToZero` does not distinguish "sysfs unreadable /
  node >= 64 (the node-0 answer may be wrong)" from "no NUMA topology at
  all (node 0 is genuinely correct)".
- **`mock::MockCall::CurrentNodeResolution(NodeResolution)` and
  `current_node_resolution()` recording under `mock` at all** — additive
  mock-surface follow-up to the entry above (task #1277, finding N6 of the
  tenth review; recorded per E7 of the eleventh): the function's `mock` arm
  originally skipped the call log entirely, contradicting the module's
  "records every invocation" contract; it now records its resolved return
  value through this new variant. The variant is new public API inside the
  `mock` surface that published 0.1.0 already carries — non-breaking, since
  `MockCall` is `#[non_exhaustive]`.
- The sysfs cpumap parser was extracted into a target-independent module with
  real behavioral oracles runnable on every host, not only real Linux (task
  #721) — test infrastructure (`#[doc(hidden)]`), not public API.
- **`NodeId`** (task #1306) — newtype over `u32` for NUMA node identifiers
  in the reservation/policy API (`NodeId::new(u32) -> Option<NodeId>`
  constructor rejecting exactly the `NO_NODE` sentinel — made fallible by
  task #1309, same unreleased cycle, see the Changed entry below;
  `NodeId::get() -> u32` accessor). Detection APIs keep returning
  `Option<u32>`; the ergonomic path is `current_node().and_then(NodeId::new)`
  (flattening to `Option<NodeId>`).
- **`ReserveNumaError`** (task #1306) — `#[non_exhaustive]` error enum
  (`UnsupportedPlatform`, `UnsupportedArchitecture`, `InvalidArguments`,
  `InvalidNode`, `Os(std::io::Error)`) implementing `Display` and
  `std::error::Error`, returned by `reserve_preferred_on_node`.

### Changed

- `aligned-vmem` optional dependency bumped from `0.1` to `0.2`
  (the sibling crate's own 0.2 release); `reserve_on_node`'s return type
  moves with it.
- `Cargo.toml` metadata (non-breaking): `categories` dropped
  `"no-std::no-alloc"` — correct, the crate links std
  (`std::thread_local!`, `std::sync::OnceLock`); and `homepage` moved from
  `.../crates/numa` to `.../crates/numa-shim`, matching the post-publish
  directory rename (both flagged by finding N1 of the tenth review;
  recorded by task #1274).
- **The two `#[doc(hidden)]` test-only modules are declared semver-exempt**
  — `pub mod cpumap` (sysfs cpumap parser helpers) and `pub mod linux`
  (`dbg_node_resolution_for_cpu`), resolving the "Owner decisions pending"
  entry above (audit finding F5; task #1289, owner-confirmed): both keep
  their exact current structure (still `#[doc(hidden)]`, still `pub`) and
  are now explicitly exempt from this crate's SemVer guarantees —
  everything in them (signatures, names, existence) may change or be
  removed in ANY release, including patch releases, without a deprecation
  period. Do not depend on them from code outside this crate's own
  `tests/`. This is the `serde::__private` convention (hidden from
  rendered docs, exemption stated in each module's own doc comment), and
  `cargo-semver-checks` already excludes `#[doc(hidden)]` items from its
  public-API model. Docs-only change: zero code, zero visibility change.
- **BREAKING (task #1306) — `reserve_on_node(size, align, node: u32) ->
  Option<Reservation>` replaced by `reserve_preferred_on_node(size, align,
  node: NodeId) -> Result<Reservation, ReserveNumaError>`** (still behind
  `vmem-integration`). The rename states what the operation actually does —
  `MPOL_PREFERRED` is a soft preference, not a bind. Behavioral changes
  beyond the signature:
  - Linux now applies the policy to the COMPLETE underlying OS reservation
    span (`reservation_ptr()`/`reservation_len()`), not just the aligned
    usable subrange — policy lifetime matches mapping lifetime and no VMA
    splitting occurs around alignment slack.
  - The `mbind(2)` return value is now CHECKED (previously discarded): a
    policy failure after a successful reservation RELEASES the reservation
    and returns `Err(ReserveNumaError::Os(..))` with errno captured
    immediately at the failing syscall — never a half-bound reservation,
    never `Ok` with a silent no-binding.
  - The silent `node >= 64` no-op is now `Err(ReserveNumaError::InvalidNode)`
    (documented single-`u64` nodemask implementation limit).
  - Linux architectures without a known `SYS_MBIND` number now return
    `Err(ReserveNumaError::UnsupportedArchitecture)` instead of silently
    skipping the bind.
  - macOS / miri / other unsupported platforms now return
    `Err(ReserveNumaError::UnsupportedPlatform)` instead of silently
    falling back to an UNBOUND reservation. No best-effort fallback exists
    anywhere inside the function; callers wanting best-effort compose it
    visibly at the call site:
    `reserve_preferred_on_node(size, align, node).or_else(|_| aligned_vmem::reserve_aligned(size, align))`.
  - Invalid `size`/`align` (aligned-vmem contract violations) now return
    `Err(ReserveNumaError::InvalidArguments)` instead of `None`
    (indistinguishable from OOM in the old API).
- **BREAKING (task #1306) — raw `u32` node parameters removed from the
  reservation API**: `NodeId` is now the node parameter type, and `NO_NODE`
  (`u32::MAX`) is no longer accepted by any reservation/policy signature —
  "no preference" is expressed by calling `aligned_vmem::reserve_aligned`
  directly (or the documented `.or_else` composition), not by a sentinel.
  `NO_NODE` still exists as a detection-side interop constant
  (`current_node()` returns `Option<u32>` and never the sentinel).
- **BREAKING (task #1306) — `mock::MockCall::ReserveOnNode` replaced by
  `mock::MockCall::ReservePreferredOnNode { size, align, node: u32 }`**
  (still `#[non_exhaustive]`): records the call BEFORE validation (unlike
  the old `BindRange`, which recorded only past its short-circuit), so
  error paths such as `InvalidNode` are observable in the call log. The
  old API's separate `BindRange` record no longer exists — the new function
  installs policy inside the platform backend, which the mock replaces
  wholesale.
- `Cargo.toml` `description` (task #1306, metadata): no longer claims
  "except the one unsafe fn, bind_range" — the crate now has NO
  `pub unsafe fn` at all.

### Removed

Four changes in this tree are **semver-breaking against published
0.1.0's `--features mock` surface** and are recorded here as a group (task
#1274, finding N1 of the tenth independent review,
`docs/reviews/2026-08-23-183220-numa-shim-publication-readiness-review-oh.md`).
Three were made under `53b3ca2`'s stated premise "all decided now, before this
crate's first crates.io publish" — which was false: 0.1.0 was published
2026-06-29, six weeks earlier. Task #1263's premise correction covered only
the `mock` Cargo-feature decision (decision 5 of 5 in that commit), not
these API breaks — that remainder is recorded on
`docs/correctness-open-items/ACTIVE.md` item 42. A fourth breaking change
was added by task #1288 (2026-08-23), and the main-API breaks of task #1306 (2026-08-24) follow it. Under Cargo's 0.x rules a release
containing any of these cannot be `0.1.1`; which version this becomes is
the still-open F1 owner decision (task #1262) — this section records what
already broke, independent of that choice.

- **BREAKING — `mock::MockCall` is now `#[non_exhaustive]`** (commit
  `dbfeca3`, 2026-07-19, three weeks after the 0.1.0 publish): a 0.1.0
  consumer's exhaustive `match` over the three variants stops compiling
  until a wildcard arm is added.
- **BREAKING — `mock::CALLS` and `mock::CURRENT_NODE_SLOT` are no longer
  public** (narrowed `pub` → `pub(crate)`, commit `53b3ca2`, task #726,
  2026-08-09): an item removal — a 0.1.0 consumer that names either
  thread-local directly now gets a not-found/unresolved-import error. The
  public API was and remains the encapsulating pair `mock::drain()` /
  `mock::set_current_node()`.
- **BREAKING — the `MockCall::BindRange` and `MockCall::ReserveOnNode`
  struct variants are now `#[non_exhaustive]`** (commit `53b3ca2`, task
  #726, 2026-08-09): struct-literal construction
  (`MockCall::BindRange { base, len, node }`) and exhaustive field patterns
  both stop compiling — every construction site and field pattern needs
  `..`. `53b3ca2`'s own commit body records this exact failure occurring
  in-repo the moment it landed (`tests/mock_dispatch.rs`'s construction and
  field-pattern tests both failed to compile until switched to `matches!`
  with `..`), which is the downstream experience by demonstration.
- **BREAKING — the `mock` Cargo feature itself is removed** (task #1288,
  2026-08-23, item 42 option (a), mirroring aligned-vmem's task #962): the
  recording mock backend is now enabled ONLY by the build-time cfg flag
  `numa_shim_mock` (`RUSTFLAGS="--cfg numa_shim_mock"`). The cfg still
  applies build-graph-wide once set; what changed is WHO can set it — only
  the top-level build invoker via an explicit RUSTFLAGS/build-script choice,
  never a transitive dependency through Cargo's additive feature-unification,
  and never `--all-features`/docs.rs/`cargo add` by accident. Migration for a
  0.1.0 `--features mock` consumer (one line): replace `cargo test --features numa-shim/mock`
  (or `--features mock` inside the crate) with `RUSTFLAGS="--cfg numa_shim_mock" cargo test`
  — the `numa_shim::mock` module, its API, and the dispatch behavior are unchanged;
  only the activation mechanism moved.
- **BREAKING (task #1306, 2026-08-24) — `bind_range(base, len, node)` removed
  entirely** (the crate's single `pub unsafe fn`). The byte-range binding API
  was confirmed broken by design, not by a fixable bug: `mbind(2)` requires a
  page-aligned `addr`, so an ordinary heap `Vec` (the crate's own README
  example) silently got `EINVAL` — and the discarded return value hid it; and
  even with alignment fixed, `mbind` with default flags only affects FUTURE
  page faults, so an already-touched allocation could never be retroactively
  placed. Full analysis:
  `docs/NUMA_BIND_RANGE_CONTRACT_RECOMMENDATION_2026-08-24-121245-Sol-codex.md`.
  There is no replacement for the "bind an existing object" use case — it is
  not truthfully implementable. Reserve with `reserve_preferred_on_node`
  instead.
- **BREAKING (task #1306) — `mock::MockCall::BindRange` variant removed**
  together with `bind_range` itself (see the Changed entry for its
  replacement).

## 0.1.0 - 2026-06-29

First crates.io release, published from this repository's then-`crates/numa`
directory (renamed to `crates/numa-shim` after the publish; the crates.io
homepage field still points at the old path). Description, feature map, and
dependency set verified against the crates.io version record; the tree's last
commit before the publish timestamp was `845560f` (2026-06-28).

### Added

- **`current_node() -> Option<u32>`** — NUMA node detection with zero C
  library dependencies: Linux via `sched_getcpu` + a sysfs
  `/sys/devices/system/node/nodeN/cpumap` reader (POSIX `open`/`read`/`close`,
  stack buffer, no heap); Windows via `GetCurrentProcessorNumberEx` +
  `GetNumaProcessorNodeEx`; macOS/miri report `None` (no public NUMA API on
  macOS; miri has no OS topology).
- **`bind_range(base, len, node)`** (`unsafe fn`) — bind a mapped range to a
  NUMA node: Linux issues `mbind(2)` with `MPOL_PREFERRED` **via raw
  `syscall(2)` with the syscall number baked in as a constant** — no
  `libnuma`, no `hwloc`; Windows and macOS/miri are documented no-ops (Windows
  has no post-reserve NUMA binding API — use `reserve_on_node`).
- **`reserve_on_node(size, align, node)`** (behind the `vmem-integration`
  feature) — reserve aligned anonymous virtual memory with a NUMA preference,
  returning `aligned-vmem`'s `Reservation` (re-exported as
  `numa_shim::Reservation`): Linux reserves then binds before first
  page-fault; Windows uses `VirtualAllocExNuma` directly at reservation time;
  macOS/miri fall back to an unbound reservation.
- **`NO_NODE`** — the `u32::MAX` sentinel for interop with raw-`u32` node
  APIs; the `Option`-returning functions never return it.
- **`mock` feature** — test-only recording backend replacing the platform
  syscalls (`MockCall` log, `drain()`, `set_current_node()`), so the wrapping
  logic is assertable on any CI target, including macOS and miri.
- Zero crate dependencies by default; the only dependency (`aligned-vmem`
  `0.1`) is optional behind `vmem-integration`.
