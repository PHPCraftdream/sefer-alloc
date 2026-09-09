//! End-to-end guard for the P2-3 fix (review round 2, P3-3a): the Linux
//! backend must read `/proc/thread-self/status` as BYTES
//! (`std::fs::read`), not via `read_to_string`.
//!
//! `tests/status_parse.rs`'s
//! `the_utf8_route_this_parser_replaced_would_still_lose_every_field` only
//! proves `String::from_utf8` fails on a fabricated fixture — it never
//! exercises the production reader. If the backend regressed to
//! `read_to_string`, every existing live test would stay green: a test
//! binary has an ASCII name, hence an ASCII status, which both routes read
//! fine. This file closes that gap by running the REAL backend function
//! (`proc_memstat::try_snapshot`) against a GENUINELY non-UTF-8 status.
//!
//! Kernel facts this relies on (verified empirically on a 6.18 kernel and
//! against mainline `fs/proc/array.c`):
//!
//! * `prctl(PR_SET_NAME)` sets the CALLING THREAD's `comm` from raw bytes,
//!   with no UTF-8 validation (16-byte bound including the NUL).
//! * `/proc/<pid>/status`'s `Name:` line prints the comm through
//!   `seq_escape_str(m, tcomm, ESCAPE_SPACE | ESCAPE_SPECIAL, "\n\\")`,
//!   which escapes only space/special ASCII — bytes >= 0x80 (e.g. 0xFF)
//!   pass through RAW. A comm containing 0xFF therefore makes the whole
//!   status file invalid UTF-8: exactly the input a `read_to_string`
//!   reader fails on with `InvalidData`.
//!
//! **Fresh-process isolation** (the `tests/monotonicity.rs` pattern): the
//! runner re-executes this binary with `--exact <test>` plus a marker env
//! var; the child runs the scenario alone. This keeps the `prctl` name
//! change inside a throwaway process — comm is per-task and dies with the
//! child, so the runner (and any sibling test binary) never sees it.
//!
//! **Portability** (round 3, P3-1): on kernels without `/proc/thread-self`
//! (pre-3.17) the scenario SKIPS with an explicit message: the renamed COMM
//! lives on the calling (libtest worker) thread, so it is unobservable
//! through the library's `/proc/self` fallback. The main 3.17+ regression
//! coverage (thread-self path, bytes-not-read_to_string) is unchanged.
//! The skip is made OBSERVABLE (round 4, P4-1): the child writes a
//! machine-readable `SKIP` marker to a runner-provided result file, and the
//! runner prints a visible skip notice from it on its own output — the
//! child's bare `println!` is hidden by its own libtest harness, and the
//! runner's captured child output previously appeared only on the failure
//! path (`tests/thread_leader_exit.rs` result-file pattern, adapted to a
//! child that runs under libtest).
//!
//! **Under the counterfactual regression** (`read_to_string` in
//! `src/lib.rs`'s `read_status`): the non-UTF-8 status fails
//! `InvalidData` → `try_snapshot()` returns `Err(SnapshotError::Os)` → the
//! assertion below fails, instead of silently passing like every other
//! live test would.

#![cfg(all(target_os = "linux", not(miri)))]

// The parser, pulled in by `#[path]` — the sanctioned pattern (see
// `tests/status_parse.rs`) — to prove it reads the ASCII fields out of the
// same non-UTF-8 bytes the backend just consumed end-to-end.
#[path = "../src/status_parse.rs"]
mod status_parse;

/// Marker env var, set ONLY on the freshly-spawned scenario process: the
/// marked process runs its scenario body directly instead of spawning yet
/// another copy of itself.
const SCENARIO_CHILD: &str = "PROC_MEMSTAT_NON_UTF8_NAME_CHILD";

/// Per-run result-file env var, set ONLY on the freshly-spawned scenario
/// process: on a legacy-kernel skip the child writes an explicit
/// machine-readable `SKIP ...` marker there, and the runner surfaces it on
/// ITS OWN output. The child's bare `println!` cannot serve: the child is a
/// libtest binary whose harness hides a passing `#[test]`'s captured
/// output, and the runner prints its `Command::output()` text only inside
/// the failure-path assert — on exit 0 both are invisible (Sol-codex round
/// 4, P4-1). A result FILE rather than a distinguishable exit code, because
/// the child is judged by libtest's exit status (a skip exits 0 exactly
/// like a pass, and any real assertion failure already exits non-zero):
/// the file adds the marker without touching that contract. This is the
/// `tests/thread_leader_exit.rs` result-file pattern, adapted to a child
/// that runs UNDER libtest.
const SCENARIO_RESULT: &str = "PROC_MEMSTAT_NON_UTF8_NAME_RESULT";

