//! Bounded dispatch metadata, not proof of actual source, compiler or executable identity.
#![forbid(unsafe_code)]
use serde_json::{Map, Value};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArtifactRequest {
    pub run_id: String,
    pub artifact_id: String,
    pub variant: String,
    pub source_commit: String,
    pub source_sha256: String,
    pub binary_sha256: String,
    pub rustc: String,
    pub features: String,
    pub original_source_sha256: Option<String>,
    pub overlay_sha256: Option<String>,
}

fn text(object: &Map<String, Value>, key: &str) -> Result<String, &'static str> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or("artifact_field_type")
}
fn optional_hash(object: &Map<String, Value>, key: &str) -> Result<Option<String>, &'static str> {
    match object.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err("artifact_overlay_field_type"),
    }
}
fn hash(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

// serde_json validates the complete grammar first. This scan only identifies
// decoded top-level keys, because Value would otherwise discard duplicates.
fn unique_keys(input: &str) -> Result<(), &'static str> {
    let bytes = input.as_bytes();
    let mut keys = BTreeSet::new();
    let mut depth = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth = depth.checked_sub(1).ok_or("artifact_json_depth")?,
            b'"' => {
                let start = index;
                index += 1;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' => index += 2,
                        b'"' => {
                            index += 1;
                            break;
                        }
                        _ => index += 1,
                    }
                }
                let mut next = index;
                while bytes.get(next).is_some_and(u8::is_ascii_whitespace) {
                    next += 1;
                }
                if depth == 1 && bytes.get(next) == Some(&b':') {
                    let literal = input.get(start..index).ok_or("artifact_key_range")?;
                    let key: String =
                        serde_json::from_str(literal).map_err(|_| "artifact_key_invalid")?;
                    if !keys.insert(key) {
                        return Err("artifact_duplicate_key");
                    }
                }
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    Ok(())
}

impl ArtifactRequest {
    pub const MAX_BYTES: usize = 64 * 1024;
    const FIELDS: [&'static str; 10] = [
        "run_id",
        "artifact_id",
        "variant",
        "source_commit",
        "source_sha256",
        "binary_sha256",
        "rustc",
        "features",
        "original_source_sha256",
        "overlay_sha256",
    ];

    pub(crate) fn parse(bytes: &[u8], arm: &str) -> Result<Self, &'static str> {
        if bytes.is_empty() || bytes.len() > Self::MAX_BYTES {
            return Err("artifact_metadata_size");
        }
        let input = std::str::from_utf8(bytes).map_err(|_| "artifact_metadata_utf8")?;
        let value: Value = serde_json::from_str(input).map_err(|_| "artifact_metadata_json")?;
        let object = value.as_object().ok_or("artifact_metadata_object")?;
        if object.len() != Self::FIELDS.len()
            || object
                .keys()
                .any(|key| !Self::FIELDS.contains(&key.as_str()))
        {
            return Err("artifact_metadata_fields");
        }
        unique_keys(input)?;
        let request = Self {
            run_id: text(object, "run_id")?,
            artifact_id: text(object, "artifact_id")?,
            variant: text(object, "variant")?,
            source_commit: text(object, "source_commit")?,
            source_sha256: text(object, "source_sha256")?,
            binary_sha256: text(object, "binary_sha256")?,
            rustc: text(object, "rustc")?,
            features: text(object, "features")?,
            original_source_sha256: optional_hash(object, "original_source_sha256")?,
            overlay_sha256: optional_hash(object, "overlay_sha256")?,
        };
        request.validate(arm)?;
        Ok(request)
    }

    fn validate(&self, arm: &str) -> Result<(), &'static str> {
        if [
            &self.run_id,
            &self.artifact_id,
            &self.variant,
            &self.rustc,
            &self.features,
        ]
        .iter()
        .any(|value| value.trim().is_empty() || value.contains('\0'))
        {
            return Err("artifact_empty_or_nul_field");
        }
        let id = self.artifact_id.as_bytes();
        if id.len() > 64
            || !id.first().is_some_and(u8::is_ascii_lowercase)
            || !id
                .iter()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        {
            return Err("artifact_identifier");
        }
        if !hash(&self.source_commit, 40)
            || !hash(&self.source_sha256, 64)
            || !hash(&self.binary_sha256, 64)
        {
            return Err("artifact_hash_or_commit");
        }
        let expected = match arm {
            "d" | "n" | "b" | "a" | "ab" => "candidate",
            "f" => "static-control",
            "base" | "base2" | "mi" => "baseline",
            _ => return Err("artifact_unknown_arm"),
        };
        if self.variant != expected {
            return Err("artifact_arm_variant_mismatch");
        }
        match (&self.original_source_sha256, &self.overlay_sha256) {
            (Some(original), Some(overlay))
                if self.variant == "static-control"
                    && hash(original, 64)
                    && hash(overlay, 64)
                    && original != &self.source_sha256 =>
            {
                Ok(())
            }
            (None, None) if self.variant != "static-control" => Ok(()),
            _ => Err("artifact_overlay_provenance"),
        }
    }
}
