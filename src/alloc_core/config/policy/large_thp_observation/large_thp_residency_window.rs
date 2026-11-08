//! `LargeThpResidencyWindow` — one planned residency-sampling location: a
//! contiguous, page-aligned byte range inside the span, plus its page
//! coordinates. Pure data for the FUTURE owning `mincore` walk; the
//! `large_thp_observation` family computes locations only and performs
//! NO sampling itself (no pinning, no proc/sysfs reads, no live or
//! foreign-mapping access).
//!
//! This module is a standalone pure component; runtime integration into
//! the allocator is a separate future task.

/// One planned sampling window within a validated span geometry.
///
/// Invariants (established by the planner that constructs windows, never
/// violated by construction): `byte_offset` is a multiple of the
/// geometry's base page size; `byte_offset == start_page_index *
/// base_page_size`; `page_count >= 1`; the window's pages lie within
/// `[0, full_page_count)` of its geometry; `page_count` fits `u32`
/// because a window is capped at 2 MiB and a base page is a
/// power-of-two size >= 1 (so at most 2^21 pages of 1 byte each).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct LargeThpResidencyWindow {
    /// Span-relative byte offset of the window's first byte; always a
    /// multiple of the geometry's base page size.
    pub byte_offset: u64,
    /// Page index of the window's first page within the span's full
    /// page sequence (`byte_offset == start_page_index *
    /// base_page_size`).
    pub start_page_index: u64,
    /// Window length in whole base pages (always >= 1; the 2 MiB
    /// per-window cap keeps this within `u32` for any power-of-two page
    /// size).
    pub page_count: u32,
}
