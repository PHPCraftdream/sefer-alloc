//! Real global-allocator acceptance in isolated processes: no test may stop a
//! process-lifetime worker used by another consumer. Pass acknowledgements, not
//! sleeps, establish worker progress; timeouts bound failures only.
#![cfg(all(feature = "alloc-global", feature = "internals"))]

#[cfg(feature = "bench-internals")]
use std::alloc::{GlobalAlloc, Layout};
use std::process::Command;
use std::time::{Duration, Instant};

use sefer_alloc::global::MaintenanceService;
use sefer_alloc::registry::segment_route::RouteDirectory;
use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const CHILD: &str = "SEFER_R8_MAINTENANCE_CHILD";
const TEST: &str = "autonomous_maintenance_global_alloc";
const BOUND: Duration = Duration::from_secs(15);

fn has_route(address: usize) -> bool {
    RouteDirectory::global()
        .lookup(core::ptr::without_provenance_mut::<u8>(address))
        .is_some()
}

fn wait_for_retirement(address: usize) {
    let deadline = Instant::now() + BOUND;
    loop {
        let before = MaintenanceService::passes_for_test();
        if !has_route(address) {
            // Unlinking can precede the OS release inside trim. Observe an
            // absent route first, then require a completed worker pass strictly
            // after this fresh acknowledgement snapshot. This is logical
            // retirement plus completed trim, not standalone OS-release proof.
            let after_unlink = MaintenanceService::passes_for_test();
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                MaintenanceService::wait_after_for_test(after_unlink, remaining),
                "worker did not acknowledge a completed pass after unlink"
            );
            assert!(!has_route(address), "retired route reappeared");
            return;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            MaintenanceService::wait_after_for_test(before, remaining),
            "worker stopped progressing"
        );
        assert!(
            Instant::now() < deadline || !has_route(address),
            "issued route was not retired"
        );
    }
}

fn ownerless() {
    assert!(!SeferAlloc::maintenance_running());
    SeferAlloc::start_maintenance().expect("explicit activation");
    assert!(SeferAlloc::maintenance_running());
    SeferAlloc::start_maintenance().expect("idempotent activation");

    // The owner exits holding only these user allocations. Joining recycles
    // its slot before the final frees. Box carries the actual allocation
    // provenance across threads, not an invented metadata pointer.
    let (small, large) = std::thread::spawn(|| {
        // Fill beyond the primordial reservation so the retained user block
        // belongs to a releasable ordinary Small segment, not the permanent
        // metadata carrier. This is allocation traffic, not background load.
        let count = 2 * sefer_alloc::SegmentLayout::SEGMENT / 2048;
        let mut blocks = Vec::with_capacity(count);
        for _ in 0..count {
            blocks.push(vec![0xa5u8; 2048].into_boxed_slice());
        }
        let small = blocks.pop().expect("retained last small block");
        drop(blocks);
        (small, vec![0x5au8; 5 * 1024 * 1024].into_boxed_slice())
    })
    .join()
    .expect("owner exits normally");
    let small_address = small.as_ptr().addr();
    let large_address = large.as_ptr().addr();
    assert!(has_route(small_address));
    assert!(has_route(large_address));
    let small_pin = RouteDirectory::global()
        .lookup(core::ptr::without_provenance_mut::<u8>(small_address))
        .expect("retained small route");
    assert_eq!(
        small_pin.kind(),
        sefer_alloc::registry::segment_route::RouteKind::Small
    );
    drop(small_pin);
    assert_eq!(small[2047], 0xa5);
    assert_eq!(large[large.len() - 1], 0x5a);

    drop(small);
    drop(large); // Last correct free; no alloc, claim or new thread below.
    wait_for_retirement(large_address);
    #[cfg(feature = "alloc-decommit")]
    wait_for_retirement(small_address);
    println!("RETIRED_WITHOUT_ALLOCATOR_CALL");
}

#[cfg(feature = "bench-internals")]
fn fallback_busy_then_idle() {
    use sefer_alloc::registry::HeapCore;

    let layout = Layout::from_size_align(5 * 1024 * 1024, 16).expect("valid layout");
    let pointer = HeapCore::dbg_with_fallback_for_test(|heap| heap.alloc(layout))
        .expect("fallback initialization");
    assert!(!pointer.is_null());
    let address = pointer.addr();
    assert!(has_route(address));
    SeferAlloc::start_maintenance().expect("activation");

    HeapCore::dbg_with_fallback_for_test(|_| {
        // SAFETY: unique live fallback allocation with its original layout.
        // The fallback try-lock is busy, so this must publish via the sidecar.
        unsafe { GlobalAlloc::dealloc(&GLOBAL, pointer, layout) };
        let before = MaintenanceService::passes_for_test();
        assert!(MaintenanceService::wait_after_for_test(before, BOUND));
        // Any acquisition preceding our lock is acknowledged by this point.
        let visits = MaintenanceService::fallback_visits_for_test();
        let before = MaintenanceService::passes_for_test();
        assert!(MaintenanceService::wait_after_for_test(before, BOUND));
        assert_eq!(MaintenanceService::fallback_visits_for_test(), visits);
        assert!(has_route(address), "busy fallback must not be taken over");
    })
    .expect("fallback lock");
    // No allocation or claim after unlocking; a periodic pass must retry.
    wait_for_retirement(address);
    println!("FALLBACK_RETIRED_AFTER_UNLOCK");
}

