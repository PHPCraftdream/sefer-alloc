# NUMA on Windows dev hosts — Hyper-V virtual topology recipe

Phase 3 deliverable for the NUMA testing strategy (task #98 / Phase 3 of
[`NUMA_TESTING_OPTIONS.md`](NUMA_TESTING_OPTIONS.md)).

> **Status: draft / verification pending.** The PowerShell recipe below
> reflects the documented Hyper-V `Set-VMProcessor` API as of June 2026.
> It has **not yet been end-to-end verified** on a Hyper-V dev host
> running this workload — that requires a Windows Pro/Enterprise host
> with Hyper-V Gen2 support and ~1-2 hours of setup time. Run the recipe
> and update the "Verification log" section at the bottom with what
> happened. If `Get-WmiObject Win32_NumaNode | Measure` inside the guest
> reports `Count: 1`, Hyper-V refused the topology override and we fall
> back to Plan B (QEMU/KVM with a Windows guest — option D in
> `NUMA_TESTING_OPTIONS.md`).

## Goal

Validate the `VirtualAllocExNuma` code path on a single-socket Windows
dev box by giving a Hyper-V Gen2 Windows guest a **virtual** multi-node
NUMA topology. The host hardware does not change; Hyper-V tells the
guest "you have 2 NUMA nodes" via the processor compatibility settings,
and the guest's Windows kernel honors that boundary when serving
`VirtualAllocExNuma(node=1)`.

## Prerequisites

- Windows 10/11 Pro or Enterprise (Hyper-V Home is NOT supported for
  Gen2 VMs with virtual-NUMA control).
- Hyper-V role installed (`Enable-WindowsOptionalFeature -Online
  -FeatureName Microsoft-Hyper-V -All` then reboot).
- Windows 11 Dev VM image (free, signed; download from Microsoft's
  developer site). Saves ~30 minutes vs installing from scratch.
- ≥8 GiB of free RAM on the host (the guest will reserve 4 GiB).
- ≥30 GiB of free disk space.
- This repository cloned somewhere the guest can reach (shared folder
  or `git clone` from inside the guest).

## Step-by-step

### 1. Create the VM (PowerShell, run as Administrator)

```powershell
$VmName = "sefer-numa"
$VhdPath = "C:\Hyper-V\sefer-numa\sefer-numa.vhdx"
$WinDevVhd = "C:\Hyper-V\WinDev2406Eval\WinDev.vhdx"  # download from Microsoft

New-VM -Name $VmName `
    -MemoryStartupBytes 4GB `
    -Generation 2 `
    -VHDPath $VhdPath `
    -NewVHDSizeBytes 100GB
```

(If you already have a `WinDev*.vhdx`, point `-VHDPath` at a COPY of it
and skip `-NewVHDSizeBytes`.)

### 2. Apply virtual NUMA topology

This is the load-bearing step. `Set-VMProcessor` is the Hyper-V API that
configures the virtual NUMA layout the guest sees.

```powershell
# Disable migration compatibility (required for the override to apply on
# some CPU families — Hyper-V may quietly ignore the override otherwise).
Set-VMProcessor -VMName $VmName `
    -Count 4 `
    -MaximumCountPerNumaNode 2 `
    -MaximumCountPerNumaSocket 1 `
    -CompatibilityForMigrationEnabled $false

# Dynamic memory MUST be off — virtual NUMA requires static memory.
Set-VMMemory -VMName $VmName `
    -DynamicMemoryEnabled $false `
    -StartupBytes 4GB
```

**Why these specific values:**
- `Count 4`: 4 virtual CPUs in the guest.
- `MaximumCountPerNumaNode 2`: each virtual NUMA node holds at most 2
  vCPUs → 4 vCPUs / 2-per-node = 2 nodes.
- `MaximumCountPerNumaSocket 1`: each virtual socket has 1 NUMA node →
  combined with the above we end up with 2 sockets × 1 node × 2 vCPUs.
- `CompatibilityForMigrationEnabled $false`: lets Hyper-V expose the
  *actual* host CPU features (without this, Hyper-V applies a lowest-
  common-denominator mask that on some Ryzen/Atom CPUs masks away NUMA
  topology control too).
- Static memory: virtual NUMA is incompatible with Dynamic Memory.

### 3. Boot and verify the topology

```powershell
Start-VM -Name $VmName
vmconnect.exe localhost $VmName
```

Inside the guest, in PowerShell **as Administrator**:

```powershell
Get-WmiObject -Class Win32_NumaNode | Measure-Object | Select-Object Count
# Expected: Count : 2
#
# If you see "Count : 1", Hyper-V refused the topology override.
# See "Troubleshooting" below.

# Optional: show per-node CPU mapping.
Get-WmiObject -Class Win32_NumaNode | Format-Table NodeID, *Address* -AutoSize
```

### 4. Install Rust + clone the repo inside the guest

