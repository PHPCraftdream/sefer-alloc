//! Bounded artifact JSON grammar/key checks shared by dispatch and compiled metadata.
#![forbid(unsafe_code)]
use serde_json::{Map, Value};
use std::collections::BTreeSet;

#[derive(Debug)]
pub(crate) struct StrictObject {
    fields: Map<String, Value>,
}

impl StrictObject {
    pub const MAX_BYTES: usize = 64 * 1024;

    pub(crate) fn parse(bytes: &[u8], expected: &[&str]) -> Result<Self, &'static str> {
        if bytes.is_empty() || bytes.len() > Self::MAX_BYTES {
            return Err("artifact_metadata_size");
        }
        let input = std::str::from_utf8(bytes).map_err(|_| "artifact_metadata_utf8")?;
        let value: Value = serde_json::from_str(input).map_err(|_| "artifact_metadata_json")?;
        let Value::Object(fields) = value else {
            return Err("artifact_metadata_object");
        };
        if fields.len() != expected.len()
            || fields.keys().any(|key| !expected.contains(&key.as_str()))
        {
            return Err("artifact_metadata_fields");
        }
        unique_keys(input)?;
        Ok(Self { fields })
    }

    pub(crate) fn text(&self, key: &str) -> Result<String, &'static str> {
        self.fields
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or("artifact_field_type")
    }

    pub(crate) fn nullable_text(&self, key: &str) -> Result<Option<String>, &'static str> {
        match self.fields.get(key) {
            Some(Value::Null) => Ok(None),
            Some(Value::String(value)) => Ok(Some(value.clone())),
            _ => Err("artifact_overlay_field_type"),
        }
    }
}

// Full grammar/string decoding stays in serde_json; only decoded duplicates
// discarded by Value need this additional bounded top-level key inventory.
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
