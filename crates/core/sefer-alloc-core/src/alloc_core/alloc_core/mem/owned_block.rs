/// Table-derived root and block; membership does not prove layout or liveness.
/// Callers keep the allocation live through consumption, including realloc.
/// Non-owning, non-generic raw fields retain !Send/!Sync; no marker or Drop is needed.
///
/// P2.2 cross-crate exposure: the root crate's `dealloc_own_thread_with_base`
/// and `HeapCore::realloc` move-leg take this witness by value; the fields
/// stay private and only the two accessors below are visible.
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct OwnedBlock {
    root: *mut u8,
    block: *mut u8,
}

impl OwnedBlock {
    #[inline(always)]
    pub(super) fn new(root: *mut u8, block: *mut u8) -> Self {
        Self { root, block }
    }

    /// P2.2 cross-crate exposure: the root crate unwraps the witness.
    #[doc(hidden)]
    #[inline(always)]
    pub fn root(self) -> *mut u8 {
        self.root
    }

    /// P2.2 cross-crate exposure: the root crate unwraps the witness.
    #[doc(hidden)]
    #[inline(always)]
    pub fn block(self) -> *mut u8 {
        self.block
    }
}
