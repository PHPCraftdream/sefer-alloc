//! Native lifecycle scenario for the Linux backend's thread identity
//! (Sol-codex proc-memstat review round 2, P2-1).
//!
//! The scenario: the process's THREAD-GROUP LEADER — its real main thread —
//! exits while a worker thread keeps the process running, and the worker
//! must still get a real reading from [`try_snapshot`] / [`snapshot`].
//!
//! # The kernel condition this reproduces
//!
//! Not a simulation; every step is a documented kernel behaviour, and the
//! test asserts each link it depends on at runtime:
//!
//! 1. `/proc/self` resolves to the TGID — the thread-group LEADER's task —
//!    for every caller (`proc_self_get_link`, fs/proc/self.c).
//! 2. Every thread exit clears THAT task's `task->mm` (`exit_mm`,
//!    kernel/exit.c) — including the leader's, with no group-still-alive
//!    exception — while surviving threads keep the shared `mm_struct` alive
//!    through their own references.
//! 3. `proc_pid_status` prints the `VmRSS`/`VmSize`/`VmHWM` lines only
//!    inside its `if (mm)` branch (fs/proc/array.c), so after the exit the
//!    leader's status file still EXISTS (the leader task sits in
//!    EXIT_ZOMBIE until thread-group death, `exit_notify`) but carries none
//!    of those fields.
//! 4. A main thread may legitimately exit this way while its process
//!    continues — that is the usage pthread_exit(3) NOTES documents for
//!    main threads ("To allow other threads to continue execution, the main
//!    thread should terminate by calling pthread_exit() rather than
//!    exit(3)").
//!
//! Pre-fix, the backend read the leader-named file — so a live, measurable
//! process got `Err(Malformed)` from `try_snapshot()` and the all-zero
//! fallback from `snapshot()`, indistinguishable from a full memory
//! release.
//!
//! # Why the leader exits via the thread-exit syscall, not pthread_exit(3)
//!
//! Calling `pthread_exit` from a Rust frame is the one part of this
//! scenario that would NOT be faithful: it tears the calling stack down
//! with a forced foreign unwind, and whether such an unwind may cross Rust
//! frames is a compiler-version-dependent ABI question (modern rustc is
//! entitled to turn it into an abort). The direct thread-exit syscall is
//! the operation glibc's own thread teardown issues after pthread_exit
//! unwinds (nptl's `__exit_thread` → `SYS_exit`): the kernel traverses the
//! identical `do_exit()` → `exit_mm()` path with none of the userspace
//! unwinding — the calling task simply stops, and nothing on its stack is
//! expected to resume. The main thread of this scenario binary exists to
//! do exactly this and makes the syscall its last action. This is the
//! OS-level equivalent the POSIX main-thread `pthread_exit` exists to
//! reach, minus the unwind hazard.
//!
//! # Why this target is `harness = false`
//!
//! The leader is defined by TID == TGID — the process's real main thread.
//! libtest's harness main cannot play that role (`#[test]` functions run on
//! spawned worker threads, and no spawned thread can ever BE the leader),
//! so this target has its own `fn main` and no harness: `cargo test` runs
//! it as a plain binary and judges it by its exit code.
//!
//! # Runner/child split, and why a bare exit code is not trusted
//!
//! `main` re-executes itself (the `tests/monotonicity.rs` fresh-process
//! pattern) with a marker env var; the CHILD's main thread is the leader
//! that exits. The child cannot be judged by its exit status alone: if the
//! leader's exit wrongly took the whole process down with status 0, that
//! exit code would look exactly like a pass. So the child's worker writes
//! a result FILE whose last line must be `PASS`, and the runner asserts
//! the child's exit status AND the file. The runner gives the child a
//! fresh per-run path and deletes any pre-existing file before spawning,
//! so a stale report from an earlier run can never satisfy this run.
//!
//! # Counterfactual — why this test is not vacuous
//!
//! Inside the same run, after the leader is gone, the worker proves the
//! OLD source is dead: `/proc/self/status` still reads, still describes
//! this process, its leader task is a zombie — and it has NO `VmRSS` line
//! left (checked through this crate's own byte parser, pulled in with
//! `#[path]` like `tests/status_parse.rs` does). On pre-fix code the
//! backend read exactly that file, so the same scenario made
//! `try_snapshot()` return `Err(Malformed)` and failed this test at its
//! final step; with the fix the worker gets a real reading. This was
//! verified against the pre-fix tree before the fix landed: reverting
//! `src/lib.rs` made the child report `FAIL ... try_snapshot must succeed`
//! and the runner red.
//!
//! # What is asserted, in order
//!
//! 1. the child's main thread really is the leader
//!    (`Tgid == Pid == process id`);
//! 2. the worker really is a non-leader thread (`Pid != Tgid`) and its own
//!    status carries `VmRSS` while the leader is alive;
//! 3. after the leader exit, `/proc/self/status` still resolves, still
//!    describes this process (`Tgid` intact), its leader task is
//!    `State:` Z (zombie) — and it has NO `VmRSS`/`VmSize`/`VmHWM`;
//! 4. the worker's own `/proc/thread-self/status` carries all three
//!    fields;
//! 5. `try_snapshot()` returns `Ok` with `rss > 0` and the documented
//!    Linux field shape, and `snapshot()` does NOT take the all-zero
//!    fallback.
//!
//! # Sanctioned skip paths
//!
//! Two conditions skip the scenario instead of failing it, each loudly
//! and each with the reason printed:
//!
//! - An arch whose `__NR_exit` the `SYS_EXIT` table has not verified
//!   (`None`) skips at RUN time instead of failing the whole test
//!   package's compile (Sol-codex round 3, P2-1); the table's refusal to
//!   guess a syscall number is unchanged.
//! - A kernel without `/proc/thread-self` (pre-3.17) makes the runner
//!   skip: the scenario is thread-self-based by design, and the library's
//!   own documented `/proc/self` fallback cannot serve it (Sol-codex
//!   round 3, P3-1). Any other I/O error probing the file still fails.
// Only the Linux scenario calls the parser; gating the module the same way
// keeps it from being dead code on Windows/macOS/stub hosts (the pattern
// `tests/platform_contract.rs` uses). Hoisted to the file's top level because
// rustfmt cannot resolve a `#[path]` module nested inside another module.
#[cfg(all(target_os = "linux", not(miri)))]
#[path = "../src/status_parse.rs"]
mod status_parse;

