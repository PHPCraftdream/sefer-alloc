# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

Changes since the 0.1.0 publish of 2026-06-29. This section's heading is
deliberately version-neutral: the next version number is an owner decision
not yet made, and this repository does not bump versions without an explicit
request. At release time, consolidate this section under a dated
`## <version> - <date>` heading.

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
- The sysfs cpumap parser was extracted into a target-independent module with
  real behavioral oracles runnable on every host, not only real Linux (task
  #721) — test infrastructure (`#[doc(hidden)]`), not public API.

### Changed

- `aligned-vmem` optional dependency bumped from `0.1` to `0.2`
  (the sibling crate's own 0.2 release); `reserve_on_node`'s return type
  moves with it.

### Owner decisions pending

- **Semver policy for the two `#[doc(hidden)]` test-only modules** —
  `pub mod cpumap` (parser helpers) and `pub mod linux`
  (`dbg_node_resolution_for_cpu`); audit finding F5, scope-expanded by a
  later zero-trust review to cover both. Recommendation recorded (option
  (c): commit both to the published surface at the next release, with
  `cpumap` promoted to documented API); **owner decision pending, nothing
  implemented**. Neither module is in published 0.1.0 — the next publish is
  what freezes them as public API, so the decision must land in that
  release's scope. Full writeup: task #1267 addendum on item 100 in
  `docs/correctness-open-items/TRACKED_publish_readiness.md` (task #1267).

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
