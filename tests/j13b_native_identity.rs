//! Host-format fixtures plus an explicitly activated native process-facts test.
#[path = "../examples/_shared/j13b_executable_identity.rs"]
mod executable_identity;
#[path = "../examples/_shared/j13b_host_facts.rs"]
mod host_facts;
#[path = "../examples/_shared/j13b_native_facts.rs"]
#[cfg_attr(any(miri, not(target_os = "linux")), allow(dead_code))]
mod native_facts;
use host_facts::HostFacts;
const BOOT: &str = "01234567-89ab-cdef-0123-456789abcdef";
fn host(cpu: &str, kernel: &str, boot: &str, thp: &str) -> std::io::Result<HostFacts> {
    HostFacts::parse(cpu, kernel, boot, thp, "x86_64", "x86_64")
}

#[test]
fn host_formats_preserve_actual_values_and_active_top_level_thp() {
    for (record, mode) in [
        ("[always] madvise never", "always"),
        ("always [madvise] never", "madvise"),
        ("always madvise [never]", "never"),
    ] {
        let facts = host(
            "processor : 0\nmodel name : fixture CPU λ\n",
            "kernel-fixture\n",
            BOOT,
            record,
        )
        .unwrap();
        assert_eq!(facts.cpu, "fixture CPU λ");
        assert_eq!(facts.kernel, "kernel-fixture");
        assert_eq!(facts.boot_id, BOOT);
        assert_eq!(facts.thp_top_level_mode, mode);
        let json = facts.to_json();
        assert_eq!(json["arch"], "x86_64");
        assert!(json["thp_policy_scope"]
            .as_str()
            .unwrap()
            .contains("per-size/defrag"));
    }
}

#[test]
fn cpu_model_requires_a_complete_clean_record_not_a_truncated_guess() {
    for cpu in [
        "",
        "processor : 0\n",
        "model name : truncated",
        "model name : \n",
        "model name : bad\0cpu\n",
    ] {
        assert!(host(cpu, "kernel", BOOT, "always [madvise] never").is_err());
    }
    assert_eq!(
        host(
            "Hardware : fixture ARM\n",
            "kernel",
            BOOT,
            "always [madvise] never"
        )
        .unwrap()
        .cpu,
        "fixture ARM"
    );
}

#[test]
fn malformed_kernel_and_boot_identity_never_become_unknown_defaults() {
    for kernel in ["", " \n", "bad\0kernel", "bad\nkernel"] {
        assert!(host("model name : cpu\n", kernel, BOOT, "always [madvise] never").is_err());
    }
    for boot in [
        "",
        "unknown",
        "0123456789ab-cdef-0123-456789abcdef",
        "01234567-89ab-CDEF-0123-456789abcdef",
    ] {
        assert!(host(
            "model name : cpu\n",
            "kernel",
            boot,
            "always [madvise] never"
        )
        .is_err());
    }
}

#[test]
fn malformed_or_ambiguous_thp_modes_are_unavailable_not_never() {
    for thp in [
        "",
        "always madvise never",
        "[always] [madvise] never",
        "always [madvise] madvise",
        "always [madvise]",
        "always [other] never",
        "always [madvise] never extra",
    ] {
        assert!(host("model name : cpu\n", "kernel", BOOT, thp).is_err());
    }
}

#[test]
fn architecture_mismatch_or_unverified_target_rejects_native_interpretation() {
    for (observed, compiled) in [
        ("x86_64", "aarch64"),
        ("aarch64", "x86_64"),
        ("unknown", "unknown"),
    ] {
        let error = HostFacts::parse(
            "model name : cpu\n",
            "kernel",
            BOOT,
            "always [madvise] never",
            observed,
            compiled,
        )
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
    }
}

#[test]
fn host_inputs_and_labels_have_fixed_byte_bounds() {
    let cpu = format!("model name : cpu\n{}", "x".repeat(16384));
    assert!(host(&cpu, "kernel", BOOT, "always [madvise] never").is_err());
    assert!(host(
        "model name : cpu\n",
        &"x".repeat(4097),
        BOOT,
        "always [madvise] never"
    )
    .is_err());
    assert!(host("model name : cpu\n", "kernel", BOOT, &"x".repeat(257)).is_err());
}

#[cfg(any(miri, not(target_os = "linux")))]
#[test]
fn process_facts_reject_models_before_files_commands_or_payload() {
    let error = native_facts::NativeFacts::collect(&"0".repeat(64)).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
    assert_eq!(
        executable_identity::executable_sha256().unwrap_err().kind(),
        std::io::ErrorKind::Unsupported
    );
    assert_eq!(
        HostFacts::observe().unwrap_err().kind(),
        std::io::ErrorKind::Unsupported
    );
}

#[cfg(all(not(miri), target_os = "linux"))]
#[test]
#[ignore = "requires dedicated native Linux process-facts CI activation"]
fn actual_running_image_and_host_are_observed_without_payload_or_source_claims() {
    let actual = executable_identity::executable_sha256().unwrap();
    let facts = match native_facts::NativeFacts::collect(&actual) {
        Ok(facts) => facts,
        Err(error) if error.kind() == std::io::ErrorKind::Unsupported => {
            eprintln!("native process facts unavailable: {error}");
            return;
        }
        Err(error) => panic!("native process facts failed: {error}"),
    };
    assert_eq!(facts.binary_sha256, actual);
    assert_eq!(facts.host.arch, std::env::consts::ARCH);
    assert!(!facts.host.cpu.is_empty() && !facts.host.kernel.is_empty());
    assert!(matches!(
        facts.host.thp_top_level_mode.as_str(),
        "always" | "madvise" | "never"
    ));
    assert_eq!(
        native_facts::NativeFacts::collect(&"0".repeat(64))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::InvalidInput
    );
    let record = serde_json::json!({"executable_path":std::env::current_exe().unwrap(), "observed":facts.to_json()});
    println!("[j13b-native-facts-json] {record}");
    println!("[j13b-native-facts] actual-image-and-host PASS");
}