#[cfg(all(target_os = "linux", not(miri)))]
mod scenario {
    use std::io::ErrorKind;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::mpsc;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use proc_memstat::{snapshot, try_snapshot};

    use crate::status_parse::{read_int_field, read_kib_field};

    /// Marker env var, set ONLY on the freshly-spawned child whose main
    /// thread will play (and lose) the leader.
    pub(crate) const CHILD_MARKER: &str = "PROC_MEMSTAT_LEADER_EXIT_CHILD";

    /// The two task-status spellings. The OLD one is the negative control's
    /// source (what the pre-fix backend read); the NEW one is what the
    /// backend reads now and what the identity checks below use.
    const THREAD_SELF_STATUS: &str = "/proc/thread-self/status";
    const PROCESS_STATUS: &str = "/proc/self/status";

    const POLL_INTERVAL: Duration = Duration::from_millis(2);
    /// Generous: the leader exit lands within microseconds of the worker's
    /// ready signal; the deadline only bounds a wedged host.
    const LEADER_EXIT_DEADLINE: Duration = Duration::from_secs(10);
    const READY_TIMEOUT: Duration = Duration::from_secs(15);

    // `__NR_exit` — the THREAD-exit syscall (NOT `exit_group`), the syscall
    // glibc's own thread teardown issues. Numbers from each arch's
    // `asm/unistd*.h`; none has changed since long before this repo existed.
    //
    // `None` marks an arch whose `__NR_exit` this table has not verified:
    // the scenario then SKIPS at run time instead of failing the whole test
    // package's compile (Sol-codex round 3, P2-1). Refusing to GUESS the
    // NUMBER is deliberate — the skip message says so.
    #[cfg(target_arch = "x86_64")]
    pub(crate) const SYS_EXIT: Option<core::ffi::c_long> = Some(60);
    // asm-generic tables (aarch64, riscv64, loongarch64).
    #[cfg(any(
        target_arch = "aarch64",
        target_arch = "riscv64",
        target_arch = "loongarch64"
    ))]
    pub(crate) const SYS_EXIT: Option<core::ffi::c_long> = Some(93);
    // Legacy tables (x86, arm, powerpc64, s390x, sparc64).
    #[cfg(any(
        target_arch = "x86",
        target_arch = "arm",
        target_arch = "powerpc64",
        target_arch = "s390x",
        target_arch = "sparc64"
    ))]
    pub(crate) const SYS_EXIT: Option<core::ffi::c_long> = Some(1);
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64",
        target_arch = "loongarch64",
        target_arch = "x86",
        target_arch = "arm",
        target_arch = "powerpc64",
        target_arch = "s390x",
        target_arch = "sparc64"
    )))]
    pub(crate) const SYS_EXIT: Option<core::ffi::c_long> = None;

    extern "C" {
        fn syscall(number: core::ffi::c_long, ...) -> core::ffi::c_long;
    }

    /// Terminate the CALLING THREAD only — here the process's thread-group
    /// leader — leaving the worker alive. The OS-level equivalent of the
    /// POSIX-sanctioned main-thread `pthread_exit`: the exact syscall
    /// glibc's thread teardown bottoms out in, so the kernel runs the same
    /// `do_exit()` → `exit_mm()` without unwinding any userspace frame.
    fn leader_exit_thread_only(sys_exit: core::ffi::c_long) -> ! {
        // SAFETY: a VERIFIED fixed number passed in, one zero argument —
        // no pointers, no layout assumptions. `SYS_exit` does not return:
        // the calling task stops here and its `task->mm` is cleared
        // in-kernel, while the worker thread keeps the thread group (and
        // the shared `mm`) alive.
        unsafe {
            syscall(sys_exit, 0);
        }
        unreachable!("the thread-exit syscall does not return")
    }

    /// First byte-line of `bytes` that starts with `prefix`.
    fn line_starting_with<'a>(bytes: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
        bytes
            .split(|&b| b == b'\n')
            .find(|line| line.starts_with(prefix))
    }

    /// Verifies THIS process's main thread is the thread-group leader
    /// before anything else in the child is trusted.
    pub fn assert_main_thread_is_leader() {
        let status = std::fs::read(THREAD_SELF_STATUS)
            .expect("main thread reads its own /proc/thread-self/status");
        let tgid = read_int_field(&status, b"Tgid:").expect("status carries Tgid:");
        let pid = read_int_field(&status, b"Pid:").expect("status carries Pid:");
        assert_eq!(
            tgid, pid,
            "the scenario's main thread must be the thread-group leader \
             (Tgid == Pid)"
        );
        assert_eq!(
            tgid,
            u64::from(std::process::id()),
            "Tgid must equal the process id"
        );
    }

    pub fn runner_main() {
        // The scenario is thread-self-based by design (it measures the
        // worker's own status after the leader exits — impossible through
        // the library's documented /proc/self fallback), so on a kernel
        // without /proc/thread-self (pre-3.17) skip cleanly. Any OTHER I/O
        // error must fail loudly, not be masked as "must be an old kernel".
        match std::fs::read(THREAD_SELF_STATUS) {
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {
                println!(
                    "[thread_leader_exit] SKIPPED: not applicable — this \
                     kernel has no /proc/thread-self (added in Linux 3.17); \
                     the leader-exit scenario requires the calling thread's \
                     own status file (Sol-codex round 3, P3-1)"
                );
                return;
            }
            Err(e) => panic!(
                "cannot probe /proc/thread-self/status before spawning the \
                 child: {e}"
            ),
        }

        let exe = std::env::current_exe().expect("current_exe resolves this binary");
        // Per-run path: a stale report from an earlier run can never satisfy
        // this run even if the pre-spawn delete below were to fail.
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is set")
            .as_nanos();
        let pid = std::process::id();
        let result_path = std::env::temp_dir().join(format!("proc_memstat_p2_1_{pid}_{nanos}.txt"));
        let _ = std::fs::remove_file(&result_path);

        let out = Command::new(&exe)
            .arg(&result_path)
            .env(CHILD_MARKER, "1")
            .output()
            .unwrap_or_else(|e| {
                panic!("failed to re-spawn this binary as the leader-exit child: {e}")
            });
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);

        assert!(
            out.status.success(),
            "leader-exit child failed ({status}):\n\
             --- child stdout ---\n{stdout}\n\
             --- child stderr ---\n{stderr}",
            status = out.status,
        );
        let report = std::fs::read_to_string(&result_path).unwrap_or_else(|e| {
            panic!(
                "child exited 0 but wrote NO result file — the file is what \
                 distinguishes a verified pass from a spurious whole-process \
                 exit at the leader's syscall (a bare exit 0 would be \
                 indistinguishable from success): {e}\n\
                 --- child stdout ---\n{stdout}\n\
                 --- child stderr ---\n{stderr}"
            )
        });
        for line in report.lines() {
            assert!(
                !line.starts_with("FAIL"),
                "child reported failure: {line}\n\
                 --- child stdout ---\n{stdout}\n\
                 --- child stderr ---\n{stderr}"
            );
        }
        assert_eq!(
            report.lines().last(),
            Some("PASS"),
            "child report must end in PASS; got:\n{report}\n\
             --- child stderr ---\n{stderr}"
        );

        println!("[thread_leader_exit] child report:\n{report}");
        let _ = std::fs::remove_file(&result_path);
    }

    pub fn child_main(sys_exit: core::ffi::c_long) -> ! {
        let result_path = match std::env::args_os().nth(1) {
            Some(p) => PathBuf::from(p),
            None => {
                eprintln!("[thread_leader_exit] CHILD FAIL: no result-file path argument");
                std::process::exit(2);
            }
        };
        let path_for_worker = result_path.clone();
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        let _worker = std::thread::spawn(move || {
            // Every worker failure is funnelled through this catch so ANY
            // assertion failure still leaves a FAIL report and a non-zero
            // exit code for the runner.
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                worker_verify(ready_tx, &path_for_worker)
            }));
            match outcome {
                // worker_verify ends the process itself on success; Ok is
                // unreachable by construction.
                Ok(()) => {}
                Err(panic) => {
                    let msg = panic_message(panic);
                    eprintln!("[thread_leader_exit] CHILD FAIL: {msg}");
                    let _ = std::fs::write(&path_for_worker, format!("FAIL {msg}\n"));
                    std::process::exit(2);
                }
            }
        });

        ready_rx.recv_timeout(READY_TIMEOUT).expect(
            "worker signals ready (a worker that died first has \
                     already failed the process with its own report)",
        );
        eprintln!("[thread_leader_exit] leader (main thread) exiting; worker continues");
        leader_exit_thread_only(sys_exit);
    }

    fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
        if let Some(s) = panic.downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = panic.downcast_ref::<String>() {
            s.clone()
        } else {
            "panic with a non-string payload".to_string()
        }
    }

    /// Runs on the worker thread. The leader (this process's main thread)
    /// exits as soon as this function sends `ready_tx` — everything after
    /// that send runs in a leader-less process. Ends the process with the
    /// PASS report; a failure propagates as a panic to the caller's catch.
    fn worker_verify(ready_tx: mpsc::Sender<()>, result_path: &Path) {
        // Identity of THIS thread: a non-leader worker with a live mm.
        let own = std::fs::read(THREAD_SELF_STATUS)
            .expect("worker reads its own /proc/thread-self/status");
        let tid = read_int_field(&own, b"Pid:").expect("worker status carries Pid:");
        let tgid = read_int_field(&own, b"Tgid:").expect("worker status carries Tgid:");
        assert_ne!(
            tid, tgid,
            "the spawned worker must be a non-leader thread (Pid != Tgid)"
        );
        assert!(
            read_kib_field(&own, b"VmRSS:").is_some(),
            "worker status must carry VmRSS while the leader is alive"
        );
        eprintln!("[thread_leader_exit] worker tid={tid} tgid={tgid}: ready");
        let _ = ready_tx.send(());

        // --- Everything below runs AFTER the leader's task is gone. ---

        let deadline = Instant::now() + LEADER_EXIT_DEADLINE;
        let old_status = loop {
            match std::fs::read(PROCESS_STATUS) {
                Ok(bytes) => {
                    // The zombie state is set AFTER exit_mm in do_exit, so
                    // `State:` Z implies the mm is already cleared; both
                    // facts are asserted explicitly below anyway.
                    let is_zombie = line_starting_with(&bytes, b"State:\tZ").is_some();
                    let vm_gone = read_kib_field(&bytes, b"VmRSS:").is_none()
                        && read_kib_field(&bytes, b"VmSize:").is_none()
                        && read_kib_field(&bytes, b"VmHWM:").is_none();
                    if is_zombie && vm_gone {
                        break bytes;
                    }
                }
                // The zombie leader is expected to keep /proc/self
                // resolvable (exit_notify: a leader with live threads is not
                // reaped). A vanished file would also break the old
                // backend, but it is NOT the documented shape, so fail
                // loudly rather than pass on a different mechanism.
                Err(e) if e.kind() == ErrorKind::NotFound => panic!(
                    "/proc/self/status vanished after the leader exit — the \
                     zombie leader should keep it resolvable"
                ),
                Err(e) => panic!("/proc/self/status read failed unexpectedly: {e}"),
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for the leader's mm to be cleared — the \
                 P2-1 kernel condition did not reproduce"
            );
            std::thread::sleep(POLL_INTERVAL);
        };
        eprintln!(
            "[thread_leader_exit] leader gone: /proc/self/status now \
             carries no VmRSS/VmSize/VmHWM; it now reads:\n{}",
            String::from_utf8_lossy(&old_status)
        );

        // The old source is dead, and dead in exactly the documented way.
        let observed_tgid =
            read_int_field(&old_status, b"Tgid:").expect("zombie status keeps Tgid:");
        assert_eq!(
            observed_tgid, tgid,
            "/proc/self/status must still describe THIS process"
        );
        assert_eq!(
            read_kib_field(&old_status, b"VmRSS:"),
            None,
            "counterfactual broken: the pre-fix source still has VmRSS — \
             this run would prove nothing"
        );

        // The new source is alive: the worker's own status.
        let own_status = std::fs::read(THREAD_SELF_STATUS)
            .expect("worker still reads its own /proc/thread-self/status");
        let fields = ["VmRSS:", "VmSize:", "VmHWM:"];
        for field in fields {
            assert!(
                read_kib_field(&own_status, field.as_bytes()).is_some(),
                "the calling thread's status must still carry {field} after \
                 the leader exit"
            );
        }

        // THE assertion: the crate still measures this live process.
        let m = try_snapshot().expect("try_snapshot must succeed after the leader exit");
        assert!(
            m.rss > 0,
            "a live process must not read as rss=0 after the leader exit; \
             got {m:?}"
        );
        assert!(
            m.virtual_size.is_some() && m.peak_rss.is_some(),
            "the documented Linux field shape must survive the leader exit; \
             got {m:?}"
        );
        let best_effort = snapshot();
        assert!(
            best_effort.rss > 0,
            "snapshot() must not take the all-zero fallback while the \
             process is alive; got {best_effort:?}"
        );

        let own_rss = read_kib_field(&own_status, b"VmRSS:").unwrap();
        let rss_bytes = m.rss;
        let snap_rss = best_effort.rss;
        let report = format!(
            "leader exited; /proc/self/status (zombie leader) carries no \
             VmRSS/VmSize/VmHWM\n\
             /proc/thread-self/status (worker tid={tid}) carries \
             VmRSS={own_rss} kB\n\
             try_snapshot: Ok, rss={rss_bytes} bytes, virtual_size=Some, \
             peak_rss=Some\n\
             snapshot: rss={snap_rss} bytes (zero fallback NOT taken)\n\
             PASS\n"
        );
        std::fs::write(result_path, report).expect("child writes its result file");
        eprintln!("[thread_leader_exit] child PASS");
        std::process::exit(0);
    }
}

#[cfg(all(target_os = "linux", not(miri)))]
fn main() {
    let Some(sys_exit) = scenario::SYS_EXIT else {
        println!(
            "[thread_leader_exit] SKIPPED: no verified __NR_exit for arch \
             '{}' — the scenario refuses to guess a syscall number \
             (Sol-codex round 3, P2-1)",
            std::env::consts::ARCH
        );
        return;
    };
    if std::env::var_os(scenario::CHILD_MARKER).is_some() {
        scenario::assert_main_thread_is_leader();
        scenario::child_main(sys_exit);
    } else {
        scenario::runner_main();
    }
}

/// Every other host (Windows/macOS/other/miri) compiles an empty binary:
/// the scenario is a Linux kernel condition, and an empty `main` exits 0,
/// which `cargo test` reads as a pass — the same shape as the cfg-gated
/// stub tests elsewhere in this crate.
#[cfg(any(miri, not(target_os = "linux")))]
fn main() {}
