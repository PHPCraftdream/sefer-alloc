# Nineteenth independent pre-publication audit — `numa-shim` @ `623062d`

**Author:** `@oh` (Opus, effort=high).
**Reported:** 2026-08-25, Europe/Berlin.
**Revision reviewed:** `623062d059b391a7becb5ed509df1c8e73741eaa` (`main`, confirmed via `git log -1`).
**Previous bases:** `b275a225ffe31567261ccf70df0384e711f801a9` (seventeenth review, `@oh`) and `555deba751be367ad64c5325b484bf3eababfb1b` (eighteenth review, Sol-codex). Remediation waves under review: `#1324`/`#1325`/`#1326`/`#1327` (seventeenth) and `#1329`/`#1331`/`#1332`/`#1333`/`#1334` (eighteenth), plus the `#1335` process fix.
**Mode:** READ-ONLY, STATIC. No sub-agents. No file in the repository edited, no `git` write command run. **Nothing was built or executed** — no `cargo build`/`check`/`test`/`clippy`/`rustdoc`, no Miri, no benchmark, no `cargo publish --dry-run`. Every conclusion below is source reading, verified against the Linux kernel's own `mm/mempolicy.c` / `include/uapi/linux/mempolicy.h` / `asm-generic/fcntl.h` contracts and against the sibling `aligned-vmem` source in this workspace. The only working-tree write is this report.

---

## Verdict

**NO-GO — narrowly, and for the same class of reason as the seventeenth review: nothing that ships is wrong.**

I found **no P1**, and **no memory-safety, provenance, UB, ownership, or FFI-contract defect anywhere in the shipped code**. Both remediation waves land correctly. I independently re-derived the kernel contracts behind the eighteenth review's most intricate fix (`#1329`) and they check out: `MAXNODE = 1024` exactly matches `copy_nodes_to_user`'s `ALIGN(maxnode-1, 64) / 8 = 128` bytes for a `[u64; 16]`; `kernel_get_mempolicy`'s `maxnode < nr_node_ids ⇒ EINVAL` precheck is genuinely satisfied on every x86_64/arm64 kernel (`CONFIG_NODES_SHIFT` really is `range 1 10` on both); the `MPOL_F_MEMS_ALLOWED` calling convention is right (NULL `policy` is safe because `kernel_get_mempolicy` guards its `put_user` with `if (policy && ...)`; NULL `addr` is required without `MPOL_F_ADDR`; the flag is rejected in combination with `F_NODE`/`F_ADDR`, and the code combines it with neither); and the positive oracle's `nodemask == single_node_mask(node)` equality assert is sound precisely *because* `mpol_set_nodemask` intersects the requested mask with `cpuset_current_mems_allowed` — which the new preflight is what makes safe to assume.

The block is on **three P2s, all in the verification/gate layer**, two of which turn a **legitimate, documented-as-supported environment into a hard test failure**:

1. the new `MPOL_F_MEMS_ALLOWED` preflight **panics in exactly the seccomp/container case** the file's own `ERRNO_EPERM` allowlist entry exists to tolerate — the code contradicts its own doc comment;
2. `tests/smoke.rs`'s platform predicate is **arch-blind**: four tests hard-fail on every Linux target that is not x86_64/aarch64 — a row `src/lib.rs`'s own platform matrix documents as supported. This is the **third dimension** of the same predicate defect the seventeenth review's P2-2 already patched for the second;
3. the **one** CI row that executes the flagship policy oracle carries **no green-and-dead sentinel**, while every less-important numa-shim row in the same job does.

All three are cheap and mechanical. If the owner prefers, each can be explicitly risk-accepted with a recorded note, in which case the **code-level verdict is GO**.

---

## Limitations and method

