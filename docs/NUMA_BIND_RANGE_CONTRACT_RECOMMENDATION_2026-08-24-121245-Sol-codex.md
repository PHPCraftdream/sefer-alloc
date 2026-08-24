# `bind_range` contract recommendation

**Author:** Sol-codex

**Written:** 2026-08-24 12:12:45 (Europe/Berlin)

**Scope:** owner decision for [`NUMA_BIND_RANGE_CONTRACT_DECISION_BRIEF.md`](NUMA_BIND_RANGE_CONTRACT_DECISION_BRIEF.md)

## Verdict

Do not choose any of the four options currently listed in the brief. The best
design is a fifth option: remove the public `bind_range` API in its current
object-range form and rebuild the surface around NUMA policy for complete OS
mappings.

The recommended release API is a truthful, fallible, cross-platform operation
that reserves virtual memory with a preferred NUMA node. A low-level operation
for changing policy after reservation should be added only if there is a real
consumer for it; if added, it must accept an exact, runtime-validated page
range, expose the treatment of existing pages, and return `Result`.

## Why `(b) + (c)` is still not correct

### 1. A page envelope does not fix the `Vec` example

The README constructs the buffer with `vec![0; 4096]`. That initialization has
already written to the pages and therefore faulted them in before `mbind` is
called.

