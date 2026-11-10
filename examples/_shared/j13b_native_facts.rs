//! Observed process facts only; compiled build declarations require reviewed builder provenance.
#![forbid(unsafe_code)]
use super::executable_identity::executable_sha256;
use super::host_facts::HostFacts;
use std::io;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeFacts {
    pub binary_sha256: String,
    pub host: HostFacts,
}

impl NativeFacts {
    pub(crate) fn collect(expected_binary: &str) -> io::Result<Self> {
        if !cfg!(all(target_os = "linux", not(miri))) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "native process identity requires Linux",
            ));
        }
        if expected_binary.len() != 64
            || !expected_binary
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "binary_identity_format",
            ));
        }
        let binary_sha256 = executable_sha256()?;
        if binary_sha256 != expected_binary {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "executable_identity_mismatch",
            ));
        }
        let host = HostFacts::observe()?;
        Ok(Self {
            binary_sha256,
            host,
        })
    }

    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": "observed-linux-process-facts",
            "binary_sha256": self.binary_sha256,
            "host": self.host.to_json(),
            "build_provenance": "compiled declarations still require reviewed builder",
            "scope": "trusted host/coreutils; not hostile-host attestation or payload admission",
        })
    }
}