/// Run the scenario alone in a fresh process, and fail this test (with the
/// child's captured output) if that child fails. Same shape as
/// `tests/monotonicity.rs`.
///
/// The child records its outcome in a per-run result file (the
/// `tests/thread_leader_exit.rs` pattern, adapted to a child that runs
/// UNDER libtest): a legacy-kernel skip must be visible on the PARENT's own
/// output, not buried in the child's internally-captured text — the child's
/// `println!` is hidden by its own libtest harness (a passing `#[test]`),
/// and this runner only prints its `Command::output()` capture on the
/// failure path (Sol-codex round 4, P4-1).
fn run_scenario_alone(scenario: &'static str) {
    let exe = std::env::current_exe().expect("current_exe resolves this test binary");

    // Per-run path + pre-spawn delete: a stale report from an earlier run
    // can never satisfy this run (the `tests/thread_leader_exit.rs` rule).
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let result_path = std::env::temp_dir().join(format!(
        "proc_memstat_non_utf8_name_{pid}_{nanos}_result.txt"
    ));
    let _ = std::fs::remove_file(&result_path);

    let out = std::process::Command::new(&exe)
        .args(["--exact", scenario])
        .env(SCENARIO_CHILD, "1")
        .env(SCENARIO_RESULT, &result_path)
        .output()
        .unwrap_or_else(|e| panic!("failed to re-spawn this binary for scenario {scenario}: {e}"));
    assert!(
        out.status.success(),
        "fresh-process scenario {scenario} failed ({status}):\n\
         --- child stdout ---\n{stdout}\n\
         --- child stderr ---\n{stderr}",
        status = out.status,
        stdout = String::from_utf8_lossy(&out.stdout),
        stderr = String::from_utf8_lossy(&out.stderr),
    );

    // A machine-readable SKIP marker in the result file is surfaced on the
    // runner's own output UNCONDITIONALLY — not only inside a failure-path
    // assert message. This stays a skip, not a failure: the fix is purely
    // about observing an already-legitimate legacy-kernel skip, not about
    // changing when a skip happens (Sol-codex round 4, P4-1).
    if let Ok(report) = std::fs::read_to_string(&result_path) {
        if report.starts_with("SKIP") {
            println!(
                "[non_utf8_name] SKIPPED — child scenario report:\n{report}\
                 (the scenario's real assertions did NOT run on this kernel)"
            );
        }
    }
    let _ = std::fs::remove_file(&result_path);
}

// `prctl` is variadic in glibc; declared locally so the test crate needs no
// `libc` dependency (the crate's other locally-declared FFI, same shape).
extern "C" {
    fn prctl(option: core::ffi::c_int, ...) -> core::ffi::c_int;
}

