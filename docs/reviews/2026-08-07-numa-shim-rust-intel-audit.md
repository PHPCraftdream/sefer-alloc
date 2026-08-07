# rust-cc-audit report — `numa-shim`

**Date:** 2026-08-07
**Produced by:** the `/rust-intel` skill's fan-out audit workflow (`audit-project.workflow.js`,
run `wf_42d49739-b45`) — 14 agents total: 2 prepare (trigger-table slicer + crate scoper),
10 per-module auditors, 1 synthesis. ~1.02M subagent tokens, 83 tool calls.
**Audited tree:** `main` @ current HEAD, `crates/numa`.

---

**Scope:** D:\dev\rust\sefer-alloc\crates\numa
**Pinned versions:** numa-shim v0.1.0; sole dependency aligned-vmem 0.2 (in-repo path crate, version-consistent with crates/vmem); MSRV 1.88; resolver v2; no build.rs, no [patch] tables, no git sources
**Found:** 0 critical, 0 high, 12 medium, 10 info

---

## CRITICAL

none

## HIGH

none

## MEDIUM

### [§A3] crates/numa/src/lib.rs:96-101 — mock thread-locals are unrestricted `pub`, committing RefCell internals to the semver surface

`std::thread_local! { pub static CALLS: RefCell<Vec<MockCall>> ... pub static CURRENT_NODE_SLOT: RefCell<u32> ... }` — both statics are `pub` inside the published `mock` module, committing the internal representation to the semver surface, while the intended API is already the encapsulating pair `drain()`/`set_current_node()`; no external code (tests/mock_dispatch.rs included) touches them directly, and the sibling helpers `record()`/`current_node_slot()` are correctly `pub(crate)`.

**Fix:** change both thread-local statics to `pub(crate)`; external consumers keep `drain()`/`set_current_node()` as the only surface. The `mock` feature is published on crates.io even though docs.rs hides it, so this IS external surface.

### [§B5] crates/numa/src/lib.rs:188 — bind_range's `# Safety` contract is stated unconditionally, making five green test call sites UB-by-contract

"`[base, base + len)` must be a valid OS reservation owned exclusively by the caller for the duration of the call." — stated unconditionally, yet tests/mock_dispatch.rs:45,56,67 pass a dummy `0x1000 as *mut u8` and tests/smoke.rs:59,68 pass stack-array pointers, relying on the NO_NODE/len==0 short-circuit (or mock intercept) rather than satisfying the contract. Per §B5 the contract IS the only guard: a future edit that reorders the short-circuit after a platform call silently turns five green tests into real UB.

**Fix:** scope the `# Safety` doc to the actual precondition: "When `node != NO_NODE` and `len != 0`, `[base, base+len)` must be a valid mapped range owned by the caller; otherwise the call returns without touching the pointer and any address value is permitted." This makes all five test call sites contract-compliant.

### [§B14] crates/numa/src/lib.rs:98 — CALLS thread-local Vec is an insert-only log with no cap in the documented global-allocator scenario

`pub static CALLS: RefCell<Vec<MockCall>> = const { RefCell::new(Vec::new()) };` — under the documented sefer-alloc-as-global `numa-aware-mock` scenario (lib.rs:121-129) every allocation calls `current_node()` → `record()` → `Vec::push`, and by that comment's own admission nothing ever `drain()`s in that scenario — the Vec grows linearly with allocation count per thread (the §B14 "Vec pushed in a hot loop with no consumer" shape; the consumer exists only for direct mock tests).

**Fix:** bound the log — skip the push when `CALLS.len()` exceeds a stated cap (e.g. 4096) or use a fixed-capacity ring; alternatively gate recording off when consumed as the global allocator's backend, and state the bound in the module doc.

### [§B25] crates/numa/src/lib.rs:724 — `#[repr(C)] ProcessorNumber` mirror of PROCESSOR_NUMBER has no layout assertions

