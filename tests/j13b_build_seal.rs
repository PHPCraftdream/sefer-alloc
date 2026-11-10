//! Compiled metadata consistency fixtures, not actual executable/host authenticity.
#[path = "../examples/_shared/j13b_artifact_request.rs"]
mod artifact_request;
#[path = "../examples/_shared/j13b_build_seal.rs"]
mod build_seal;
#[path = "../examples/_shared/j13b_strict_object.rs"]
mod strict_object;
use artifact_request::ArtifactRequest as Request;
use build_seal::BuildSeal;
use serde_json::{json, Value};

fn dispatch(variant: &str) -> Value {
    let mut value = json!({ "run_id":"r", "artifact_id":"artifact", "variant":variant,
        "source_commit":"a".repeat(40), "source_sha256":"b".repeat(64), "binary_sha256":"c".repeat(64),
        "rustc":"compiler λ", "features":"production internals", "original_source_sha256":null, "overlay_sha256":null });
    if variant == "static-control" {
        value["original_source_sha256"] = json!("d".repeat(64));
        value["overlay_sha256"] = json!("e".repeat(64));
    }
    value
}
fn seal(dispatch: &Value) -> Value {
    let mut value = dispatch.clone();
    for key in ["run_id", "artifact_id", "binary_sha256"] {
        value.as_object_mut().unwrap().remove(key);
    }
    value
}
fn request(dispatch: &Value, arm: &str) -> Request {
    Request::parse(dispatch.to_string().as_bytes(), arm).unwrap()
}
fn bind(value: &Value, request: &Request) -> Result<BuildSeal, &'static str> {
    BuildSeal::parse_and_match(value.to_string().as_bytes(), request)
}

#[test]
fn matching_static_build_metadata_does_not_claim_running_image_identity() {
    for (variant, arm) in [
        ("candidate", "a"),
        ("static-control", "f"),
        ("baseline", "base"),
    ] {
        let dispatch = dispatch(variant);
        let request = request(&dispatch, arm);
        let bound = bind(&seal(&dispatch), &request).unwrap();
        assert_eq!(bound.source_commit, request.source_commit);
        assert_eq!(bound.source_sha256, request.source_sha256);
        assert_eq!(bound.variant, variant);
    }
}

#[test]
fn every_compiled_field_must_match_validated_dispatch_exactly() {
    let dispatch = dispatch("static-control");
    let request = request(&dispatch, "f");
    let original = seal(&dispatch);
    for key in original.as_object().unwrap().keys() {
        let mut changed = original.clone();
        changed[key] = json!("different");
        assert!(bind(&changed, &request).is_err(), "mismatch {key}");
    }
}

#[test]
fn binary_hash_and_invocation_labels_are_not_self_referential_seal_fields() {
    let mut dispatch = dispatch("candidate");
    let original = seal(&dispatch);
    dispatch["run_id"] = json!("different-run");
    dispatch["artifact_id"] = json!("other-artifact");
    dispatch["binary_sha256"] = json!("f".repeat(64));
    assert!(bind(&original, &request(&dispatch, "a")).is_ok());
    for key in ["run_id", "artifact_id", "binary_sha256"] {
        let mut invalid = original.clone();
        invalid[key] = dispatch[key].clone();
        assert!(bind(&invalid, &request(&dispatch, "a")).is_err());
    }
}

#[test]
fn seal_rejects_missing_extra_and_non_scalar_fields() {
    let dispatch = dispatch("candidate");
    let request = request(&dispatch, "a");
    let original = seal(&dispatch);
    for key in original.as_object().unwrap().keys() {
        let mut missing = original.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert!(bind(&missing, &request).is_err());
        let mut wrong = original.clone();
        wrong[key] = json!({"nested":true});
        assert!(bind(&wrong, &request).is_err());
    }
}

#[test]
fn duplicate_or_escaped_compiled_keys_cannot_silently_override() {
    let dispatch = dispatch("candidate");
    let request = request(&dispatch, "a");
    let original = seal(&dispatch).to_string();
    for duplicate in [
        "\"source_commit\":\"other\",\"source_commit\":",
        "\"source_commit\":\"other\",\"\\u0073ource_commit\":",
    ] {
        let input = original.replacen("\"source_commit\":", duplicate, 1);
        assert_eq!(
            BuildSeal::parse_and_match(input.as_bytes(), &request).unwrap_err(),
            "artifact_duplicate_key"
        );
    }
}

#[test]
fn compiled_json_uses_the_same_bounded_utf8_and_grammar_contract() {
    let dispatch = dispatch("candidate");
    let request = request(&dispatch, "a");
    for input in [b"".as_slice(), b"{", b"[]", &[0xff]] {
        assert!(BuildSeal::parse_and_match(input, &request).is_err());
    }
    let oversized = vec![b' '; Request::MAX_BYTES + 1];
    assert!(BuildSeal::parse_and_match(&oversized, &request).is_err());
}

#[test]
fn nullable_overlay_pairs_and_unicode_values_survive_exact_matching() {
    let mut dispatch = dispatch("static-control");
    dispatch["rustc"] = json!("compiler λ {} \\\"source\\\"");
    dispatch["features"] = json!("production internals \\ [] :");
    let request = request(&dispatch, "f");
    let mut value = seal(&dispatch);
    assert!(bind(&value, &request).is_ok());
    value["overlay_sha256"] = Value::Null;
    assert!(bind(&value, &request).is_err());
}
