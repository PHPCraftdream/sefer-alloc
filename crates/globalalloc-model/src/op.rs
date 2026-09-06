//! The op stream element: one operation against the allocator and the
//! reference model.

/// One operation against the allocator and the reference model.
///
/// `Dealloc` / `Realloc` index fields are reduced modulo the live count when
/// applied, so they are always in range regardless of the value generated;
/// when the model is EMPTY the op is silently skipped entirely (index
/// reduction happens only against a non-empty live set).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// Allocate `size` bytes at `align` (a power of two).
    Alloc {
        /// Size in bytes.
        size: usize,
        /// Alignment, a power of two.
        align: usize,
    },
    /// Allocate zeroed `size` bytes at `align` (exercises the zero path).
    AllocZeroed {
        /// Size in bytes.
        size: usize,
        /// Alignment, a power of two.
        align: usize,
    },
    /// Free the `i`-th live allocation (index reduced modulo the live count;
    /// a no-op when the model is empty).
    Dealloc(usize),
    /// Realloc the `i`-th live allocation to `new_size` (index reduced modulo
    /// the live count; skipped entirely when the model is empty).
    Realloc {
        /// Index into the live set.
        i: usize,
        /// New size in bytes.
        new_size: usize,
    },
}
