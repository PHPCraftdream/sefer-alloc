use super::SmallSidecar;

/// Producer-only Small ingress capability borrowed from a route pin.
pub struct SmallPublisher<'a> {
    pub(super) sidecar: &'a SmallSidecar,
}

impl SmallPublisher<'_> {
    /// Terminal publication; the caller may only release its pin afterward.
    pub fn publish(&self, offset: u32) -> bool {
        self.sidecar.publish(offset)
    }
}
