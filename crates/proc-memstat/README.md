# proc-memstat

Single-read self-probe of a process's **own** memory: RSS + **commit charge**
+ virtual size + peak RSS, in bytes, from one call. 100% Rust with **zero
crate dependencies** — no `sysinfo`, no `libc`. (Not "no C
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

## When a reading fails, say so — `try_snapshot`

`snapshot()` is best-effort: when no reading is available it returns an
all-zero `MemStat` — `rss: 0`, every optional field `None`. A normally
succeeding backend never returns that shape (the `Some` fields its platform
matrix row requires are there even when a counter's own value is 0), but a
caller reading `rss` alone sees `0` either way, and the fallback erases the
cause — so a before/after pair whose *second* read failed reads as a complete
release of memory.

```rust
match proc_memstat::try_snapshot() {
    Ok(m) => println!("rss={}", m.rss),
    Err(e) => eprintln!("no reading: {e}"),
}
```

`try_snapshot() -> Result<MemStat, SnapshotError>` says which happened.
`SnapshotError` is `Unsupported` (no backend for this target) / `Os` (the
platform call failed) / `Malformed` (the reading came back unusable), and is
`#[non_exhaustive]`. Use `snapshot()` when a missing reading is acceptable and
`try_snapshot()` when a wrong conclusion from one would not be.

## `commit_charge` and `virtual_size` are different things

RSS (resident set) only counts pages the OS has actually faulted in. It is
also not the process's total physical footprint: on Windows the working set
contains only pageable allocations, so nonpageable ones (AWE, large-page
allocations) are not in it; on Linux `VmRSS` excludes explicit HugeTLB pages
(reported separately as `HugetlbPages`) while transparent huge pages DO count
as RSS. **Commit
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
| Linux    | `/proc/thread-self/status` `VmRSS` | `VmSize` (`Some`) | `None` | `VmHWM` (`Some`) |
| Windows  | `K32GetProcessMemoryInfo` `WorkingSetSize` | `None` | `PagefileUsage` (`Some`) | `PeakWorkingSetSize` (`Some`) |
| macOS    | `task_info(MACH_TASK_BASIC_INFO)` `resident_size` | `virtual_size` (`Some`) | `None` | `resident_size_max` (`Some`) |
| other / miri | `0` | `None` | `None` | `None` (honest fallback) |

The `None`s are structural, not unimplemented: `PROCESS_MEMORY_COUNTERS` has no
virtual-size field, and neither the Linux task status nor `MACH_TASK_BASIC_INFO`
has a commit-charge counter.

All three Linux figures come from the calling THREAD's own task status,
`/proc/thread-self/status` — not `/proc/self/status`, which names the
thread-group leader (the main thread): a process whose main thread has
exited via `pthread_exit` is still alive, but the leader's status file no
longer carries `VmRSS`/`VmSize`/`VmHWM`. All threads share one `mm`, so the
calling thread's figures are the whole process's — nothing is summed across
threads. Kernels older than 3.17, which have no `/proc/thread-self`, fall
back to `/proc/self/status` (and never to `/proc/self/statm`), whose values
are in kB regardless of the kernel's base page size — so no
page-size query is needed and the numbers stay correct on 16 KiB/64 KiB-page
kernels.

All memory-reading `unsafe` is confined to the platform modules and carries a
`// SAFETY:` proof; the public API is safe.

## License

MIT OR Apache-2.0.
