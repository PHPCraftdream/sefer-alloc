# numa-shim

**100 % Rust NUMA detection and binding — no C / C++ libraries.**

The key differentiator: **zero C / C++ crate dependencies** — no `libnuma`, no
`hwloc`, no `libcuda`. Only the system libc / `kernel32` syscalls that any
Rust program already links to.

| Platform | Node detection | Memory binding |
|----------|---------------|----------------|
| Linux x86_64 / aarch64 | `sched_getcpu` + sysfs `/sys/devices/system/node/nodeN/cpumap` | `mbind(2)` via raw `syscall(2)` — **no libnuma, no hwloc** |
| Windows | `GetCurrentProcessorNumberEx` + `GetNumaProcessorNodeEx` | `VirtualAllocExNuma` (via `vmem-integration` feature) |
| macOS | not available (no public NUMA API) | not supported — `Err(UnsupportedPlatform)` |
| miri | not available | not supported — `Err(UnsupportedPlatform)` |

## Why yet another NUMA crate?

Most Rust NUMA crates link to `libnuma` or `hwloc`, pulling in heavy C
dependencies that complicate cross-compilation and static linking. `numa-shim`
calls the kernel directly:

- Linux: `mbind(2)` via `syscall(number, ...)` — the number is baked in as a
  constant (`SYS_MBIND = 237` on x86_64, `235` on aarch64). No `libnuma`
  symbol needed; `syscall(2)` is always present in glibc and musl.
- Linux node detection: reads `/sys/devices/system/node/nodeN/cpumap` via
  POSIX `open`/`read`/`close` with no heap allocation (stack buffer only).
- Windows: Win32 APIs from `kernel32.dll` — always linked, no extra import lib.

## Usage

```toml
[dependencies]
numa-shim = "0.1"

# Optional: enables reserve_preferred_on_node() which wraps aligned-vmem
# numa-shim = { version = "0.1", features = ["vmem-integration"] }
```

```rust
use numa_shim::current_node;

// Detect the current thread's NUMA node.
match current_node() {
    Some(node) => println!("on NUMA node {node}"),
    None       => println!("NUMA unavailable"),
}
```