```powershell
# Rust via rustup (writes to %USERPROFILE%\.rustup):
Invoke-WebRequest -Uri https://win.rustup.rs -OutFile rustup-init.exe
.\rustup-init.exe -y --default-toolchain stable
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"

# Repo: either shared folder, or:
git clone https://github.com/PHPCraftdream/sefer-alloc.git
cd sefer-alloc
```

### 5. Run the NUMA test suite

```powershell
# Build + run env-guarded tests (the guard activates the multi-node paths).
$env:SEFER_NUMA_TEST = "1"

cargo test --features "production numa-aware" --test numa_alloc -- --nocapture
cargo test --features "production numa-aware" --test numa_segment_id -- --nocapture
cargo test --features "production numa-aware" --test numa_seam -- --nocapture
```

What to look for in the output:
- `numa::current_node()` should occasionally report node 1 (not just 0).
- Allocations that pass `node=1` to `VirtualAllocExNuma` should succeed
  AND have their pages backed by what the kernel considers node 1.
- All tests should pass with `0 failed`.

To prove `VirtualAllocExNuma` actually steered the allocation, dump the
process VAD inside the guest (advanced; uses Sysinternals' `VMMap`):

```powershell
# Allocate via test seam, then inspect:
vmmap.exe -p $PID
# Look for the SeferMalloc-owned large segments — VMMap shows the NUMA
# node each segment is bound to. Should match what the test requested.
```

## Troubleshooting

### `Get-WmiObject Win32_NumaNode` reports `Count: 1` inside the guest

Hyper-V silently refused the topology override. Known causes:

1. **`CompatibilityForMigrationEnabled` is still on.** Check with
   `Get-VMProcessor -VMName $VmName | Format-List Compat*` — must be
   `False`. If it is `True`, the host enforces a CPU feature mask that
   may strip the NUMA topology bits.

2. **Dynamic Memory is on.** Run
   `Get-VMMemory -VMName $VmName | Format-List Dynamic*` — must be
   `False`. Even one `True` flag disables virtual NUMA.

3. **CPU family limitation.** Some Atom, low-end Ryzen, and ARM-based
   Windows hosts simply do not expose virtual-NUMA control. There is
   no fix on these CPUs — see Plan B below.

4. **Generation 1 VM.** Virtual NUMA control requires Gen2.
   `Get-VM -VMName $VmName | Format-List Generation` must be `2`.

If none of the above applies and the guest still shows `Count: 1`, fall
back to **Plan B** documented in
[`NUMA_TESTING_OPTIONS.md`](NUMA_TESTING_OPTIONS.md) (option D — QEMU/KVM
with a Windows guest under WSL2 + KVM on the Linux side).

### Tests pass but `VirtualAllocExNuma(node=1)` shows pages on node 0

The kernel honors the request as a *preference*, not a hard binding.
On a single-socket host with virtual NUMA, the kernel may still place
all physical pages on node 0 because that is where the actual RAM lives.
This is correct behavior — the allocator API was called correctly, the
kernel chose to place differently. The test should still pass: it
verifies that the API was *invoked* with the right arguments, not the
final physical placement (which is the host's prerogative).

To verify physical placement, you would need a real multi-socket host
(Phase 4 of `NUMA_TESTING_OPTIONS.md` — cloud VM recipe).

## Verification log

> Fill this in after running the recipe end-to-end on a real Hyper-V
> dev host. Date / who / outcome / any deviations.

| Date | Host | Outcome | Notes |
|------|------|---------|-------|
| _pending_ | _e.g. Win11 Pro 23H2, Intel i7-12700K_ | _e.g. guest shows 2 nodes, tests pass_ | _e.g. needed `Disable-VMIntegrationService -Name "Heartbeat"` first_ |

If verification succeeds, update the table above and remove the "draft /
verification pending" banner at the top. If verification fails despite
following the troubleshooting steps, document the host + outcome and
either fall back to Plan B or file a follow-up task to investigate.

## Why this is "developer / release gate", not CI

GitHub Actions runners do not expose Hyper-V management to workflows, so
this recipe is run **manually** on a developer's Windows dev box as a
**release gate**: before any release that touches `crates/numa/src/lib.rs`
(the Windows `VirtualAllocExNuma` path), one maintainer with Hyper-V access
runs the recipe and reports the outcome. For per-PR CI coverage of the
Windows code path, see Phase 1 (`numa-shim-mock`) — that does run on every
CI run, on every supported target.

## Related

- [`NUMA_TESTING_OPTIONS.md`](NUMA_TESTING_OPTIONS.md) — the master Phase 1-4 plan
- [`PHASE_NUMA_DESIGN.md`](PHASE_NUMA_DESIGN.md) — the underlying NUMA design
- [`tests/numa_alloc.rs`](../tests/numa_alloc.rs) — env-guarded multi-NUMA tests
- Task #83 — Windows `VirtualAllocExNuma` direct path follow-up (separate code change in `numa-shim`)
