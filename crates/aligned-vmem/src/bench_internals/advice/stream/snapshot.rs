//! Bounded raw capture collected outside allocator operations.
#![forbid(unsafe_code)]
use super::event::AdviceEvent;

/// Raw events plus actual capture footprint, not accepted-advice totals or RSS.
/// Collection allocates bookkeeping; the judge must use System for that allocator.
#[derive(Debug)]
pub struct AdviceCaptureSnapshot {
    pub encoding_version: u32,
    pub records: Vec<AdviceEvent>,
    pub boundary: u64,
    pub slot_storage_bytes: usize,
    pub total_storage_bytes: usize,
    /// Scalar source context only, not complete OS/thread-local runtime overhead.
    pub tls_context_bytes_per_thread: usize,
}
