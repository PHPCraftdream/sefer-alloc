# Audit 9 — Public API / semver / crate-extraction hygiene

Scope: read-only static review of the workspace root `Cargo.toml` and all 11
`crates/*` (`Cargo.toml` + `src/lib.rs` + submodules where relevant), against
semver hazards, public-API minimality, feature-flag coherence,
version/dependency consistency, publish metadata, and the project's
"single-file seam crate" / "one export" conventions. No `cargo`/build/test
commands were run (parallel agents active); findings are from direct reading
of source + `git log`/`grep`.

Crates covered: `aligned-vmem` (crates/vmem), `numa-shim` (crates/numa),
`malloc-bench-rs` (crates/malloc-bench), `sefer-region` (crates/region),
`racy-ptr-cell`, `ring-mpsc`, `size-classes`, `globalalloc-model`,
`tagged-index-stack`, `proc-memstat`, `proc-probe`.

---

## Findings

### F1 — [Low] `ring-mpsc` is a workspace member with zero consumers anywhere in the tree

- **Crate + file:** `crates/ring-mpsc/Cargo.toml:1`, root `Cargo.toml:84`
  (workspace `members`), root `Cargo.toml` `[dependencies]`/`[dev-dependencies]`
  (no `ring-mpsc` entry anywhere).
- **Type:** publish-metadata gap / dead workspace member.
- **Impact:** `ring-mpsc` was extracted (commit `4c20f0c`) and the in-tree
  swap of `RemoteFreeRing`/`HeapOverflow` onto it was evaluated and explicitly
  rejected — commit `d062798` "CRATE-P4-followup (#187) ring-mpsc in-tree swap
  = verified NO-GO". That is a legitimate, documented engineering decision
  (`src/alloc_core/remote_free_ring.rs` and `src/registry/heap_overflow.rs`
  keep their own hand-rolled Vyukov-style rings, structurally identical in
  protocol but not literally the same type). The residual problem is
  purely a **hygiene** one: nothing in the workspace — not even a
  `dev-dependency` or a doctest/example — ever compiles or exercises
  `ring-mpsc` from inside this repo's CI matrix. Its only test coverage is
  its own `crates/ring-mpsc/tests/*`. A regression that only manifests when
  the crate is consumed generically (e.g. an MSRV or feature-unification
  interaction with the rest of the workspace) would not be caught by
  `cargo test --workspace` failing to build `ring-mpsc` as a dependency of
  anything — it's already an already-known, already-partially-documented gap
  (see `docs/reviews/2026-07-17-deep-audit/03-compliance-conventions.md:149`,
  which proposes annotating the README crate table). This audit corroborates
  that finding from the semver/publish-readiness angle: a crate nobody in the
  workspace depends on is the crate most likely to silently bit-rot between
  publishes.
- **Fix:** no code change needed beyond what `03-compliance-conventions.md`
  already proposes (annotate `README.md`'s "External publishable crates"
  table with `ring-mpsc`'s NO-GO status and a `d062798` pointer). Optionally,
  add a `[dev-dependencies]` smoke build (e.g. a tiny example or doctest-free
  integration test in the root crate) that constructs an `MpscRing` just to
  keep it compiling against the workspace's own dependency versions —
  cosmetic, not required for correctness.

### F2 — [Low] Root README's crate tables list only 4 of 11 published crates

- **File:** `README.md:322-325` (crates.io/docs.rs badge table),
  `README.md:347-350` ("External publishable crates" unsafe-story table).
- **Type:** publish-metadata gap (discoverability).
- **Impact:** Both tables enumerate `sefer-region`, `aligned-vmem`,
  `numa-shim`, `malloc-bench-rs` only. `racy-ptr-cell`, `ring-mpsc`,
  `size-classes`, `globalalloc-model`, `tagged-index-stack`, `proc-memstat`,
  `proc-probe` — seven crates, all with their own `Cargo.toml`
  `description`/`repository`/`documentation`/`keywords`/`categories` fully
  filled in and clearly intended for crates.io — are absent from the root
  README's own crate index. A user landing on the root README (the most
  likely entry point) has no way to discover these seven exist unless they
  browse `crates/` directly. This is purely a documentation-surface gap, not
  a functional one — each crate's own `crates/*/README.md` is complete (all
  11 have `README.md` + `LICENSE-APACHE` + `LICENSE-MIT`).
