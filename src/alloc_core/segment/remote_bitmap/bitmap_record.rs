/// One detached remote free, copied before owner-side reclaim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BitmapRecord {
    pub offset: u32,
    pub class: u8,
}
