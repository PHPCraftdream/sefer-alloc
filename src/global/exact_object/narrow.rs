// Seam: `System` alloc/dealloc of exact narrow objects. Every pointer handed
// out is the original `System` pointer; the only metadata lives out-of-object
// in the descriptor table, never in the payload.
#![allow(unsafe_code)]

use super::exact_fatal::fatal;
use super::exact_table::ExactTable;
use core::alloc::Layout;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::alloc::{GlobalAlloc, System};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static DEALLOCS: AtomicUsize = AtomicUsize::new(0);

/// Miri-witness mutation switch. Read only under the test cfg.
#[cfg(all(miri, feature = "internals", feature = "bench-internals"))]
static EARLY_FREE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Exact-object route for narrow requests (opt-in prototype).
///
/// Order in `dealloc`: find descriptor by exact address, check the layout,
/// unlink it, only then free the original pointer with the original layout.
pub struct ExactNarrow;

impl ExactNarrow {
    /// Largest served size in bytes.
    pub const MAX_SIZE: usize = 16;
    /// Largest served alignment.
    pub const MAX_ALIGN: usize = 8;

    #[inline(always)]
    pub fn is_narrow(layout: Layout) -> bool {
        layout.size() >= 1 && layout.size() <= Self::MAX_SIZE && layout.align() <= Self::MAX_ALIGN
    }

    /// Serve a narrow `layout` (caller checked `is_narrow`) from `System`;
    /// null on OOM (object or table).
    pub(crate) fn alloc(layout: Layout, zeroed: bool) -> *mut u8 {
        // SAFETY: narrow layouts have non-zero size; the exact layout is passed.
        let ptr = unsafe {
            if zeroed {
                System.alloc_zeroed(layout)
            } else {
                System.alloc(layout)
            }
        };
        if ptr.is_null() {
            return ptr;
        }
        if ExactTable::register(ptr.addr(), layout.size(), layout.align()).is_none() {
            // SAFETY: just allocated above with `layout`, never published.
            unsafe { System.dealloc(ptr, layout) };
            return core::ptr::null_mut();
        }
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        ptr
    }

    /// Free `ptr` if it is a registered exact object; `false` = not ours.
    ///
    /// # Safety
    /// `ptr`/`layout` are the `GlobalAlloc::dealloc` contract pair.
    pub(crate) unsafe fn try_dealloc(ptr: *mut u8, layout: Layout) -> bool {
        let addr = ptr.addr();
        #[cfg(all(miri, feature = "internals", feature = "bench-internals"))]
        if EARLY_FREE.load(Ordering::Relaxed) {
            // Mutant: physical free BEFORE the descriptor is unlinked.
            let Some((size, align, _)) = ExactTable::peek(addr) else {
                return false;
            };
            Self::check_layout(size, align, layout);
            // SAFETY: original pointer and layout.
            unsafe { System.dealloc(ptr, layout) };
            DEALLOCS.fetch_add(1, Ordering::Relaxed);
            crate::registry::segment_route::TerminalPublicationGate::pause_after_publication(addr);
            ExactTable::take(addr);
            return true;
        }
        let Some((size, align, _)) = ExactTable::take(addr) else {
            return false;
        };
        Self::check_layout(size, align, layout);
        // SAFETY: descriptor unlinked; original pointer, original layout.
        unsafe { System.dealloc(ptr, layout) };
        DEALLOCS.fetch_add(1, Ordering::Relaxed);
        // Test gate: keep the caller's dealloc frame alive after the terminal
        // accounting (free + unlink) is complete.
        #[cfg(all(miri, feature = "internals", feature = "bench-internals"))]
        crate::registry::segment_route::TerminalPublicationGate::pause_after_publication(addr);
        true
    }

    fn check_layout(size: usize, align: usize, layout: Layout) {
        if size != layout.size() || align != layout.align() {
            fatal(b"exact-object: dealloc layout differs from descriptor\n");
        }
    }

    /// Exact-object `(allocs, deallocs)` since process start.
    #[cfg(feature = "bench-internals")]
    #[doc(hidden)]
    pub fn dbg_counts() -> (usize, usize) {
        (
            ALLOCS.load(Ordering::Relaxed),
            DEALLOCS.load(Ordering::Relaxed),
        )
    }

    /// Generation of the live descriptor at `addr`, if any (read-only).
    #[cfg(feature = "bench-internals")]
    #[doc(hidden)]
    pub fn dbg_generation_of(addr: usize) -> Option<u64> {
        ExactTable::peek(addr).map(|(_, _, generation)| generation)
    }

    /// Miri-witness mutant switch: physical free BEFORE the descriptor unlink.
    #[cfg(all(miri, feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn dbg_set_early_free(on: bool) {
        EARLY_FREE.store(on, Ordering::Relaxed);
    }
}
