//! Dispatch-shape fixtures only; valid labels are not actual artifact identity.
#[path = "../examples/_shared/j13b_artifact_request.rs"]
mod artifact_request;
use artifact_request::ArtifactRequest as Request;
use serde_json::{json, Value};

fn metadata(variant: &str) -> Value {
    let mut value = json!({
        "run_id": "ci-run", "artifact_id": "candidate", "variant": variant,
        "source_commit": "a".repeat(40), "source_sha256": "b".repeat(64),
        "binary_sha256": "c".repeat(64), "rustc": "recorded-rustc", "features": "production",
        "original_source_sha256": null, "overlay_sha256": null,
    });
    if variant == "static-control" {
        value["original_source_sha256"] = json!("d".repeat(64));
        value["overlay_sha256"] = json!("e".repeat(64));
    }
    value
}
fn parse(value: &Value, arm: &str) -> Result<Request, &'static str> {
    Request::parse(value.to_string().as_bytes(), arm)
}

#[test]
fn request_routes_all_arms_without_claiming_native_capabilities() {
    for (variant, arms) in [
        ("candidate", &["d", "n", "b", "a", "ab"][..]),
        ("static-control", &["f"][..]),
        ("baseline", &["base", "base2", "mi"][..]),
    ] {
        for arm in arms {
            let request = parse(&metadata(variant), arm).unwrap();
            assert_eq!(request.variant, variant);
            assert_eq!(request.run_id, "ci-run");
            assert_eq!(request.binary_sha256, "c".repeat(64));
        }
    }
    assert!(parse(&metadata("candidate"), "base").is_err());
    assert!(parse(&metadata("baseline"), "f").is_err());
    assert!(parse(&metadata("static-control"), "a").is_err());
    assert!(parse(&metadata("candidate"), "unknown").is_err());
}

#[test]
fn request_bounds_utf8_json_and_root_shape_before_any_payload() {
    for bytes in [b"".as_slice(), b"{", b"[]", b"null", &[0xff]] {
        assert!(Request::parse(bytes, "a").is_err());
    }
    let oversized = vec![b' '; Request::MAX_BYTES + 1];
    assert_eq!(
        Request::parse(&oversized, "a").unwrap_err(),
        "artifact_metadata_size"
    );
    let valid = metadata("candidate").to_string();
    let at_limit = format!("{valid}{}", " ".repeat(Request::MAX_BYTES - valid.len()));
    assert!(Request::parse(at_limit.as_bytes(), "a").is_ok());
}

#[test]
fn request_requires_exact_keys_and_scalar_types() {
    let original = metadata("candidate");
    for key in original.as_object().unwrap().keys() {
        let mut missing = original.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert!(parse(&missing, "a").is_err(), "missing {key}");
        let mut wrong = original.clone();
        wrong[key] = json!({"nested": true});
        assert!(parse(&wrong, "a").is_err(), "wrong type {key}");
    }
    let mut extra = original;
    extra["path"] = json!("not part of child provenance");
    assert!(parse(&extra, "a").is_err());
}

#[test]
fn duplicate_decoded_keys_and_escaped_aliases_are_rejected() {
    let valid = metadata("candidate").to_string();
    for duplicate in [
        "\"run_id\":\"other\",\"run_id\":",
        "\"run_id\":\"other\",\"\\u0072un_id\":",
    ] {
        let text = valid.replacen("\"run_id\":", duplicate, 1);
        assert_eq!(
            Request::parse(text.as_bytes(), "a").unwrap_err(),
            "artifact_duplicate_key"
        );
    }
}

#[test]
fn punctuation_and_unicode_inside_values_do_not_become_keys() {
    let mut value = metadata("candidate");
    value["run_id"] = json!("run-λ {} [] : \\\"run_id\\\":");
    value["rustc"] = json!("compiler \\\\ \"source\" {} :");
    let request = parse(&value, "a").unwrap();
    assert_eq!(request.run_id, value["run_id"].as_str().unwrap());
    assert_eq!(request.rustc, value["rustc"].as_str().unwrap());
}

#[test]
fn hashes_commits_identifiers_empty_and_nul_fields_are_strict() {
    for key in ["source_commit", "source_sha256", "binary_sha256"] {
        for invalid in ["", "a", "G", "A".repeat(64).as_str()] {
            let mut value = metadata("candidate");
            value[key] = json!(invalid);
            assert!(parse(&value, "a").is_err());
        }
    }
    for key in ["run_id", "artifact_id", "variant", "rustc", "features"] {
        for invalid in ["", " ", "nul\0value"] {
            let mut value = metadata("candidate");
            value[key] = json!(invalid);
            assert!(parse(&value, "a").is_err());
        }
    }
    for invalid in ["Upper", "../outside", "with space", &"a".repeat(65)] {
        let mut value = metadata("candidate");
        value["artifact_id"] = json!(invalid);
        assert!(parse(&value, "a").is_err());
    }
}

#[test]
fn overlay_fields_are_required_only_for_the_actual_static_variant() {
    for key in ["original_source_sha256", "overlay_sha256"] {
        let mut absent = metadata("static-control");
        absent[key] = Value::Null;
        assert!(parse(&absent, "f").is_err());
        let mut malformed = metadata("static-control");
        malformed[key] = json!("not-a-hash");
        assert!(parse(&malformed, "f").is_err());
        let mut forged = metadata("candidate");
        forged[key] = json!("a".repeat(64));
        assert!(parse(&forged, "a").is_err());
    }
    let mut unchanged = metadata("static-control");
    unchanged["original_source_sha256"] = unchanged["source_sha256"].clone();
    assert!(parse(&unchanged, "f").is_err());
}
