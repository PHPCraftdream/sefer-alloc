/// The minimum page size this crate assumes for decommit/recommit granularity:
/// 4 KiB, the smallest unit both `mmap` and `VirtualAlloc` will commit/decommit
/// on the platforms this crate targets.
///
/// Decommit/recommit offsets must be multiples of the runtime [`page_size()`](crate::page_size::page_size);
/// this constant is only the guaranteed lower bound. `page_size()` may be larger
/// (e.g. 16 KiB on Apple Silicon macOS), so callers computing decommit offsets
/// must round to `page_size()`, not `PAGE`.
///
/// # Naming
///
/// This constant was named `PAGE` in the 0.1.0 release. The name is misleading
/// because it is not the actual page size on all platforms (e.g., 16 KiB on Apple
/// Silicon macOS, 64 KiB on some Linux configurations). For new code, prefer
/// [`MIN_PAGE`](crate::min_page::MIN_PAGE) instead, which more accurately describes what this constant
/// represents: the *minimum* granularity the crate assumes, not the platform's
/// page size.
pub const PAGE: usize = 1 << 12;
