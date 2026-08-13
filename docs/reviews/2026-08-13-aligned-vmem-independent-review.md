# `aligned-vmem` independent pre-release review

Date: 2026-08-13  
Base: `a1d75ec` (`main`)  
Crate: `crates/vmem` / `aligned-vmem` 0.2.0  
Reviewer mode: read-only source audit; this report is the only repository artifact created.

## Executive verdict

**Not ready to publish unchanged.** I found one HIGH and two MEDIUM defects. The highest-risk issue reopens a pinned-HugeTLB mapping leak that task #714 and several later reviews considered closed: the implementation validates against a hard-coded 2 MiB huge-page size even though its `MAP_HUGETLB` call requests Linux's configurable default huge-page size. I also found that `Reservation::from_raw_parts` still accepts two classes of `reservation_len` values forbidden by its own safety contract, and independently reject the prior conclusion that the `mmap` `off_t: i64` declaration is sound on the crate's broad `cfg(unix)` surface.

I found no evidence-backed shipping-path performance optimization. The remaining exact-`mmap` miss trade-off is already measured and is not an unconditional win.

## Method and verification

- Read `CLAUDE.md`, `crates/vmem/README.md`, the crate manifest, all four source files, all seven integration-test files, the benchmark and example.
- Read both open-item indexes first. Items 1/2 are process records, item 6 is the established Windows crash-not-refault incident, and aligned-vmem items 41-51 were treated as hypotheses/status records rather than rediscovered findings.
- Skimmed the full aligned-vmem review-file inventory, then read the prior documents needed to verify or falsify the conclusions cited below.
- Audited every explicit `unsafe` block and each implicit unsafe operation inside `unsafe fn`; checked raw-pointer provenance, FFI ABI shapes, immediate errno/GetLastError capture, release/error paths, manual `Send`, and concurrency claims.
- Checked test assertions counterfactually: whether the test would fail if its named operation were removed or its guarded bug restored.
- Primary references used for external conformance:
  - Linux [`mmap(2)` / HugeTLB rules](https://man7.org/linux/man-pages/man2/munmap.2.html): a zero `MAP_HUGE_*` size field selects the default huge-page size; `mmap` rounds length to the underlying huge-page size; `munmap` requires both address and length to be multiples of it.
  - Linux kernel [HugeTLB documentation](https://www.kernel.org/doc/html/latest/admin-guide/mm/hugetlbpage.html): the default is architecture/configuration dependent and selectable with `default_hugepagesz`; a too-small/non-huge-aligned `munmap` length fails.
  - glibc [`_FILE_OFFSET_BITS`](https://sourceware.org/glibc/manual/latest/html_node/Feature-Test-Macros.html) and [`mmap`](https://sourceware.org/glibc/manual/latest/html_node/Memory_002dmapped-I_002fO.html) documentation: `mmap` takes `off_t`; without a 64-bit interface `off_t` defaults to 32 bits on traditional 32-bit architectures including i686/ARM.

Commands run on `rustc 1.97.0 (2d8144b78 2026-07-07)`, host `x86_64-pc-windows-msvc`:

- `cargo test -p aligned-vmem --no-fail-fast` — pass (25 integration tests; doctests 0).
- `cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" --no-fail-fast` — pass (42 integration tests; doctests 0).
- `cargo test -p aligned-vmem --all-features --no-fail-fast` — pass (47 integration tests; doctests 0).
- `cargo clippy -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" --all-targets -- -D warnings` — pass.
- `cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings` — pass.
- `cargo bench -p aligned-vmem --no-run` — pass.
- Linux cross-target `cargo check` and `cargo clippy --all-targets -D warnings` with the named feature set — pass.
- `RUSTFLAGS="--cfg miri" cargo check -p aligned-vmem --all-features` — pass (compile-only, not interpreter execution).
- `RUSTFLAGS="-W unsafe_op_in_unsafe_fn"` confirms four current-host and four Linux implicit unsafe operations, consistent with tracked item 49; no new unsoundness follows from those warnings.

The green matrix does not contradict the findings: none has a currently executing negative or platform-configuration oracle.

## Findings

### HIGH — H1: a non-2-MiB Linux default HugeTLB size can leak the entire pinned huge-page mapping

**Where:** `crates/vmem/src/lib.rs:1988-2002`, `:2104-2113`, `:2157-2166`, `:2330-2342`, `:2454-2457`, `:2482-2495`.

`LINUX_HUGE_PAGE_SIZE` is fixed at 2 MiB and `unix_reserve` rejects only values that are not multiples of that constant. But `libc_mmap` adds plain `MAP_HUGETLB` and no `MAP_HUGE_2MB` (or other size encoding). Linux therefore uses the system's **default** HugeTLB size, which is configurable and architecture-dependent, not universally 2 MiB.

Concrete failure sequence:

1. Boot/configure x86_64 Linux with `default_hugepagesz=1G` and at least one free default HugeTLB page.
2. Call `reserve_aligned_huge(2 * MiB, 2 * MiB)`.
3. The 2 MiB precheck passes. The exact `MAP_HUGETLB` `mmap` may succeed; Linux rounds the requested 2 MiB length to the underlying 1 GiB huge-page size and returns a suitably aligned mapping.
4. `try_reserve_aligned_exact` records `reservation_len = size`, i.e. 2 MiB, rather than the actual rounded 1 GiB mapping.
5. `Reservation::drop` calls `munmap(ptr, 2 MiB)`. HugeTLB `munmap` requires the length to be a multiple of the underlying 1 GiB page, so it returns `EINVAL`; `libc_munmap` discards the return.
6. The whole 1 GiB VA mapping and pinned huge page remain until process exit. Repetition exhausts the HugeTLB pool (and consumes VA).

The over-reserve path has the same bookkeeping premise: it records `over`, not the kernel-rounded HugeTLB length.

**Prior-review disagreement:** this falsifies the durable premise in `docs/reviews/2026-08-07-aligned-vmem-rust-intel-audit.md` (the original task-#714 leak fix) and the acceptance in `docs/reviews/2026-08-09-aligned-vmem-round-closing-review.md:97-103`. Later reviews repeatedly re-derived only the 2-MiB case; for example `docs/reviews/2026-08-12-aligned-vmem-round4-closing-review.md:344-347` varies requested alignment but never the configured default huge-page size. The code itself at `lib.rs:2330-2340` asserts that plain `MAP_HUGETLB` always requests 2 MiB on the supported mainstream configurations; Linux's interface does not provide that guarantee.

**Suggested direction:** either encode `MAP_HUGE_2MB` so the public 2 MiB contract matches the actual request, or discover/use the actual default `Hugepagesize` for validation and release bookkeeping. Add an oracle on a host configured with a non-2-MiB default; ordinary CI with no successful HugeTLB mapping cannot distinguish this bug from the fallback path.

### MEDIUM — M1: `from_raw_parts` accepts forbidden `reservation_len` values and can leak or deallocate with the wrong layout

**Where:** `crates/vmem/src/lib.rs:693-712` (safety contract), `:724-761` (checks), `:773-785` (Drop), `:2163-2166` (Unix release), `:2573-2578` (miri release); missing oracle at `crates/vmem/tests/smoke.rs:654-710`.

The safety contract requires `reservation_len` to be a non-zero multiple of `PAGE`. The constructor claims to validate the caller-supplied pair immediately, but its assertion only asks whether `Layout::from_size_align(reservation_len, align)` succeeds. Both `Layout::from_size_align(0, PAGE)` and `Layout::from_size_align(PAGE / 2, PAGE)` are valid layouts, so both documented violations are accepted.

Concrete failure sequence:

1. Obtain a real `PAGE`-byte reservation, extract its pointer, and unsafely adopt the same live allocation with `Reservation::from_raw_parts(raw, PAGE, raw, 0, PAGE)` (or a non-page-multiple length such as `PAGE / 2`).
2. Contrary to the constructor's immediate-validation claim, construction returns normally.
3. On Unix, Drop calls `munmap(raw, 0)` (or an incomplete rounded range). The zero case returns `EINVAL`, which is discarded, leaking the live mapping.
4. Under miri, Drop constructs the accepted wrong layout and calls `std::alloc::dealloc` with a layout different from the one used by `alloc`; that violates the allocator API contract and is undefined behavior.

This is caller misuse of an `unsafe fn`, so the existence of UB does not by itself make the API unsound. The defect is that the constructor and its comments promise to convert this exact documented-contract misuse into an immediate, attributable failure and do not do so. The test suite covers non-power-of-two alignment and `usize::MAX` overflow only; neither test fails if the missing nonzero/PAGE-multiple predicates remain absent.

**Prior-review disagreement:** `docs/reviews/2026-08-09-aligned-vmem-round-closing-review.md:F7` correctly observed that `reservation_len` was only half-guarded and offered two alternatives: `Layout::from_size_align(...).is_ok()`, or explicit `reservation_len != 0 && reservation_len.is_multiple_of(PAGE) && ...`. The subsequent implementation chose the first and its current comments (`lib.rs:744-753`) claim that this covers “both halves of the documented contract.” That conclusion is false: it covers layout overflow, but not the separately documented nonzero/page-multiple invariants. F7 was only partially closed.

**Suggested direction:** add the two explicit predicates to the construction-time assertion and add negative tests for zero and a non-`PAGE`-multiple reservation length. Keep the miri `dealloc` proof tied to allocation-layout identity, not merely layout validity.

### MEDIUM — M2: the hand-written `mmap` declaration has the wrong `off_t` ABI on supported-by-cfg 32-bit Unix targets

**Where:** `crates/vmem/src/lib.rs:2418-2445` and call at `:2461-2471`.

The crate declares `mmap(..., offset: i64)` for every `cfg(unix)` target. On glibc i686 (and traditional 32-bit ARM without the large-file interface), `off_t` is 32-bit by default, while the Rust declaration supplies a 64-bit final parameter. The literal value `0` prevents numeric truncation, but it does not make an incompatible foreign function declaration ABI-correct.

Concrete failure scenario: compile the crate for Tier-1 `i686-unknown-linux-gnu`, then call `reserve_aligned(PAGE, PAGE)`. The call crosses the C boundary through a prototype whose final parameter width differs from libc's `mmap(void *, size_t, int, int, int, off_t)`. Register-based ABIs may happen to tolerate a zero in the last position; that is an implementation accident, not a portable FFI contract, and another 32-bit Unix ABI is free to misread the call frame/register assignment.

**Prior-review disagreement:** `docs/reviews/2026-08-09-aligned-vmem-round-closing-review.md:700-703` calls the deferral sound because the value is always zero and cannot truncate. That addresses value semantics, not ABI compatibility. The current source comment itself admits an “ABI shape mismatch” at `lib.rs:2431-2432`, yet gates the declaration over targets where the mismatch exists. I therefore reject the settled “sound deferral” conclusion. If the project intentionally supports only the enumerated 64-bit Unix targets, that restriction must be encoded in `cfg`/compile-time guards and publication docs; the current code and “Unix” marketing are broader.

**Suggested direction:** use per-target `off_t` aliases matching the actual C ABI (or a maintained bindings source), and add at least an i686 compile/link/run gate. A compile-only check may not diagnose a hand-written extern mismatch; the useful oracle calls the function.

### LOW — L1: the benchmark records failed/no-op work as valid faster samples

**Where:** `crates/vmem/benches/vmem_bench.rs:53-61`, `:69-83`, `:90-112`, `:120-138`, `:148-160`.

All helpers turn reserve failure (and one recommit failure) into `false`; the harness merely `black_box`s that boolean. It neither aborts nor excludes the sample. The void-returning decommit arms also have no path-activation/effect oracle.

Concrete counterfactuals:

- Replace `reserve_aligned` with `None`: all four workloads still complete and report a dramatic “speedup” while executing no VM lifecycle.
- Replace `decommit` with an immediate return: both decommit-labelled workloads remain green and get faster.

No published release claim currently depends on these measurements, so this is LOW measurement-integrity risk rather than a shipping correctness defect. It is distinct from prior V18 (`docs/reviews/2026-08-12-aligned-vmem-code-quality-review.md`), which found missing CI compilation and asymmetric black-box/path identity but did not identify accepted failure samples. `cargo bench --no-run` now compiles the harness, but compilation cannot validate its oracle.

**Suggested direction:** make any failed iteration abort the benchmark with the operation/error named; for path-sensitive arms, assert an available diagnostic counter around the measurement setup or perform a separate untimed activation check.

### LOW (already tracked) — L2: `decommit_lazy_roundtrip` is vacuous with respect to lazy decommit

**Where:** `crates/vmem/tests/smoke.rs:393-410`; tracked in `docs/CORRECTNESS_OPEN_ITEMS.md` item 48 (S4 remainder).

The test writes `0x9E`, calls `decommit_lazy`, calls `recommit`, then overwrites with `0x3C` before reading. Replacing `decommit_lazy` with `return` leaves the test passing on every backend. Thus a Linux `MADV_FREE` regression can remove all reclaim behavior while this named test remains green.

This confirms, rather than rediscovers, round-6 S4. The macOS `bench-internals` test checks syscall issuance there; the index correctly records that Linux real-backend CI has no equivalent counter assertion. Severity remains LOW because lazy reclaim is advisory/resource behavior, not pointer validity.

## Confirmed known items and null results

- **Item 6 confirmed, not re-derived as new:** Windows `MEM_DECOMMIT` requires explicit recommit before access; current docs state the crash scenario accurately.
- **Item 41 remains a CI gap:** crate-specific miri execution is absent; `--cfg miri` compilation passes. Root jobs exercise some fallback code transitively, but they do not substitute for this crate's own test oracles. The intentional leak remains the known runner-policy blocker.
- **Item 43 remains partially open:** BSD `_SC_PAGESIZE` values are still reasoned from headers, not empirically run. The unconditional alignment check prevents this table from violating reserve alignment, but decommit rounding can still be poisoned by a plausible wrong value.
- **Item 48 confirmed:** Darwin eager decommit does not provide Linux-style zero-fill/RSS semantics. Current docs are candid; no additional Darwin defect was found.
- **Item 49 confirmed exactly in spirit:** `unsafe_op_in_unsafe_fn` warns at four native Windows and four Linux operations in the configurations run. This is edition-migration/maintainability debt, not current unsoundness; explicit unsafe blocks elsewhere all have adjacent `SAFETY` proofs.
- **Item 50 confirmed:** Windows reserve counter behavior and the `page_size()` rejection branch remain without direct injection/oracles.
- **Item 51 was handled for this review:** Linux cross-target check and clippy both passed, but they cannot exercise runtime HugeTLB/default-page behavior.
- **Safety nulls:** no new provenance loss, aliasing defect, double release, errno/GetLastError timing error, invalid manual `Send`, union misuse, exported callback boundary, or Windows/miri backend UB was found beyond M1/M2. `Reservation`'s manual `Send` proof is consistent with exclusive ownership transfer; it is intentionally not `Sync`.
- **Concurrency:** the documented concurrent Windows commit claim matches `VirtualAlloc(MEM_COMMIT)`'s idempotence. The already-documented `fault_injection::arm_fail_at` re-arm-vs-self-disarm race remains an opt-in test-hook limitation; no new production race was found.
- **Error taxonomy:** `VmemError` distinguishes invalid arguments, known OS codes (including zero), and unknown-code OS refusal. That is sufficient for current fallible operations. Decommit/madvise/munmap failure remains intentionally unobservable; the resulting resource/semantic limitations are documented, though H1 shows the HugeTLB premise feeding that design was incomplete.
- **Maintainability/conventions:** no inline `#[cfg(test)] mod tests`, no runnable source doctests, and no `mod.rs` code. The large seam layout and partial mock-backend `cfg_attr(dead_code)` structure are previously reviewed/deferred design smells, not newly manufactured findings. The mock Cargo feature remains a documented non-additive feature-unification hazard that is especially worth settling before first publish.
- **Release-profile divergence:** CI does not run crate tests under `--release`, but relevant arithmetic uses checked operations and I found no concrete release-only failure; null result.
- **Performance:** null. Successful common paths are syscall-dominated and allocation-free. Removing Unix's exact-size attempt helps misses but costs permanent VA over-reservation on hits; existing hit-rate measurements demonstrate a workload/platform trade-off, not a universal speedup. No additional syscall, allocation, cache, or branch optimization met the evidence bar.

## Recommended release order

1. Fix H1 and validate the chosen HugeTLB-size policy against a successful non-2-MiB-default configuration.
2. Fix M1 and add zero/non-page-multiple negative tests, including a miri-focused layout-identity oracle.
3. Resolve M2 by matching `off_t` per target or narrowing the supported target surface explicitly.
4. Make the benchmark fail closed (L1) before using it for any release/performance claim.
5. Close the already-tracked miri/Linux lazy-decommit coverage gaps as release infrastructure, without treating their current absence as evidence that H1-M2 are safe.
