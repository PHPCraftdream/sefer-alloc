# proc-memstat

Single-read self-probe of a process's **own** memory: RSS + **commit charge**
+ virtual size + peak RSS, in bytes, from one call. Zero dependencies, 100%
Rust with **zero crate dependencies** — no `sysinfo`, no `libc`. (Not "no C
libraries": the Windows and macOS backends call native OS APIs through
locally-declared FFI. What this crate has none of is *crate* dependencies.)

```rust
let m = proc_memstat::snapshot();
println!(
    "rss={} virtual_size={:?} commit_charge={:?} peak_rss={:?}",
    m.rss, m.virtual_size, m.commit_charge, m.peak_rss
);
```

`snapshot() -> MemStat { rss: u64, virtual_size: Option<u64>, commit_charge:
Option<u64>, peak_rss: Option<u64> }` — all fields in **bytes**, gathered from
one source in one go.

That is a best-effort observation, not an atomic one, and the crate does not
claim otherwise: the kernel documents RSS accounting as asynchronous, procfs
assembles these lines with separate reads, and a `std` file read is not a
single syscall. Reading the fields from one `MemStat` beats two `snapshot()`
calls, which is the point — but a comparison that would be wrong if the
figures were microseconds apart needs a stronger mechanism than this.

## `commit_charge` and `virtual_size` are different things

RSS (resident set) only counts pages the OS has actually faulted in. **Commit
charge** counts memory charged against the system commit limit whether or not
it is resident yet — so a `VirtualAlloc(MEM_COMMIT)` is visible in
`commit_charge` while still invisible to `rss`. It is the axis that catches
commit-heavy designs RSS hides, and it is almost never surfaced by existing
crates.

**Virtual size** is the size of the address space, which is NOT the same
quantity: a large read-only file mapping inflates `virtual_size` while costing
nothing in Linux overcommit accounting, and reserved-but-uncommitted address
space is not memory anything has been charged for. Reporting one under the
other's name would make retained address space read as retained commit — so
each is its own field, and whichever the platform does not provide is `None`
rather than filled in with a "nearest analogue".

## Platform matrix

| Platform | `rss` | `virtual_size` | `commit_charge` | `peak_rss` |
|----------|-------|----------------|-----------------|------------|
| Linux    | `/proc/self/status` `VmRSS` | `VmSize` (`Some`) | `None` | `VmHWM` (`Some`) |
| Windows  | `K32GetProcessMemoryInfo` `WorkingSetSize` | `None` | `PagefileUsage` (`Some`) | `PeakWorkingSetSize` (`Some`) |
| macOS    | `task_info(MACH_TASK_BASIC_INFO)` `resident_size` | `virtual_size` (`Some`) | `None` | `resident_size_max` (`Some`) |
| other / miri | `0` | `None` | `None` | `None` (honest fallback) |

The `None`s are structural, not unimplemented: `PROCESS_MEMORY_COUNTERS` has no
virtual-size field, and neither `/proc/self/status` nor `MACH_TASK_BASIC_INFO`
has a commit-charge counter.

All three Linux figures come from `/proc/self/status` (not `/proc/self/statm`),
whose values are in kB regardless of the kernel's base page size — so no
page-size query is needed and the numbers stay correct on 16 KiB/64 KiB-page
kernels.

All memory-reading `unsafe` is confined to the platform modules and carries a
`// SAFETY:` proof; the public API is safe.

## License

MIT OR Apache-2.0.
