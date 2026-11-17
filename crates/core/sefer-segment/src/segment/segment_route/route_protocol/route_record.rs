/// One detached Small free copied before owner-side reclaim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteRecord {
    pub offset: u32,
    pub class: u8,
}
