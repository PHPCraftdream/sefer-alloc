//! The segment subtree is retained under its historical module names.

pub mod bitmap;
#[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
pub mod remote_bitmap;
#[path = "support/segment_directory/mod.rs"]
pub mod segment_directory;
#[doc(hidden)]
pub mod segment_header;
#[path = "segment_header/segment_header_layout.rs"]
pub(crate) mod segment_header_layout;
#[path = "segment_header/segment_header_impl/segment_header_meta_fields.rs"]
pub(crate) mod segment_header_meta_fields;
#[path = "segment_header/segment_header_impl/segment_header_views.rs"]
pub(crate) mod segment_header_views;
#[path = "support/segment_directory/segment_layout.rs"]
pub mod segment_layout;
#[cfg(feature = "alloc-global")]
#[path = "segment_route/mod.rs"]
#[doc(hidden)]
pub mod segment_route;
pub mod segment_table;
