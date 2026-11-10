#![cfg(all(
    target_os = "linux",
    not(miri),
    feature = "production",
    feature = "alloc-stats",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

#[path = "../examples/g1_big_judge.rs"]
#[allow(dead_code)]
mod judge;

use std::ffi::OsString;

#[test]
fn always_executes_eligible_cycle_once() {
    let args = ["g1_big_judge", "cycle", "sefer", "8", "1", "1"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let vars = [
        (OsString::from("G1_BIG_THP"), OsString::from("always")),
        (OsString::from("G1_BIG_CACHE"), OsString::from("off")),
    ];
    assert!(judge::study_run(&args, vars).unwrap());
}
