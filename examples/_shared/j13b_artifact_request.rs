//! Bounded dispatch metadata, not proof of actual source, compiler or executable identity.
#![forbid(unsafe_code)]
use super::strict_object::StrictObject;

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

fn hash(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl ArtifactRequest {
    pub const MAX_BYTES: usize = StrictObject::MAX_BYTES;
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
        if bytes.len() > Self::MAX_BYTES {
            return Err("artifact_metadata_size");
        }
        let object = StrictObject::parse(bytes, &Self::FIELDS)?;
        let request = Self {
            run_id: object.text("run_id")?,
            artifact_id: object.text("artifact_id")?,
            variant: object.text("variant")?,
            source_commit: object.text("source_commit")?,
            source_sha256: object.text("source_sha256")?,
            binary_sha256: object.text("binary_sha256")?,
            rustc: object.text("rustc")?,
            features: object.text("features")?,
            original_source_sha256: object.nullable_text("original_source_sha256")?,
            overlay_sha256: object.nullable_text("overlay_sha256")?,
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
