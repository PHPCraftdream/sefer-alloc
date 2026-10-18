// R1-07: this file's own tier-1 `#![allow(unsafe_code)]` seam — `unsafe impl
// Send`/`Sync` for `OncePtrCell<T>` (only a raw `*mut T` ever crosses
// threads, mirroring the real `once_ptr_cell::OncePtrCell`) plus three
// `NonNull::new_unchecked` call sites, each proved sound by the immediately
// preceding `is_ready(p)` check (rules out null and the sentinel address, so
// `p` is a real published pointer — see each site's own `// SAFETY:`
// comment). Compiled ONLY under `--cfg loom` (`bootstrap/mod.rs`'s
// `#[cfg(loom)] pub(crate) mod loom_shim;`), so this seam adds no unsafe
// surface to a normal (non-loom) build; previously this file had no allow of
// its own and compiled only because `bootstrap/mod.rs` carried one blanket
// `#![allow(unsafe_code)]` for the whole directory, which is why this file
// was missing from both `README.md`'s and `src/lib.rs`'s unsafe inventory —
// fixed in the same commit as this attribute.
#![allow(unsafe_code)]
//! Const-capable, `core::sync::atomic`-backed stand-in for
//! `once_ptr_cell::OncePtrCell`, used ONLY under `--cfg loom` so sefer's own
//! (unrelated) shadow-model loom harnesses can link the crate with its const
//! `REGISTRY` static intact — see the import-site comment above. Mirrors the
//! real cell's CAS/Release-publish/spin/rollback protocol AND its panic
//! safety, alignment guard, and `dbg_rollback_reenterable` clobber
//! protection (task #1359, first once-ptr-cell publication audit finding
//! F2 — an earlier version of this shim was missing all four and its doc
//! comment overclaimed "behaviourally faithful" anyway). Built on `core`
//! atomics so `new` stays `const` (`once_ptr_cell::OncePtrCell::new` is
//! non-`const` under the crate's OWN `--cfg loom`, since loom atomics have
//! no const constructor — this shim exists so the ROOT crate's `--cfg
//! loom` builds, which force once-ptr-cell to compile under its `loom`
//! branch too via the shared RUSTFLAGS cfg, still get a workable `static
//! REGISTRY` initializer). It is NEVER on a loom-modeled interleaving
//! (sefer's loom tests do not touch chunk cells), so it needs no loom
//! atomics for the interleavings themselves — only for this same
//! const-constructor constraint.

use core::marker::PhantomData;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicPtr, Ordering};

// The probe's RESULT type is pure data with no atomics, so it is
// loom-agnostic and the shim reuses the real crate's enum rather than
// duplicating it -- same reasoning as the `TaggedIndex` packing reused by
// the CRATE-P7 shim below.
pub(crate) use once_ptr_cell::RollbackProbe;

const SENTINEL_INITIALIZING: usize = 1;

// repr(transparent): mirrors the real crate's own layout guarantee
// (once-ptr-cell's publication readiness review, run 5, finding F1) --
// this shim is a test-only stand-in for the real type, so its layout
// should not silently diverge from what the real crate now promises.
#[repr(transparent)]
pub(crate) struct OncePtrCell<T> {
    ptr: AtomicPtr<T>,
    _marker: PhantomData<*mut T>,
}

// SAFETY: mirrors the real cell — only a raw `*mut T` ever crosses threads.
unsafe impl<T> Send for OncePtrCell<T> {}
// SAFETY: see the `Send` impl.
unsafe impl<T> Sync for OncePtrCell<T> {}

/// Rolls the sentinel back to `null` on `Drop` unless [`defuse`](Self::defuse)
/// was called first — mirrors `once_ptr_cell`'s own `RollbackGuard`
/// (task #706's fix), so a panicking `init` closure here also leaves the
/// cell `UNINIT` instead of wedged in `INITIALIZING` forever.
struct RollbackGuard<'a, T> {
    ptr: &'a AtomicPtr<T>,
    defused: bool,
}

impl<'a, T> RollbackGuard<'a, T> {
    #[inline]
    fn new(ptr: &'a AtomicPtr<T>) -> Self {
        Self {
            ptr,
            defused: false,
        }
    }

    #[inline]
    fn defuse(&mut self) {
        self.defused = true;
    }
}

impl<T> Drop for RollbackGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        if !self.defused {
            self.ptr.store(core::ptr::null_mut(), Ordering::Release);
        }
    }
}

