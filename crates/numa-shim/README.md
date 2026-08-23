# numa-shim

**100 % Rust NUMA detection and binding — no C / C++ libraries.**

The key differentiator: **zero C / C++ crate dependencies** — no `libnuma`, no
`hwloc`, no `libcuda`. Only the system libc / `kernel32` syscalls that any
Rust program already links to.

| Platform | Node detection | Memory binding |
|----------|---------------|----------------|
| Linux x86_64 / aarch64 | `sched_getcpu` + sysfs `/sys/devices/system/node/nodeN/cpumap` | `mbind(2)` via raw `syscall(2)` — **no libnuma, no hwloc** |
| Windows | `GetCurrentProcessorNumberEx` + `GetNumaProcessorNodeEx` | `VirtualAllocExNuma` (via `vmem-integration` feature) |
| macOS | not available (no public NUMA API) | no-op |
| miri | not available | no-op |

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

# Optional: enables reserve_on_node() which wraps aligned-vmem
# numa-shim = { version = "0.1", features = ["vmem-integration"] }
```

```rust
use numa_shim::{current_node, bind_range, NO_NODE};

// Detect the current thread's NUMA node.
match current_node() {
    Some(node) => println!("on NUMA node {node}"),
    None       => println!("NUMA unavailable"),
}

// Bind a live allocation to a NUMA node (Linux: mbind; Windows/macOS: no-op).
let mut buf = vec![0u8; 4096];
let node = current_node().unwrap_or(0);
// SAFETY: `buf` is a live allocation owned by this scope.
unsafe { bind_range(buf.as_mut_ptr(), buf.len(), node) };
```

## Feature flags

### `vmem-integration`

Enables `reserve_on_node`, which reserves aligned anonymous virtual memory
with a NUMA preference using [`aligned-vmem`](https://crates.io/crates/aligned-vmem):

```toml
[dependencies]
numa-shim = { version = "0.1", features = ["vmem-integration"] }
# `reserve_on_node` returns an `aligned_vmem::Reservation`, so you need the
# crate as a DIRECT dependency to name that type / its constants in your own
# code — enabling a feature on numa-shim alone does NOT put `aligned_vmem`
# in your crate's extern prelude (it is only an optional transitive dep).
aligned-vmem = "0.2"
```

```rust
use numa_shim::{reserve_on_node, current_node};
use aligned_vmem::{page_size, PAGE};

let node = current_node().unwrap_or(0);
// `page_size()` is the OS's actual runtime page size; `PAGE` (4 KiB) is the
// compile-time minimum. Prefer `page_size()` when alignment must match what
// the kernel actually uses.
let ps = page_size();
let r = reserve_on_node(ps * 16, PAGE.max(ps), node).expect("OOM");
// r is an `aligned_vmem::Reservation` — RAII, drops cleanly.
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
/// Sentinel: no NUMA node / unsupported platform.
pub const NO_NODE: u32 = u32::MAX;

/// NUMA node of the calling thread, or None if unavailable.
pub fn current_node() -> Option<u32>;

/// Outcome of a NUMA-node determination attempt for the calling thread:
/// Resolved(n) — CPU genuinely resolved to node n via the platform
/// topology; FellBackToZero (Linux only) — CPU index obtained but not in
/// any cached sysfs cpumap (unreadable topology, node >= 64, or no NUMA
/// sysfs at all), which current_node() collapses to Some(0); Unavailable —
/// no NUMA API on this platform or the OS API failed (current_node()
/// returns None).
#[non_exhaustive]
pub enum NodeResolution { Resolved(u32), FellBackToZero, Unavailable }

/// Additive alternative to current_node(): same resolution logic, but
/// distinguishes the Linux node-0 fallback from a genuinely resolved
/// node — use it when a fallback warning / diagnostic logging matters.
pub fn current_node_resolution() -> NodeResolution;

/// Bind [base, base+len) to a NUMA node (Linux: mbind; others: no-op).
/// # Safety
/// [base, base+len) must be a valid MAPPED RANGE (OS reservation, heap
/// allocation, or any other live mapping) owned exclusively by the caller.
/// `mbind(MPOL_PREFERRED)` applies at PAGE granularity, so neighboring data
/// sharing a page is affected too. When `node == NO_NODE` or `len == 0`, the
/// function returns immediately without touching `base` (any address permitted).
pub unsafe fn bind_range(base: *mut u8, len: usize, node: u32);

/// Reserve aligned anonymous memory with NUMA preference (feature = "vmem-integration").
#[cfg(feature = "vmem-integration")]
pub fn reserve_on_node(size: usize, align: usize, node: u32) -> Option<aligned_vmem::Reservation>;
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

On other Linux architectures `bind_range` is a documented no-op (the syscall
number is unknown; contributions welcome).

**`node >= 64` is silently skipped** (task #722): the Linux nodemask is a
single `u64`, so only node IDs 0..63 can be addressed, even though
`mbind(2)` itself supports node counts up to `MAX_NUMNODES` (commonly 1024
on real kernels).

## MSRV

Rust 1.88

## License

MIT OR Apache-2.0