`#[repr(C)] struct ProcessorNumber { group: u16, number: u8, reserved: u8 }` is passed as `*mut`/`*const` to GetCurrentProcessorNumberEx/GetNumaProcessorNodeEx with no layout assertion anywhere in the crate. §B25 REQUIRED: every `#[repr(C)]` struct crossing the boundary gets size/align/offset assertions; the hand-written mirror happens to match (size 4, align 2, offsets 0/2/3) but nothing pins it, so a future field edit silently corrupts the out-parameter write.

**Fix:** add compile-time const assertions next to the struct (not tests, so no CLAUDE.md conflict): `const _: () = { assert!(size_of::<ProcessorNumber>() == 4); assert!(align_of::<ProcessorNumber>() == 2); assert!(mem::offset_of!(ProcessorNumber, number) == 2); assert!(mem::offset_of!(ProcessorNumber, reserved) == 3); };`

### [§C4] crates/numa/src/lib.rs:383 — single 256-byte cpumap read treated as complete; silent truncation and wrong-node answers on ~900+-CPU hosts *(also flagged by §B5 unsafe-and-ffi, §D3 testing, §F1 semantics at :380)*

`let n = unsafe { libc_read(fd, buf.as_mut_ptr() as *mut core::ffi::c_void, 256) };` — a single read into a fixed 256-byte buffer is treated as the complete cpumap. On hosts with more than ~28 mask words (~896-900+ CPUs — exactly the large-NUMA machines this crate targets) the file exceeds 256 bytes; the truncated tail holds the LOW-index CPU words (leftmost word is most-significant), so `word_count` and the `left_index = word_count - 1 - target_word` arithmetic in parse_cpumap_contains_cpu (lib.rs:397-416) misalign, silently returning a wrong node or a false miss cascading into cpu_to_numa_node's node-0 fallback. Same class for any short read — the code never distinguishes "got everything" from "got a prefix", and no test exists at the documented 64-node scale boundary.

**Fix:** loop the read until EOF (n == 0) accumulating into a larger buffer (e.g. 4 KiB covers ~14,500 CPUs, still heap-free); treat `n == buf.len()` with no EOF observed as "possibly truncated" and return false/fallback rather than parsing a prefix; add a boundary-scale parser test for CPU 0 and the highest CPU.

### [§C10] crates/numa/Cargo.toml:34 — `mock` is a non-additive feature: unification silently swaps every consumer's real NUMA syscalls for the recording stub

`mock = []` REPLACES the real platform backend (lib.rs:152/196/232 dispatch away from `platform::*` entirely), so Cargo feature unification means ANY crate in the graph enabling `mock` mocks out real NUMA for all consumers — the §C10 "workspace-internal features are not private" trap. Already bitten once in-repo: lib.rs:120-129 (R11-5) documents workspace `--all-features` enabling it through `numa-aware-mock`, producing a real reentrancy-deadlock hazard in `record()`. Once published, an external consumer whose test profile unifies `mock` on gets tests that "pass" against a mock they never asked for.

**Fix:** (a) add `#[cfg(all(feature = "mock", not(any(test, debug_assertions))))] compile_error!` or at minimum a documented loud marker so a release build with `mock` unified on cannot ship silently; or follow §C10's REQUIRED move — a `cfg` flag (`cargo:rustc-cfg=numa_mock`) which does not unify; (b) state in the crate-level rustdoc (not only Cargo.toml comments) that `mock` is behavior-replacing.

### [§D1a] crates/numa/src/lib.rs:531 — the mbind path (the crate's key selling point) has no behavioral oracle anywhere

`libc_mbind(base as *mut core::ffi::c_void, len as u64, MPOL_PREFERRED, ...)` — "Errors are silently discarded"; the return value is never checked by any test. Mutating SYS_MBIND (lib.rs:560, 237→999) or scrambling argument marshalling leaves every suite green: smoke.rs asserts only no-panic, mock_dispatch.rs asserts only that a MockCall::BindRange record was emitted (a declaration, §D1a shape 4), the mock arm of reserve_on_node (lib.rs:234-246) reimplements the chain-to-bind logic in parallel with the real Linux impl, and even the weekly numa-real-kernel CI job's tests are consistency/no-SEGV shaped.