/// Child body: give THIS task a non-UTF-8 name, then run the real backend
/// against the status file that name corrupts. Every link the test depends
/// on is asserted (the `tests/thread_leader_exit.rs` discipline: prove the
/// scenario is real, then assert on it).
fn scenario_a_non_utf8_task_name_cannot_blank_the_real_backend_read() {
    // 1. Set the CALLING THREAD's comm to raw bytes including 0xFF. 14 bytes
    //    including the NUL, inside the kernel's 16-byte comm bound.
    //    SAFETY: `name` is a valid NUL-terminated pointer for the duration
    //    of the call; PR_SET_NAME only copies from it.
    let name: &[u8; 14] = b"proc-memstat\xff\0";
    // The trailing zeros are typed explicitly (`c_ulong`, glibc's documented
    // `unsigned long` vararg type for prctl) because prctl(2)'s CAVEATS
    // (https://man7.org/linux/man-pages/man2/prctl.2.html) warns that zero
    // arguments must be passed at full machine width — through the variadic
    // ABI an untyped `0` defaults to `i32`, and correctness would then rest
    // on ABI register/stack-slot padding happening to zero-extend on
    // x86-64/aarch64. Portability fix; no behavior change on hosts where the
    // padding already worked. (Sol-codex review round 3, P4-2.)
    let ret = unsafe {
        prctl(
            15, /* PR_SET_NAME */
            name.as_ptr(),
            0 as core::ffi::c_ulong,
            0 as core::ffi::c_ulong,
            0 as core::ffi::c_ulong,
        )
    };
    assert_eq!(ret, 0, "prctl(PR_SET_NAME) must succeed");

    // 2. Read the status BYTES — the same call shape the backend's
    //    acquisition step makes.
    //    ENOENT-aware, SKIP (not fallback): the test body runs on a libtest
    //    WORKER thread, and `prctl(PR_SET_NAME)` names the CALLING thread —
    //    so on a pre-3.17 kernel (no `/proc/thread-self`, added in 3.17)
    //    `/proc/self/status` would name the untouched MAIN thread, whose
    //    `Name:` has no 0xFF and self-checks 3/4 would fail. The scenario is
    //    simply unobservable through the legacy fallback: skip explicitly.
    //    (Sol-codex round 3, P3-1.)
    let status_bytes = match std::fs::read("/proc/thread-self/status") {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let reason = "kernel has no /proc/thread-self (added in Linux 3.17); \
                 this scenario — which proves the backend reads the CALLING \
                 THREAD's own renamed status — requires that file and cannot \
                 be observed via /proc/self (Sol-codex round 3, P3-1)";
            println!("SKIP: {reason}.");
            // Machine-readable marker for the runner (see `SCENARIO_RESULT`):
            // the runner prints a visible skip notice from this file on ITS
            // OWN output, unconditionally — this child's `println!` above is
            // captured (hidden) inside a passing libtest run, and the
            // runner's Command::output() text appears only on the failure
            // path (Sol-codex round 4, P4-1). Best-effort: a failed write
            // degrades to the previous, invisible-skip behavior — it must
            // NOT turn a legitimate legacy-kernel skip into a failure.
            if let Some(result_path) = std::env::var_os(SCENARIO_RESULT) {
                let _ = std::fs::write(result_path, format!("SKIP {reason}\n"));
            }
            return;
        }
        Err(e) => panic!("read /proc/thread-self/status failed: {e}"),
    };

    // 3. Premise self-check A: the `Name:` line carries the RAW 0xFF byte,
    //    not an ASCII `\xFF` escape. On a kernel that escaped it the
    //    scenario cannot exist — fail loudly rather than pass vacuously.
    let name_line = status_bytes
        .split(|&b| b == b'\n')
        .find(|line| line.starts_with(b"Name:"))
        .expect("status must carry a Name: line");
    assert!(
        name_line.contains(&0xFF),
        "kernel escaped the 0xFF in comm (line {name_line:?}) — the scenario \
         cannot exist on this kernel"
    );

    // 4. Premise self-check B: the file is genuinely NOT valid UTF-8 — the
    //    in-run counterfactual that makes the assertion below mean something
    //    (a `read_to_string` reader fails exactly this).
    assert!(
        String::from_utf8(status_bytes.clone()).is_err(),
        "status bytes must be invalid UTF-8 for this scenario to exist"
    );

    // 5. THE assertion: the real backend reads it fine. Under the
    //    read_to_string regression this read fails InvalidData →
    //    Err(SnapshotError::Os) → this test fails.
    let m = proc_memstat::try_snapshot().expect(
        "try_snapshot must read a non-UTF-8-named task's status (bytes, not \
         read_to_string)",
    );
    assert!(m.rss > 0, "rss must be non-zero, got {}", m.rss);
    assert!(
        m.virtual_size.is_some(),
        "Linux must report virtual_size (VmSize)"
    );
    assert!(m.peak_rss.is_some(), "Linux must report peak_rss (VmHWM)");
    assert!(
        m.commit_charge.is_none(),
        "Linux must leave commit_charge None; got {:?}",
        m.commit_charge
    );

    // 6. The best-effort wrapper must not take the all-zero fallback either.
    assert!(
        proc_memstat::snapshot().rss > 0,
        "snapshot() must not fall back to zeros on a non-UTF-8 task name"
    );

    // 7. The parser reads the ASCII fields out of these very bytes. Presence
    //    only — NOT compared against try_snapshot()'s rss: the two reads are
    //    at different instants and RSS legitimately moves.
    assert!(
        status_parse::read_kib_field(&status_bytes, b"VmRSS:").is_some(),
        "parser must find VmRSS in the non-UTF-8 status bytes"
    );
}

/// A non-UTF-8 task name must not be able to blank the real backend's read.
#[cfg(all(target_os = "linux", not(miri)))]
#[test]
fn a_non_utf8_task_name_cannot_blank_the_real_backend_read() {
    if std::env::var_os(SCENARIO_CHILD).is_some() {
        scenario_a_non_utf8_task_name_cannot_blank_the_real_backend_read();
    } else {
        run_scenario_alone("a_non_utf8_task_name_cannot_blank_the_real_backend_read");
    }
}
