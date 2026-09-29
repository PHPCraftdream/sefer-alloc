use super::{RouteDirectory, RouteKind};

/// A counted identity pin. It does not retain reservation memory.
pub struct RoutePin<'a> {
    pub(super) directory: &'a RouteDirectory,
    pub(super) key: usize,
    pub(super) index: usize,
    pub(super) incarnation: u64,
    pub(super) owner: usize,
    pub(super) kind: RouteKind,
}

impl RoutePin<'_> {
    pub fn owner(&self) -> usize {
        self.owner
    }

    pub fn kind(&self) -> RouteKind {
        self.kind
    }

    pub fn incarnation(&self) -> u64 {
        self.incarnation
    }
}

impl Drop for RoutePin<'_> {
    fn drop(&mut self) {
        self.directory
            .release_pin(self.key, self.index, self.incarnation);
    }
}
