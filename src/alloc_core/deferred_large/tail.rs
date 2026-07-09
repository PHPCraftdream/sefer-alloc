//! 0.3.0 hardening (post-A1), extracted for #132: the **on-this-stack** tail
//! sentinel for the deferred-free Treiber stack that the `HeapCore` face
//! reuses its `thread_free`/identity
//! `AtomicPtr<u8>` head for (see the `push`/`drain` doc comments in this
//! module for the full mechanism).
//!
//! This MUST be distinct from
//! [`ABANDONED_TAIL`](crate::alloc_core::segment_header::ABANDONED_TAIL)
//! (`u64::MAX`), which the same `next_abandoned` header field uses to mean
//! "not linked into ANY stack" (the value every fresh/reclaimed segment
//! header starts with). If the two sentinels were the same value, a `base`
//! pushed onto an EMPTY deferred-free stack would end this push with
//! `next_abandoned == ABANDONED_TAIL` — indistinguishable from "never
//! pushed" — silently defeating the double-push guard in
//! [`push_large_deferred_free`](super::push::push_large_deferred_free) for
//! the common case (an idle heap, empty stack) the very first time it's
//! used. `u64::MAX - 1` is never a valid link value either way: a real "has
//! next" link is a `SEGMENT`-aligned base address cast to `u64`, and no
//! platform this crate targets produces `usize::MAX - 1` as a valid,
//! aligned pointer.
#[cfg(feature = "alloc-xthread")]
pub(crate) const DEFERRED_LARGE_TAIL: u64 = u64::MAX - 1;
