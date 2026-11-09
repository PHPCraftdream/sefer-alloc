//! Pre-native diagnostic position and source class, not a mapping capability.
#![forbid(unsafe_code)]

#[derive(Clone, Copy, Debug)]
pub struct AdviceReleaseMarker {
    pub(crate) begin: u64,
    pub(crate) large: bool,
}
