// ---------------------------------------------------------------------------
// Shared helpers.
// ---------------------------------------------------------------------------

/// Discriminates the eager (`MADV_DONTNEED` / `MEM_DECOMMIT`) vs lazy
/// (`MADV_FREE`) decommit paths. Threaded into `decommit_pages_impl` so both
/// [`decommit`] and [`decommit_lazy`] share one platform routine.
#[derive(Clone, Copy)]
// task #719: this was a blanket `#[allow(dead_code)]`, suppressing the lint
// in EVERY feature config (not just under `mock`, where it is genuinely
// unused) -- exactly the crate-wide-suppression hazard task #646/F8 already
// narrowed every other dead-code allow in this file away from (see the
// module doc above). `DecommitKind` was missed from that pass. Narrowed to
// match the established per-item pattern.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
pub(crate) enum DecommitKind {
    Eager,
    Lazy,
}
