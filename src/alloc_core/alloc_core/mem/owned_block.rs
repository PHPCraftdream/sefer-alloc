/// Table-derived root and block; membership does not prove layout or liveness.
/// Callers keep the allocation live through consumption, including realloc.
/// Non-owning, non-generic raw fields retain !Send/!Sync; no marker or Drop is needed.
#[derive(Clone, Copy)]
pub(crate) struct OwnedBlock {
    root: *mut u8,
    block: *mut u8,
}

impl OwnedBlock {
    #[inline(always)]
    pub(super) fn new(root: *mut u8, block: *mut u8) -> Self {
        Self { root, block }
    }

    #[inline(always)]
    pub(crate) fn root(self) -> *mut u8 {
        self.root
    }

    #[inline(always)]
    pub(crate) fn block(self) -> *mut u8 {
        self.block
    }
}