**Fix:** add a behavioral oracle viable on a single-node kernel — an env-guarded Linux test asserting the syscall return is 0 for a valid single-node bind (a wrong syscall number yields -1/ENOSYS and goes red), and/or a `get_mempolicy(2)` readback asserting MPOL_PREFERRED with the expected nodemask. Multi-node QEMU (documented Phase 2.1) remains the full-fidelity follow-up.

### [§D1a] crates/numa/src/lib.rs:397 — the sysfs cpumap parser has zero direct tests on any target; spec vectors live only in comments *(also flagged by §F1 semantics)*

`fn parse_cpumap_contains_cpu(data: &[u8], cpu_idx: u32) -> bool` — private; the doc example `"00000000,00000003\n"` appears only in a comment, never as a test vector. The tricky most-significant-word-first ordering and hand-rolled hex/decimal parsing (parse_cpumap_contains_cpu / format_sysfs_path / nth_token / parse_hex_u32) are never exercised: the mock feature bypasses the platform module entirely, and the only real-Linux exercise is smoke.rs:11's `node < 64 || None` oracle, which passes even if word order is inverted. No negative controls (empty token, invalid hex, partial word) either — §F1 REQUIRED external-oracle test where one trivially exists.

**Fix:** expose the parser via the project's established `#[doc(hidden)]` test-only-forwarder pattern and add spec-derived vector tests in crates/numa/tests/: multi-word masks asserting MSW-first ordering (e.g. `"00000001,00000000"` contains CPU 32, not CPU 0), partial-width first words (`"ff\n"`), plus false cases for invalid hex, empty tokens, out-of-range target_word.

### [§E5] crates/numa/src/lib.rs:309 — boot-static cpu→node topology re-derived by up to 64 open/read/close triples per `current_node()` call *(also flagged by §E3 data-and-types at :311)*

`fn cpu_to_numa_node(cpu_idx: u32) -> u32 { for node in 0u32..64 { if node_contains_cpu(node, cpu_idx) ... }` — each call re-derives the mapping via sysfs though the node→cpumask topology is static for the life of the boot (only the CPU index changes on migration). Load-bearing, not cosmetic: lib.rs:122-124's own R11-5 comment establishes that `current_node()` is re-entered from an ALLOCATION path when sefer-alloc consumes this crate (`numa-aware`), so a multi-node Linux host pays a syscall burst (worst case ~3×64 syscalls on a miss-heavy scan) per lookup.

**Fix:** hoist the parsed topology into a `std::sync::OnceLock` of per-node CPU masks with a non-panicking initializer (fall back to the existing node-0 path on read failure); `cpu_to_numa_node` becomes a pure bit-test with `sched_getcpu()` the only per-call syscall. CPU-hotplug staleness is acceptable for a MPOL_PREFERRED hint; note it in the doc comment.

### [§F1] crates/numa/src/lib.rs:527 — mbind maxnode off-by-one: node 63's bit is silently dropped

`let nodemask: u64 = 1u64 << node; let maxnode: u64 = 64;` — kernel `get_nodes()` decrements maxnode before building the end-mask (the documented mbind(2) ABI quirk libnuma compensates for by passing size+1), so maxnode=64 covers only bits 0..62 — `bind_range(node=63)` passes an effectively empty nodemask and MPOL_PREFERRED silently degrades to local allocation; the guard at :522 allows node 63 through, and errors are discarded by design, so nothing surfaces it.

