# NUMA testing without multi-socket hardware

Research doc for task #89. Closes the practical question: **how do we keep
the NUMA code path (mbind on Linux, VirtualAllocExNuma on Windows) working
when neither developer nor CI has a multi-socket box?**

> TL;DR — three layers stacked: **(1) mock-shim unit tests** in CI for
> wrapping logic; **(2) QEMU `-numa` Linux runbook** (already documented,
> add a periodic CI job); **(3) Hyper-V virtual NUMA topology** for the
> Windows-`VirtualAllocExNuma` path. Real multi-socket Windows hardware
> stays a community-contributor gate before any release that touches the
> Windows NUMA seam.

---

## Why this matters

The NUMA-aware path is real production code: Linux `mbind(2)` via raw
syscall, Windows `VirtualAllocExNuma`, Linux sysfs cpumap reader,
`SegmentHeader::node_id` stamping, cross-thread free between segments on
different nodes. None of it executes on the typical single-socket dev box
(`current_node()` returns `0`, `bind_range` issues an `mbind` that the
kernel silently ignores because there's only one node, `VirtualAllocExNuma`
on `node=0` is indistinguishable from `VirtualAlloc`).

This is the classic "wrote the code, can't prove it works" problem. The
options below are ordered by **what they actually validate**, not by
sophistication.

---

## What we ALREADY validate today

| Layer | Mechanism | Status |
|---|---|---|
| Linux mbind path | QEMU `-numa` runbook in `tests/numa_alloc.rs` (env-guarded `SEFER_NUMA_TEST=1`) | ✅ documented, run manually pre-release |
| API plumbing | `tests/numa_seam.rs` — `current_node()` / `bind_segment()` return-on-no-node / zero-len / null-base | ✅ runs everywhere (single-NUMA-safe) |
| Segment-header stamping | `tests/numa_segment_id.rs` | ✅ runs everywhere |
| sysfs cpumap parser | smoke tests in `numa-shim` | ✅ runs on any Linux host |
| Windows VirtualAllocExNuma | **nothing** — code exists, runs in production builds, never executed by CI | ❌ blind spot |
| Multi-node alloc correctness | `tests/numa_alloc.rs` with `SEFER_NUMA_TEST=1` under QEMU `-numa` | ⚠️ documented, no scheduled CI job |

The Windows path is the biggest blind spot. Phase 1 below closes it.

---

## Option matrix

### A) Mock-shim test seam — feature `numa-mock`

**What:** add a test-only feature to `numa-shim` that replaces
`current_node` / `bind_range` / `reserve_on_node` with a recording mock.
The mock stores a Vec of `(operation, args)` tuples in thread-local
storage. Unit tests assert that the right syscalls were issued with the
right arguments.

**Validates:** our wrapping logic (do we call `mbind` with the right
node? Do we honor `NO_NODE` short-circuit? Does `reserve_on_node` chain
to `reserve_aligned` first?).

**Does NOT validate:** that the OS actually binds memory. The kernel
behavior is uninstrumented.

**Cost:** ~150 lines (mock + 5-8 unit tests). Runs on every CI run, every
target. Zero hardware, zero emulator.

**Verdict:** **MUST-HAVE.** This is the bedrock — it catches regressions
in our code before any OS or emulator gate. Should land first.

### B) QEMU `-numa` Linux runbook (already documented)

**What:** boot a Linux VM under QEMU with synthetic NUMA topology
(`-numa node,...`). Inside the VM, `numactl --hardware` shows 2+ nodes,
`mbind` actually binds pages, `cat /proc/PID/numa_maps` proves it.

**Validates:** the full Linux `mbind` + sysfs cpumap + node-stamping
path end-to-end. Real kernel, real syscalls, just virtualized topology.

**Does NOT validate:** anything Windows. Anything macOS.