There is deliberately **no "bind an existing allocation" API**. Linux
`mbind(2)` with default flags only affects *future* page faults, and requires
a page-aligned address — so binding an already-touched object (e.g. a heap
`Vec`) would be a silent no-op lie. NUMA placement must be requested at
**reservation time**, before the first touch — see
[`reserve_preferred_on_node`](#vmem-integration) below.

## Feature flags

### `vmem-integration`

Enables `reserve_preferred_on_node`, which reserves aligned anonymous virtual memory
with a NUMA preference using [`aligned-vmem`](https://crates.io/crates/aligned-vmem):

```toml
[dependencies]
numa-shim = { version = "0.1", features = ["vmem-integration"] }
# `reserve_preferred_on_node` returns an `aligned_vmem::Reservation`, so you need the
# crate as a DIRECT dependency to name that type / its constants in your own
# code — enabling a feature on numa-shim alone does NOT put `aligned_vmem`
# in your crate's extern prelude (it is only an optional transitive dep).
aligned-vmem = "0.2"
```

```rust
use numa_shim::{current_node, NodeId, ReserveNumaError, reserve_preferred_on_node};
use aligned_vmem::{page_size, PAGE};

// Reserve fresh memory with a NUMA preference installed BEFORE the first
// page fault — the only point where "successfully bound" is a true
// statement. `NodeId::new` rejects only the `NO_NODE` sentinel (u32::MAX):
// an id the platform cannot address still constructs and surfaces as
// `Err(ReserveNumaError::InvalidNode)` (Linux nodemask limit) or
// `Err(ReserveNumaError::Os(..))` (Windows forwards any id to the OS).
// `None` from `current_node()` means undetermined topology -> no NUMA
// preference (task #1308; `Some(0)` only ever means genuinely-resolved node 0).
let ps = page_size();
let r = match current_node() {
    Some(node) => {
        // current_node() remaps the sentinel to None, so `node` here is
        // never NO_NODE and NodeId::new cannot fail.
        reserve_preferred_on_node(ps * 16, PAGE.max(ps), NodeId::new(node).expect("never NO_NODE"))
            .expect("NUMA-preferred reservation failed")
    }
    None => {
        // No NUMA preference — plain aligned reservation.
        aligned_vmem::reserve_aligned(ps * 16, PAGE.max(ps)).expect("OOM")
    }
};

// Best-effort fallback with more detailed error handling:
let r = match current_node() {
    Some(node) => {
        // As above: `node` is never the sentinel here.
        match reserve_preferred_on_node(
            ps * 16,
            PAGE.max(ps),
            NodeId::new(node).expect("never NO_NODE"),
        ) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "NUMA preference on node {} failed ({}); using an unbound reservation",
                    node, e
                );
                aligned_vmem::reserve_aligned(ps * 16, PAGE.max(ps)).expect("OOM")
            }
        }
    }
    None => {
        aligned_vmem::reserve_aligned(ps * 16, PAGE.max(ps)).expect("OOM")
    }
};
```

Without this feature, `numa-shim` has **zero runtime dependencies**.

### `mock` (build-time cfg flag)

Test-only: replaces the real platform NUMA syscalls with a recording stub
(`numa_shim::mock`) so CI can assert the wrapping logic on any target,
including macOS and miri, where no real NUMA API exists. Enabled by the
build-time cfg flag `numa_shim_mock` via `RUSTFLAGS="--cfg numa_shim_mock"`
(task #1288, mirroring aligned-vmem's task #962), NOT a Cargo feature.

**Resolution:** This used to be a Cargo feature whose unification hazard was
documented as an open risk (task #726). Resolved 2026-08-23 (task #1288) by
converting it to the build-time `--cfg numa_shim_mock` flag. The cfg still
applies build-graph-wide once set — what changed is WHO can set it: only the
top-level build invoker via an explicit RUSTFLAGS/build-script choice, never a
transitive dependency through Cargo's additive feature-unification, and never
`--all-features`/docs.rs/`cargo add` by accident. Migration for 0.1.0
`--features mock` consumers: see the CHANGELOG's "Removed" section.

## Public API

```rust
/// Sentinel: no NUMA node / unsupported platform (detection-side interop only; the reservation API takes `NodeId`, never the sentinel).
pub const NO_NODE: u32 = u32::MAX;

/// NUMA node of the calling thread, or None if undeterminable (no NUMA API,
/// OS failure, or — on Linux — topology could not resolve this CPU).
pub fn current_node() -> Option<u32>;

/// Outcome of a NUMA-node determination attempt for the calling thread:
/// Resolved(n) — CPU genuinely resolved to node n via the platform
/// topology; TopologyUnavailable (Linux only) — CPU index obtained but not in
/// any cached sysfs cpumap (unreadable topology, node >= 64, or no NUMA
/// sysfs at all), which current_node() maps to None; Unavailable — no NUMA
/// API on this platform or the OS API failed (current_node() returns None).
/// Fail-closed behavior: both non-Resolved variants map to None (task #1308).
#[non_exhaustive]
pub enum NodeResolution { Resolved(u32), TopologyUnavailable, Unavailable }

/// Additive alternative to current_node(): same resolution logic, but
/// distinguishes WHY detection failed (diagnostics) — both non-Resolved
/// outcomes map to None, not a way to recover a node-0 answer.
pub fn current_node_resolution() -> NodeResolution;

/// NUMA node identifier for the reservation/policy API.
/// `NodeId::new(u32) -> Option<NodeId>` rejects only the `NO_NODE`
/// sentinel; platform-specific node validity surfaces as a typed error
/// from the fallible reservation API.
pub struct NodeId(u32);

/// Failure cause of a NUMA-preferred reservation attempt.
#[non_exhaustive]
pub enum ReserveNumaError {
    UnsupportedPlatform,
    UnsupportedArchitecture,
    InvalidArguments,
    InvalidNode,
    Os(std::io::Error),
}

/// Reserve aligned anonymous memory with a preferred NUMA node, installing
/// the preference at reservation time — before the first page fault.
/// Linux: mbind(MPOL_PREFERRED) on the COMPLETE OS reservation span, return
/// value checked, reservation released on policy failure. Windows:
/// VirtualAllocExNuma. No silent fallback anywhere.
#[cfg(feature = "vmem-integration")]
pub fn reserve_preferred_on_node(
    size: usize,
    align: usize,
    node: NodeId,
) -> Result<aligned_vmem::Reservation, ReserveNumaError>;
```

The two `#[doc(hidden)]` test-only modules — `numa_shim::cpumap` and
`numa_shim::linux` — are not part of this surface: they are test oracles,
exempt from this crate's SemVer guarantees, and may change or be removed
in any release (including patch releases) without a deprecation period
(task #1289).

## Linux syscall numbers

| Architecture | `SYS_MBIND` |
|-------------|-------------|
| x86_64      | 237         |
| aarch64     | 235         |

On other Linux architectures `reserve_preferred_on_node` returns
`Err(ReserveNumaError::UnsupportedArchitecture)` — no silent skip (the
syscall number is unknown; contributions welcome).

**`node >= 64` returns `Err(ReserveNumaError::InvalidNode)`** (task #1306;
previously a silent skip): the Linux nodemask is a single `u64`, so only
node IDs 0..63 can be addressed, even though `mbind(2)` itself supports
node counts up to `MAX_NUMNODES` (commonly 1024 on real kernels).

## MSRV

Rust 1.88

## License

MIT OR Apache-2.0
