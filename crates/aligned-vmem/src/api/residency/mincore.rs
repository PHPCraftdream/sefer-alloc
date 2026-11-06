//! Linux/Android `mincore(2)` FFI seam for residency sampling (task #53).
// FreeBSD/NetBSD also have a `mincore(2)` call but with DIFFERENT flag
// semantics — their low bits are virtual-map flags, not residency bits —
// so the "low bit = resident" contract is honored on Linux/Android only.
// Every other target takes the typed unsupported fallback and never
// issues a syscall.
#[cfg(all(
    unix,
    not(miri),
    not(aligned_vmem_mock),
    any(target_os = "linux", target_os = "android")
))]
extern "C" {
    // glibc/bionic/musl `mincore(2)`. `length` must be a multiple of the
    // page size (the kernel rounds a misaligned length up otherwise — this
    // API only ever passes exact multiples); `vec` receives exactly
    // `length / page_size` bytes, bit 0 of each byte = the page is resident
    // per the kernel's page tables. Capture `errno` immediately after a
    // failed call (task #713 discipline).
    pub(crate) fn mincore(addr: *mut core::ffi::c_void, length: usize, vec: *mut u8) -> i32;
}
