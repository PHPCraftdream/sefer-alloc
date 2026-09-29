use super::LargeState;

/// Producer-only Large ingress capability borrowed from a route pin.
pub struct LargePublisher<'a> {
    pub(super) state: &'a LargeState,
}

impl LargePublisher<'_> {
    /// Terminal LIVE(g) -> PENDING(g) CAS.
    pub fn publish(&self) -> bool {
        self.state.publish()
    }
}
