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
fn cache_executes_two_cycles_hits_on_second_and_emits_no_second_advice() {
    let args = ["g1_big_judge", "cycle", "sefer", "16", "2", "1"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let vars = [
        (OsString::from("G1_BIG_THP"), OsString::from("always")),
        (OsString::from("G1_BIG_CACHE"), OsString::from("on")),
    ];
    assert!(judge::study_run(&args, vars).unwrap());
}
