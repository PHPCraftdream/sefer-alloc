//! Phase A characterizes process::exit ordering, not a product fix.
//! System stays installed so harness/runtime traffic cannot bind E or initialize
//! fallback before the init witness. The operations under test call SeferAlloc's
//! real GlobalAlloc face explicitly. Phase B changes expectation to exit <=20s.
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::global::{self, tls_heap};
use sefer_alloc::registry::{segment_route::RouteDirectory, HeapCore};
use sefer_alloc::{AllocCore, SeferAlloc};
use std::alloc::{GlobalAlloc, Layout};
use std::cell::Cell;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const CHILD: &str = "SEFER_CON01_CHILD";

const BOUND: Duration = Duration::from_secs(20);
static ALLOC: SeferAlloc = SeferAlloc::new();
static HELD: AtomicBool = AtomicBool::new(false);
static RELEASE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
enum Scenario {
    Shard(usize),
    Fallback,
    Init,
}

thread_local! {
    static LATE: Cell<Option<Scenario>> = const { Cell::new(None) };
    static DTOR: Late = const { Late };
}
struct Late;

fn fail(reason: &str) -> ! {
    eprintln!("CON01_FAIL invalid: {reason}");
    std::process::exit(2)
}

impl Drop for Late {
    fn drop(&mut self) {
        let scenario = LATE.with(Cell::get).expect("armed scenario");
        let bound = tls_heap::current_for_trim().is_some();
        eprintln!("CON01_DTOR_ENTERED bound={bound}");
        if bound {
            fail("E still bound: TLS ordering/precondition violated");
        }
        let frees = AllocCore::dbg_foreign_or_unroutable_frees();
        let locks = global::dbg_fallback_lock_acquisitions();
        RELEASE.store(true, Ordering::Release);
        if matches!(scenario, Scenario::Init) {
            global::dbg_set_hold_fallback_init(false);
        }
        match scenario {
            Scenario::Shard(address) => {
                // SAFETY: sole free of the live allocation issued by ALLOC on
                // the child test thread; that owner stays alive until exit.
                unsafe { ALLOC.dealloc(address as *mut u8, Layout::new::<[u8; 64]>()) };
            }
            Scenario::Fallback | Scenario::Init => {
                // SAFETY: valid nonzero layout. Leak deliberately: a dealloc
                // would add another lock acquisition and obscure this oracle.
                let p = unsafe { ALLOC.alloc(Layout::new::<u64>()) };
                if p.is_null() {
                    fail("late allocation returned null");
                }
                eprintln!("CON01_ALLOC_SUCCESS");
            }
        }
        let lock_delta = global::dbg_fallback_lock_acquisitions() - locks;
        let free_delta = AllocCore::dbg_foreign_or_unroutable_frees() - frees;
        eprintln!("CON01_COUNTERS fallback_delta={lock_delta} unroutable_delta={free_delta}");
        if matches!(scenario, Scenario::Fallback) && lock_delta != 1 {
            fail("fallback delta lock acquisitions != 1");
        }
        if matches!(scenario, Scenario::Shard(_)) && (free_delta != 0 || lock_delta != 0) {
            fail("shard control did not publish through the live route");
        }
        eprintln!("CON01_DTOR_DONE");
    }
}

