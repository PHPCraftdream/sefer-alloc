# proc-probe

The one-dependency toolkit a **fresh-process measurement probe** needs: emit
the `RESULT key=value` stdout protocol a per-sample runner greps, plus a
re-export of [`proc-memstat`](../proc-memstat)'s single-read RSS +
**commit charge** + peak-RSS `snapshot()`. *Measure your process, report it in
one line.*

```rust
let t0 = std::time::Instant::now();             // start of the measured region
let m = proc_probe::snapshot();                 // measure (bytes, one read)
proc_probe::emit_u64("rss_kib", m.rss / 1024);  // report
proc_probe::emit_ns("elapsed_ns", t0.elapsed().as_nanos());
// stdout:
//   RESULT rss_kib=1234
//   RESULT elapsed_ns=987654
```

`snapshot()` is best-effort: on a read failure it returns an all-zero
`MemStat`, so a judge reading `RESULT rss_kib=0` cannot tell "memory was
freed" from "the measurement failed". When that distinction matters, take the
fallible re-export instead and make the failure loud rather than a
well-formed zero:

```rust
match proc_probe::try_snapshot() {
    Ok(m) => proc_probe::emit_u64("rss_kib", m.rss / 1024),
    Err(e) => {
        eprintln!("proc-probe: memory measurement failed: {e}");
        std::process::exit(3); // this probe's failure signal to its runner
    }
}
```

## The protocol

A *probe* is a tiny binary a *runner* launches N times as fresh OS processes,
parsing one machine-readable line per metric out of each run's stdout:

```text
RESULT <key>=<value>
```

`<key>` is `[a-z0-9_]+`; `<value>` is any non-whitespace token. This crate is
the emitting half — one function per value shape (`emit`, `emit_u64`,
`emit_i64`, `emit_f64`, `emit_ns`) so a probe never hand-rolls the
`println!("RESULT ...")` string and the library side cannot drift the format
the runner parses against. `RESULT_PREFIX` exposes the prefix as a constant.

The `emit*` family is **unchecked**: that shape is a caller precondition, and
violating it — a newline inside a value (prints extra stdout lines a runner
may read as real metrics), an empty key or value (a line no runner accepts) —
is a logic error in the caller. Nothing is validated, sanitized, or truncated.
A write failure to stdout panics, exactly as `println!` does.

## Why a fresh process?

Criterion/iai-style in-process iteration **structurally cannot** see
once-per-process effects — first-touch RSS, commit charge, TLS-bind latency —
because after the first iteration the cost is already paid and every page is
resident. The only way to measure it is one fresh process per sample. This crate
is the shared protocol those probes emit; the paired A/B/B/A runner in the
parent repo is one consumer.

## Features

- `std` (default): the `emit*` family (they write to stdout) plus the
  `proc-memstat` re-export (`snapshot` / `MemStat`, and the fallible
  `try_snapshot` / `SnapshotError` — `proc-memstat` itself is an std-only
  crate). Disable (`default-features = false`) for a genuinely
  dependency-free `no_std` build that only wants `RESULT_PREFIX` and brings
  its own sink.

`#![forbid(unsafe_code)]` — all the OS FFI stays confined to `proc-memstat`.

## License

MIT OR Apache-2.0.
