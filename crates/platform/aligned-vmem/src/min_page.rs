use super::page::PAGE;

/// Alias for [`PAGE`] under a name that doesn't imply "the OS page size".
///
/// Prefer this name in new code — it makes explicit that the value is the
/// *minimum* decommit/recommit granularity, not necessarily the actual OS page
/// size (which may be larger — see [`page_size`](crate::page_size::page_size)).
pub const MIN_PAGE: usize = PAGE;