fn wait_for(mut ready: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ready() {
        if start.elapsed() >= Duration::from_secs(5) {
            fail("readiness deadline");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn hold() {
    HELD.store(true, Ordering::Release);
    while !RELEASE.load(Ordering::Acquire) {
        std::thread::park_timeout(Duration::from_millis(1));
    }
}

fn prepare(mode: &str) -> Scenario {
    match mode {
        "shard" => {
            // SAFETY: valid nonzero layout; retained until Late's unique free.
            let p = unsafe { ALLOC.alloc(Layout::new::<[u8; 64]>()) };
            if p.is_null() || RouteDirectory::global().lookup(p).is_none() {
                fail("missing issued shard route");
            }
            let address = p.addr();
            std::thread::spawn(move || {
                RouteDirectory::global()
                    .dbg_with_shard_of_locked_for_test(address as *mut u8, hold);
            });
            wait_for(|| HELD.load(Ordering::Acquire));
            Scenario::Shard(address)
        }
        "fallback" => {
            std::thread::spawn(|| {
                if HeapCore::dbg_with_fallback_for_test(|_| hold()).is_none() {
                    fail("fallback unavailable");
                }
            });
            wait_for(|| HELD.load(Ordering::Acquire));
            Scenario::Fallback
        }
        "init" => {
            if global::dbg_init_state() != global::STATE_UNINIT {
                fail("fallback already initialized");
            }
            global::dbg_set_hold_fallback_init(true);
            std::thread::spawn(|| {
                if HeapCore::dbg_with_fallback_for_test(|_| ()).is_none() {
                    fail("init winner OOM");
                }
            });
            wait_for(|| global::dbg_init_state() == global::STATE_INITIALIZING);
            let yields = global::dbg_fallback_init_wait_yields();
            // A second real caller proves the loser wait loop is active, not
            // merely an assumed hold. Neither caller touches Sefer TLS.
            std::thread::spawn(|| {
                if HeapCore::dbg_with_fallback_for_test(|_| ()).is_none() {
                    fail("init loser OOM");
                }
            });
            wait_for(|| global::dbg_fallback_init_wait_yields() > yields);
            if global::dbg_init_state() != global::STATE_INITIALIZING {
                fail("init hold lost");
            }
            eprintln!("CON01_INIT_READY state=INITIALIZING loser_yields_positive=true");
            Scenario::Init
        }
        _ => fail("unknown scenario"),
    }
}

fn arm(scenario: Scenario, mode: &str) {
    if tls_heap::current_for_trim().is_some() {
        fail("E unexpectedly bound before setup");
    }
    LATE.with(|cell| cell.set(Some(scenario)));
    DTOR.with(|_| ());
    if !matches!(scenario, Scenario::Shard(_)) {
        if !matches!(tls_heap::current_for_alloc(), tls_heap::CurrentHeap::Own(_)) {
            fail("E could not bind its empty registry heap");
        }
        if tls_heap::current_for_trim().is_none() {
            fail("E bind witness missing");
        }
    }
    eprintln!("CON01_READY scenario={mode}");
    std::io::stderr().flush().expect("ready flush");
}

#[cfg(test)]
fn child(mode: &str, live: bool) -> ! {
    let scenario = prepare(mode);
    // System runtime traffic cannot bind E. Late is armed BEFORE GUARD;
    // passive current_for_trim in Drop rejects an unexpected TLS order.
    let exiting = std::thread::spawn(move || {
        let mode = match scenario {
            Scenario::Shard(_) => "shard",
            Scenario::Fallback => "fallback",
            Scenario::Init => "init",
        };
        arm(scenario, mode);
        if !live {
            std::process::exit(0);
        }
        // Normal return: OS TLS destructors execute while H is still alive.
    });
    if live {
        exiting.join().expect("E normal return and TLS teardown");
        eprintln!("CON01_E_JOINED");
        std::process::exit(0);
    }
    // Keep the owner of p alive; do not join process-exit E or holder H.
    loop {
        std::thread::park();
    }
}

/// Arm main's TLS with a foreign allocation owned by a still-live worker.
/// Returning from this function is setup only; the caller must return main.
#[allow(dead_code)] // Used by the harness=false sibling, not libtest.
pub(crate) fn prepare_main_return() {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // SAFETY: valid nonzero layout; ownership transfers to main's Late
        // for exactly one foreign free. This owner never exits before main.
        let p = unsafe { ALLOC.alloc(Layout::new::<[u8; 64]>()) };
        if p.is_null() || RouteDirectory::global().lookup(p).is_none() {
            fail("main_return missing foreign route");
        }
        tx.send(p.addr()).expect("foreign allocation transfer");
        loop {
            std::thread::park();
        }
    });
    let address = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("foreign pointer");
    std::thread::spawn(move || {
        RouteDirectory::global().dbg_with_shard_of_locked_for_test(address as *mut u8, hold);
    });
    wait_for(|| HELD.load(Ordering::Acquire));
    arm(Scenario::Shard(address), "main_return");
}

fn reader(
    mut pipe: impl Read + Send + 'static,
) -> std::thread::JoinHandle<std::io::Result<String>> {
    std::thread::spawn(move || {
        const CAP: usize = 64 * 1024;
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        let mut overflow = false;
        loop {
            let n = pipe.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            let retained = n.min(CAP - bytes.len());
            bytes.extend_from_slice(&buffer[..retained]);
            overflow |= retained != n;
        }
        if overflow {
            return Err(std::io::Error::other("child output exceeded 64 KiB"));
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    })
}

// Kill and reap only our handle even if a reader/spawn/wait assertion panics.
struct OwnedChild(std::process::Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn classify(
    ready: bool,
    entered: bool,
    done: bool,
    invalid: bool,
    timed_out: bool,
    code: Option<i32>,
    live: bool,
) -> &'static str {
    if !ready || invalid || (live && timed_out) {
        "OTHER"
    } else if timed_out && entered && !done {
        "CONFIRMED_HANG"
    } else if !timed_out && code == Some(0) && entered && done {
        "REFUTED_NO_HANG"
    } else if !timed_out && code == Some(0) && !entered && !done {
        "DTORS_NOT_RUN"
    } else {
        "OTHER"
    }
}

/// Shared bounded parent: libtest process-exit, live-holder, or real main return.
pub(crate) fn run_scenario(mode: &str, test: Option<&str>, live: bool) -> bool {
    let exe = std::env::current_exe().expect("test executable");
    let start = Instant::now();
    let mut command = Command::new(&exe);
    if let Some(test) = test {
        command.args(["--exact", test, "--nocapture", "--test-threads=1"]);
    }
    let child = command
        .env(CHILD, mode)
        .env("SEFER_CON01_LIVE", if live { "1" } else { "0" })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("own child");
    let mut child = OwnedChild(child);
    let pid = child.0.id();
    let stdout = reader(child.0.stdout.take().expect("stdout pipe"));
    let stderr = reader(child.0.stderr.take().expect("stderr pipe"));
    let mut timed_out = false;
    let status = loop {
        match child.0.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < BOUND => {
                std::thread::sleep(Duration::from_millis(50));
            }
            result => {
                timed_out = matches!(result, Ok(None));
                let killed = child.0.kill();
                let status = child.0.wait().expect("reap own child");
                killed.expect("kill own child");
                if let Err(error) = result {
                    panic!("try_wait failed: {error}");
                }
                break status;
            }
        }
    };
    let elapsed = start.elapsed().as_millis();
    let stdout = stdout.join().expect("stdout reader").expect("stdout read");
    let stderr = stderr.join().expect("stderr reader").expect("stderr read");
    print!(
        "CON01_STDOUT scenario={mode} live={live}
{stdout}"
    );
    print!(
        "CON01_STDERR scenario={mode} live={live}
{stderr}"
    );
    let ready = stderr
        .lines()
        .any(|line| line == format!("CON01_READY scenario={mode}"));
    let entered = stderr
        .lines()
        .any(|line| line == "CON01_DTOR_ENTERED bound=false");
    let done = stderr.lines().any(|line| line == "CON01_DTOR_DONE");
    let invalid = stderr.contains("CON01_FAIL")
        || (mode == "init" && !stderr.contains("CON01_INIT_READY"))
        || (live && !stderr.lines().any(|line| line == "CON01_E_JOINED"));
    let verdict = classify(
        ready,
        entered,
        done,
        invalid,
        timed_out,
        status.code(),
        live,
    );
    println!("CON01_VERDICT {mode} {verdict} live={live} pid={pid} elapsed_ms={elapsed} status={status} timed_out={timed_out}");
    if live {
        verdict == "REFUTED_NO_HANG"
    } else if cfg!(windows) {
        verdict != "OTHER"
    } else {
        // No TLS dtors on process exit is an exit-0 no-hang control, not
        // evidence that RELEASE ran. Live controls always require DONE.
        status.code() == Some(0)
            && !timed_out
            && matches!(verdict, "REFUTED_NO_HANG" | "DTORS_NOT_RUN")
    }
}

#[cfg(test)]
#[test]
fn exit_stranded_lock() {
    if let Ok(mode) = std::env::var(CHILD) {
        child(&mode, false);
    }
    let mut valid = true;
    for mode in ["shard", "fallback", "init"] {
        valid &= run_scenario(mode, Some("exit_stranded_lock"), false);
    }
    assert!(valid, "invalid CON-01 characterization");
}

#[cfg(test)]
#[test]
fn live_holder_control() {
    if let Ok(mode) = std::env::var(CHILD) {
        child(&mode, true);
    }
    assert!(
        run_scenario("shard", Some("live_holder_control"), true),
        "expected REFUTED_NO_HANG live-holder shard control"
    );
}

#[cfg(test)]
#[test]
fn live_holder_fallback_init_controls() {
    if let Ok(mode) = std::env::var(CHILD) {
        child(&mode, true);
    }
    let mut valid = true;
    for mode in ["fallback", "init"] {
        valid &= run_scenario(mode, Some("live_holder_fallback_init_controls"), true);
    }
    assert!(
        valid,
        "expected REFUTED_NO_HANG live-holder fallback/init controls"
    );
}

#[cfg(test)]
#[test]
fn invalid_fallback_oracle_is_other() {
    // Even exit-0 + DONE cannot mask a failed fallback activation witness.
    assert_eq!(
        classify(true, true, true, true, false, Some(0), false),
        "OTHER"
    );
    assert_eq!(
        classify(true, true, true, false, false, Some(0), false),
        "REFUTED_NO_HANG"
    );
}