**Cost:** runbook already in `tests/numa_alloc.rs` (`Option A — QEMU
fake-NUMA`). Today: manual. **Recommended Phase 2:** add a
GitHub-Actions job that boots an Ubuntu image under QEMU with
`-numa node,...` and runs the env-guarded suite. Doable with
[`docker/qemu-action`](https://github.com/marketplace/actions/qemu-arm-runner)
or a custom step that uses the host's KVM. ~30 lines of `ci.yml`,
periodic schedule (weekly is enough — NUMA path doesn't change daily).

**Verdict:** **DO IT in Phase 2.** Linux is half the production surface;
this is the single most impactful CI gate.

### C) Hyper-V virtual NUMA topology (Windows)

**What:** on a single-NUMA Windows host, configure a Hyper-V VM (Gen2
required) with virtual NUMA via `Set-VMProcessor`:

```powershell
Set-VMProcessor -VMName MyVM `
  -MaximumCountPerNumaNode 2 `
  -MaximumCountPerNumaSocket 1 `
  -CompatibilityForMigrationEnabled $false
Set-VMMemory   -VMName MyVM -DynamicMemoryEnabled $false -StartupBytes 4GB
```

Inside the Windows guest, `Get-WmiObject -Class Win32_NumaNode | Measure`
should report `Count: 2`, and `VirtualAllocExNuma(node=1)` will actually
go to the second virtual node.

**Validates:** the Windows `VirtualAllocExNuma` path end-to-end on a real
Windows kernel, even when the dev host is single-socket. The kernel
honors virtual NUMA boundaries.

**Does NOT validate:** behavior on **real** multi-socket Windows
hardware (page-migration patterns, cross-socket latency are different).

**Cost:** one-time setup (~1 hour), Windows Pro/Enterprise license,
Hyper-V on the dev host. No CI integration (GitHub Actions Windows
runners don't expose Hyper-V); this is **developer/release-gate only**.

**Risks:** virtual NUMA on Gen2 VMs has a few documented constraints
(no dynamic memory, no live migration to a host with different
topology). Worth verifying experimentally — there's a chance the guest
sees `Win32_NumaNode.Count = 1` despite the config if Hyper-V refuses
the override on certain CPU families.

**Verdict:** **EXPERIMENT in Phase 3.** If it works → becomes the
Windows manual gate. If Hyper-V refuses → fall back to (D) or (E).

**Status (task #98):** draft recipe lives in
[`NUMA_WINDOWS_DEV_RECIPE.md`](NUMA_WINDOWS_DEV_RECIPE.md) — PowerShell
commands for VM creation, virtual NUMA topology, in-guest verification,
test run, and troubleshooting. End-to-end verification on a real Hyper-V
host is **pending**; whoever runs the recipe first should fill in the
"Verification log" table at the bottom of that file.

### D) QEMU with Windows guest + `-numa`

**What:** run a full Windows image under QEMU/KVM (host: Linux/WSL2)
with `-numa node,...`. Windows guest sees a multi-NUMA topology, the
`VirtualAllocExNuma` path executes.

**Validates:** same as (C), independent of Hyper-V quirks.

**Cost:** Windows ISO (developer license: free Windows 11 Dev VM),
QEMU+KVM setup, ~30 GB disk image, slow boot (~minutes), Windows
licensing. Not CI-able (image size + licensing). Heavy for routine use.

**Verdict:** **FALLBACK for (C).** Use only if Hyper-V virtual NUMA
doesn't materialize the topology in the guest.

### E) Real multi-socket cloud VM

**What:** rent a dual-socket EC2 instance (`c5n.metal`, `m5n.metal`)
for 10 minutes, run the test suite, destroy. ~$1.

**Validates:** real silicon, real NUMA distances, real cross-socket
latency. Catches things synthetic NUMA can't (BIOS topology bugs,
firmware quirks).

**Cost:** real money (small), no automation in OSS CI (Actions can't
hold an AWS account), needs manual recipe.

**Verdict:** **PRE-RELEASE GATE only.** Document the recipe in
`docs/`, run before every release that touches NUMA code.

### F) Larger GitHub Actions runners

**What:** `ubuntu-latest-16-core` and similar paid runners. **Most are
single-NUMA** — verified via `numactl --hardware` on a sample run.
Some `xlarge` Windows runners are multi-socket but unconfirmed.

**Cost:** paid runners, gated on the test detecting `> 1` NUMA nodes.

**Verdict:** **NOT WORTH IT.** Paid + uncertain. Option (B) on
free runners gives the same Linux coverage deterministically.

### G) numactl --interleave / cpuset constraints on real hardware

**What:** on a real multi-socket box, force the test to allocate via
specific nodes using `numactl --membind=1 cargo test ...`.

**Verdict:** **N/A.** Pre-supposes the hardware we don't have. Useful
for option (E) cloud VM recipes.

---

## Recommended stack

| Phase | What | Cost | When | Coverage gained |
|---|---|---|---|---|
| **1** | (A) Mock-shim seam + unit tests | ~150 LoC | this week | Logic regressions in `numa-shim`, both platforms |
| **2** | (B) QEMU `-numa` Linux job in CI (weekly schedule) | ~30 LoC `ci.yml` | next push | Linux `mbind` end-to-end on a real kernel |
| **3** | (C) Hyper-V virtual NUMA developer recipe + experiment whether it works | ~1 hr setup + docs | within month | Windows `VirtualAllocExNuma` end-to-end |
| **4** | (E) Cloud-VM pre-release recipe | docs only | per-release | Real silicon, real NUMA distances |
| **deferred** | Anything Windows that needs a community-contributor with multi-socket Windows hardware | community | when offered | #83 follow-up |

**Phase 1 is the immediate next step.** It's pure logic coverage,
deterministic, runs in every CI build, and catches the regressions that
would otherwise slip through to a release where someone notices "the
NUMA path doesn't bind correctly". Phase 2 then validates the real
syscall behavior on the half of the platform matrix (Linux) where
emulation is essentially free.

---

## Phase 1 — mock-shim concrete design (for follow-up task)

```rust
// crates/numa/Cargo.toml
[features]
mock = []

// crates/numa/src/lib.rs
#[cfg(feature = "mock")]
mod mock {
    use std::cell::RefCell;
    thread_local! {
        pub static CALLS: RefCell<Vec<MockCall>> = RefCell::new(Vec::new());
    }
    #[derive(Debug, Clone, PartialEq)]
    pub enum MockCall {
        CurrentNode(u32),                 // returned value
        BindRange { base: usize, len: usize, node: u32 },
        ReserveOnNode { size: usize, align: usize, node: u32 },
    }
    pub fn drain() -> Vec<MockCall> { CALLS.with(|c| c.borrow_mut().drain(..).collect()) }
    pub fn set_current_node(n: u32) { /* TLS slot */ }
}

// Dispatch in current_node():
#[cfg(feature = "mock")]
{
    let n = mock::current_node_slot();
    mock::CALLS.with(|c| c.borrow_mut().push(MockCall::CurrentNode(n)));
    return Some(n);
}
#[cfg(not(feature = "mock"))]
// ... existing platform dispatch
```

Tests in `crates/numa/tests/mock_dispatch.rs` (gated on `feature = "mock"`):

```rust
#[test]
fn bind_range_no_node_is_no_op() {
    let _ = numa_shim::CALLS.with(|c| c.borrow_mut().clear());
    unsafe { numa_shim::bind_range(0x1000 as *mut u8, 4096, numa_shim::NO_NODE) };
    assert!(numa_shim::drain().is_empty(), "NO_NODE must short-circuit");
}

#[test]
fn bind_range_records_args() { /* ... */ }

#[test]
fn reserve_on_node_chains_to_reserve_aligned() { /* ... */ }
```

This gives every CI run on every target (Windows, macOS, Linux, miri)
deterministic coverage of the wrapping logic without touching any real
kernel NUMA mechanism.

---

## Open questions / honest unknowns

1. **Hyper-V virtual NUMA actually creates topology in the guest?** —
   needs experimental verification. If `Win32_NumaNode.Count` stays at 1
   in the guest, Phase 3 falls back to QEMU Windows (option D).
2. **GitHub Actions `ubuntu-latest` + nested KVM** — some hosts allow
   nested virtualization, others don't. The Phase 2 CI job needs a
   prototype run to confirm; if `kvm-ok` returns "INFO: KVM acceleration
   can NOT be used", we'd need a slower software-only QEMU run (still
   works for correctness, just minutes instead of seconds).
3. **mbind() return value on single-node kernels** — currently we
   silently ignore EINVAL. The mock would let us assert "the syscall
   was attempted with the right node id" regardless of the kernel's
   ability to honor it.

These are tracked in the follow-up tasks; nothing here blocks Phase 1.
