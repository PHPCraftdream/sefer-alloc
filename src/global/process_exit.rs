//! FFI query of Windows loader shutdown state (exact `BOOLEAN (u8)` export type, R6-SEC-16).
#![allow(unsafe_code)] // Sole reason: FFI query of loader state.

#[cfg(windows)]
use core::sync::atomic::{AtomicUsize, Ordering};

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut core::ffi::c_void;
    fn GetProcAddress(
        module: *mut core::ffi::c_void,
        name: *const u8,
    ) -> Option<unsafe extern "system" fn() -> u8>;
}

#[cfg(windows)]
const UNRESOLVED: usize = 0;
#[cfg(windows)]
const ABSENT: usize = 1;

/// Address of `RtlDllShutdownInProgress`, `UNRESOLVED` until the first probe.
/// The module/export lookup enters the loader, and a lock-waiter probes in a
/// loop: a thread terminated inside the lookup makes `ExitProcess` skip the
/// detach callbacks, TLS destructors included. So the lookup runs once.
#[cfg(windows)]
static QUERY: AtomicUsize = AtomicUsize::new(UNRESOLVED);

#[cfg(windows)]
#[cold]
#[inline(never)]
fn resolve() -> usize {
    const NTDLL: [u16; 10] = [110, 116, 100, 108, 108, 46, 100, 108, 108, 0];
    // SAFETY: both names are static and NUL-terminated. GetModuleHandleW
    // returns a borrowed handle; ntdll remains loaded for the process lifetime.
    // GetProcAddress only reads the export table; the address is kept as an
    // integer and is converted back only by the caller.
    let address = unsafe {
        let module = GetModuleHandleW(NTDLL.as_ptr());
        if module.is_null() {
            ABSENT
        } else {
            match GetProcAddress(module, c"RtlDllShutdownInProgress".as_ptr().cast()) {
                Some(address) => address as usize,
                None => ABSENT,
            }
        }
    };
    // Racing first probes store the same value.
    QUERY.store(address, Ordering::Relaxed);
    address
}

#[inline]
pub(crate) fn process_exit_in_progress() -> bool {
    #[cfg(windows)]
    {
        let mut address = QUERY.load(Ordering::Relaxed);
        if address == UNRESOLVED {
            address = resolve();
        }
        if address == ABSENT {
            return false;
        }
        // SAFETY: `address` is the non-absent export resolved above. The
        // declared type IS the export's documented one (`BOOLEAN
        // RtlDllShutdownInProgress(void)` — system ABI, no arguments, u8
        // result), so the single transmute only undoes the AtomicUsize
        // round-trip; there is no second conversion at the call. The query
        // reads loader state and transfers no ownership to Rust.
        unsafe {
            let query = core::mem::transmute::<usize, unsafe extern "system" fn() -> u8>(address);
            query() != 0
        }
    }
    #[cfg(not(windows))]
    {
        false
    }
}
