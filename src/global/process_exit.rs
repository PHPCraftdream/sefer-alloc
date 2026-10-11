//! FFI query of Windows loader shutdown state.
#![allow(unsafe_code)] // Sole reason: FFI query of loader state.

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut core::ffi::c_void;
    fn GetProcAddress(
        module: *mut core::ffi::c_void,
        name: *const u8,
    ) -> Option<unsafe extern "system" fn() -> isize>;
}

#[inline]
pub(crate) fn process_exit_in_progress() -> bool {
    #[cfg(windows)]
    {
        const NTDLL: [u16; 10] = [110, 116, 100, 108, 108, 46, 100, 108, 108, 0];
        // SAFETY: both names are static and NUL-terminated. GetModuleHandleW
        // returns a borrowed handle; ntdll remains loaded for the process lifetime.
        // The named export has the system ABI, no arguments and a BOOLEAN (u8)
        // result. Windows function pointers have equal size; GetProcAddress's
        // generic function pointer is converted only after checking for absence.
        // The query reads loader state and transfers no ownership to Rust.
        unsafe {
            let module = GetModuleHandleW(NTDLL.as_ptr());
            if module.is_null() {
                return false;
            }
            let Some(address) = GetProcAddress(module, c"RtlDllShutdownInProgress".as_ptr().cast())
            else {
                return false;
            };
            let query = core::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                unsafe extern "system" fn() -> u8,
            >(address);
            query() != 0
        }
    }
    #[cfg(not(windows))]
    {
        false
    }
}