- **Fix:** extend both tables to all 11 crates. For the unsafe-story table
  specifically, the four `#![allow(unsafe_code)]` single-file seam crates not
  yet listed are `racy-ptr-cell` (crates/racy-ptr-cell/src/lib.rs:87),
  `ring-mpsc` (crates/ring-mpsc/src/lib.rs:96), `proc-memstat`
  (crates/proc-memstat/src/lib.rs:52); the three `#![forbid(unsafe_code)]`
  crates (shown for contrast, alongside `sefer-region`) are `size-classes`
  (crates/size-classes/src/lib.rs:43), `tagged-index-stack`
  (crates/tagged-index-stack/src/lib.rs:108), `proc-probe`
  (crates/proc-probe/src/lib.rs:45); `globalalloc-model`
  (crates/globalalloc-model/src/lib.rs:63) is `#![allow(unsafe_code)]` for the
  documented reason of dereferencing the allocator-under-test's raw pointers.

### F3 — [Info] `mock::Call` (aligned-vmem) and `mock::MockCall` (numa-shim) enums lack `#[non_exhaustive]`

- **File:** `crates/vmem/src/mock.rs:31` (`pub enum Call`),
  `crates/numa/src/lib.rs:72` (`pub enum MockCall`).
- **Type:** semver-hazard (minor).
- **Impact:** Both enums are feature-gated (`mock`) test-only recording
  surfaces that are very likely to grow new variants as new operations get
  mock-recorded (e.g. a future `huge-pages`-flavoured `ReserveHuge` already
  exists; a hypothetical `DecommitLazy`-adjacent variant would be natural).
  Any downstream test harness that `match`es exhaustively on `Call`/`MockCall`
  (rather than `if let`/`matches!`) would break on the next added variant —
  a minor-version-incompatible change slipping out under a patch release if
  not caught. Low severity because these are `mock`-feature-gated dev/test
  surfaces, not the crate's primary API, and the crates are still pre-1.0
  (0.1.x/0.2.x), where such breaks are within semver's tolerance for `0.x`
  (a minor bump is already "may break"). Still worth flagging since the
  crates otherwise show good semver discipline elsewhere (e.g.
  `LargeCacheMode` in the root crate is explicitly `#[non_exhaustive]` per
  `README.md:110`).
- **Fix:** add `#[non_exhaustive]` to both enums now, while both crates are
  still pre-1.0 and the annotation costs nothing (no existing external
  consumer to break).

### F4 — [Info] `globalalloc-model::Live` and `Config`, `malloc-bench-rs::Config`, `size-classes::Params` are plain pub-field structs (no `#[non_exhaustive]`)

- **File:** `crates/globalalloc-model/src/lib.rs:160` (`Live`) and `:184`
  (`Config`); `crates/malloc-bench/src/lib.rs:380` (`Config`);
  `crates/size-classes/src/lib.rs:51` (`Params<'a>`).
- **Type:** semver-hazard (minor, by design).
- **Impact:** All four are intentionally plain, fully-public "bag of config
  knobs" structs constructed via struct-literal syntax
  (`Config { threads: 1, .. }`, `Params { min_block: 16, .. }`) rather than a
  builder — every one of them already documents `Default` (three of the four)
  as the escape hatch for future-field-addition
  (`..Default::default()` pattern), which is the standard mitigation.
  `size-classes::Params` has no `Default` impl and is meant to be
  fully-specified at every call site (it is a `const`-context input, where
  `..Default::default()` spread syntax is not available anyway pre-Rust
  const-trait support) — adding a field there IS a hard breaking change with
  no soft-landing, more so than the other three. This is flagged as
  **informational** rather than a hazard needing a fix: struct-literal
  configs with all-pub fields is the deliberate, documented idiom this
  project already uses at the root crate too (e.g. `AllocStats`), and forcing
  a builder pattern onto four small, rarely-changing config structs would be
  over-engineering for the crates' actual audience (internal-plus-a-few-
  downstream-consumers, all pre-1.0).
