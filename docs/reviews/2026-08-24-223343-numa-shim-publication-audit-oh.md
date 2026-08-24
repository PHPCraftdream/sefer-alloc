# Seventeenth independent pre-publication audit — `numa-shim` @ `b275a22`

**Author:** `@oh` (Opus, effort=high).
**Reported:** 2026-08-24 22:33:43 Europe/Berlin (UTC+02:00).
**Revision reviewed:** `b275a225ffe31567261ccf70df0384e711f801a9` (`main`; confirmed via
`git log -1`, matching the revision named in the brief).
**Previous base:** `9137c514775ca539a99e454945cf6a6103cc7ecb` — the revision the sixteenth
review (`docs/reviews/2026-08-24-204022-numa-shim-publication-audit-Sol-codex.md`) examined.
Remediation wave under review: `546e8d8` (#1318), `652d505` (#1319, merged via `2571f4b`),
`f325bb1` (#1323, incidental CI-red fix), `6f242f1` (#1320), `25e25e7` (#1321).
**Mode:** READ-ONLY, STATIC. No sub-agents. No file in the repository edited, no `git`
write command run. **Nothing was built or executed** — no `cargo build`/`check`/`test`,
no `clippy`, no `rustdoc`, no Miri, no benchmark, no `cargo publish --dry-run`. Every
conclusion below is source reading. The only working-tree write is this report.

## Verdict

**NO-GO — narrowly, and for a materially weaker reason than the previous three reviews.**

I found **no P1**, and **no correctness, memory-safety, provenance, ownership or FFI-contract
defect in the code that would actually ship**. All five P1/P2 findings of the sixteenth review
are present, correct, and introduce no new defect (§2). The Linux `mbind` seam, the Windows
`VirtualAllocExNuma` seam, the cpumap parser/reverse index, and the two-stage
reserve-then-policy cleanup contract all re-read clean at this revision.

The block is on **three P2s, all in the VERIFICATION and GATE layer rather than in shipped
behavior** — the same class this campaign has consistently treated as verdict-gating (the
sixteenth review's own mock-hygiene and 32-bit-policy P2s were both exactly this shape).
The most important of the three is a direct continuation of that review's P1: the policy
oracle it hardened can **still** pass vacuously, through a different arm, and a regression in
the crate's own detection path is precisely what opens it.

If the owner prefers, all three are cheap and mechanical; alternatively any of them can be
explicitly risk-accepted with a recorded note, in which case the code-level verdict is GO.

## 1. Findings

### P2-1 — the policy oracle (and three more positive Linux tests) still skip silently, and a regression in the crate's OWN detection path is what triggers it

**Sites:** `crates/numa-shim/tests/policy_oracle_linux.rs:206-213` and `:325-331`;
`crates/numa-shim/tests/smoke.rs:157-165` and `:240-248`.

Task #1318 closed the `Err(ReserveNumaError::Os(..))` half of the vacuous-skip hazard
correctly (§2, fix 1). The **other** skip arm in the same test was left untouched:

```rust
let Some(node) = current_node() else {
    eprintln!("skip: current_node() could not resolve a node on this host …");
    return;
};
```

This arm is not merely an environment concern. `current_node()` is *this crate's own code* —
`sched_getcpu` → `read_cpumap_into` → `ReverseIndex::index_node` → `lookup`, all of it
touched by the #1310 reverse-index rewrite and by #1319's EINTR retry in this very wave. A
regression anywhere in that chain that turns detection into `None` therefore **silently
disables the crate's own flagship-API oracle**, and CI stays green. Four positive tests
share the identical skip arm and would go dark together on the one CI row where the real
Linux backend runs (`numa-shim-mock` job's `cargo test -p numa-shim --features
vmem-integration`, `.github/workflows/ci.yml:2692`):

- `policy_oracle_linux::reserve_preferred_on_node_installs_mpol_preferred_on_the_usable_span`
- `policy_oracle_linux::plain_unbound_reservation_is_not_reported_as_preferred_for_our_node`
- `smoke::reserve_preferred_on_node_returns_valid_span`
- `smoke::reserve_preferred_on_node_large_align_round_trip`

The CI sentinel greps that guard the mock rows (`grep -F "test … ok"`, ci.yml:2665-2675)
would not help even if extended here: an internally-skipped test still prints `... ok`.

This is the same defect shape the sixteenth review's P1 named — "a test that reports success
exactly when the thing it exists to prove is broken" — reached through the detection half
instead of the policy half.

**Fix:** this repo already owns the pattern. `numa-real-kernel` gates its real-kernel tests on
`SEFER_NUMA_TEST=1`; do the equivalent in reverse — an env flag (e.g.
`NUMA_SHIM_REQUIRE_ORACLE=1`) set on the one ubuntu row that runs the real backend, under
which the `current_node() == None` arm **panics** instead of skipping. Local/dev runs keep
today's skip. That converts "silently green forever" into "loud on the exact CI row that is
supposed to be proving this," at the cost of one `std::env::var` check per test.

### P2-2 — `tests/smoke.rs` is compiled under the mock but its platform expectations are written for the real backend: `--cfg numa_shim_mock` + `vmem-integration` is red on macOS and under miri

**Sites:** `crates/numa-shim/tests/smoke.rs` (no `#![cfg]` gate anywhere in the file — contrast
`tests/mock_dispatch.rs:18`, `tests/node_resolution.rs:8`, `tests/node_resolution_linux.rs:9`,
`tests/policy_oracle_linux.rs:45-51`, all of which state their backend); the three
real-backend-only branches at `smoke.rs:143-155`, `:226-238`, `:413-419`; the mock dispatch arm
at `crates/numa-shim/src/lib.rs:808-877`.

Each of those three branches asserts:

```rust
if !cfg!(all(any(target_os = "linux", windows), not(miri))) {
    // Unsupported platform: must error, not succeed.
    assert!(matches!(result, Err(ReserveNumaError::UnsupportedPlatform)), …);
```

`cfg!(…)` sees only the TARGET, never the backend. Under `--cfg numa_shim_mock` the mock arm
(`src/lib.rs:808-877`) performs **no platform check at all** — it records, checks `node < 64`,
then calls `aligned_vmem::try_reserve_aligned` and returns `Ok(r)`. On macOS that reservation
succeeds. So on macOS with `RUSTFLAGS="--cfg numa_shim_mock" cargo test -p numa-shim
--features vmem-integration`, three tests fail:

- `reserve_preferred_on_node_returns_valid_span` (expects `UnsupportedPlatform`, gets `Ok`)
- `reserve_preferred_on_node_large_align_round_trip` (same)
- `reserve_preferred_on_node_rejects_zero_size_with_invalid_arguments` (expects
  `UnsupportedPlatform`, the mock returns `InvalidArguments`)

No CI row runs that combination, which is why it is green today: `numa-shim-macos`
(ci.yml:2792-2795) runs mock **without** `vmem-integration`; `numa-shim-macos-miri`
(ci.yml:2828-2833) does the same; only the Linux and Windows jobs pair mock **with** the
feature, and on those two targets the `cfg!` guard happens to take the other branch.

This matters beyond a hypothetical CI row. The README advertises the mock as the mechanism
that lets CI "assert the wrapping logic on any target, **including macOS and miri**"
(`README.md:150-156`), and `src/lib.rs:239-243` repeats it ("Records every invocation … so unit
tests can assert the wrapping logic is correct on any target (including macOS and miri)"). The
crate's own test suite is not in fact runnable in that advertised configuration, and a
contributor on a Mac running `RUSTFLAGS="--cfg numa_shim_mock" cargo test -p numa-shim
--all-features` — an entirely reasonable thing to do — gets three red tests with no indication
that the combination is unsupported.

This descends from the fifteenth review's F6 (mock/real backend divergence), which task #1311
deliberately resolved with a doc-honesty note rather than platform-faithful mocking
(`src/lib.rs:818-825`). That decision is defensible; what is missing is the consequence being
carried into the test file. **Fix (cheapest):** make the three branches backend-aware —
`if !cfg!(any(numa_shim_mock, all(any(target_os = "linux", windows), not(miri))))` — so under
the mock they take the positive path on every target, matching what the mock actually does.
Adding the macOS mock+vmem row to CI afterwards then proves it.

### P2-3 — three unresolved intra-doc links in the DEFAULT feature set, which no CI rustdoc row builds

**Sites:** `crates/numa-shim/src/lib.rs:51` (crate-level platform matrix), `:133` (`NodeId::new`),
`:575` (`NodeResolution::TopologyUnavailable`) — all three write `` [`reserve_preferred_on_node`] ``
from an **ungated** doc comment, while the item itself is `#[cfg(feature = "vmem-integration")]`
(`src/lib.rs:802-803`).

With default features the item is stripped before rustdoc builds its item tree, so each of the
three is an `rustdoc::broken_intra_doc_links` diagnostic ("unresolved link to
`reserve_preferred_on_node`"). The other four link sites are fine: `:229`/`:234` sit inside the
`#[cfg(feature = "vmem-integration")]` re-export's own doc, and `:789`/`:790` inside
`reserve_preferred_on_node` itself.

CI has exactly two rustdoc rows, `.github/workflows/ci.yml:2717` (`--all-features`) and
`:2718-2724` (the derived `package.metadata.docs.rs` set). For this crate both resolve to
`vmem-integration`, which is the one configuration where all three links resolve — so the
`-D warnings` doc gate is structurally blind to the default set. That is the same
gate-blindness shape CLAUDE.md's own doc-lint rule (task #1142) describes, and it is
inconsistent with how this repo already treats default features for **clippy**:
ci.yml:2763-2768 carries a dedicated default-feature clippy row with the explicit comment
"DEFAULT features is what `cargo add numa-shim` produces for a downstream consumer -- checked
here". Rustdoc got no such row.

Blast radius is genuinely small — docs.rs renders with `vmem-integration`, and Cargo passes
`--cap-lints allow` to registry dependencies, so a downstream consumer sees nothing. The
concrete costs are (a) the default-feature rendered docs show plain text where a link belongs,
(b) `cargo doc -p numa-shim` in this workspace is noisy, and (c) task #1277 already had to fix
this exact class once ("ложные intra-doc ссылки"), with nothing added afterwards to stop it
recurring — which is what happened.

**Fix:** two lines of work. Wrap the three links' feature-dependence
(`#[cfg_attr(not(feature = "vmem-integration"), doc = "…")]`, or simply drop the brackets and
leave the name in backticks at those three sites), and add
`RUSTDOCFLAGS="-D warnings" cargo doc -p numa-shim --no-deps` (no `--features`) as a third row
next to ci.yml:2717.

Not verified by me, and deliberately not claimed as a defect: no CI row runs rustdoc under
`--cfg numa_shim_mock` either, so the `mock` module's own intra-doc links
(`src/lib.rs:248-250`, `:317`, `:331`) are unchecked in every configuration. That module never
reaches docs.rs, so it is not a publication concern — but it is part of the same coverage gap
and worth folding into the same fix.

### P3-1 — the EINTR fix hardcodes errno 4 and is placed where no test can reach it

**Sites:** `crates/numa-shim/src/lib.rs:1466` (`const EINTR: i32 = 4;`), `:1490-1492`
(`should_retry_eintr`).

The predicate is correct as written (§2, fix 2), but two choices in it are worth revisiting
before the number is frozen into a release:

1. **`std::io::ErrorKind::Interrupted` already exists and is exactly this.** The crate avoids
   the `libc` *crate*, which is why `SYS_MBIND`/`MPOL_PREFERRED` are local constants — but
   `std::io::Error` is already in use here, and std's own `decode_error_kind` maps `EINTR` to
   `ErrorKind::Interrupted` on every Unix, maintained by the standard library. Writing
   `err.kind() == std::io::ErrorKind::Interrupted` adds no dependency, removes a hand-copied
   errno number, and is portable by construction. The `SYS_MBIND` precedent does not transfer:
   there is no std API for a syscall number, and there IS one for this.
2. **The helper is unreachable from `tests/`.** It sits inside
   `#[cfg(all(target_os = "linux", not(miri)))] mod platform` as a private `fn`, so the fix
   ships with **zero automated coverage** — the CHANGELOG says so in its own words ("no unit
   test was added because the module is cfg-gated to real Linux … the predicate is a single
   boolean expression"). But nothing about `should_retry_eintr(&std::io::Error, u32) -> bool`
   is Linux-specific except the constant's value; it is a pure function of two portable inputs.
   This crate's own `cpumap` module exists **precisely** to refute the "it's Linux-only so it
   can't be tested" argument — `src/lib.rs:885-901` states it outright: "gating them inside
   `#[cfg(target_os = "linux")]` was an accident of code organization, not a genuine platform
   requirement … it meant the crate's own most intricate parsing logic could ONLY be exercised
   on a real Linux host."

Combined, these mean a wrong constant would silently restore exactly the bug #1319 fixed (the
retry never fires; detection goes permanently `None` on one signal) with no failing test on any
host and no CI signal — and the EINTR branch is not exercised by the Linux CI row either, since
signals do not arrive on cue. Moving the predicate into the existing target-independent
`#[doc(hidden)] pub mod cpumap`-style seam and asserting
`should_retry_eintr(&Error::from_raw_os_error(4), 0) == true`,
`…(…, EINTR_RETRY_LIMIT) == false`, and `…(&Error::from_raw_os_error(13), 0) == false` costs one
small test file and runs on this project's Windows dev host.

### P3-2 — the sysfs cpumap fd is opened without `O_CLOEXEC`

**Site:** `crates/numa-shim/src/lib.rs:1526` — `libc_open(path.as_ptr() as *const c_char, 0)`;
declaration at `:1616-1622`.

`flags = 0` is `O_RDONLY` with no `O_CLOEXEC`. A library that opens files in a process it does
not control should set it: if another thread calls `fork()`+`exec()` while the topology
initializer holds the fd, the descriptor leaks into the child. The window is one
`open`/`read`/`close` triple (×64 nodes on the first call only) and the consequence is a leaked
read-only sysfs fd, not a correctness or safety bug — hence P3. But `numa-shim`'s stated
consumer is an allocator running inside arbitrary applications, `current_node()` is documented
as reachable from an allocation path, and the fix is `0o2000000` (`O_CLOEXEC`, identical on
x86_64/aarch64 Linux, same local-constant precedent as `EINTR` above) added to the flags
argument, with no behavior change otherwise. Pre-existing, not introduced by this wave.

### Already-known, deliberately not re-litigated

- **F2 / version metadata (re-confirmation, not a new finding).** `Cargo.toml:3` is still
  `version = "0.1.0"`, `CHANGELOG.md:7` is still `## Unreleased`, `README.md:48,51,85` still
  say `numa-shim = "0.1"`, and the root pin at `Cargo.toml:933` is still `version = "0.1"`,
  while the CHANGELOG itself lists four BREAKING changes since the published 0.1.0. I record
  this only to confirm the state is unchanged; per the brief it is owner-deferred ("ignore the
  version bump, I'll say when to do it") and I do not count it against the code verdict.
  Mechanically, a publish attempt in the present state still cannot succeed against
  `.github/workflows/release.yml`'s dated-section guard.
- **The NUMA release gate.** `CHANGELOG.md:16-33` still records Phases 2 and 4 as
  owner-waived, Phase 3 as PARTIAL, and — its own words — "the final pre-tag re-run [of Phase 1]
  is still owed per the eleventh review's E1 ordering rule". That is a release-process
  prerequisite on the final SHA, not a code finding, but it is outstanding and should not be
  forgotten at tag time.
- **The two P3 perf candidates** (Windows one-call `MEM_RESERVE|MEM_COMMIT` fast path,
  `docs/perf/OPEN_ITEMS.md` item 60; reverse-index cold start vs `getcpu(2)`, item 59) remain
  unmeasured and correctly tracked. I found nothing new about either and endorse the standing
  position: do not touch the current correct implementation before numbers exist.

## 2. What I checked as fixed — independently, against the source, not the summary

**Fix 1 — P1, policy oracle no longer hides implementation errno (`546e8d8`, task #1318). CONFIRMED.**
`tests/policy_oracle_linux.rs:224-277`: the `Err(ReserveNumaError::Os(e))` arm now matches on
`e.raw_os_error()`; `ERRNO_EINVAL` (22, `:110`), `ERRNO_EFAULT` (14, `:103`) and `ERRNO_ENOSYS`
(38, `:117`) each `panic!` with the errno number and an explicit "implementation bug in its
mbind(2) syscall marshalling, NOT an environment limitation" message; `Some(errno)` otherwise
skips **with the errno printed** (`:253-265`); `None` panics (`:266-270`). The three constants'
values are correct for x86_64/aarch64 Linux. The `None` arm is effectively unreachable on Linux
(`std::io::Error::last_os_error()` always carries a raw errno, and the only errno-free `Os`
construction in the crate is the Windows contract-violation path at `src/lib.rs:2009-2011`) —
panicking there is the right posture anyway. The negative control (`:322-367`) correctly needs
no classification: it never touches the policy path and already fails loud via `.expect`,
documented at `:339-344`. One residual observation, not a finding: the skip diagnostic's wording
attributes the refusal to the policy stage ("possibly a cgroup-restricted node"), but an `Os`
error can equally come from `try_reserve_aligned` before mbind is ever reached; nothing asserts
which stage failed. Harmless — there is nothing to assert in either case — but the message is
slightly narrower than the truth.

**Fix 2 — P2, bounded EINTR retry (`652d505`, task #1319). CONFIRMED, no new defect.**
`src/lib.rs:1520-1596`. Read line by line against the pre-fix version (`git show 652d505`):
- Open loop (`:1521-1541`): errno captured **before** anything else can overwrite it; on retry it
  `continue`s without closing anything, which is correct — `EINTR` from `open` means no fd was
  created, so there is no leak and nothing to close.
- Read loop (`:1544-1589`): errno captured before the `libc_close` cleanup, preserving task
  #1306's errno-timing contract; the retry re-issues the identical `read` with the same `fd`,
  the same `out[total..]` offset and the same remaining length — sound, because POSIX guarantees
  an `EINTR`-failed `read` transferred zero bytes, so `total` is the correct resume point. The
  `continue` re-enters at the loop head, which re-checks `total >= out.len()` — no bypass of the
  buffer-full guard.
- Bound (`:1482`, `:1490-1492`): `EINTR_RETRY_LIMIT = 16` **consecutive**, with
  `read_eintr_streak = 0` on forward progress (`:1588`), so a long progressing read cannot be
  aborted by an accumulated count, and a signal storm cannot spin the `OnceLock` initializer
  forever. The open loop has no progress notion and so uses a plain 16-attempt cap — correct.
- fd hygiene: every exit path from the read loop is preceded by exactly one `libc_close` —
  buffer-full (`:1545-1548`), non-retryable error (`:1577-1579`), EOF (`:1591`). No path leaks,
  no path double-closes.
- Allocation-freedom (task #777's `OnceLock` reentrancy requirement) is preserved:
  `std::io::Error::last_os_error()` stores the errno inline in its tagged representation and
  performs no heap allocation, and `raw_os_error()` is a read. The comment at `:1562-1567`
  states this and it is accurate.
- Non-EINTR behavior is byte-for-byte the pre-fix behavior. Signature and `Option<usize>`
  contract unchanged.
See P3-1 above for the two residual observations about this fix (hardcoded errno, no test
reachability); neither is a defect in the code as written.

**Fix 3 — P2, strict provenance in the policy oracle (`6f242f1`, task #1320). CONFIRMED.**
`tests/policy_oracle_linux.rs:155` — `get_mempolicy_addr(addr: *mut core::ffi::c_void)`; the
`usize` parameter and the `addr as *mut c_void` cast inside the helper are both gone, and the
syscall site (`:175-182`) passes `addr` through with no cast. Both call sites now produce the
pointer directly: `:285` and `:350`, `unsafe { r.as_ptr().add(page) }.cast::<core::ffi::c_void>()`
— `.add` is provenance-preserving pointer arithmetic and `.cast` a plain type cast, so no
integer intermediary survives anywhere on the path. The `maxnode = 65` output-direction quirk
(`:160`, documented `:139-147`) is correct: the kernel's `copy_nodes_to_user` copies
`ALIGN(maxnode - 1, 64) / 8` = 8 bytes into the 8-byte `u64` local — exactly sized, no overflow.
Argument order matches `get_mempolicy(int *mode, unsigned long *nodemask, unsigned long maxnode,
void *addr, unsigned long flags)`. `SYS_GET_MEMPOLICY` 239 (x86_64) / 236 (aarch64) are correct.

**Fix 4 — P2, mock test cleanup (`6f242f1`, task #1320). CONFIRMED.**
`tests/mock_dispatch.rs:330-337`: `policy_failure_script_for_other_node_does_not_fire` now ends
with an explicit `mock::clear_policy_failure()` and a comment explaining why the slot would
otherwise stay armed. I independently scanned the file's other three `set_policy_failure` sites:
`:224` (consumed by its own failing call at `:225-229`), `:284` (this test, now cleared), `:349`
(consumed at `:351-355` — the slot is one-shot, `src/lib.rs:484-502`). No further leak. The fix
is load-bearing specifically under `--test-threads=1`, where libtest runs every test on the same
thread and thread-locals genuinely persist across tests.

**Fix 5 — P2, 32-bit Windows compile gate (`25e25e7`, task #1321). CONFIRMED.**
`src/lib.rs:94-100` — `#[cfg(all(windows, target_pointer_width = "32"))] compile_error!(…)`,
placed after the inner attributes (correct: a macro invocation is an item and must not separate
`//!`/`#![…]` from each other), with a message naming the policy and both doc sites. The
condition is Windows-specific, so `i686-unknown-linux-gnu` and other 32-bit non-Windows targets
are unaffected, exactly as claimed. Three policy-statement sites are in sync and all now say
"compile-time enforced": `src/lib.rs:60-71`, `Cargo.toml:15-26`, `README.md:16-29`. The
`_WIN64`-shaped `MEMORY_BASIC_INFORMATION` mirrors at `tests/smoke.rs:289-299` and `:480-490` are
now legitimate rather than an unstated assumption.

**Incidental — `f325bb1` (task #1323). CONFIRMED and correct.**
`src/lib.rs:483` — `#[cfg_attr(not(feature = "vmem-integration"), allow(dead_code))]` on
`take_policy_failure_for`, whose only caller is the `vmem-integration`-gated mock arm. Matches
the established `mod platform` precedent (`:1264`), and the comment at `:477-482` states the
exact CI row that exposed it.

**Also re-verified clean at this revision (spot checks beyond the five fixes):**
- Linux `reserve_preferred_on_node_impl` (`src/lib.rs:1298-1340`): `node >= 64` rejected before
  any syscall; `mbind` applied to `reservation_ptr()`/`reservation_len()`; errno captured
  **before** `drop(r)` runs `munmap`; `maxnode = 65` (`:1668`) is the correct compensation for
  the kernel's `get_nodes()` decrement. I confirmed against `crates/aligned-vmem/src/os/unix.rs`
  that the Unix backend keeps the **whole** over-reserved mapping rather than trimming with
  head/tail `munmap` (task #842's deliberate one-`munmap` design, stated at `unix.rs:89` and
  `:82-83`), so `[reservation_ptr, +reservation_len)` is contiguously mapped and `mbind` over it
  cannot hit a hole. This is the load-bearing precondition of the "complete OS span" design and
  it holds.
- Windows `reserve_aligned_numa` (`src/lib.rs:1881-2042`): `base + size <= raw + over` holds by
  construction; `.addr()`/`.with_addr()` provenance derivation (`:1930-1948`) is correct;
  `raw as *mut u8` at `:2035` is a plain type cast, not an integer round-trip; all three failure
  paths (`checked_add` overflow `:1937-1942`, commit failure `:1974-1993`, commit-base mismatch
  `:2004-2012`) release the reservation exactly once before any owning handle exists — no
  double-release path; `GetLastError` captured before every `VirtualFree`.
- `cpumap` parsing and `ReverseIndex` (`src/lib.rs:913-1187`): fail-closed on every malformed
  input; two-stage validate-then-write in `index_node` (`:1152-1172`) means no partial commit;
  `node > 63` rejected before the `node as u8` narrowing; `lookup` bounds-checks via
  `self.map.get(..)`. `topology()` (`:1413-1426`) reuses one scratch buffer but only ever reads
  `&buf[..n]` from the current iteration, so no stale bytes leak between nodes.
- Public API surface: no `pub unsafe fn`; `NodeId::new` rejects exactly `NO_NODE` and nothing
  else, matching its own doc and `tests/node_id.rs`; `ReserveNumaError` and `NodeResolution` are
  `#[non_exhaustive]`; the two `#[doc(hidden)]` modules carry their semver-exemption notices.
- MSRV: everything used (`usize::is_multiple_of` 1.87, `ptr::addr`/`with_addr` 1.84,
  `offset_of!` 1.77, `io::Error::other` 1.74, inline `const {}` blocks 1.79) is within the
  declared `rust-version = "1.88"`.

## 3. Recommended order

1. **P2-1** — make the `current_node() == None` skip fatal on the one CI row that runs the real
   Linux backend (env-flag pattern, mirroring `SEFER_NUMA_TEST=1`). This is the finding with the
   most leverage: it is what keeps the other three positive tests honest too.
2. **P2-2** — make `tests/smoke.rs`'s three platform branches backend-aware, then add the
   macOS mock + `vmem-integration` CI row that proves it.
3. **P2-3** — de-link or feature-gate the three doc sites, and add the default-feature
   `RUSTDOCFLAGS="-D warnings" cargo doc` row.
4. **P3-1** — switch `should_retry_eintr` to `ErrorKind::Interrupted` and move it somewhere a
   test can reach it; add the three-assertion unit test.
5. **P3-2** — add `O_CLOEXEC` to the sysfs `open`.
6. Then the owner's F2 release-metadata decision, the final pre-tag Phase-1 gate re-run on the
   frozen SHA, `cargo publish --dry-run`, and a confirmed-green CI on that exact SHA read from
   `origin/main`, before the tag.
7. The two P3 perf candidates stay untouched until measured (items 59/60).

## 4. Scope and limits of this audit

Single agent, static, read-only, by the brief's explicit instruction. Fully read at this
revision: `Cargo.toml`, `README.md`, `CHANGELOG.md` (head, through the `### Fixed` section),
`src/lib.rs` in its entirety (2213 lines), all eight test files, `benches/numa_bench.rs`, the
five `numa-shim-*` CI jobs plus the MSRV rows, the relevant `aligned-vmem` accessors and Unix
reservation strategy, and item 108's card in
`docs/correctness-open-items/TRACKED_publish_readiness.md`. Not exercised: any runtime path —
so every statement about real kernel behavior (mbind acceptance, `get_mempolicy` readback shape,
`VirtualQuery` states, EINTR delivery) is a reading of code and kernel/Win32 contracts, not an
observation. The two P2 findings that predict a red test (P2-2) or a rustdoc warning (P2-3) are
static predictions; both are cheap to confirm by running the one command each names, and I
recommend doing so before acting on them rather than taking this report's word for it.

## 5. Final assessment

The crate's shipped code is in the best state this campaign has seen. Sixteen reviews of
pressure have removed a broken `bind_range`, a lying `Some(0)` fallback, an unchecked `mbind`
result, an errno-clobbering cleanup order, an unenforced `NodeId` invariant, an O(nodes × bytes)
hot lookup justified by wrong arithmetic, an integer pointer round-trip on the Windows
production path, a permanently-detection-killing `EINTR`, and a documentation-only platform
policy — and this revision adds no new defect in exchange. I could not find a memory-safety,
provenance, ownership or FFI-contract problem in it.

What is left is the *evidence* layer, and it is left in a specific and recognisable way: three
of the four positive Linux tests and the flagship policy oracle can all go silent together,
triggered by a regression in the crate's own detection code, with CI green — which is the same
sentence the sixteenth review's P1 wrote about a different arm of the same test one wave ago.
Closing that (P2-1) is what turns "we believe the flagship API works" into "CI would tell us if
it stopped." That, plus the two mechanical gate gaps, is the whole distance to GO.
