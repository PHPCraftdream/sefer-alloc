//! Requires bounded process exit despite stranded allocator locks.
//! System stays installed so harness/runtime traffic cannot bind E or initialize
//! fallback before the init witness. The operations under test call SeferAlloc's
//! real GlobalAlloc face explicitly.
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::global::{self, tls_heap};
use sefer_alloc::registry::segment_route::{RouteKind, RoutePin, RouteRegistration};
use sefer_alloc::registry::{segment_route::RouteDirectory, HeapCore};
use sefer_alloc::{AllocCore, SeferAlloc};
use std::alloc::{GlobalAlloc, Layout};
use std::cell::{Cell, RefCell};
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
    ShardRealloc(usize),
    OwnOom,
    ExactShard(usize),
    Fallback,
    Init,
    Remove,
}

thread_local! {
    static REMOVE: RefCell<Option<(RouteRegistration<'static>, RoutePin)>> = const { RefCell::new(None) };
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
        let giveups = global::dbg_exit_lock_giveups();
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
            Scenario::ShardRealloc(address) => {
                let old = Layout::new::<[u8; 64]>();
                // SAFETY: sole owner of the live ALLOC-issued block prepared
                // for exactly this exit-path realloc; the result leaks.
                let grown = unsafe { ALLOC.realloc(address as *mut u8, old, 128) };
                if grown.is_null() {
                    // Platforms without the exit detector legitimately return
                    // null (route unavailable, no escape gate): control
                    // outcome, not a failure. Never `fail()` from a TLS dtor
                    // during exit (re-entrant exit aborts); the parent
                    // counters enforce the strict escape branch.
                    eprintln!("CON01_SHARD_REALLOC_NULL_NO_DETECTOR");
                } else {
                    // SAFETY: 128 bytes were just issued by realloc above.
                    let copied = unsafe { std::slice::from_raw_parts(grown, 64) };
                    assert!(copied.iter().all(|&b| b == 0xa5));
                    eprintln!("CON01_SHARD_REALLOC_OK");
                }
            }
            Scenario::OwnOom => {
                // Force the cold null path: the next route registration fails
                // as OOM. Where the exit detector is live (Windows) the gate
                // must escape the allocation to System; elsewhere null is the
                // expected control outcome. Never `fail()` from a TLS dtor
                // during exit (re-entrant exit aborts); the parent counters
                // require the escape marker, so a Windows null still fails.
                RouteDirectory::fail_next_registration_for_test();
                let layout = Layout::from_size_align(1024 * 1024, 4096).expect("oom layout");
                // SAFETY: valid nonzero layout; the escaped block leaks.
                let p = unsafe { ALLOC.alloc(layout) };
                if p.is_null() {
                    eprintln!("CON01_OWN_OOM_NULL_NO_DETECTOR");
                } else {
                    eprintln!("CON01_OWN_OOM_ESCAPE_OK");
                }
            }
            Scenario::ExactShard(address) => {
                // A dead-holder shard lock must refuse during exit: the free
                // degrades to a counted unroutable no-op instead of spinning.
                #[cfg(feature = "exact-object-proto")]
                {
                    // SAFETY: sole free of the narrow allocation prepared above.
                    unsafe { ALLOC.dealloc(address as *mut u8, Layout::new::<u64>()) };
                }
                #[cfg(not(feature = "exact-object-proto"))]
                {
                    let _ = address;
                }
                eprintln!("CON01_EXACT_SHARD_DEALLOC_OK");
            }
            Scenario::Remove => {
                let (registration, pin) = REMOVE
                    .with(|cell| cell.borrow_mut().take())
                    .expect("remove witness");
                assert_eq!(RouteDirectory::dbg_route_pin_refs(&pin), 2);
                drop(registration);
                let refs = RouteDirectory::dbg_route_pin_refs(&pin);
                eprintln!("CON01_REMOVE owner_refs={refs}");
                assert_eq!(
                    refs,
                    if cfg!(windows) { 2 } else { 1 },
                    "failed unlink must retain owner count"
                );
                drop(pin);
            }
            Scenario::Fallback | Scenario::Init => {
                // SAFETY: valid nonzero layout. Leak deliberately: a dealloc
                // would add another lock acquisition and obscure this oracle.
                let p = unsafe { ALLOC.alloc(Layout::new::<u64>()) };
                if p.is_null() {
                    fail("late allocation returned null");
                }
                eprintln!("CON01_ALLOC_SUCCESS");
                if matches!(scenario, Scenario::Fallback) {
                    let layout = Layout::from_size_align(32, 64).expect("escape layout");
                    // SAFETY: valid nonzero layout; each successful block is uniquely
                    // owned here, initialized before realloc and freed once.
                    unsafe {
                        let zeroed = ALLOC.alloc_zeroed(layout);
                        assert!(!zeroed.is_null());
                        assert_eq!(zeroed.addr() % 64, 0);
                        assert!(std::slice::from_raw_parts(zeroed, 32)
                            .iter()
                            .all(|&b| b == 0));
                        zeroed.write_bytes(0x5a, 32);
                        let grown = ALLOC.realloc(zeroed, layout, 96);
                        assert!(!grown.is_null());
                        assert_eq!(grown.addr() % 64, 0);
                        assert!(std::slice::from_raw_parts(grown, 32)
                            .iter()
                            .all(|&b| b == 0x5a));
                        let before_free = AllocCore::dbg_foreign_or_unroutable_frees();
                        ALLOC.dealloc(
                            grown,
                            Layout::from_size_align(96, 64).expect("grown layout"),
                        );
                        let free_delta = AllocCore::dbg_foreign_or_unroutable_frees() - before_free;
                        assert_eq!(
                            free_delta,
                            u64::from(
                                cfg!(windows)
                                    && std::env::var_os("SEFER_CON01_LIVE").as_deref()
                                        == Some(std::ffi::OsStr::new("0"))
                            )
                        );
                        ALLOC.dealloc(p, Layout::new::<u64>());
                    }
                    eprintln!("CON01_ESCAPE_ZEROED_REALLOC_FREE_OK");
                }
            }
        }
        let giveup_delta = global::dbg_exit_lock_giveups() - giveups;
        eprintln!("CON01_GIVEUPS delta={giveup_delta}");
        let lock_delta = global::dbg_fallback_lock_acquisitions() - locks;
        let free_delta = AllocCore::dbg_foreign_or_unroutable_frees() - frees;
        eprintln!("CON01_COUNTERS fallback_delta={lock_delta} unroutable_delta={free_delta}");
        let windows_exit = cfg!(windows)
            && std::env::var_os("SEFER_CON01_LIVE").as_deref() == Some(std::ffi::OsStr::new("0"));
        // `exact-object-proto` serves the narrow u64 alloc and its free without the
        // fallback lock; the 32-byte, 64-aligned zeroed/realloc/free still take it.
        let narrow_without_lock = if cfg!(feature = "exact-object-proto") {
            2
        } else {
            0
        };
        let expected_locks = if windows_exit {
            0
        } else {
            5 - narrow_without_lock
        };
        if matches!(scenario, Scenario::Fallback) && lock_delta != expected_locks {
            fail("fallback lock acquisition delta violated live/exit contract");
        }
        let expected_frees = u64::from(windows_exit);
        if !windows_exit && giveup_delta != 0 {
            fail("live/non-Windows control unexpectedly abandoned a wait");
        }
        if matches!(scenario, Scenario::Shard(_))
            && (free_delta != expected_frees || lock_delta != 0 || giveup_delta != expected_frees)
        {
            fail("shard unroutable delta violated live/exit contract");
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
        "remove" => Scenario::Remove,
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
        "shard_realloc" => {
            // SAFETY: valid nonzero layout; retained until Late's realloc.
            let p = unsafe { ALLOC.alloc(Layout::new::<[u8; 64]>()) };
            if p.is_null() || RouteDirectory::global().lookup(p).is_none() {
                fail("shard_realloc missing issued route");
            }
            // SAFETY: the block is a live ALLOC-issued 64-byte allocation
            // owned solely by this test until the exit-path realloc.
            unsafe { p.cast::<[u8; 64]>().write([0xa5u8; 64]) };
            let address = p.addr();
            std::thread::spawn(move || {
                RouteDirectory::global()
                    .dbg_with_shard_of_locked_for_test(address as *mut u8, hold);
            });
            wait_for(|| HELD.load(Ordering::Acquire));
            Scenario::ShardRealloc(address)
        }
        "own_oom" => Scenario::OwnOom,
        "exact_shard" => {
            #[cfg(feature = "exact-object-proto")]
            {
                // SAFETY: valid nonzero layout; narrow requests are served by
                // ExactNarrow from System and registered in the exact table.
                let p = unsafe { ALLOC.alloc(Layout::new::<u64>()) };
                if p.is_null() {
                    fail("exact_shard narrow alloc failed");
                }
                let address = p.addr();
                std::thread::spawn(move || {
                    sefer_alloc::global::exact_object::ExactNarrow
                        ::dbg_with_exact_shard_locked_for_test(address, hold);
                });
                wait_for(|| HELD.load(Ordering::Acquire));
                Scenario::ExactShard(address)
            }
            #[cfg(not(feature = "exact-object-proto"))]
            {
                fail("exact_shard requires exact-object-proto");
            }
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
    if matches!(scenario, Scenario::Remove) {
        let size = 4 * 1024 * 1024;
        let layout = Layout::from_size_align(size, size).expect("route layout");
        // SAFETY: nonzero System layout. Reservation remains allocated through
        // process exit; only the independent route descriptor is dropped.
        let root = unsafe { std::alloc::System.alloc(layout) };
        assert!(!root.is_null());
        let directory = RouteDirectory::global();
        let registration = directory
            .register(root, size, root, 0, RouteKind::Large)
            .expect("test route");
        let pin = directory.lookup(root).expect("retained pin");
        REMOVE.with(|cell| *cell.borrow_mut() = Some((registration, pin)));
        let address = root.addr();
        std::thread::spawn(move || {
            RouteDirectory::global().dbg_with_shard_of_locked_for_test(address as *mut u8, hold);
        });
        wait_for(|| HELD.load(Ordering::Acquire));
    }
    LATE.with(|cell| cell.set(Some(scenario)));
    DTOR.with(|_| ());
    if !matches!(
        scenario,
        Scenario::Shard(_) | Scenario::ExactShard(_) | Scenario::Remove
    ) {
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
            Scenario::ShardRealloc(_) => "shard_realloc",
            Scenario::OwnOom => "own_oom",
            Scenario::ExactShard(_) => "exact_shard",
            Scenario::Fallback => "fallback",
            Scenario::Init => "init",
            Scenario::Remove => "remove",
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
        verdict == "REFUTED_NO_HANG" && stderr.lines().any(|line| line == "CON01_GIVEUPS delta=0")
    } else if cfg!(windows) {
        let counters = match mode {
            "shard" | "main_return" => {
                stderr
                    .lines()
                    .any(|line| line == "CON01_COUNTERS fallback_delta=0 unroutable_delta=1")
                    && stderr.lines().any(|line| line == "CON01_GIVEUPS delta=1")
            }
            "remove" => {
                stderr
                    .lines()
                    .any(|line| line == "CON01_REMOVE owner_refs=2")
                    && stderr.lines().any(|line| line == "CON01_GIVEUPS delta=1")
            }
            "fallback" => {
                stderr.lines().any(|line| line == "CON01_ALLOC_SUCCESS")
                    && stderr
                        .lines()
                        .any(|line| line == "CON01_ESCAPE_ZEROED_REALLOC_FREE_OK")
            }
            "shard_realloc" => {
                stderr.lines().any(|line| line == "CON01_SHARD_REALLOC_OK")
                    && stderr.lines().any(|line| line == "CON01_GIVEUPS delta=1")
            }
            "own_oom" => stderr.lines().any(|line| line == "CON01_OWN_OOM_ESCAPE_OK"),
            "exact_shard" => {
                stderr
                    .lines()
                    .any(|line| line == "CON01_EXACT_SHARD_DEALLOC_OK")
                    && stderr.lines().any(|line| line == "CON01_GIVEUPS delta=1")
            }
            "init" => stderr.lines().any(|line| line == "CON01_ALLOC_SUCCESS"),
            _ => false,
        };
        verdict == "REFUTED_NO_HANG" && elapsed <= BOUND.as_millis() && counters
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
    assert!(
        valid,
        "expected bounded CON-01 exit within 20s with exit-path witnesses"
    );
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
fn exit_remove_owner_retained() {
    if let Ok(mode) = std::env::var(CHILD) {
        child(&mode, false);
    }
    assert!(run_scenario(
        "remove",
        Some("exit_remove_owner_retained"),
        false
    ));
}

#[cfg(test)]
#[test]
fn exit_fallback_escape() {
    if let Ok(mode) = std::env::var(CHILD) {
        child(&mode, false);
    }
    assert!(run_scenario(
        "fallback",
        Some("exit_fallback_escape"),
        false
    ));
}

#[cfg(test)]
#[test]
fn exit_shard_realloc_escape() {
    if let Ok(mode) = std::env::var(CHILD) {
        child(&mode, false);
    }
    assert!(run_scenario(
        "shard_realloc",
        Some("exit_shard_realloc_escape"),
        false
    ));
}

#[cfg(test)]
#[test]
fn exit_own_oom_escape() {
    if let Ok(mode) = std::env::var(CHILD) {
        child(&mode, false);
    }
    assert!(run_scenario("own_oom", Some("exit_own_oom_escape"), false));
}

#[cfg(all(test, feature = "exact-object-proto"))]
#[test]
fn exit_exact_shard_refusal() {
    if let Ok(mode) = std::env::var(CHILD) {
        child(&mode, false);
    }
    assert!(run_scenario(
        "exact_shard",
        Some("exit_exact_shard_refusal"),
        false
    ));
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
