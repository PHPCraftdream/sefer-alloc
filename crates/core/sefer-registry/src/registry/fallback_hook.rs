// Safe, facade-registered access to the process fallback heap.
use std::sync::OnceLock;

use crate::registry::HeapCore;

/// Callback the facade registers: attempts a nonblocking exclusive borrow of the fallback heap for the caller's closure.
pub(crate) type FallbackTryWithHeap = fn(&mut dyn FnMut(&mut HeapCore)) -> bool;

/// A write-once safe callback boundary for fallback-heap owner access.
///
/// The callback receives only the caller's safe closure; it is responsible
/// for granting that closure exclusive access to an already-available heap.
pub(crate) struct FallbackHook {
    callback: OnceLock<FallbackTryWithHeap>,
}

impl FallbackHook {
    /// Construct an empty hook (useful for isolated source-including tests).
    pub(crate) const fn new() -> Self {
        Self {
            callback: OnceLock::new(),
        }
    }

    /// Return the process-wide hook shared by the facade and registry router.
    pub(crate) fn global() -> &'static Self {
        static HOOK: FallbackHook = FallbackHook::new();
        &HOOK
    }

    /// Install the facade callback once. A `false` result means it was already
    /// registered; the first callback remains authoritative.
    #[inline]
    pub(crate) fn register(&self, callback: FallbackTryWithHeap) -> bool {
        self.callback.set(callback).is_ok()
    }

    /// Invoke the callback if registered. `false` means unavailable, whether
    /// because registration has not happened or the fallback is not usable.
    #[inline]
    pub(crate) fn try_with_heap(&self, f: &mut dyn FnMut(&mut HeapCore)) -> bool {
        match self.callback.get() {
            Some(callback) => callback(f),
            None => false,
        }
    }
}
