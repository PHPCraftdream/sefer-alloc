#[path = "../examples/g1_big_judge.rs"]
#[allow(dead_code)]
mod judge;

use std::ffi::OsString;

fn vars(mode: &str, cache: &str) -> Vec<(OsString, OsString)> {
    vec![
        (OsString::from("G1_BIG_THP"), OsString::from(mode)),
        (OsString::from("G1_BIG_CACHE"), OsString::from(cache)),
    ]
}

fn args(scenario: &str, arm: &str, mib: usize, reps: usize, k: usize) -> Vec<String> {
    vec![
        "g1_big_judge".into(),
        scenario.into(),
        arm.into(),
        mib.to_string(),
        reps.to_string(),
        k.to_string(),
    ]
}

fn configured(mode: &str, cache: &str, size: usize, reps: usize) -> judge::g1_big_study::Request {
    judge::g1_big_study::parse_request(&args("cycle", "sefer", size, reps, 1), vars(mode, cache))
        .unwrap()
        .expect("study config")
}

#[test]
fn no_environment_is_legacy() {
    assert!(judge::g1_big_study::parse_request(
        &args("cycle", "sefer", 1, 1, 1),
        std::iter::empty()
    )
    .unwrap()
    .is_none());
}

#[test]
fn parser_rejects_partial_unknown_duplicate_nonunicode_and_bad_values() {
    assert!(judge::g1_big_study::parse_request(
        &args("cycle", "sefer", 8, 1, 1),
        vars("always", "off").into_iter().take(1)
    )
    .is_err());
    assert!(judge::g1_big_study::parse_request(
        &args("cycle", "sefer", 8, 1, 1),
        vars("off", "off")
            .into_iter()
            .chain([(OsString::from("G1_BIG_EXTRA"), OsString::from("x"))])
    )
    .is_err());
    assert!(judge::g1_big_study::parse_request(
        &args("cycle", "sefer", 8, 1, 1),
        vars("off", "off")
            .into_iter()
            .chain([(OsString::from("G1_BIG_THP"), OsString::from("always"))])
    )
    .is_err());
    assert!(judge::g1_big_study::parse_request(
        &args("cycle", "sefer", 8, 1, 1),
        vars("invalid", "off")
    )
    .is_err());
    assert!(judge::g1_big_study::parse_request(
        &args("cycle", "sefer", 8, 1, 1),
        vars("off", "invalid")
    )
    .is_err());
    assert!(
        judge::g1_big_study::parse_request(&args("cycle", "mi", 8, 1, 1), vars("off", "off"))
            .is_err()
    );
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        assert!(judge::g1_big_study::parse_request(
            &args("cycle", "sefer", 8, 1, 1),
            vars("off", "off").into_iter().chain([(
                OsString::from_vec(b"G1_BIG_\xff".to_vec()),
                OsString::from("x"),
            )])
        )
        .is_err());
        assert!(judge::g1_big_study::parse_request(
            &args("cycle", "sefer", 8, 1, 1),
            vars("off", "off").into_iter().chain([(
                OsString::from("G1_BIG_BYTES"),
                OsString::from_vec(vec![0xff]),
            )])
        )
        .is_err());
    }
}

#[test]
fn byte_override_coherence_and_repetition_contract_are_exact() {
    let mut edge = vars("always", "off");
    edge.push((
        OsString::from("G1_BIG_BYTES"),
        OsString::from((8 * 1024 * 1024 - 1).to_string()),
    ));
    let request = judge::g1_big_study::parse_request(&args("cycle", "sefer", 8, 1, 1), edge)
        .unwrap()
        .unwrap();
    assert_eq!(request.bytes, 8 * 1024 * 1024 - 1);

    for invalid in ["", "0", "+8388608", "7340032", "8388609"] {
        let mut supplied = vars("always", "off");
        supplied.push((OsString::from("G1_BIG_BYTES"), OsString::from(invalid)));
        assert!(
            judge::g1_big_study::parse_request(&args("cycle", "sefer", 8, 1, 1), supplied).is_err(),
            "accepted invalid byte override {invalid:?}"
        );
    }
    assert!(judge::g1_big_study::parse_request(
        &args("cycle", "sefer", 8, 2, 1),
        vars("off", "off")
    )
    .is_err());
    assert!(judge::g1_big_study::parse_request(
        &args("cycle", "sefer", 8, 2, 1),
        vars("always", "off")
    )
    .is_err());
}

fn capture(records: Vec<aligned_vmem::AdviceEvent>) -> aligned_vmem::AdviceCaptureSnapshot {
    aligned_vmem::AdviceCaptureSnapshot {
        encoding_version: 1,
        boundary: u64::try_from(records.len()).unwrap(),
        records,
        slot_storage_bytes: 0,
        total_storage_bytes: 0,
        tls_context_bytes_per_thread: 0,
    }
}

fn fresh(requested: usize, mode: u8) -> aligned_vmem::AdviceEvent {
    aligned_vmem::AdviceEvent::fresh(
        0x100000,
        32 * 1024 * 1024,
        0x100000,
        16 * 1024 * 1024,
        requested,
        8 * 1024 * 1024,
        mode,
    )
    .unwrap()
}

#[test]
fn validator_rejects_empty_eligible_missing_advice_and_wrong_accepted_prefix() {
    let request = configured("always", "off", 8, 1);
    assert!(judge::g1_big_study::validate_capture(&request, &capture(Vec::new())).is_err());

    let only_fresh = capture(vec![fresh(8 * 1024 * 1024, 1)]);
    assert!(judge::g1_big_study::validate_capture(&request, &only_fresh).is_err());

    let accepted = aligned_vmem::AdviceEvent::accepted(
        0x100000,
        32 * 1024 * 1024,
        0x100000,
        16 * 1024 * 1024,
        8 * 1024 * 1024,
        15 * 1024 * 1024,
        1,
    )
    .unwrap();
    let wrong_prefix = capture(vec![fresh(8 * 1024 * 1024, 1), accepted]);
    assert!(judge::g1_big_study::validate_capture(&request, &wrong_prefix).is_err());
}

#[test]
fn cache_only_admits_two_16_mib_cycles() {
    let request = configured("always", "on", 16, 2);
    assert_eq!(request.mode, sefer_alloc::LargeThpMode::Always);
    assert!(request.cache);
    assert!(judge::g1_big_study::parse_request(
        &args("cycle", "sefer", 16, 1, 1),
        vars("off", "on")
    )
    .is_err());
    assert!(judge::g1_big_study::parse_request(
        &args("many", "sefer", 16, 2, 1),
        vars("off", "on")
    )
    .is_err());
}

#[cfg(feature = "numa-aware")]
#[test]
fn numa_runtime_admission_refuses_before_payload_execution() {
    let study_args = args("cycle", "sefer", 8, 1, 1);
    let study_vars = vars("off", "off");
    assert!(matches!(
        judge::g1_big_study::parse_request(&study_args, study_vars.clone()),
        Ok(Some(_))
    ));
    assert_eq!(
        judge::study_run(&study_args, study_vars),
        Err("G1 big study runtime admission does not support numa-aware".into())
    );
}
