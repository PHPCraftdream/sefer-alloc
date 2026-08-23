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

Enables [`reserve_on_node`], which reserves aligned anonymous virtual memory
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

### `mock`

Test-only: replaces the real platform NUMA syscalls with a recording stub
(`numa_shim::mock`) so CI can assert the wrapping logic on any target,
including macOS and miri, where no real NUMA API exists.

**⚠ Cargo feature-unification hazard (task #726):** `mock` is a
non-additive, backend-REPLACING feature, and Cargo unifies features across a
build's whole dependency graph, not per edge. If ANY crate anywhere
downstream of your build enables `numa-shim/mock` — including a sibling
workspace member's own `[dev-dependencies]` — every other consumer in that
same build silently gets the mock backend too, with no compile error and no
warning. Enable this feature only in a leaf test/dev target — never in a
library's own `[dependencies]`, and never in a shared `[dev-dependencies]`
entry reachable from more than one build target in the same workspace. See
the `mock` feature's own doc comment in `Cargo.toml` and the `mock` module's
own rustdoc for the full reasoning (matching `aligned-vmem`'s identical
`mock` feature, task #715).

## Public API

```rust
/// Sentinel: no NUMA node / unsupported platform.
pub const NO_NODE: u32 = u32::MAX;

/// NUMA node of the calling thread, or None if unavailable.
pub fn current_node() -> Option<u32>;

/// Bind [base, base+len) to a NUMA node (Linux: mbind; others: no-op).
/// # Safety
/// [base, base+len) must be a valid OS reservation owned by the caller.
pub unsafe fn bind_range(base: *mut u8, len: usize, node: u32);

/// Reserve aligned anonymous memory with NUMA preference (feature = "vmem-integration").
#[cfg(feature = "vmem-integration")]
pub fn reserve_on_node(size: usize, align: usize, node: u32) -> Option<aligned_vmem::Reservation>;
```

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