- **Fix:** none required. If `size-classes::Params` gains a field in the
  future, treat it as the documented pre-1.0 minor-bump breaking change it
  is; no code action needed now.

### F5 — [Info] `proc-memstat::MemStat` is a plain pub-field struct without `Default`-mediated forward-compat, unlike its siblings

- **File:** `crates/proc-memstat/src/lib.rs:62`.
- **Type:** semver-hazard (minor).
- **Impact:** `MemStat` **does** derive `Default` (line 61:
  `#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]`), so a future field
  addition is actually well-mitigated via `..Default::default()` — this is
  correctly done, not a gap. Noted here only to record that the audit
  checked it and found it already follows the mitigation pattern F4 describes
  for the other structs; no action needed. (Downgraded from a candidate
  finding to a confirmation note.)

### F6 — [Info] `ring-mpsc::RingStore` — correctly sealed; no leak

- **File:** `crates/ring-mpsc/src/lib.rs:356` (`pub trait RingStore:
  sealed::Sealed`), `:367-373` (`mod sealed { pub trait Sealed {} ... }`).
- **Type:** N/A — confirmation, not a finding.
- **Impact:** `RingStore` must be `pub` because it appears in `MpscRing<S>`'s
  method signatures (`S: RingStore` bound is publicly visible), but the crate
  correctly seals it via the private `mod sealed` + `sealed::Sealed`
  supertrait pattern, so no downstream crate can implement `RingStore` for
  its own type and reach into `MpscRing`'s internals. This is exactly the
  idiomatic sealed-trait pattern and is called out here only as a positive
  confirmation (the audit specifically checked "pub trait without seal" per
  the task brief, and this is the one pub trait in the crate set that needed
  checking — `globalalloc_model::RawAllocator` is deliberately `unsafe pub
  trait` and open for implementation by design, which is correct: any
  `GlobalAlloc` needs to be pluggable).

### F7 — [Info] Version/dependency wiring is internally consistent

- **File:** root `Cargo.toml:327,348,357,365,374,378`;
  `crates/numa/Cargo.toml:24`; `crates/proc-probe/Cargo.toml:24`.
- **Type:** N/A — confirmation.
- **Impact:** Every intra-workspace `path = "..."` dependency also carries a
  matching `version = "..."` (`sefer-region` 0.1, `aligned-vmem` 0.2,
  `racy-ptr-cell` 0.1, `size-classes` 0.1, `tagged-index-stack` 0.1,
  `numa-shim` 0.1, `numa-shim`→`aligned-vmem` 0.2, `proc-probe`→`proc-memstat`
  0.1) — the path+version pairing crates.io requires for a publishable crate
  that depends on a workspace sibling. `rust-version = "1.88"` and
  `edition = "2021"` are identical across the root crate and all 11 member
  crates (verified by direct `grep` across every `Cargo.toml`) — no MSRV
  drift. `aligned-vmem`'s 0.2 bump (from a prior 0.1) is the one sanctioned
  version bump on record (per `CLAUDE.md`'s "vmem 0.2 санкционирован" and the
  crate's own changelog note about `alloc-lazy-commit` being kept as a
  backward-compat alias for one release) — no other crate shows signs of an
  unsanctioned version bump.

### F8 — [Info] Feature-flag coherence in `aligned-vmem` — additive and correctly gated

- **File:** `crates/vmem/Cargo.toml:15-58`; `crates/vmem/src/lib.rs:75-102`.
- **Type:** N/A — confirmation.
- **Impact:** `lazy-commit`, `huge-pages`, `mock`, `fault-injection` are all
  independently off-by-default and additive; `alloc-lazy-commit` is
  explicitly a deprecated-but-kept alias forwarding to `lazy-commit` (one
  release, documented). The one cross-feature interaction —
  `fault-injection` without `lazy-commit` compiling the hook module but never
  reaching it — is correctly handled with a scoped
  `#[cfg_attr(all(feature = "fault-injection", not(feature = "lazy-commit")),
  allow(dead_code))]` (`crates/vmem/src/lib.rs:87-90`) rather than a blanket
  `allow(dead_code)`, so genuine dead code elsewhere in the crate would still
  be caught by `--all-features` clippy. No mutually-exclusive feature pairs
  were found across any of the 11 crates' feature tables.

