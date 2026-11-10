//! Compiled metadata consistency only; actual executable/host and builder trust are separate.
#![forbid(unsafe_code)]
use super::artifact_request::ArtifactRequest;
use super::strict_object::StrictObject;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BuildSeal {
    pub source_commit: String,
    pub source_sha256: String,
    pub rustc: String,
    pub features: String,
    pub variant: String,
    pub original_source_sha256: Option<String>,
    pub overlay_sha256: Option<String>,
}

impl BuildSeal {
    const FIELDS: [&'static str; 7] = [
        "source_commit",
        "source_sha256",
        "rustc",
        "features",
        "variant",
        "original_source_sha256",
        "overlay_sha256",
    ];

    /// The dispatch must already pass ArtifactRequest validation. Dynamic run /
    /// artifact labels and the independently observed binary hash are not sealed.
    pub(crate) fn parse_and_match(
        bytes: &[u8],
        request: &ArtifactRequest,
    ) -> Result<Self, &'static str> {
        let object = StrictObject::parse(bytes, &Self::FIELDS)?;
        let seal = Self {
            source_commit: object.text("source_commit")?,
            source_sha256: object.text("source_sha256")?,
            rustc: object.text("rustc")?,
            features: object.text("features")?,
            variant: object.text("variant")?,
            original_source_sha256: object.nullable_text("original_source_sha256")?,
            overlay_sha256: object.nullable_text("overlay_sha256")?,
        };
        if seal.source_commit != request.source_commit
            || seal.source_sha256 != request.source_sha256
            || seal.rustc != request.rustc
            || seal.features != request.features
            || seal.variant != request.variant
            || seal.original_source_sha256 != request.original_source_sha256
            || seal.overlay_sha256 != request.overlay_sha256
        {
            return Err("build_seal_mismatch");
        }
        Ok(seal)
    }
}
