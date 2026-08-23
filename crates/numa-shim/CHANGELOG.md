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
  `docs/correctness-open-items/ACTIVE.md` item 42; task #1264): before the
  next release the owner must pick between (a) converting the seam to a
  build-time `--cfg` flag (semver-breaking, rides the already-breaking
  0.2.0), (b) a separate unpublished test-support crate, or (c) keeping the
  feature with explicit risk acceptance. Recommendation written, decision
  NOT yet made — see item 42's "RECOMMENDATION (2026-08-23, task #1264)"
  section. Resolve this heading before cutting the release section.
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

### Fixed

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
  candidate node) to populate a `OnceLock` cache; every subsequent call is a
  pure in-memory bit-test with no syscalls (task #723). The cache's
  initializer stores only static/stack data and performs no heap allocation,
  so calling `current_node()` from inside a `#[global_allocator]` that itself
  consults it cannot deadlock or re-enter `OnceLock::get_or_init` mid-init —
  the original heap-allocating initializer from #723 was replaced after that
  hazard was found (task #777).
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

### Removed

Three changes already in this tree are **semver-breaking against published
0.1.0's `--features mock` surface** and are recorded here as a group (task
#1274, finding N1 of the tenth independent review,
`docs/reviews/2026-08-23-183220-numa-shim-publication-readiness-review-oh.md`).
Two were made under `53b3ca2`'s stated premise "all decided now, before this
crate's first crates.io publish" — which was false: 0.1.0 was published
2026-06-29, six weeks earlier. Task #1263's premise correction covered only
the `mock` Cargo-feature decision (decision 5 of 5 in that commit), not
these API breaks — that remainder is recorded on
`docs/correctness-open-items/ACTIVE.md` item 42. Under Cargo's 0.x rules a
release containing any of the three cannot be `0.1.1`; which version this
becomes is the still-open F1 owner decision (task #1262) — this section
records what already broke, independent of that choice.

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
