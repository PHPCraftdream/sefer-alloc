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

- **BREAKING: `current_node()` is fail-closed on undetermined Linux topologies** — task #1308 (F1, the only P1, of the fifteenth independent review, `docs/reviews/2026-08-24-170047-numa-shim-publication-audit-Sol-codex.md`). Previously every undetermined case — sysfs unreadable, CPU on a node >= 64, no NUMA sysfs at all — collapsed into `Some(0)`, indistinguishable from a genuinely resolved node 0; combined with task #1306's strict `reserve_preferred_on_node`, a caller could successfully install a NUMA preference for the WRONG node. Now `cpu_to_numa_node` maps lookup failure to `NO_NODE` — both failure paths (`sched_getcpu(2)` failure and topology-lookup failure) converge on the same sentinel — and `current_node()` returns `None` for it; `Some(0)` occurs only for a CPU genuinely resolved to node 0. Granular failure reason: `current_node_resolution()`.
- **BREAKING: `NodeResolution::FellBackToZero` renamed to `NodeResolution::TopologyUnavailable`** — task #1308. The old name described a node-0 fallback that no longer exists once `current_node()` fails closed; the new name states what actually happens. Both `TopologyUnavailable` and `Unavailable` map to `None`; their distinction is diagnostic ("platform has a NUMA API and detection ran, but this CPU could not be resolved" vs "no NUMA API / the OS call itself failed"). The variant was introduced in this same unreleased cycle (task #1266), so the net 0.1.0 -> next diff contains only the new name.

### Fixed

- **`current_node()` silently collapsed an undeterminable node into `Some(0)`** — task #1308 (finding F1 of the fifteenth review): unreadable sysfs, a CPU on a node >= 64, or a kernel with no NUMA sysfs at all produced the same `Some(0)` as a genuinely node-0-resolved CPU, so a caller could install a NUMA preference for the wrong node with an `Ok` result (worse after task #1306 made `reserve_preferred_on_node` actually install `MPOL_PREFERRED` and check the result). Fixed by the fail-closed remap under `### Changed`.

- **`cpu_to_numa_node_checked` O(nodes × bytes) per-lookup re-parse and false per-node cpumap buffer rationale** — task #1310 (review findings F5 and F10 from `docs/reviews/2026-08-24-170047-numa-shim-publication-audit-Sol-codex.md`). F5: the old `NODE_CPUMAP_BUF_LEN = 1024` comment justified the buffer as covering "~3640 CPUs on a SINGLE node," but a Linux node cpumap is the GLOBAL cpumask `cpumask_of_node(node) & cpu_online_mask` — bit indices are global logical CPU IDs, so the buffer bounds global CPU-ID space, not per-node CPU count. The false rationale hid a real defect: on many-node/sparse-ID systems ALL nodes' cpumaps can simultaneously exceed a small buffer, silently dropping the whole topology into the node-0 fallback. F10: after the first call, each `cpu_to_numa_node_checked` lookup re-parsed up to ~64 KiB of cached raw text — O(nodes × cpumap-bytes) per lookup. The fix replaces the per-node raw-text cache (`[[u8; 1024]; 64]`, ~64.5 KiB) with an allocation-free reverse index `cpu -> node` (`[u8; 8192]`, 8 KiB) built ONCE inside the same `OnceLock` initializer by parsing each node's file exactly once through the single `parse_each_set_cpu` interpreter (no second divergent parsing path); lookup is now a single array probe. `MAX_INDEXED_CPUS = 8192` derives from the kernel's per-arch `NR_CPUS` ceiling (x86_64 caps at 8192, arm64 at 4096 — the archs this crate supports), covering every possible set bit on supported kernels; CPU IDs beyond it degrade exactly like the old oversized-file case (unmapped → `FellBackToZero` via `cpu_to_numa_node_checked`'s `None`). Observable behavior is preserved: same `None`/`FellBackToZero`/`Some(0)` conditions, same lowest-node-wins for overlapping masks (first-mapping-wins under the real caller's ascending scan), same fail-closed per-node handling of malformed/oversized files, same `current_node()`/`current_node_resolution()` signatures and semantics. One internal nuance: `parse_contains_cpu` (doc-hidden, semver-exempt) now fails closed when ANY token is malformed, not just the target word's. No wall-clock performance claim (unmeasured); this records complexity/footprint facts only, per this repo's commit-prefix/evidence discipline.
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
- **`NodeId`** (task #1306) — newtype over `u32` for NUMA node identifiers
  in the reservation/policy API (`NodeId::new(u32)` unchecked constructor,
  `NodeId::get() -> u32` accessor). Detection APIs keep returning
  `Option<u32>`; the ergonomic path is `current_node().map(NodeId::new)`.
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
