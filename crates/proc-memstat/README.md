# proc-memstat

Same-instant self-probe of a process's **own** memory: RSS + **commit charge**
+ peak RSS, in bytes, from one call. Zero dependencies, 100% Rust (no
`sysinfo`, no C libraries).

```rust
let m = proc_memstat::snapshot();
println!("rss={} commit={} peak_rss={:?}", m.rss, m.commit, m.peak_rss);
```

`snapshot() -> MemStat { rss: u64, commit: u64, peak_rss: Option<u64> }` — all
fields in **bytes**, read from one OS query so `rss` and `commit` describe the
same instant.

## Why commit charge?

RSS (resident set) only counts pages the OS has actually faulted in. **Commit
charge** counts memory charged against the system commit limit whether or not
it is resident yet — so a `VirtualAlloc(MEM_COMMIT)` or an over-committing
reservation is visible in `commit` while still invisible to `rss`. It is the
axis that catches commit-heavy designs RSS hides, and it is almost never
surfaced by existing crates.

## Platform matrix

| Platform | `rss` | `commit` | `peak_rss` |
|----------|-------|----------|------------|
| Linux    | `/proc/self/statm` resident × page size | `/proc/self/statm` size × page size | `/proc/self/status` `VmHWM` (`Some`) |
| Windows  | `K32GetProcessMemoryInfo` `WorkingSetSize` | `PagefileUsage` | `PeakWorkingSetSize` (`Some`) |
| macOS    | `task_info(MACH_TASK_BASIC_INFO)` `resident_size` | `virtual_size` | `resident_size_max` (`Some`) |
| other / miri | `0` | `0` | `None` (honest fallback) |

All memory-reading `unsafe` is confined to the platform modules and carries a
`// SAFETY:` proof; the public API is safe.

## License

MIT OR Apache-2.0.