#[cfg(feature = "bench-internals")]
fn startup_failure_and_race() {
    use sefer_alloc::global::MaintenanceStartError;

    MaintenanceService::fail_next_start_for_test();
    assert!(matches!(
        SeferAlloc::start_maintenance(),
        Err(MaintenanceStartError::Spawn(_))
    ));
    assert!(!SeferAlloc::maintenance_running());

    // A deterministic startup gate covers both concurrent and reentrant
    // callers: no silent success and no wait on a starter that is paused.
    MaintenanceService::pause_start_for_test(true);
    struct ReleasePause;
    impl Drop for ReleasePause {
        fn drop(&mut self) {
            MaintenanceService::pause_start_for_test(false);
        }
    }
    let release = ReleasePause;
    let starter = std::thread::spawn(SeferAlloc::start_maintenance);
    assert!(MaintenanceService::wait_start_paused_for_test(BOUND));
    assert!(matches!(
        SeferAlloc::start_maintenance(),
        Err(MaintenanceStartError::InProgress)
    ));
    assert!(!SeferAlloc::maintenance_running());
    drop(release);
    starter.join().expect("starter").expect("retry activation");
    assert!(SeferAlloc::maintenance_running());
    SeferAlloc::start_maintenance().expect("repeated start");
    println!("STARTUP_FAILURE_RACE_AND_RETRY");
}

#[cfg(feature = "bench-internals")]
fn terminal_worker_failure(unwind: bool) {
    SeferAlloc::start_maintenance().expect("activation");
    assert!(SeferAlloc::maintenance_running());
    if unwind {
        // Avoid test logging/allocation in the injected panic hook. Production
        // terminal failure itself performs no logging and never returns.
        std::panic::set_hook(Box::new(|_| {}));
    }
    println!("RUNNING_BEFORE_TERMINAL_FAILURE");
    MaintenanceService::fail_worker_for_test(unwind);
    let before = MaintenanceService::passes_for_test();
    let deadline = Instant::now() + BOUND;
    while Instant::now() < deadline {
        let _ = MaintenanceService::wait_after_for_test(before, Duration::from_secs(1));
    }
    panic!("SURVIVED_TERMINAL_FAILURE");
}

fn child_command(exe: &std::path::Path) -> Command {
    // Cargo's target runner does not propagate to subprocesses automatically.
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    if let Ok(runner) = std::env::var("CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER") {
        let mut words = runner.split_ascii_whitespace();
        let mut command = Command::new(words.next().expect("nonempty target runner"));
        command.args(words).arg(exe);
        return command;
    }
    Command::new(exe)
}

#[test]
fn autonomous_maintenance_global_alloc() {
    if let Some(mode) = std::env::var_os(CHILD) {
        match mode.to_str().expect("scenario name") {
            "ownerless" => ownerless(),
            #[cfg(feature = "bench-internals")]
            "fallback" => fallback_busy_then_idle(),
            #[cfg(feature = "bench-internals")]
            "startup" => startup_failure_and_race(),
            #[cfg(feature = "bench-internals")]
            "return" => terminal_worker_failure(false),
            #[cfg(feature = "bench-internals")]
            "unwind" => terminal_worker_failure(true),
            _ => panic!("unknown scenario"),
        }
        return;
    }
    let exe = std::env::current_exe().expect("test binary");
    #[cfg(not(feature = "bench-internals"))]
    let scenarios = [("ownerless", "RETIRED_WITHOUT_ALLOCATOR_CALL", false)];
    #[cfg(feature = "bench-internals")]
    let scenarios = [
        ("ownerless", "RETIRED_WITHOUT_ALLOCATOR_CALL", false),
        ("fallback", "FALLBACK_RETIRED_AFTER_UNLOCK", false),
        ("startup", "STARTUP_FAILURE_RACE_AND_RETRY", false),
        ("return", "RUNNING_BEFORE_TERMINAL_FAILURE", true),
        ("unwind", "RUNNING_BEFORE_TERMINAL_FAILURE", true),
    ];
    for (mode, marker, terminal) in scenarios {
        let out = child_command(&exe)
            .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD, mode)
            .output()
            .expect("scenario subprocess");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stdout.contains(marker),
            "scenario {mode} missed its behavior oracle: {stdout}\n{stderr}"
        );
        if terminal {
            assert!(!out.status.success());
            assert_ne!(
                out.status.code(),
                Some(101),
                "test panic is not terminal service abort: {stderr}"
            );
            assert!(!stderr.contains("SURVIVED_TERMINAL_FAILURE"));
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                assert_eq!(out.status.signal(), Some(6), "expected SIGABRT");
            }
        } else {
            assert!(
                out.status.success(),
                "scenario {mode}: {:?}\n{stdout}\n{stderr}",
                out.status
            );
        }
    }
}
