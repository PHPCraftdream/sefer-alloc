# Security Review — sefer-alloc (narrow-sense security angle)

**Date:** 2026-07-09
**Reviewer angle (1 of 7 parallel reviewers):** security in the narrow sense —
supply chain, CI hygiene, FFI boundaries, integer-overflow attack surface,
attacker model. NOT the general `unsafe` audit (another reviewer) and NOT the
general bug hunt (another reviewer).

**Scope:**
- Verification of `docs/security/SECURITY_COMPLIANCE_AUDIT_2026-07-06.md` (3 days old).
- New CI workflow `.github/workflows/kani.yml` (added 2026-07-09).
- `deny.toml` + live `cargo deny check` run (the audit's NOT-VERIFIED `cargo audit` gap).
- Dependency drift `Cargo.toml`/`Cargo.lock` since 2026-07-06.
- Integer-overflow surface from `GlobalAlloc::alloc(Layout)` → segment/size-class.
- FFI wrappers: `crates/vmem/src/lib.rs`, `crates/numa/src/lib.rs`, `src/alloc_core/os.rs`.
- All four CI workflows: `ci.yml`, `release.yml`, `perf-gate.yml`, `kani.yml`.

**Methodology:** read the prior audit fully; re-ran `cargo deny check` locally
(cargo-deny 0.19.9 is installed; cargo-audit is NOT); `git log`/`git diff` on the
dependency manifests since the audit commit; traced the `Layout` arithmetic path
`SeferAlloc::alloc` → `HeapCore::alloc` → `AllocCore::alloc`/`alloc_large` →
`size_classes` / `align_up` / `reserve_aligned`; read the FFI syscall wrappers
line by line for pre-syscall argument validation; grepped every workflow for
`pull_request_target`, `secrets.`, and untrusted-`${{ }}`-into-`run:` injection;
verified the `actions/checkout@v5` SHA-pin against the live upstream tag.

---

## A. ПОДТВЕРЖДЕНО СТАРЫМ АУДИТОМ, ВСЁ ЕЩЁ АКТУАЛЬНО

### A.1 Runtime dependency tree still minimal and clean — CONFIRMED
`Cargo.lock` has **not changed** since the audit (`git diff 4b0978b HEAD --
Cargo.lock` is empty; the only manifest change is `Cargo.toml` — see NEW §B.2).
The runtime tree is still effectively `slotmap` + workspace path crates; optional
`crossbeam-epoch`/`arc-swap`/`core_affinity` unchanged. No slopsquat, no new
transitive crate. Audit §1.2 verdict holds verbatim.

### A.2 `unsafe` confinement seam list — CONFIRMED, no drift
`grep -rln 'allow(unsafe_code)'` today returns exactly the same seam set the
audit enumerated (§1.1): `node.rs`, `os.rs`, `numa.rs` (alloc-core), `hand.rs`,
`sefer_alloc.rs`, `fallback.rs`, `tls_heap.rs`, `bootstrap.rs`,
`heap_registry.rs`, `heap_slot.rs`, plus the three sibling crates
(`crates/{vmem,numa,malloc-bench}`) and `src/lib.rs` itself. No new
`allow(unsafe_code)` file appeared. The compiler-enforced `#![forbid]`/`#![deny]`
posture is intact. (This is scoped to the security-boundary question "did a new
unsafe aperture open"; the per-block soundness audit is the other reviewer's.)

### A.3 `GlobalAlloc` seam bodies delegate to safe entry points — CONFIRMED
`src/global/sefer_alloc.rs` still has its only `unsafe` as the `unsafe impl
GlobalAlloc` trait obligation; every method null-guards and delegates to
`HeapCore`. No panic sites, no `std::alloc`/`format!`/collection on the path
(re-verified). M5/M10 discipline intact.

### A.4 No hardcoded secrets — CONFIRMED
Only reference to a credential anywhere is `${{ secrets.CARGO_REGISTRY_TOKEN }}`,
still confined to the single `cargo publish` step's `env:` in `release.yml:195`.
No literal token, key block, or provider prefix in tracked files.

### A.5 License posture — CONFIRMED and now MACHINE-CHECKED
`cargo deny check` reports `licenses ok` against the full-feature graph
(`all-features = true` in `deny.toml`). The allow-list is `MIT`/`Apache-2.0`/`Zlib`.
No copyleft. This upgrades audit §2.2 from "assessed from cargo tree, NOT
machine-verified" to **machine-verified locally today**.

---

## B. НОВОЕ С 2026-07-06

### B.1 Kani workflow `.github/workflows/kani.yml` — security-clean (INFO / GOOD)
Added 2026-07-09 (`137792b`). Audited for the escalation risk the task flagged:

- **`permissions: contents: read` IS present** (`kani.yml:17-18`), with an
  explicit SEC-4 rationale comment mirroring the other workflows. No permission
  escalation. GOOD.
- Triggers: `push`/`pull_request` on `main` + `workflow_dispatch`. **No
  `pull_request_target`** — so a fork PR cannot run this workflow with repo
  secrets or a write token. Correct.
- The job installs Kani via `cargo install --locked kani-verifier` then
  `cargo kani setup` (which downloads a pinned nightly + CBMC on the runner).
  This is a **CI-only, dev-only** toolchain; see §B.3 for the dep-tree check.
- Actions are tag-pinned (`actions/checkout@v5`, `dtolnay/rust-toolchain@stable`)
  — same LOW tag-vs-SHA posture the audit already noted (§1.6 #2). This workflow
  carries **no publish token**, so the tag-rewrite vector here is low-impact
  (a compromised checkout tag could tamper with proof results but not publish).
  Consistent with the project's conscious decision to SHA-pin only `release.yml`.

**Verdict:** the new workflow follows the SEC-4 least-privilege pattern already
established. No new attacker-reachable surface.

### B.2 `Cargo.toml` feature-graph churn since audit — no new dependency (INFO)
`git diff` since 2026-07-06 on `Cargo.toml`:
- Removed the `alloc` feature and rewired `alloc-xthread`/`alloc-global` to depend
  directly on `alloc-core` (task #204). No security impact — pure feature-edge
  refactor.
- Added `cfg(kani)` to the `unexpected_cfgs` check-cfg list (so `#[cfg(kani)] mod
  kani_proofs` at `src/lib.rs:252-253` compiles warning-clean). Cosmetic.
- Added `alloc-runfreelist` experimental feature (`= ["alloc-core"]`), OFF by
  default, no new dep, no new `unsafe` site (RunStack is safe arithmetic).
- Removed the `heap_alloc` bench (depended on the deleted `alloc` feature).

**No new dependency entered any tree** (runtime, optional, dev, or the fuzz
sub-workspace). `Cargo.lock` unchanged. Dependency-drift risk since the audit:
**none.**

### B.3 kani-verifier is NOT in the runtime/dev dep tree — CONFIRMED SAFE (INFO)
The task asked specifically whether `kani-verifier` leaked into the runtime tree.
It does not appear in `Cargo.toml` or `Cargo.lock` at all — it is installed
imperatively on the CI runner (`cargo install kani-verifier`) and never declared
as a dependency. `src/kani_proofs.rs` is gated behind `#[cfg(kani)]`, a cfg set
only by the Kani harness at proof time, never by a normal `cargo build`. So Kani
adds **zero** crates to what a downstream consumer of `sefer-alloc` compiles or
links. Correct isolation.

### B.4 SEC-1/2/3/4/5/6 remediations from the audit all LANDED (INFO / GOOD)
The audit's action items were executed as tasks #198–#203 (all committed
2026-07-06/07, after the audit):
- **SEC-1/2:** `SECURITY.md` placeholder e-mail removed; supported-versions table
  fixed to "Latest 0.x" (re-read `SECURITY.md` today — clean, no
  `REPLACE_WITH_REAL_EMAIL`, no stale 0.1.x row). Audit §2.3 medium items CLOSED.
- **SEC-3:** README `hardened` row + 1/256-wrap residual now documented
  (`README.md:772,838`). Audit §1.5 doc-gap CLOSED.
- **SEC-4:** `permissions: contents: read` added to ALL workflows (verified in
  `ci.yml:24`, `release.yml:86`, `perf-gate.yml:48`, `kani.yml:17`). Audit §1.6
  #1 CLOSED.
- **SEC-5:** `cargo-deny` job added (`ci.yml:662-679`) + `deny.toml`. Audit §1.6
  #3 / §1.3 CLOSED.
- **SEC-6:** `actions/checkout` SHA-pinned in `release.yml:100`
  (`93cb6efe…bf9bfd # v5`) — **I verified this SHA is exactly what upstream
  `refs/tags/v5` points to today** (`git ls-remote` → `93cb6efe18208431cddfb8368fd83d5badbf9bfd`).
  Pin is accurate. `dtolnay/rust-toolchain@stable` left as a moving branch by
  documented conscious decision. Audit §1.6 #2 addressed for the token-bearing
  workflow.

The prior audit's three MEDIUM items and the actionable LOW items are all
remediated. This is a materially improved posture vs. 3 days ago.

---

## C. УГЛУБЛЕНИЕ NOT VERIFIED (the audit's open gaps, now closed)

### C.1 `cargo audit` gap — CLOSED via `cargo deny check` (advisories: OK)
The audit could not run `cargo audit` (tool absent). I confirmed **cargo-audit is
still not installed** (`error: no such command: audit`) and, per task rules, did
NOT install it. However **cargo-deny 0.19.9 IS installed**, and I ran the full
check locally:

```
$ cargo deny check
advisories ok, bans ok, licenses ok, sources ok
```

All four categories PASS today. This is the machine-checked RustSec-advisory
verification the audit listed as NOT VERIFIED (§1.3). The three suppressed
advisories in `deny.toml` were each reviewed:

- **RUSTSEC-2025-0141** (`bincode` unmaintained) — dev-only, via
  `iai-callgrind`. Not in the published tree. Suppression justified. STILL VALID.
- **RUSTSEC-2026-0173** (`proc-macro-error2` unmaintained) — dev-only, same
  `iai-callgrind` chain. Suppression justified. STILL VALID.
- **RUSTSEC-2026-0204** (`crossbeam-epoch` 0.9.18 `fmt::Display` derefs a
  possibly-null `Shared`/`Atomic` sentinel; fixed >=0.9.20) — this is the ONE
  advisory that touches a *non-dev* path (the `experimental` feature's
  `dep:crossbeam-epoch` behind `src/concurrent/hand.rs`). I **re-ran the
  suppression's own falsifiability test** that `deny.toml:59-66` documents:
  grepped `src/concurrent/{hand,epoch_region}.rs` for any `Display`/`format!`/
  `write!`/`{}`/`{:?}` on a `Shared`/`Atomic` value — **none exists**. `hand.rs`
  only touches these via `.as_ref()`/pointer-load APIs, never the vulnerable
  Display path. The suppression note's stated invariant ("re-grep before trusting
  this note") holds as of today. **Suppression STILL SOUND.** (Standing caveat,
  unchanged from the deny.toml comment: this advisory is only dodged, not fixed;
  a future `cargo update -p crossbeam-epoch` to >=0.9.20 would retire it, but
  that is a version bump deferred to explicit request per project rule.)

**Residual note (LOW, unchanged):** the suppressions are correct *today* but are
a standing maintenance obligation — `advisories ok` is contingent on those three
ignores, two of which mask *unmaintained* transitive dev-deps (`iai-callgrind`
chain). If `iai-callgrind` is ever replaced/dropped, both ignores should be
removed so future advisories on those crates resurface. This is exactly the
"human-decision deferral" the config documents; no action required now.

### C.2 Integer-overflow attack surface `Layout` → segment — NO EXPLOITABLE WRAP FOUND
Threat model as the task frames it: `sefer-alloc` installed as `#[global_allocator]`
in a binary where some caller (not necessarily trusted) drives `alloc(Layout)`
with adversarial `size`/`align`. Note `Layout` itself already guarantees
`size + align` does not overflow `isize` at construction — but I traced the
internal arithmetic regardless, for defense-in-depth:

Path `SeferAlloc::alloc` → `HeapCore::alloc` → `AllocCore::alloc`:
- `size = layout.size().max(MIN_BLOCK)` — no arithmetic, just a clamp.
- Small path: `SizeClasses::class_for(size, align)` is pure table indexing;
  `(need - 1) >> MIN_BLOCK_SHIFT` where `need = max(size,align) <= SMALL_MAX`
  (checked `> SMALL_MAX → None` first), so the shift index is bounded and cannot
  wrap. No `size*count` anywhere on the small classify path.
- Large path `alloc_large(size, align)` (`alloc_core.rs:3349`):
  - `align >= SEGMENT → null` (task #130 guard) — rejects the dangerous
    large-align case before any arithmetic.
  - `hdr_aligned = align_up(size_of::<SegmentHeader>(), align.max(PAGE))`.
  - `needed = hdr_aligned + align_up(size, align)` — **this is an unchecked
    `usize` add**, and `align_up` (`segment_header.rs:602`) is
    `n.div_ceil(a) * q` (also unchecked `q * a`).

**Is that add/mul exploitable?** For it to wrap, `size` would need to be within
`~align`/`~SegmentHeader` of `usize::MAX`. But such a `Layout` cannot be
constructed: `Layout::from_size_align` rejects any `size` where
`size` rounded up to `align` exceeds `isize::MAX`. `GlobalAlloc::alloc` receives
a `Layout`, and every internal `realloc` constructs its new `Layout` via
`Layout::from_size_align(new_size, align)` with an `Err(_) → null` guard
(`heap_core.rs:1220,1239`; `alloc_core.rs:1620`). So `size <= isize::MAX < usize::MAX/2`
always holds by the time it reaches `align_up`, and `hdr_aligned + align_up(size,
align)` with both operands `<= isize::MAX + SEGMENT` cannot reach `usize::MAX`.
The subsequent `needed.div_ceil(SEGMENT) * SEGMENT` likewise stays bounded.

**Verdict:** no reachable integer-overflow-to-undersized-allocation on the
`GlobalAlloc` entry path. `align_up`'s comment already claims it "avoids overflow
vs `n + a - 1`" (the div_ceil form), and the load-bearing `checked_add` IS
present exactly where an attacker-influenced value could otherwise matter — the
realloc in-place grow bound (`alloc_core.rs:1751`
`payload_off.checked_add(new_eff)`) and the FFI over-reserve (`vmem`
`size.checked_add(align)` at lib.rs:330/455). Severity: **INFO** (defense-in-depth
already correct). One cosmetic observation below.

**LOW (defense-in-depth, cosmetic):** `alloc_large`'s `needed = hdr_aligned +
align_up(size, align)` (`alloc_core.rs:3370`) is the one spot on the large path
doing an unchecked add of two size-derived quantities. It is UNREACHABLE-to-wrap
today (Layout invariant proven above), so this is not a bug. If you ever want
belt-and-suspenders parity with the realloc path (which DOES use `checked_add`),
a `checked_add` here returning null would make the "cannot wrap" argument local
rather than relying on the caller's `Layout` invariant. Optional.

### C.3 FFI boundary argument validation — DEFENSE-IN-DEPTH PRESENT (INFO / GOOD)
The task asks whether the OS-syscall wrappers validate arguments *before* the
syscall or blindly trust the caller. Findings:

- **`crates/vmem` `reserve_aligned`** (lib.rs:249): validates
  `size != 0 && align.is_power_of_two() && align >= PAGE && size % PAGE == 0`
  and returns `None` on violation — BEFORE any `mmap`/`VirtualAlloc`. The
  over-reserve uses `size.checked_add(align)?` (lib.rs:330/455) — overflow-safe.
  `decommit`/`recommit` both validate `start < end` and page-alignment before the
  syscall (lib.rs:296,316). This is real pre-syscall validation, not blind trust.
  GOOD.
- **`Reservation::from_raw_parts`** (lib.rs:199) is `unsafe fn` with a documented
  5-point contract; it defensively `NonNull::new(...).expect()`s both pointers so
  a null slips into a panic (in a well-formed call: dead branch), never into a
  later `release(null)`. Reasonable.
- **`crates/numa` `bind_range`** (lib.rs:174) is `unsafe fn`; short-circuits
  `node == NO_NODE || len == 0` before the syscall. `bind_range_impl_linux`
  additionally guards `node >= 64` before building the nodemask
  (`1u64 << node` — so no shift-overflow UB; a `node >= 64` shift would be UB in
  Rust, and the guard prevents it). `reserve_aligned_numa` (Windows) re-validates
  the same size/align contract as vmem and uses `checked_add`. GOOD.
- **`numa` sysfs parser** (`node_contains_cpu` → `read_cpumap_contains_cpu` →
  `parse_cpumap_contains_cpu`): reads `/sys/devices/system/node/nodeN/cpumap`
  into a fixed `[u8; 256]` (bounded read), the path is built into a fixed
  `[u8; 64]` with a hand-rolled decimal formatter capped at node<64 (loop bound),
  and the hex parser rejects non-hex bytes. No unbounded read, no format-string,
  no heap. This is a file the kernel controls (not an external attacker) so the
  trust boundary is the local kernel; the parsing is nonetheless bounded and
  panic-free. GOOD.
- **`src/alloc_core/os.rs`** `release_segment` null-guards before the vmem call;
  `decommit_pages`/`recommit_pages` forward to the validated vmem entry points.

**Verdict on FFI:** the crate exercises defense-in-depth — the syscall wrappers
validate their own arguments even though the only in-tree caller is `sefer-alloc`
itself. This exceeds "trust the caller because it's internal." No missing
validation found. (Per-block `unsafe` soundness is the other reviewer's remit;
here the finding is specifically "boundary inputs are checked pre-syscall": YES.)

### C.4 CI script-injection / `pull_request_target` sweep — NONE FOUND (INFO)
Swept all four workflows:
- **No `pull_request_target` anywhere.** Fork PRs run under `pull_request` with
  `contents: read` and no secrets access — the safe default. No fork can reach
  `CARGO_REGISTRY_TOKEN`.
- **Untrusted-input-into-`run:` injection:** the only `${{ }}` interpolations into
  shell bodies are `github.ref_name` (a git tag/ref — write-access-gated to push),
  `inputs.crate` (a `choice` enum, not free text — cannot carry a payload), and
  `github.event.label.name == 'perf'` (an `if:` comparison, not a shell splice).
  All are single-quoted where spliced into bash (`release.yml`), and none derive
  from attacker-controllable PR title/body/branch-name. `perf-gate.yml`'s
  label-triggered path additionally guards `github.event.label.name == 'perf'`
  before running. No command-injection vector.
- **`$GITHUB_STEP_SUMMARY`/`$GITHUB_OUTPUT` writes** in `release.yml` are writes to
  runner-provided files, not contents-API calls — consistent with the
  `contents: read` scope. Correct.
- `set -euo pipefail` is used in every non-trivial `release.yml` script step.

**Verdict:** CI is injection-clean and least-privilege. The only residual is the
already-documented tag-vs-SHA pinning choice (LOW, conscious).

---

## Итоговый вердикт

**Posture as of 2026-07-09: STRONG, and measurably improved since the 2026-07-06
audit.** All three MEDIUM items and the actionable LOW items from the prior audit
were remediated (SEC-1..6, tasks #198–#203). The new Kani workflow was added with
correct least-privilege permissions and no fork-secret exposure, and Kani stays
entirely out of the shipped dependency tree.

The prior audit's single biggest NOT-VERIFIED gap — no machine-checked RustSec
advisory scan — is now **closed and green**: `cargo deny check` passes all four
categories locally today (`advisories ok, bans ok, licenses ok, sources ok`), and
the one non-dev advisory suppression (crossbeam-epoch RUSTSEC-2026-0204) was
re-verified sound against the crate's actual (non-Display) usage.

No exploitable integer-overflow was found on the `GlobalAlloc(Layout)` → segment
path (the `Layout` invariant plus existing `checked_add` guards close it). The
FFI syscall wrappers in `crates/vmem` and `crates/numa` validate their arguments
pre-syscall (power-of-two align, page-multiple size, `checked_add` over-reserve,
`node < 64` shift guard) — genuine defense-in-depth, not blind internal trust.

### Findings ledger (this review)

| # | Severity | Category | Finding | Location | Status |
|---|----------|----------|---------|----------|--------|
| 1 | INFO | CI supply chain | `kani.yml` correct `permissions: contents: read`, no `pull_request_target`, tag-pinned actions (no publish token here) | `.github/workflows/kani.yml:17` | Clean |
| 2 | INFO | Supply chain | Kani is CI-install-only; NOT in `Cargo.toml`/`Cargo.lock`; `src/kani_proofs.rs` behind `#[cfg(kani)]` | `src/lib.rs:252` | Clean |
| 3 | INFO | Dep drift | No dependency added since audit; `Cargo.lock` unchanged; only feature-graph refactor + experimental `alloc-runfreelist` (no dep, no unsafe) | `Cargo.toml` | Clean |
| 4 | INFO | Advisories | `cargo deny check` PASSES all four categories locally (closes audit §1.3 NOT-VERIFIED); 3 suppressions each re-verified | `deny.toml`, live run | Verified |
| 5 | LOW | Advisories (maint.) | `advisories ok` is contingent on 3 documented ignores (2 unmaintained dev-dep chains via `iai-callgrind`); standing obligation to re-check if that dep changes | `deny.toml:25-67` | Accept/track |
| 6 | INFO | Overflow surface | No reachable int-overflow `alloc(Layout)`→segment; `Layout` invariant + `checked_add` at realloc/FFI cover it | `alloc_core.rs`, `size_classes.rs` | Clean |
| 7 | LOW | Overflow (cosmetic) | `alloc_large`'s `needed = hdr_aligned + align_up(size,align)` is unchecked (unreachable-to-wrap today); `checked_add` here would localize the safety argument | `src/alloc_core/alloc_core.rs:3370` | Optional |
| 8 | INFO | FFI boundary | vmem/numa wrappers validate align/size/node pre-syscall + `checked_add` over-reserve + `node<64` shift guard — defense-in-depth present | `crates/vmem/src/lib.rs:249,330,455`; `crates/numa/src/lib.rs:498,646,649` | Clean |
| 9 | INFO | CI injection | No `pull_request_target`; no untrusted PR-title/body/branch spliced into `run:`; enum/ref-name inputs only, single-quoted | all 4 workflows | Clean |
| 10 | INFO | Remediation | Audit SEC-1..6 (MEDIUM disclosure + LOW CI items) all landed as #198–#203; `checkout@v5` SHA-pin verified against live upstream tag | `SECURITY.md`, `README.md`, workflows | Closed |

**Net:** no new security defect introduced in the 3-day window; one prior gap
closed (advisory scan), two optional/standing LOW notes (finding #5 maintenance,
#7 cosmetic overflow parity). Nothing here rises to a code change requirement.
