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
fn native_off_cycle_executes_real_allocator_and_readback_rejects_always() {
    let args = ["g1_big_judge", "cycle", "sefer", "8", "1", "1"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let vars = [
        (OsString::from("G1_BIG_THP"), OsString::from("off")),
        (OsString::from("G1_BIG_CACHE"), OsString::from("off")),
    ];
    let request_off = judge::g1_big_study::parse_request(&args, vars.clone())
        .unwrap()
        .unwrap();
    assert!(judge::study_run(&args, vars).unwrap());
    let allocator_off =
        sefer_alloc::SeferAlloc::with_config(judge::g1_big_study::expected_config(&request_off));
    assert!(judge::g1_big_study::configuration_readback_matches(
        &request_off,
        &allocator_off
    ));

    let request_always = judge::g1_big_study::parse_request(
        &args,
        [
            (OsString::from("G1_BIG_THP"), OsString::from("always")),
            (OsString::from("G1_BIG_CACHE"), OsString::from("off")),
        ],
    )
    .unwrap()
    .unwrap();
    assert!(!judge::g1_big_study::configuration_readback_matches(
        &request_always,
        &allocator_off
    ));
}
