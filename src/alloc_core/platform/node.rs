//! Compatibility wrapper over the canonical `sefer-segment` node seam.
//! NOTE: compat wrapper — the wrapper surface is retained verbatim for
//! historical call sites; individual wrappers are unused under narrower
//! feature sets, so dead_code is allowed crate-file-wide.
#![allow(unsafe_code)]
#![allow(dead_code)]

use core::ptr::NonNull;

#[cfg(not(feature = "hardened"))]
use crate::alloc_core::config::continuation_bounds::ContinuationBounds;
use sefer_segment::support::node_impl::RootNode as CanonicalNode;

pub(crate) const NODE_SIZE: usize = sefer_segment::support::node_impl::NODE_SIZE;

pub(crate) struct Node;

impl Node {
    #[cfg(feature = "hardened")]
    #[inline(always)]
    pub(crate) fn continuation_offset(base: *mut u8, next: *mut u8) -> Option<u32> {
        // SAFETY: this numeric-only check dereferences neither pointer; trusted caller uses a same-segment continuation.
        unsafe { CanonicalNode::continuation_offset(base, next) }
    }

    #[cfg(not(feature = "hardened"))]
    #[inline(always)]
    pub(crate) fn continuation_offset_or_fatal(
        base: *mut u8,
        next: *mut u8,
        bounds: ContinuationBounds,
    ) -> u32 {
        // SAFETY: private caller snapshots bounds from this segment under owner-exclusive access.
        unsafe { CanonicalNode::continuation_offset_or_fatal(base, next, bounds.0) }
    }

    #[inline(always)]
    pub(crate) fn write_next(block: NonNull<u8>, next: *mut u8) {
        // SAFETY: private callers own a live free block with at least NODE_SIZE writable bytes.
        unsafe { CanonicalNode::write_next(block, next) }
    }
    #[inline(always)]
    pub(crate) fn read_next(block: NonNull<u8>) -> *mut u8 {
        // SAFETY: private callers pass a live free-list node with at least NODE_SIZE readable bytes.
        unsafe { CanonicalNode::read_next(block) }
    }
    #[inline(always)]
    pub(crate) fn deref(base: *mut u8, offset: usize) -> *mut u8 {
        // SAFETY: private callers prove this offset is inside their live allocation before the pointer handoff.
        unsafe { CanonicalNode::deref(base, offset) }
    }
    #[inline(always)]
    pub(crate) fn zero(ptr: *mut u8, len: usize) {
        // SAFETY: private callers own a writable range of `len` bytes.
        unsafe { CanonicalNode::zero(ptr, len) }
    }
    #[inline(always)]
    pub(crate) fn copy_nonoverlapping(src: *const u8, dst: *mut u8, count: usize) {
        // SAFETY: private callers supply distinct live ranges of `count` bytes.
        unsafe { CanonicalNode::copy_nonoverlapping(src, dst, count) }
    }
    #[inline(always)]
    pub(crate) fn write_struct<T: Copy>(dst: *mut T, value: T) {
        // SAFETY: private callers provide aligned, exclusively writable storage for T.
        unsafe { CanonicalNode::write_struct(dst, value) }
    }
    #[inline(always)]
    pub(crate) fn write_u8(dst: *mut u8, value: u8) {
        // SAFETY: private callers provide an exclusively writable byte.
        unsafe { CanonicalNode::write_u8(dst, value) }
    }
    #[inline(always)]
    pub(crate) fn read_u8(src: *const u8) -> u8 {
        // SAFETY: private callers provide a readable byte in live memory.
        unsafe { CanonicalNode::read_u8(src) }
    }
    #[inline(always)]
    pub(crate) fn read_usize(src: *const usize) -> usize {
        // SAFETY: private callers provide an aligned readable usize in live memory.
        unsafe { CanonicalNode::read_usize(src) }
    }
    #[inline(always)]
    pub(crate) fn write_usize(dst: *mut usize, value: usize) {
        // SAFETY: private callers provide aligned, exclusively writable usize storage.
        unsafe { CanonicalNode::write_usize(dst, value) }
    }
    #[inline(always)]
    pub(crate) fn offset(base: *mut u8, off: usize) -> *mut u8 {
        // SAFETY: private callers prove provenance, in-allocation bounds and non-wrapping arithmetic.
        unsafe { CanonicalNode::offset(base, off) }
    }
    #[inline(always)]
    pub(crate) fn atomic_u32_at(
        base: *mut u8,
        off: usize,
    ) -> &'static core::sync::atomic::AtomicU32 {
        // SAFETY: callers hold live mapped storage and do not retain this view beyond release.
        unsafe { CanonicalNode::atomic_u32_at(base, off) }
    }
    #[inline(always)]
    pub(crate) fn write_u32(dst: *mut u32, value: u32) {
        // SAFETY: private callers provide aligned, exclusively writable u32 storage.
        unsafe { CanonicalNode::write_u32(dst, value) }
    }
}