**Fix:** pass maxnode = 65 (libnuma's bitmask-size+1 convention) or use a `[u64; 2]` nodemask with maxnode = 128; add a `// DEVIATION`-style comment citing mm/mempolicy.c get_nodes()'s `--maxnode`.

### [§F2] crates/numa/src/lib.rs:146 — documented None-on-unmappable-CPU case is unreachable on Linux; all mapping failures collapse to Some(0)

Doc: "Returns None when: … The CPU index cannot be mapped to a NUMA node via sysfs." — but cpu_to_numa_node (lib.rs:309-320) returns 0 whenever the CPU is found in no node's cpumap, which also covers read/permission failures, truncated reads, and CPUs on nodes ≥ 64 — so a thread genuinely on node 2 whose cpumap read fails reports Some(0), a silently wrong node the parent allocator's node_id path consumes. Docs and code disagree (§F2 REQUIRED: report, don't pick a side).

**Fix:** distinguish "sysfs topology absent" (node0 dir missing → Some(0), as documented) from "topology present but CPU unmapped" (→ None), or amend the doc to state that all Linux mapping failures collapse to Some(0).

### [§F2] crates/numa/src/lib.rs:694 — Windows path commits the entire size+align over-reservation, contradicting its own "mirrors aligned-vmem" claim

`MEM_RESERVE | MEM_COMMIT, … over  // over = size + align` — the doc comment claims the strategy "mirrors aligned-vmem's own Windows reservation", but the mirror (crates/vmem/src/lib.rs:916-956, win_reserve_commit) reserves size+align with MEM_RESERVE only and commits exactly `size` at the aligned base — numa-shim instead commits the never-usable alignment slack too, doubling commit charge for the align==size case its own test exercises (4 MiB span → 8 MiB committed), on the one platform the release gate admits CI never executes.

**Fix:** two-call mirror — VirtualAllocExNuma(MEM_RESERVE, over) then VirtualAllocExNuma(MEM_COMMIT, size) on the aligned sub-range, keeping the node preference at commit time; or correct the doc comment to state the deviation and its commit-charge cost.

## INFO

### [§A2] crates/numa/src/lib.rs:100 — RefCell<u32> where Cell<u32> would do

`pub static CURRENT_NODE_SLOT: RefCell<u32> = const { RefCell::new(0) };` — §A2 BANNED row verbatim: the slot is a Copy u32 read whole (:115) and replaced whole (:110); `Cell<u32>` has no runtime borrow flag, so it structurally cannot participate in the §B17 reentrant-borrow hazard this very module documents and defends against for its sibling CALLS cell.

**Fix:** change to `core::cell::Cell<u32>`; `set_current_node` becomes `c.set(node)`, `current_node_slot` becomes `c.get()` — no borrow guards at all.

### [§A3] crates/numa/src/lib.rs:231 — aligned_vmem::Reservation in a public signature couples numa-shim's semver to aligned-vmem 0.2

`pub fn reserve_on_node(size: usize, align: usize, node: u32) -> Option<aligned_vmem::Reservation>` — a future aligned-vmem 0.2→0.3 bump forces a breaking release of numa-shim (feature-gated, but `vmem-integration` is a published feature).

**Fix:** acknowledge deliberately — `pub use aligned_vmem;` (or re-export Reservation) so consumers name the coupled version through numa-shim, and/or document in the crate doc that `vmem-integration` pins the public aligned-vmem major version. Documentation/re-export decision only — no version bump.

### [§B5] crates/numa/tests/smoke.rs:46 — bind_range on a Vec heap buffer does not satisfy the letter of the "valid OS reservation" contract

`let mut buf: Vec<u8> = vec![0u8; page]; ... unsafe { bind_range(base, len, node) };` — a Vec buffer is not an OS reservation: it lives inside the global heap's mapping whose pages are shared with other heap objects; `mbind(MPOL_PREFERRED)` on those pages is harmless kernel metadata but the call site violates the stated contract (same root cause as the §B5 medium finding at lib.rs:188).

**Fix:** fold into the contract rewording — require "a valid mapped range" (which a heap buffer satisfies), and note that mbind policy applies at page granularity so surrounding same-page data may be affected.

### [§B7] crates/numa/src/lib.rs:344 — format_sysfs_path's [u8;4] digit buffer panics for node ≥ 10000; latent, unreachable under the current 0..64 caller bound *(also flagged by §B26 data-and-types at :351)*

`let mut tmp = [0u8; 4]; ... while n > 0 { tmp[digits] = b'0' + (n % 10) as u8; ... }` with doc "(up to 3 digits for node < 1000)" — the helper accepts an arbitrary `u32`; for node ≥ 10000 the 5th digit indexes `tmp[4]` out of bounds, a safe-code panic. Currently unreachable (the only caller iterates 0u32..64, lib.rs:311), and the doc comment (< 1000) disagrees with the array size (< 10000).

**Fix:** size tmp as `[u8; 10]` (all u32) so the helper matches its signature, or add `debug_assert!(node < 64)` matching the caller's invariant, and align the doc comment.

### [§B26] crates/numa/src/lib.rs:461 — parse_hex_u32 silently wraps instead of rejecting tokens longer than 8 hex digits *(also flagged by §F4 semantics)*

`val = val.wrapping_shl(4) | digit as u32;` — the shift silently drops the most-significant nibbles of any hex token longer than 8 digits, returning a wrong value where every other malformed input in this parser returns None — inconsistent error handling with no `// wrapping intentional` comment per the module's REQUIRED rule, and a decoder with no invalid-input negative control (§F4). Input is trusted root-owned sysfs and cpumap words are fixed 8 chars, so no live impact.

**Fix:** add `if s.len() > 8 { return None; }` at the top (oversized tokens fail like every other parse error) and cover it in the fixture-test corpus; or annotate the shl with a `// truncation intentional: <reason>` comment.

### [§C1a] crates/numa/src/lib.rs:77-93 — enum-level #[non_exhaustive] does not cover MockCall's struct-like variants' field lists

`#[non_exhaustive] ... pub enum MockCall { ... BindRange { base: usize, len: usize, node: u32 }, ReserveOnNode { ... } }` — the attribute only reserves the right to add VARIANTS; the variants' field lists remain exhaustive, so adding a field later (plausible for a call-recording enum) is still a semver-major break for downstream matches naming all fields.

**Fix:** add `#[non_exhaustive]` to the BindRange and ReserveOnNode variants themselves if field growth is plausible (downstream then matches with `..`, which tests/mock_dispatch.rs:97 already does for BindRange); decide at first publication — v0.1.0 is the moment.

### [§D1] crates/numa/tests/smoke.rs:60 — two no-op smoke tests assert no postcondition

`// If we reach here without panic the test passes.` — bind_range_no_node_is_noop (smoke.rs:55) and bind_range_zero_len_is_noop (smoke.rs:65) are the exact §D1 BANNED `do_thing(); /* no assert */` shape: on non-mock builds a no-op has no observable, so they structurally cannot fail for the property their names claim. Mitigated: the real short-circuit postcondition IS properly asserted in mock_dispatch.rs:41-61 via `drain().is_empty()`.

**Fix:** either delete the two vacuous smoke variants (the mock suite carries the property) or keep them with an explicit comment that they exist only as cross-platform doesn't-crash-the-syscall probes — do not count them as coverage of the short-circuit behavior.

### [§F1] crates/numa/src/lib.rs:522 — nodes ≥ 64 silently skipped with no doc mention

`if node == NO_NODE || node >= 64 { return; }` — mbind(2) supports nodemasks up to MAX_NUMNODES (commonly 1024); the u64-nodemask cap silently no-ops binding for nodes ≥ 64 with no mention in bind_range's docs or the README — an undocumented restriction of the named interface (§F1 BANNED: silent restriction without a DEVIATION comment).

**Fix:** add one doc line to bind_range and README: "nodes ≥ 64 are silently skipped (single-u64 nodemask)", with a `// DEVIATION` comment at the guard.

### [§F1] crates/numa/src/lib.rs:631 — GetNumaProcessorNodeEx's documented MAXUSHORT "processor does not exist" sentinel unhandled; adjacent SAFETY comment misdescribes the API

`let ok = unsafe { GetNumaProcessorNodeEx(&proc_num, &mut node) };` — the documented outcome sets NodeNumber = MAXUSHORT while still indicating success; the code would return Some(65535), violating the smoke-test's own node < 64 expectation. Low likelihood (proc_num comes from GetCurrentProcessorNumberEx), but the sentinel is unhandled and the SAFETY comment says "returns 0 on single-node or error" — it returns BOOL FALSE on error; node 0 with TRUE is the single-node success case.

**Fix:** treat `node == u16::MAX` as NO_NODE after a successful call, and correct the comment.

### [§F2] crates/numa/src/lib.rs:154 — mock arm can return Some(NO_NODE), and the None path is unscriptable

`let n = mock::current_node_slot(); … Some(n)` — under feature `mock`, current_node() unconditionally wraps the scripted slot in Some — scripting `set_current_node(u32::MAX)` yields Some(NO_NODE), violating the documented "returns Option, never the sentinel" guarantee, and every consumer's None branch is unreachable under the mock that exists precisely so "every CI run can assert the wrapping logic".

**Fix:** in the mock arm, map NO_NODE in the slot to None (mirroring the real dispatch at lib.rs:160-166), making the None path scriptable.

---

## Post-flight summary

Aggregated 🔴 inventory across all agents (deduplicated; data-and-types, drop-and-raii, and testing declare no 🔴 items and reported empty inventories):

**§B21 tokio::spawn with dropped JoinHandle** — none. Zero occurrences (flagged N/A by both async agents): no tokio, no spawn, no JoinHandle anywhere in the crate.

**§B22 impl Drop doing async work** — none. Zero `impl Drop` blocks in the crate; Drop mentions at src/lib.rs:666,714 and tests/smoke.rs:100-155 are doc/comment references to aligned_vmem::Reservation's synchronous RAII Drop in the sibling vmem crate.

**§B15b Pin::new_unchecked** — none. Zero `Pin` occurrences of any kind.

**§B13 Relaxed-publish data race** — none. Zero atomics anywhere in the crate (grep for Atomic/Ordering: no hits); no publish pattern possible.

**§B14 unbounded_channel / unbounded FuturesUnordered** — none. Zero channels and zero FuturesUnordered/JoinSet (no async runtime; only dep is aligned-vmem); nearest relative is the CALLS Vec, reported as a 🟡 §B14 medium finding above.

**§B12 any cryptographic operation** — none. Zero crypto operations, primitives, RNGs, keys, TLS/JWT, or crypto deps (src + both test files audited in full).

**§B24 `==` on secret material** — none. All `==`/`!=` sites (lib.rs:161,193,237,293,399,420,431,522,651 + test asserts) compare public sentinels (NO_NODE), lengths, or parser delimiter bytes; no secret material exists in the crate.

**§B5 unsafe fns / unsafe blocks** — 27 occurrences, all carrying `// SAFETY:` comments; the one `pub unsafe fn` (bind_range) has a `# Safety` doc:
- src/lib.rs:192 (bind_range) — justified; # Safety doc present at :186-191, though contract scope is imprecise (medium finding above)
- src/lib.rs:271 (sched_getcpu) — justified; SAFETY at :269
- src/lib.rs:282 (bind_range_impl_linux call) — justified; SAFETY at :279
- src/lib.rs:299 (bind_range_impl_linux from reserve_on_node) — justified; SAFETY at :296, fresh live Reservation ownership
- src/lib.rs:376 (libc_open) — justified; SAFETY at :374, nul-terminated path
- src/lib.rs:383 (libc_read) — justified for safety; SAFETY at :381; truncation correctness gap is the §C4 medium finding (not UB)
- src/lib.rs:385 (libc_close) — justified; SAFETY at :384, fd closed exactly once
- src/lib.rs:480,484,491,499 (private unsafe fn wrappers) — justified; each carries its own caller-contract SAFETY line; private, so no # Safety doc owed
- src/lib.rs:521 (bind_range_impl_linux, x86_64/aarch64) — justified; SAFETY at :529-530; node<64 guard precedes shift and syscall
- src/lib.rs:548 (other-arch no-op) — justified; empty body
- src/lib.rs:583 (libc_mbind) — justified; SAFETY at :591-593 names syscall number, live mapping, valid nodemask
- src/lib.rs:625 (GetCurrentProcessorNumberEx) — justified; SAFETY at :623
- src/lib.rs:631 (GetNumaProcessorNodeEx) — justified; SAFETY at :628; ok==0 remapped to NO_NODE
- src/lib.rs:689 (VirtualAllocExNuma) — justified; SAFETY at :685-688; NULL checked as OOM
- tests/mock_dispatch.rs:45,56,67 — violated-by-contract-letter: SAFETY comments present but dummy `0x1000` pointers breach the unconditional # Safety contract; safe today only via short-circuit/mock intercept (medium finding above)
- tests/smoke.rs:46 — justified-with-caveat: heap buffer is a valid mapped range but not "an OS reservation" as the contract demands (info finding above)
- tests/smoke.rs:59,68 — violated-by-contract-letter: rely on short-circuit (medium finding above)
- tests/smoke.rs:95-98 — justified; SAFETY at :93-94; r owns the span
- tests/smoke.rs:142-147 — justified; SAFETY at :141; one byte per page within r's owned span

**§B5 transmute / mem::uninitialized / mem::zeroed** — none (grep-verified, whole crate).

**§B18 manual unsafe impl Send/Sync** — none (grep-verified, whole crate).

**§B18a raw-pointer wrapper variance/PhantomData** — none. No hand-written type holds a raw-pointer/NonNull field (ProcessorNumber is POD; Reservation lives in aligned-vmem, out of scope).

**§B25 extern import blocks** — 4 occurrences, all justified:
- src/lib.rs:468-477 (extern "C": sched_getcpu/open/read/close) — imports only, per-site SAFETY, miri-isolated via cfg(not(miri)) per §B25 REQUIRED
- src/lib.rs:573-575 (extern "C" variadic syscall) — matches glibc/musl syscall(2); gated to x86_64/aarch64 with pinned SYS_MBIND numbers (:560,:565)
- src/lib.rs:717-719 (ownership-adopting aligned_vmem::Reservation::from_raw_parts) — justified; exemplary 7-point SAFETY block at :705-716
- src/lib.rs:731-735 and :743-752 (extern "system" Windows APIs) — imports only; ProcessorNumber layout unpinned by assertion (§B25 medium finding above)

**§B25a FFI calls across threads without cited thread-safety level** — all sites justified: every wrapped call is a reentrant per-thread kernel syscall/Win32 API with no library-global mutable state; errno/last_os_error is never read, so the delayed-errno ban cannot trigger.

**§C1 blanket impl in public API of a published library** — none. Zero trait impls of any kind in the crate (only `#[derive]` on MockCall at lib.rs:72).

**§A1 unverified dependency / unpinned [patch]/git source / network in build.rs** — none/justified: sole dependency `aligned-vmem = { version = "0.2", path = "../vmem", optional = true }` is the in-repo sibling (name verified against path target, no slopsquat surface); no `[patch]` tables or `git =` sources anywhere in the workspace; no build.rs exists.

**§F1/§F2 divergence affecting wire format, security guarantee, or persisted data** — none rise to 🔴 across all three boundary surfaces: mbind(2) surface (src/lib.rs:521-539, 583-603 — syscall numbers 237/235, MPOL_PREFERRED=1, argument order verified correct; the maxnode off-by-one is a real §F1 divergence but affects only a memory-placement hint, reported 🟡 medium); sysfs cpumap surface (src/lib.rs:373-464 — hex-word/comma/MSW-first interpretation matches the kernel format; truncation and never-None divergences are hint-level, reported 🟡); Win32 surface (src/lib.rs:674-759 — struct layout, constants, signatures verified correct; MEM_COMMIT-of-slack divergence is commit-charge behavior, reported 🟡).

**§F3 leaked/unclosed boundary resource / untrusted-peer read without timeout** — none: sysfs fd (src/lib.rs:376-385) closed unconditionally on every path; Windows raw NUMA reservation (src/lib.rs:689-720) NULL-checked then immediately adopted into RAII with no fallible gap; no sockets/streams/peers exist — the only external input is kernel-provided sysfs text read once into a bounded stack buffer.
