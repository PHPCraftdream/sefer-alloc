//! Pure procfs ABI fixtures. No OS reads, counter resets or workloads are executed.
#[path = "../examples/_shared/j13b_os_probe.rs"]
#[allow(dead_code)]
mod os_probe;
#[path = "../examples/_shared/j13b_os_sample.rs"]
#[allow(dead_code)]
mod os_sample;
#[path = "support/j13b_peak_epoch_witness.rs"]
mod peak_witness;
use os_probe::OsProbe;
use peak_witness::PeakEpochWitness;

#[cfg(any(miri, not(target_os = "linux")))]
#[test]
fn unsupported_os_or_model_is_rejected_before_any_probe() {
    let error = match OsProbe::begin() {
        Ok(_) => panic!("non-native backend must not produce physical OS measurements"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
}

#[test]
fn faults_use_process_fields_not_child_fields_or_comm_whitespace() {
    let input = b"123 (name (x) y) R 1 2 3 4 5 6 7 8000 9 10000 11";
    assert_eq!(OsProbe::parse_faults(input).unwrap(), (7, 9));
    let input = b"123 (odd\xff) name) S 1 2 3 4 5 6 7 1 9 2 11";
    assert_eq!(OsProbe::parse_faults(input).unwrap(), (7, 9));
    let maximum = format!("123 (max) R 1 2 3 4 5 6 {} 0 {} 0", u64::MAX, u64::MAX);
    assert_eq!(
        OsProbe::parse_faults(maximum.as_bytes()).unwrap(),
        (u64::MAX, u64::MAX)
    );
}

#[test]
fn malformed_missing_signed_and_overflowed_faults_are_errors_not_zero() {
    for input in [
        "",
        "123 missing R 1 2 3 4 5 6 7 8 9",
        "0 (name) R 1 2 3 4 5 6 7 8 9",
        "123 (name) XX 1 2 3 4 5 6 7 8 9",
        "123 (name) R 1 2",
        "123 (name) R 1 2 3 4 5 6 -1 8 9",
        "123 (name) R 1 2 3 4 5 6 +1 8 9",
        "123 (name) R 1 2 3 4 5 6 1 8 invalid",
        "123 (name) R 1 2 3 4 5 6 18446744073709551616 8 9",
    ] {
        assert!(OsProbe::parse_faults(input.as_bytes()).is_err(), "{input}");
    }
}

#[test]
fn huge_bytes_have_explicit_units_and_genuine_zero_is_distinct_from_absence() {
    assert_eq!(
        OsProbe::parse_anon_huge("Rss: 8 kB\nAnonHugePages: 2048 kB\n").unwrap(),
        2 * 1024 * 1024
    );
    assert_eq!(
        OsProbe::parse_anon_huge("AnonHugePages: 0 kB\n").unwrap(),
        0
    );
    for input in [
        "",
        "Rss: 0 kB",
        "AnonHugePages: 1 MB",
        "AnonHugePages: 1 kB extra",
        "AnonHugePages: -1 kB",
        "AnonHugePages: +1 kB",
        "AnonHugePages: 18446744073709551615 kB",
        "AnonHugePages: 0 kB\nAnonHugePages: 1 kB",
    ] {
        assert!(OsProbe::parse_anon_huge(input).is_err(), "{input}");
    }
}

#[test]
fn fault_deltas_reject_decrease_or_wrap_without_saturation() {
    assert_eq!(OsProbe::fault_delta((7, 9), (17, 12)).unwrap(), (10, 3));
    assert_eq!(OsProbe::fault_delta((7, 9), (7, 9)).unwrap(), (0, 0));
    assert!(OsProbe::fault_delta((7, 9), (6, 9)).is_err());
    assert!(OsProbe::fault_delta((7, 9), (7, 8)).is_err());
    assert!(OsProbe::fault_delta((u64::MAX, 0), (0, 0)).is_err());
}

#[test]
fn peak_witness_rejects_wrong_sources_without_a_calibrated_accuracy_tolerance() {
    const MIB: u64 = 1024 * 1024;
    let valid = PeakEpochWitness {
        old_peak: 40 * MIB,
        reset_peak: 8 * MIB,
        start_rss: 8 * MIB,
        live_peak: 24 * MIB,
        end_rss: 8 * MIB,
        retained_peak: 23 * MIB,
    };
    // This synthetic difference is not an error bound. The kernel contract
    // permits separate reported readings to differ; only causal signals matter.
    assert!(valid.retained_peak < valid.live_peak);
    assert!(valid.validate().is_ok());
    for invalid in [
        PeakEpochWitness {
            reset_peak: valid.old_peak,
            ..valid
        },
        PeakEpochWitness {
            reset_peak: valid.old_peak - 4096,
            ..valid
        },
        PeakEpochWitness {
            retained_peak: valid.old_peak,
            ..valid
        },
        PeakEpochWitness {
            retained_peak: valid.old_peak - 4096,
            ..valid
        },
        PeakEpochWitness {
            retained_peak: valid.end_rss,
            ..valid
        },
        PeakEpochWitness {
            retained_peak: 0,
            ..valid
        },
        PeakEpochWitness {
            live_peak: valid.start_rss,
            ..valid
        },
        PeakEpochWitness {
            end_rss: u64::MAX,
            ..valid
        },
    ] {
        assert!(invalid.validate().is_err());
    }
}
