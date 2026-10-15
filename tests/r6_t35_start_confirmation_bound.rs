//! R6-T35: startup confirmation is bounded. A starter whose worker can never
//! publish RUNNING must not hang the caller forever: the internal startup
//! bound aborts it. Driven through the existing `START_PAUSE` injection,
//! which the same bound covers, so the counterfactual on the pre-fix tree is
//! a child that hangs forever and a parent deadline that fails the test.
#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

use std::process::Command;
use std::time::{Duration, Instant};

use sefer_alloc::global::MaintenanceService;
use sefer_alloc::SeferAlloc;

const CHILD: &str = "SEFER_R6_T35_START_BOUND_CHILD";
const PARENT_BOUND: Duration = Duration::from_secs(90);

#[test]
fn startup_bound_terminates_wedged_starter() {
    if std::env::var(CHILD).is_ok() {
        MaintenanceService::pause_start_for_test(true);
        // On a correct build this never returns normally: the injected pause
        // exceeds STARTUP_CONFIRM_BOUND and the starter aborts.
        let started = SeferAlloc::start_maintenance();
        if started.is_ok() || SeferAlloc::maintenance_running() {
            println!("STARTER_RETURNED_WITH_RUNNING_SERVICE");
        }
        return;
    }
    let exe = std::env::current_exe().expect("current test exe");
    let start = Instant::now();
    let mut child = Command::new(&exe)
        .arg("--exact")
        .arg("startup_bound_terminates_wedged_starter")
        .env(CHILD, "1")
        .spawn()
        .expect("spawn child");
    // RED on the pre-fix tree: the child hangs in the unbounded pause wait,
    // this deadline expires and the test fails instead of hanging.
    loop {
        match child.try_wait().expect("child status") {
            Some(status) => {
                assert!(
                    !status.success(),
                    "wedged starter exited successfully instead of aborting on the bound"
                );
                return;
            }
            None => {
                if start.elapsed() >= PARENT_BOUND {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("starter was not bounded by the startup confirmation bound");
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}