By default, `mbind(..., flags = 0)` affects only future physical-page
allocations. Pages touched before the call remain where they were allocated.
Moving existing pages requires `MPOL_MF_MOVE` or `MPOL_MF_MOVE_ALL`. This is
part of the documented [`mbind(2)` contract](https://man7.org/linux/man-pages/man2/mbind.2.html).

Consequently, a successful syscall or a status named `Bound` would still be a
false promise for that example: the VMA policy may have been installed while
the physical pages of the `Vec` remain on their original nodes.

### 2. A byte range is the wrong abstraction

Linux applies NUMA policy to pages and VMAs, not to Rust objects. Expanding an
arbitrary object range to a page envelope may:

- change the policy of neighboring allocator objects;
- leave that policy behind after the `Vec` is freed while its allocator arena
  remains mapped;
- split an existing VMA into two or three VMAs;
- increase pressure on `vm.max_map_count` when repeated for many small objects.

The Linux kernel documentation explicitly describes VMA splitting when a
policy is installed on a subrange of an existing mapping:
[`NUMA Memory Policy`](https://www.kernel.org/doc/html/latest/admin-guide/mm/numa_memory_policy.html).

This behavior does not create Rust memory unsafety, but it is not harmless: it
can create persistent placement surprises and kernel-resource costs.

### 3. `bind_range` misnames `MPOL_PREFERRED`

The implementation uses `MPOL_PREFERRED`, which is a soft preference with
fallback under memory pressure. It is not strict binding. Even a successful
syscall does not guarantee that pages are physically located on the requested
node.

The operation should therefore use names such as `preferred`, `policy`, or
`placement`, not `bind` or a success value named `Bound`.

### 4. A status enum is weaker than `Result`

`Bound / Skipped / Unsupported / Failed(errno)` mixes success, absence of a
request, unsupported capability, and syscall failure into one status type.
Only a successfully installed policy is success. Unsupported targets and
failed requests are errors.

`Result` already has `#[must_use]` semantics and composes naturally with the
reservation cleanup path.

### 5. `unsafe` does not encode the real contract

The function does not dereference `base` or access payload bytes. A dangling,
unmapped, null, or misaligned address is validated by the kernel and produces a
syscall error rather than Rust undefined behavior. Page alignment and mapped
range validity should therefore be runtime-checked conditions, not `# Safety`
preconditions.

The internal FFI call remains inside an `unsafe` block, but the public policy
operation itself can be safe.

## Recommended primary API

The cross-platform headline API should be the reservation operation:

```rust
pub struct NodeId(u32);

pub fn reserve_preferred_on_node(
    size: usize,
    align: usize,
    node: NodeId,
) -> Result<Reservation, ReserveNumaError>;
```

Required semantics:

- Linux: reserve the mapping, install `MPOL_PREFERRED` before first touch, and
  return the reservation only after the policy syscall succeeds.
- Windows: use `VirtualAllocExNuma` and surface the Win32 failure.
- Unsupported platform or architecture: return an explicit error.
- Invalid size/alignment: return an explicit argument error.
- NUMA policy failure after reservation: drop the still-owned reservation and
  return the policy error.
- No silent fallback to an ordinary reservation.

Best-effort fallback belongs at the caller, where the compromise is visible:

```rust
reserve_preferred_on_node(size, align, node)
    .or_else(|_| reserve_aligned(size, align))
```

On Linux, apply the policy to the complete underlying OS reservation
(`reservation_ptr()` plus `reservation_len()`), not merely to the aligned
usable view (`as_ptr()` plus `len()`). This aligns policy lifetime with mapping
lifetime and avoids splitting the mapping around alignment slack.

## Optional low-level Linux API

If a concrete post-reservation use case exists, expose an exact page-policy API
instead of resurrecting `bind_range`:

```rust
pub struct PageRange { /* runtime-validated */ }

pub enum ExistingPages {
    FutureFaultsOnly,
    Move,
    MoveStrict,
}

pub fn set_preferred_policy(
    range: PageRange,
    node: NodeId,
    existing: ExistingPages,
) -> Result<(), PolicyError>;
```

`PageRange::new` must check in release builds:

- nonzero length;
- base aligned to the runtime page size;
- length a multiple of the runtime page size;
- no overflow in `base + len`.

It must not automatically expand a byte range to a page envelope. A caller
requesting a page policy must explicitly possess and name the complete pages
whose policy will change.

The existing-page modes map to Linux behavior as follows:

- `FutureFaultsOnly`: flags `0`;
- `Move`: `MPOL_MF_MOVE`;
- `MoveStrict`: `MPOL_MF_MOVE | MPOL_MF_STRICT`.

`MPOL_MF_MOVE_ALL` should be a separate, conspicuously privileged operation
because it can move pages shared with other processes and requires
`CAP_SYS_NICE`.

A successful result means only that the requested policy operation succeeded.
It must not be documented as proof that all pages reside on the requested node
unless strict migration was requested and the kernel confirmed it.

If there is no current external consumer for this low-level operation, omit it
from the release. A small, correct public surface is preferable to a speculative
raw-address API.

## Error model

A suitable shape is:

```rust
#[non_exhaustive]
pub enum PolicyError {
    UnsupportedPlatform,
    UnsupportedArchitecture,
    InvalidRange(RangeError),
    InvalidNode,
    Os(std::io::Error),
}
```

The `mbind` return value must be checked, and `errno` must be captured
immediately after a failing `syscall` before any other FFI or library call can
overwrite it.

## Related breaking changes worth taking now

Because compatibility is explicitly not a constraint:

- remove `NO_NODE` from policy and reservation arguments;
- replace raw node `u32` values with `NodeId`;
- do not fabricate node 0 when topology resolution fails;
- remove the silent `node >= 64` branch;
- either support a sufficiently wide Linux nodemask or return an explicit
  `NodeOutOfRange` error for a documented implementation limit;
- return an error on Linux architectures without a known `SYS_MBIND` number;
- change `reserve_on_node -> Option<_>` to a descriptive `Result`;
- remove the `Vec` example from the README;
- remove or redesign the currently non-production `bind_segment` seam.

## Recommended owner decision text

> Remove `bind_range`. `numa-shim` will expose an observable operation for
> reserving a complete OS mapping with a preferred NUMA node. If a low-level
> policy-changing API is later required, it will accept only a runtime-checked
> exact page range, explicitly select the handling of existing pages, and
> return `Result`. Arbitrary object ranges will not be silently expanded to
> page envelopes. Unsupported targets, invalid nodes, and OS failures will not
> be represented as successful no-ops.

## Final recommendation

The optimal release decision is therefore:

1. Delete the current public `bind_range` API and its `Vec` use case.
2. Rename reservation semantics around **preferred placement**, matching
   `MPOL_PREFERRED` and `VirtualAllocExNuma`.
3. Make NUMA reservation fallible and observable with `Result`.
4. Apply Linux policy to the entire owned OS mapping before first touch.
5. Add a post-reservation page-policy API only when a concrete consumer proves
   it is needed.

This removes the confirmed `EINVAL` bug without replacing it with a more subtle
semantic trap.
