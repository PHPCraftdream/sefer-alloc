//! Named OS quantities; no zero fallback for a missing measurement.
#[derive(Debug)]
pub(crate) struct OsSample {
    pub rss_start_bytes: u64,
    pub rss_end_bytes: u64,
    /// OS-reported approximate VmHWM, not an exact maximum of separate reads.
    pub rss_peak_bytes: u64,
    pub anon_huge_pages_bytes: u64,
    pub minor_faults: u64,
    pub major_faults: u64,
    pub peak_scope: &'static str,
    pub fault_scope: &'static str,
}