### F9 — [Info] `numa-shim`'s `mock` feature-gated pub statics are correctly scoped

- **File:** `crates/numa/src/lib.rs:66-121`.
- **Type:** N/A — confirmation.
- **Impact:** `pub mod mock` (with `pub static CALLS`/`CURRENT_NODE_SLOT`
  thread-locals and `pub fn drain`/`set_current_node`) is entirely behind
  `#[cfg(feature = "mock")]`, off by default — this is a deliberate,
  documented test-recording API (mirrored by `aligned-vmem`'s own `mock`
  module, `crates/vmem/src/mock.rs`), not an accidental internal-detail leak.
  Confirmed as intentional, not a finding.

---

## Publish-readiness summary table

| Crate | Version | `Cargo.toml` metadata complete | README/LICENSE present | Root README index | Feature coherence | Semver posture | Notes |
|---|---|---|---|---|---|---|---|
| `sefer-region` | 0.1.0 | yes | yes | yes (both tables) | `std` only, clean | good — no pub fields | `#![forbid(unsafe_code)]`; clean re-export-only `lib.rs` |
| `aligned-vmem` | 0.2.0 | yes | yes | yes (both tables) | 4 additive features, correctly gated (F8) | good — `Reservation` fields private, `VmemError` opaque; `mock::Call` lacks `#[non_exhaustive]` (F3) | sanctioned 0.2 bump; `alloc-lazy-commit` alias documented |
| `numa-shim` | 0.1.0 | yes | yes | yes (both tables) | `vmem-integration`, `mock` additive | good; `mock::MockCall` lacks `#[non_exhaustive]` (F3) | zero-dep default config preserved |
| `malloc-bench-rs` | 0.1.0 | yes | yes | yes (both tables) | none beyond default | good; `Config` pub-field by design, has `Default` (F4) | dev-only in root; correctly not a runtime dep |
| `racy-ptr-cell` | 0.1.0 | yes | yes | **missing** (F2) | `loom` cfg-gated dep only | good — single opaque struct, sealed state machine | not referenced in root README crate tables |
| `ring-mpsc` | 0.1.0 | yes | yes | **missing** (F2) | `loom` cfg-gated dep only | good — `RingStore` correctly sealed (F6); `Call`-equivalent N/A | **zero consumers in workspace** (F1); NO-GO documented in git history, not in README |
| `size-classes` | 0.1.0 | yes | yes | **missing** (F2) | none | good; `Params` pub-field by design, no `Default` (F4) | all struct internals private, only `Params` is a data-in struct |
| `globalalloc-model` | 0.1.0 | yes | yes | **missing** (F2) | `proptest`, `arbitrary` additive, independent | good; `Live`/`Config` pub-field by design (F4); `RawAllocator` intentionally open (F6) | dev-only; unifies 3 formerly-drifted in-tree copies |
| `tagged-index-stack` | 0.1.0 | yes | yes | **missing** (F2) | `loom` cfg-gated dep only | good — `TaggedIndex` is a zero-state namespace, `Links` trait intentionally open | — |
| `proc-memstat` | 0.1.0 | yes | yes | **missing** (F2) | none | good — `MemStat` has `Default` (F5, confirmed no gap) | FFI confined to platform submodules |
| `proc-probe` | 0.1.0 | yes | yes | **missing** (F2) | `std` default-on, additive | good — thin re-export + emit fns, no state | `#![forbid(unsafe_code)]`; depends on `proc-memstat` 0.1 |

**Overall:** no leaky-API or broken-dependency findings; the extraction
phase's crate hygiene is solid (sealed traits used correctly, `unsafe` seams
correctly confined and documented, `#![forbid]`/`#![allow(unsafe_code)]`
applied consistently with the project's own convention, MSRV/edition
uniform, path+version dependency pairing correct throughout). The two
actionable items are both **documentation-only**: extend the root README's
crate tables to all 11 crates (F2), and add the already-proposed
`ring-mpsc` NO-GO annotation (F1, cross-referenced from
`03-compliance-conventions.md`). F3 is a cheap pre-1.0 `#[non_exhaustive]`
addition with no current cost.