- One agent, no sub-agents. Source mode read-only; the only write is this report.
- Full crate re-read from scratch with no assumption that prior remediation worked: `src/lib.rs` (all 2295 lines), all 9 integration test files, `benches/numa_bench.rs`, `Cargo.toml`, `README.md`, `CHANGELOG.md`, and every `numa-shim-*` job in `.github/workflows/ci.yml`.
- Cross-checked against `crates/aligned-vmem/src/reservation.rs` (`from_raw_parts` preconditions), `src/os/unix.rs` (whole-mapping, no-trim reserve → the span `mbind` is applied to is genuinely one live VMA) and `src/os/windows.rs` (two-call path's `over`/`commit_len` shape → the mock-backend Windows oracle is not vacuous).
- Kernel-contract claims re-derived from `mm/mempolicy.c` (`kernel_get_mempolicy`, `do_get_mempolicy`, `copy_nodes_to_user`, `get_nodes`, `get_policy_nodemask`, `mpol_set_nodemask`, `mpol_new_preferred`), `include/uapi/linux/mempolicy.h`, `asm-generic/fcntl.h` plus the sparc/alpha/parisc overrides, and `arch/{x86,arm64}/Kconfig`.
- Reviewed by `rust-intel` categories: unsafe/FFI, errno timing, strict provenance, cleanup/ownership on every error exit, cfg matrix, test-oracle validity, documented-promise conformance, cold/hot paths. Async/crypto/network are absent and inapplicable.
- Runtime statements below are static-analysis results, not new executions.

---

## What changed since the last two reports

| Commit / task | Check | Assessment |
|---|---|---|
| `#1324` — `NUMA_SHIM_REQUIRE_ORACLE=1` fatal-skip gate | Present at all four sites (`policy_oracle_linux.rs` ×2, `smoke.rs` ×2); env set on exactly one CI row | **Closed correctly** — but see P2-3: it makes the *skip* loud, not the *not-compiled* case |
| `#1325` — backend-aware smoke predicate | `cfg!(any(numa_shim_mock, all(any(linux, windows), not(miri))))` at all three sites; macOS mock+vmem CI row with three grep sentinels added | **Closed for the backend dimension; the arch dimension is still open** — P2-2 |
| `#1326` — default-feature rustdoc row + de-linked intra-doc refs | Third rustdoc row present (`cargo doc -p numa-shim --no-deps`, no `--features`); the three sites are now plain backticks | **Closed correctly** |
| `#1327` — `ErrorKind::Interrupted` + `mod eintr` + `O_CLOEXEC` | Predicate is portable and now testable on every host (`tests/eintr_retry.rs`, incl. a real counterfactual oracle); `EINTR` is indeed the only errno std's `decode_error_kind` maps to `Interrupted` | **Closed correctly**, one arch-portability residual — P3-1 |
| `#1329` — policy oracle rebuild | Preflight, allowlist inversion and `[u64; 16]`/`maxnode = 1024` all verified against the real kernel contracts; the errno *classification* is now correct | **Correct in substance; introduces a new false-red in containers** — P2-1 |
| `#1331` — topology init before CPU snapshot | `let topo = topology();` precedes `sched_getcpu()` in both entry points; `.lookup` called on the already-held reference (no redundant second `OnceLock` probe) | **Closed correctly** |
| `#1332` — single-snapshot mapping oracle | One raw `sched_getcpu()` sample fed into both non-resampling forwarders; negative sample skips loudly; `#[non_exhaustive]` wildcard arm preserved | **Closed correctly** |
| `#1333` — doc-honesty wave | All four sites verified: 32-bit comment, `sched_getcpu`-per-call precision, bench honesty, "zero third-party C/C++" wording in all three of `lib.rs`/`README.md`/`Cargo.toml` | **Closed correctly** |
| `#1334` — `rsplit` linearization | I re-derived equivalence by hand on empty / whitespace-only / trailing-comma / leading-comma / double-comma / word-boundary inputs, including the *partial* `on_cpu` callbacks before a malformed token: byte-for-byte identical to the old `nth_token` ordering. `nth_token`'s retention is justified (still a live test oracle) | **Closed correctly** |

---

## Findings blocking `GO`

### P2-1 — the `MPOL_F_MEMS_ALLOWED` preflight hard-panics in exactly the sandboxed environment the file's own `ERRNO_EPERM` allowlist exists for

**Sites:** `crates/numa-shim/tests/policy_oracle_linux.rs:388-395` (the `.expect`), against `:193-199` (`ERRNO_EPERM`'s doc) and `:432-441` (the EPERM skip arm it was written to reach).

`ERRNO_EPERM`'s doc comment states the case it exists for, verbatim:

> kept on the skip allowlist anyway because seccomp-based sandboxes (e.g. docker's default profile) deny mbind with exactly EPERM: the F8.2 container case this skip arm has always existed for.

That is accurate about Docker: its default seccomp profile gates `get_mempolicy`, `mbind` and `set_mempolicy` **as one group** behind `CAP_SYS_NICE`, and denies them with `SCMP_ACT_ERRNO(EPERM)` otherwise. But task `#1329` inserted the preflight **before** the reserve call, and the preflight does not classify anything:

```rust
let allowed = unsafe {
    get_mems_allowed().expect(
        "get_mempolicy(MPOL_F_MEMS_ALLOWED) preflight must succeed; if this \
         environment sandbox-denies the NUMA policy syscalls (seccomp \
         EPERM/ENOSYS), the policy oracle cannot run here at all — ...",
    )
};
```

So in the motivating scenario the sequence is: `current_node()` succeeds (sysfs is readable inside a container), `get_mems_allowed()` returns EPERM, `.expect` **panics**. The `Some(ERRNO_EPERM) => skip` arm 40 lines below is **unreachable in the one environment it names** — because the two syscalls are denied together, there is no realistic configuration where the preflight is permitted and the `mbind` is not.

Net effect: `cargo test -p numa-shim --features vmem-integration` now goes **hard red** on containerized Linux — an extremely common downstream and CI shape, and the exact shape this crate's own CHANGELOG risk-acceptance text calls "the F8.2 container case". The defect is not the panic per se; it is that the file's code and its own documentation now assert opposite policies for the same environment, and the eighteenth review's stated intent ("bit unset ⇒ loud skip before any reserve call"; environment errnos skip) is not what the code does.

The message string's own hedge — "the policy oracle cannot run here at all" — is a description of a *skip* condition, written into a *panic*.

**Fix:** classify the preflight's errno with the same allowlist the reserve arm already has. `EPERM`/`ENOSYS` from `get_mems_allowed()` ⇒ loud `eprintln!` skip (the environment genuinely cannot run this oracle); every other errno ⇒ panic, fail-closed, exactly as today. If a stricter posture is wanted, make that skip fatal **under `NUMA_SHIM_REQUIRE_ORACLE=1`** — that env var is precisely the "this host is supposed to support NUMA" declaration, and the repo's own CI row is not a container.

Note the negative control (`:562-572`) has the same container-hostility via its own bare `.expect` on `get_mempolicy_addr`, but that one is a *pre-existing, explicitly reasoned* decision recorded in the task `#1318` comment; only the preflight is new and only the preflight contradicts a doc comment.

---

### P2-2 — `tests/smoke.rs`'s platform predicate is arch-blind: four tests hard-fail on every Linux target that is not x86_64/aarch64

**Sites:** `crates/numa-shim/tests/smoke.rs:170-185`, `:272-287`, `:483-490`, `:499-518`. Production counterpart: `src/lib.rs:1432-1436`. Contrast with `tests/policy_oracle_linux.rs:45-51`, which **does** arch-gate.

`src/lib.rs`'s own platform matrix (`:56`) documents this row as a first-class supported outcome:

> `| Linux other arch (non-miri) | sched_getcpu + sysfs cpumap | UnsupportedArchitecture error |`

…and `README.md:241-243` repeats it in prose. But the smoke suite's predicate is

```rust
if !cfg!(any(
    numa_shim_mock,
    all(any(target_os = "linux", windows), not(miri))
)) { /* expect UnsupportedPlatform */ return; }
```

— target-OS and backend, never `target_arch`. On `armv7-unknown-linux-gnueabihf`, `riscv64gc-unknown-linux-gnu`, `s390x-unknown-linux-gnu`, `powerpc64le-unknown-linux-gnu`, `i686-unknown-linux-gnu` (all real rustc targets; armv7 is a *very* common downstream), the crate compiles, `mod platform` compiles, `reserve_preferred_on_node_impl` takes its `#[cfg(not(any(x86_64, aarch64)))]` arm and returns `UnsupportedArchitecture` **before any argument or node validation** — and four tests then fail:

| Test | Expects | Actually gets |
|---|---|---|
| `reserve_preferred_on_node_returns_valid_span` (`:213`) | `Ok(_)` via `.expect("NUMA-preferred reservation failed")` | `Err(UnsupportedArchitecture)` → **panic** |
| `reserve_preferred_on_node_large_align_round_trip` (`:314`) | `Ok(_)` via `.expect(...)` | `Err(UnsupportedArchitecture)` → **panic** |
| `reserve_preferred_on_node_rejects_zero_size_with_invalid_arguments` (`:487`) | `InvalidArguments` | `UnsupportedArchitecture` → **assert fails** |
| `reserve_preferred_on_node_rejects_node_beyond_nodemask_range` (`:515`) | `InvalidNode` | `UnsupportedArchitecture` → **assert fails** (the `node >= 64` check lives *inside* the x86_64/aarch64 arm) |

No CI row builds `numa-shim` for any non-x86_64/aarch64 Linux target, which is why it is invisible: the `multi-arch`/`cross` job targets the root crate with `production internals`, and `numa-shim` is only pulled in under `numa-aware`.

The structural point is the recurrence: this predicate has now been corrected **twice** — once for the target-OS dimension, once (seventeenth review P2-2 / task `#1325`) for the backend dimension — and it still classifies the world along fewer axes than the production dispatch does. The sibling `policy_oracle_linux.rs` already carries the missing clause, which shows the crate *knows* the axis exists.

**Fix:** replace the two-way predicate with the same three-way classification the production code performs — `mock` ⇒ Linux-shaped success; real + `any(linux, windows)` + `any(x86_64, aarch64)` (Linux only) ⇒ success; real Linux other-arch ⇒ `UnsupportedArchitecture`; everything else ⇒ `UnsupportedPlatform` — and give the two `Err`-shape tests the same treatment. A `cross test --target armv7-unknown-linux-gnueabihf -p numa-shim --features vmem-integration` row would make it self-verifying (a plain `cargo check` row will not catch it — the divergence is a runtime assertion).

---

### P2-3 — the one CI row that executes the flagship policy oracle has no green-and-dead sentinel

**Site:** `.github/workflows/ci.yml:2706-2708`.

```yaml
      - run: cargo test -p numa-shim --features vmem-integration
        env:
          NUMA_SHIM_REQUIRE_ORACLE: "1"
```

`tests/policy_oracle_linux.rs` is `#![cfg(...)]`-gated on **five** simultaneous conditions (`target_os = "linux"`, `not(miri)`, `not(numa_shim_mock)`, `feature = "vmem-integration"`, `any(target_arch = "x86_64", target_arch = "aarch64")`). If any one of them stops holding — a feature rename, a stray `--cfg numa_shim_mock` reaching this row through a workspace `.cargo/config.toml` `rustflags`, an arch-clause edit, a `#![cfg]` typo — the file compiles to **zero tests**, the row exits **0**, and CI reports green with the crate's headline capability entirely unverified. `NUMA_SHIM_REQUIRE_ORACLE=1` closes the *silent-skip* hole (task `#1324`) but is powerless against the *never-compiled* hole: an absent test cannot read an env var.

This is precisely the vacuous-green hazard this repo already codified as a convention (task `#1101`; task `#1070` "Breakage B"), and every *less* important numa-shim row in the same job already implements it: `:2664-2666` greps two sentinels, `:2674-2675` greps one, and the macOS mock+vmem row at `:2855-2859` greps three. The single row that proves `mbind(2)` actually installs `MPOL_PREFERRED` — the one thing this crate exists to do — has none.

**Fix:** one `tee` + two `grep -F` lines, matching the sibling rows:

```
grep -F "test reserve_preferred_on_node_installs_mpol_preferred_on_the_usable_span ... ok" "$LOG"
grep -F "test plain_unbound_reservation_is_not_reported_as_preferred_for_our_node ... ok" "$LOG"
```

(This does not subsume P2-1 — under P2-1's container scenario the test would print `... ok` after panicking? No: it would print `FAILED`. The two findings are independent: P2-1 is a false red, P2-3 is a possible false green.)

---

## Non-blocking findings

### P3-1 — `O_CLOEXEC` is one unconditional constant, but the Linux `platform` module compiles on every Linux arch and the value is not universal

**Sites:** `src/lib.rs:1569` (the constant), `:1608` (its only use), `:1336` (the module's cfg gate).

```rust
const O_CLOEXEC: core::ffi::c_int = 0o2000000;
```

Its doc comment is honest about what was verified — "value from Linux `asm-generic/fcntl.h` … identical on x86_64 … and aarch64" — but the *code* is gated on `all(target_os = "linux", not(miri))`, with **no arch clause**, while the value is arch-specific in the kernel UAPI:

- x86_64, aarch64, riscv64, s390x, powerpc, mips, arm, i686 → `02000000` ✓ (asm-generic)
- **sparc/sparc64** → `O_CLOEXEC = 0x400000`; `0x80000` (= `02000000`) is not a defined flag there and is silently ignored
- alpha, parisc → `O_CLOEXEC = 010000000`; `02000000` is **`O_DIRECT`**

`sparc64-unknown-linux-gnu` is a real rustc target. There, the task `#1327`/P3-2 hardening is **silently absent** — the cpumap fd is opened without close-on-exec and nothing signals it. (The alpha/parisc case, where the same bits mean `O_DIRECT` and would make every sysfs read fail, is not reachable through rustc today, but it illustrates that the constant is not merely "unset elsewhere".)

**Fix:** either gate the constant per-arch, or restrict the reader to the arches whose value was verified, or state in the doc comment that the flag is a best-effort hardening that is inert on arches with a divergent UAPI value. Any of the three is honest; the current combination (verified-for-two doc, applied-to-all code) is not.

### P3-2 — the `/proc/self/maps` release oracle's own "why this is unlikely to race" reasoning is inverted for Linux's actual placement policy

**Site:** `crates/numa-shim/tests/smoke.rs:427-454` (and the twin note at `:419-423`).

> This is unlikely because the four probes are megabytes apart while every other reservation in this test binary is tiny, and the window is sub-millisecond.

That argument assumes uniformly random placement. Linux's `arch_get_unmapped_area_topdown` (the default layout on x86_64/aarch64) places a new anonymous mapping at the **top of the highest sufficient gap**. Immediately after `drop(r)`, the just-released `over`-byte region **is** that gap — everything above it is still mapped, because the top-down allocator packs downward from `mmap_base`. So a concurrent `mmap` landing in the window does not land "somewhere in megabytes of slack": it lands ending **exactly at `raw + over`**, i.e. covering probe #4 (`raw + over - page`) *preferentially*.

Concurrent `mmap`s in that window are not hypothetical: libtest spawns a fresh thread per test in the same binary, and each thread stack is a multi-MiB anonymous mapping that fits the freed 8 MiB hole comfortably.

I did not observe a failure and this has evidently been green; the finding is that the **documented mitigation is not the mitigation**, so the residual risk is governed purely by the timing window and would silently grow if the file gained more mmap-performing tests.

**Fix (any one):** snapshot `/proc/self/maps` both before `drop(r)` and after, and assert the *difference* covers the region; or drop the two extremal probes and keep the interior ones (which the top-down allocator will not preferentially reuse); or serialize this one test behind a process-global mutex shared with the other reserving test. At minimum, correct the comment.

### P3-3 — the topology `OnceLock` initializer's stack frame is ~12–20 KiB, on the one path the crate advertises as global-allocator-reentrant-safe

**Site:** `src/lib.rs:1508-1523`, and the `static TOPOLOGY` doc at `:1487-1507`.

Task `#777`'s entire point — restated at length in that doc comment and in the CHANGELOG — is that this initializer must be safe to run *from inside* a `#[global_allocator]`, which is why the heap `Vec<Vec<u8>>` was replaced by fixed-size storage. Heap use was analysed and removed; **stack** use was neither analysed nor documented. The closure materializes `ReverseIndex::new()` = `[u8; 8192]` as a stack temporary, plus `buf: [u8; 4096]`, plus `path: [u8; 64]`, and then **returns the 8 KiB value by move** into the `OnceLock` cell — a second 8 KiB copy if NRVO does not fire. Peak frame is therefore ~12 KiB best case, ~20 KiB worst.

That is fine on a default 2 MiB Rust thread stack. It is not obviously fine on a `Builder::stack_size(64 * 1024)` thread, and a stack overflow inside a `OnceLock` initializer is an abort, not a recoverable error — the same failure *class* the allocation-free rewrite was performed to eliminate, on the one axis it did not consider.

**Fix:** cheapest is to state the peak-stack figure next to the allocation-free claim, so a downstream consumer running `current_node()` on a small-stack thread has the fact. (Restructuring to build the index in place is possible but not obviously worth it.)

### P3-4 — the README's code examples are compiled by nothing, and this crate has already shipped a broken one

**Site:** `crates/numa-shim/README.md:55-63`, `:97-147`, `:170-226`.

`src/lib.rs` does not `include_str!` the README; there is no `tests/readme_*.rs`; and unlike the sibling `aligned-vmem` (which has `scripts/vmem-doc-drift-guard.mjs` and `scripts/vmem-linux-android-pairing-guard.mjs` in the pre-push gate), `numa-shim` has no doc guard at all. The `vmem-integration` example is the crate's single most-copied artifact and it has **already been broken once** — finding F6 / task `#1268`, where `aligned_vmem::PAGE` was not reachable from a downstream consumer — with nothing added afterwards to prevent recurrence.

A live, if small, symptom of the absent check: the example's own `use numa_shim::{current_node, NodeId, ReserveNumaError, reserve_preferred_on_node};` imports `ReserveNumaError`, which the snippet never names (an unused-import warning if it were ever compiled).

Separately, the `## Public API` block is ` ```rust ` -fenced but contains bodiless `pub fn …;` declarations — not valid Rust. Under CLAUDE.md's no-doctest rule the crate's own doc comments already use ` ```text ` for exactly this; the README should match, so that any future README-compiling guard does not have to special-case it.

**Fix:** a `tests/readme_examples.rs` (feature-gated on `vmem-integration`) carrying the one real example, and retag the signature block as ` ```text `.

### P3-5 — structure and stale prose inside the still-open `## Unreleased` CHANGELOG section

**Site:** `crates/numa-shim/CHANGELOG.md`.

1. **Two `### Changed` headings under one `## Unreleased`** (`:57` and `:169`). This is the exact duplicate-heading class that task `#1274` (finding N10 of the tenth review) already fixed once for this file.
2. **`### Added` describes API that no longer exists in the same unreleased cycle.** `:128-137` names the variant `FellBackToZero` (renamed to `TopologyUnavailable` at `:61`); `:139-144`'s "Scope note" states "`bind_range` with `node >= 64` still silently no-ops with no caller-detectable signal (F4's binding-side ask remains open)" — `bind_range` was removed at `:289-300`; `:104-108` documents `bind_range`'s `# Safety` contract as current. When this section is consolidated under one dated version heading (as `:12-13` instructs), the released notes will simultaneously describe `bind_range`'s behaviour and its removal, and will name a variant that never existed publicly.
3. **The gate caveat block is stale on its own numbers.** `:22-24` records Phase 1 as "31/0 at `c427dd6`" while the mock suite has grown materially since (task `#1324`'s own entry in this same file cites 46 mock tests), and it states outright that "the final pre-tag re-run is still owed per the eleventh review's E1 ordering rule". That is a real, self-declared open release-process item, not a defect — but it is worth surfacing because it is the *only* thing in this file that must be executed rather than edited before a tag.

---

## Additional observations (informational, no action required for `GO`)

- **Clippy matrix hole:** no row runs `--cfg numa_shim_mock` *together with* `--features vmem-integration`. The Linux mock-clippy row (`ci.yml:2723`) omits the feature; the `--all-features` row (`:2718`) cannot reach the cfg; the Windows mock-clippy row (`:2809`) likewise. The combination *compiles* under three test rows, so this is lint coverage only.
- **MSRV job never exercises the mock cfg** (`ci.yml:1916-1917`: `cargo check -p numa-shim`, `cargo test -p numa-shim --no-run --all-features`). A newer-than-1.88 API introduced inside `mod mock` would not be caught. Currently clean — `const {}` blocks (1.79), `is_multiple_of` (1.87), `addr`/`with_addr` (1.84), `offset_of!` (1.77), `io::Error::other` (1.74) are all within 1.88.
- **`mock::set_policy_failure`/`clear_policy_failure` remain public without `vmem-integration`** even though nothing can consume them there (which is why `take_policy_failure_for` carries `#[cfg_attr(not(feature = "vmem-integration"), allow(dead_code))]`). The eighteenth review already noted this; still open, still not a blocker.
- **The miri twin of `mock + vmem-integration` remains uncovered** by any CI row — correctly recorded in task `#1325`'s CHANGELOG entry as a known gap.
- **`aligned-vmem` is still `0.2.0` locally**, so the `version = "0.2", path = "../aligned-vmem"` dependency will still resolve to the published registry copy at `cargo publish` time — no drift since the thirteenth review's out-of-workspace tarball verification.

### Re-verified clean this pass (no finding)

The Linux two-stage reserve→`mbind`→release-on-failure cleanup and its capture-errno-before-cleanup ordering; the Windows `MEM_RESERVE`/`MEM_COMMIT`/`MEM_RELEASE` ownership on **all three** error exits (`checked_add` overflow, commit failure, `committed != base`), each releasing exactly once with no handle ever handed out; the unconditional (no longer `debug_assert`) commit-base check; `Reservation::from_raw_parts`'s documented preconditions against what the Windows path actually supplies (`base >= raw`, `base + size <= raw + over` since `base <= raw + align - 1` and `over = size + align`, `len`/`reservation_len` page multiples, `align` power-of-two ≥ `PAGE`); the strict-provenance `addr()`/`with_addr()` round-trip; the `ProcessorNumber` layout const-asserts (size 4 / align 2 / offsets 0,2,3); `mbind`'s `maxnode = 65` compensation for `get_nodes()`'s internal `--maxnode` (⇒ `nlongs = BITS_TO_LONGS(64) = 1` ⇒ exactly the 8-byte `u64` nodemask); `MAXNODE = 1024` vs `copy_nodes_to_user`'s `ALIGN(1023,64)/8 = 128` bytes exactly matching `[u64; 16]`, including the `nr_node_ids`-clamped short copy plus `clear_user` tail on smaller kernels and the `copy > PAGE_SIZE` guard; `get_policy_nodemask` returning exactly `{node}` for a flagless `MPOL_PREFERRED` (so the equality assert is neither vacuous nor over-strict); the `rsplit` linearization's byte-for-byte equivalence to the old `nth_token` ordering including partial callbacks on malformed input; `ReverseIndex`'s fail-closed validate-then-commit and first-mapping-wins semantics; `format_sysfs_path`'s 10-digit temp buffer against the 64-byte path buffer (29 + 10 + 8 = 47 max); the four `mod platform` cfg blocks' mutual exclusivity including the macOS×miri cell; `should_retry_eintr`'s `ErrorKind::Interrupted` equivalence to `EINTR` on the real `last_os_error()` path, with the streak reset on forward progress and no fd leak on either retry site; `read_cpumap_into`'s fail-closed treatment of a file at or beyond the buffer size; and `#![deny(missing_docs)]` coverage of every public item, including all three `#[doc(hidden)]` semver-exempt modules.

---

## Recommended fix order

1. **P2-1** — classify the preflight's errno (EPERM/ENOSYS ⇒ skip, else panic), so the file stops contradicting its own `ERRNO_EPERM` doc and containerized runs stop going red.
2. **P2-2** — make `smoke.rs`'s predicate arch-aware; add the `UnsupportedArchitecture` expectation to all four tests.
3. **P2-3** — add the two grep sentinels to the `NUMA_SHIM_REQUIRE_ORACLE=1` row.
4. **P3-1** — gate `O_CLOEXEC` per-arch, or scope the doc claim honestly.
5. **P3-5** — CHANGELOG: merge the duplicate `### Changed`, reconcile the `FellBackToZero`/`bind_range` prose, refresh or re-run the Phase-1 record.
6. **P3-2 / P3-3 / P3-4** — correct the release-oracle race comment; document the initializer's peak stack; add a README-example compile guard and retag the signature fence.

## Conditions for `GO`

- The Linux policy oracle no longer produces a **false red** on a sandbox that denies the NUMA policy syscalls, and its skip/panic split matches what its own constants document.
- `cargo test -p numa-shim --features vmem-integration` is green on **every Linux target the crate's platform matrix claims**, not only x86_64/aarch64.
- The real-Linux CI row **proves the positive oracle executed**, not merely that the process exited 0.
- Either the `O_CLOEXEC` constant is arch-correct wherever the reader compiles, or the doc says where it is inert.
- The CHANGELOG's `## Unreleased` section reads correctly as a *single* release's notes, and the owed Phase-1 pre-tag re-run is either executed or explicitly re-waived.
- After the fixes, the owner runs the normal dynamic verification matrix — this read-only review does not substitute for it.

Once those are closed, the production design is, in my assessment, publication-ready: a safe public API with no `pub unsafe fn`, typed failures on every path, fail-closed detection, RAII cleanup with errno captured before it, a deliberately narrow and now honestly-stated platform contract, and unsafe confined to a small, individually-justified FFI seam that I could not fault at this revision.
