//! Shared initial commit geometry for small and primordial reservations.

/// Initial committed payload chunk (256 KiB), page-aligned on supported OSes.
pub const LAZY_FIRST_CHUNK: usize = 256 * 1024;