impl<T> OncePtrCell<T> {
    /// # Panics
    ///
    /// Panics (as a const-eval failure in the `static` usage this shim
    /// exists for) if `align_of::<T>() < 2` — mirrors
    /// `once_ptr_cell::OncePtrCell::new`'s own guard: the `INITIALIZING`
    /// sentinel is the address `1`, which needs a spare low bit.
    pub(crate) const fn new() -> Self {
        assert!(
            core::mem::align_of::<T>() >= 2,
            "OncePtrCell<T> requires align_of::<T>() >= 2 so the INITIALIZING \
             sentinel (address 1) can never collide with a real published pointer"
        );
        OncePtrCell {
            ptr: AtomicPtr::new(core::ptr::null_mut()),
            _marker: PhantomData,
        }
    }

    fn is_ready(p: *mut T) -> bool {
        let a = p.addr();
        a != 0 && a != SENTINEL_INITIALIZING
    }

    pub(crate) fn get(&self) -> Option<NonNull<T>> {
        let p = self.ptr.load(Ordering::Acquire);
        if Self::is_ready(p) {
            // SAFETY: `is_ready(p)` proved `p` is neither null nor the
            // sentinel, so it is a real published pointer.
            Some(unsafe { NonNull::new_unchecked(p) })
        } else {
            None
        }
    }

    pub(crate) fn get_or_try_init<F>(&self, init: F) -> Option<NonNull<T>>
    where
        F: FnOnce() -> Option<NonNull<T>>,
    {
        let sentinel = core::ptr::without_provenance_mut::<T>(SENTINEL_INITIALIZING);
        loop {
            let p = self.ptr.load(Ordering::Acquire);
            if Self::is_ready(p) {
                // SAFETY: `is_ready(p)` proved `p` is neither null nor the
                // sentinel, so it is a real published pointer.
                return Some(unsafe { NonNull::new_unchecked(p) });
            }
            match self.ptr.compare_exchange(
                core::ptr::null_mut(),
                sentinel,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // Hold a rollback guard across `init()` so a panicking
                    // `init` also rolls the sentinel back (task #706-class
                    // fix, mirroring the real crate).
                    let mut guard = RollbackGuard::new(&self.ptr);
                    match init() {
                        Some(ptr) => {
                            let raw = ptr.as_ptr();
                            // Release-active check (task #707-class fix,
                            // mirroring the real crate): a safe `init`
                            // closure can hand back the sentinel address
                            // itself, which would otherwise get published
                            // as if `READY` and wedge every reader.
                            assert!(
                                Self::is_ready(raw),
                                "OncePtrCell (loom_shim): init returned the null/sentinel address"
                            );
                            self.ptr.store(raw, Ordering::Release);
                            guard.defuse();
                            return Some(ptr);
                        }
                        None => {
                            guard.defuse();
                            self.ptr.store(core::ptr::null_mut(), Ordering::Release);
                            return None;
                        }
                    }
                }
                Err(_) => loop {
                    let p = self.ptr.load(Ordering::Acquire);
                    let a = p.addr();
                    if a == SENTINEL_INITIALIZING {
                        core::hint::spin_loop();
                        continue;
                    }
                    if a != 0 {
                        // SAFETY: `a != 0` rules out null, and the
                        // `== SENTINEL_INITIALIZING` arm above already
                        // returned — so `p` is a real published pointer.
                        return Some(unsafe { NonNull::new_unchecked(p) });
                    }
                    break;
                },
            }
        }
    }

    pub(crate) fn dbg_is_ready(&self) -> bool {
        Self::is_ready(self.ptr.load(Ordering::Acquire))
    }

    /// Mirrors `once_ptr_cell::OncePtrCell::dbg_rollback_reenterable`'s
    /// own conditional-restore contract (task #1359-class fix): the
    /// final restore-to-null only fires if this probe's own
    /// postcondition CAS actually re-won the cell. If a concurrent
    /// `get_or_try_init` raced in during the probe's rollback-then-reCAS
    /// window and won instead, storing `null` unconditionally here would
    /// clobber that other owner's sentinel or published pointer — the
    /// exact clobber this probe must not cause.
    pub(crate) fn dbg_rollback_reenterable(&self) -> RollbackProbe {
        let sentinel = core::ptr::without_provenance_mut::<T>(SENTINEL_INITIALIZING);
        if self
            .ptr
            .compare_exchange(
                core::ptr::null_mut(),
                sentinel,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_err()
        {
            return RollbackProbe::NotApplicable;
        }
        self.ptr.store(core::ptr::null_mut(), Ordering::Release);
        let postcondition_holds = self
            .ptr
            .compare_exchange(
                core::ptr::null_mut(),
                sentinel,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok();
        if !postcondition_holds {
            return RollbackProbe::NotApplicable;
        }
        self.ptr.store(core::ptr::null_mut(), Ordering::Release);
        RollbackProbe::Proven
    }
}
