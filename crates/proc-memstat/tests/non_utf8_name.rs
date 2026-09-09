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
//! SKIP notices use direct stderr writes in both child and runner, bypassing
//! both libtest capture layers. No output flags or result files are needed.
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

use std::io::Write;

/// Marker env var, set ONLY on the freshly-spawned scenario process: the
/// marked process runs its scenario body directly instead of spawning yet
/// another copy of itself.
const SCENARIO_CHILD: &str = "PROC_MEMSTAT_NON_UTF8_NAME_CHILD";

/// Run one isolated scenario and forward diagnostics even on success.
fn run_scenario_alone(scenario: &'static str) {
    let exe = std::env::current_exe().expect("current_exe resolves this test binary");
    let out = std::process::Command::new(&exe)
        .args(["--exact", scenario])
        .env(SCENARIO_CHILD, "1")
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

    // Unlike eprintln!, Write bypasses the parent's libtest capture.
    std::io::stderr()
        .lock()
        .write_all(&out.stderr)
        .expect("forward child diagnostics");
}

fn report_skip(reason: &str) {
    writeln!(
        std::io::stderr().lock(),
        "[non_utf8_name] SKIP: {reason}; real backend assertions did not run"
    )
    .expect("report skipped scenario");
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
            report_skip(reason);
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

/// Replacing either direct stderr write with println!/eprintln! hides the notice.
#[test]
fn skip_notice_survives_both_libtest_capture_layers() {
    const TEST: &str = "skip_notice_survives_both_libtest_capture_layers";
    const RUNNER: &str = "PROC_MEMSTAT_NON_UTF8_CAPTURE_RUNNER";
    const REASON: &str = "synthetic legacy-kernel skip";

    if std::env::var_os(SCENARIO_CHILD).is_some() {
        report_skip(REASON);
        return;
    }
    if std::env::var_os(RUNNER).is_some() {
        run_scenario_alone(TEST);
        return;
    }

    let out = std::process::Command::new(std::env::current_exe().expect("current_exe"))
        .args(["--exact", TEST])
        .env(RUNNER, "1")
        .env_remove(SCENARIO_CHILD)
        .env_remove("RUST_TEST_NOCAPTURE")
        .output()
        .expect("spawn capture regression runner");
    assert!(
        out.status.success(),
        "capture regression runner failed: {out:?}"
    );
    let stderr = String::from_utf8(out.stderr).expect("UTF-8 skip diagnostic");
    assert!(
        stderr.contains(&format!("[non_utf8_name] SKIP: {REASON}")),
        "skip notice was hidden by libtest capture: {stderr:?}"
    );
}
