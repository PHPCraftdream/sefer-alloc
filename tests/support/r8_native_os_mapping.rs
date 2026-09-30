//! Nonallocating native probe for a previously captured reservation token.
#![cfg(not(miri))]

use core::ffi::c_void;

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Mapping {
    Mapped,
    Unmapped,
}

#[cfg(all(windows, target_pointer_width = "64"))]
mod platform {
    use super::{c_void, Mapping};
    use core::mem::{offset_of, size_of, MaybeUninit};

    // Native 64-bit layout: Microsoft winnt.h MEMORY_BASIC_INFORMATION.
    // https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-memory_basic_information
    #[repr(C)]
    struct MemoryBasicInformation {
        base_address: *mut c_void,
        allocation_base: *mut c_void,
        allocation_protect: u32,
        partition_id: u16,
        region_size: usize,
        state: u32,
        protect: u32,
        kind: u32,
    }

    const _: () = {
        assert!(size_of::<MemoryBasicInformation>() == 48);
        assert!(offset_of!(MemoryBasicInformation, region_size) == 24);
        assert!(offset_of!(MemoryBasicInformation, state) == 32);
    };
    const MEM_FREE: u32 = 0x10000;

    // https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-virtualquery
    // SAFETY: signature and output layout match the native Windows ABI.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn VirtualQuery(
            address: *const c_void,
            info: *mut MemoryBasicInformation,
            length: usize,
        ) -> usize;
    }

    pub(crate) fn mapping_at(address: usize) -> Mapping {
        let mut info = MaybeUninit::<MemoryBasicInformation>::zeroed();
        // SAFETY: VirtualQuery accepts a numeric page address, including a
        // free one. `info` is writable for its exact native 64-bit ABI size.
        let written = unsafe {
            VirtualQuery(
                core::ptr::without_provenance::<c_void>(address),
                info.as_mut_ptr(),
                size_of::<MemoryBasicInformation>(),
            )
        };
        let error = if written == 0 {
            Some(std::io::Error::last_os_error())
        } else {
            None
        };
        assert_eq!(
            written,
            size_of::<MemoryBasicInformation>(),
            "VirtualQuery: {error:?}"
        );
        // SAFETY: every field is a raw pointer or integer, so zero is valid;
        // fields VirtualQuery leaves undefined for MEM_FREE stay initialized.
        // A full-sized successful return supplies the State field we read.
        let info = unsafe { info.assume_init() };
        if info.state == MEM_FREE {
            Mapping::Unmapped
        } else {
            Mapping::Mapped
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{c_void, Mapping};

    const ENOMEM: i32 = 12;

    // https://man7.org/linux/man-pages/man2/mincore.2.html
    // SAFETY: signature matches the libc ABI documented for Linux mincore.
    unsafe extern "C" {
        fn mincore(address: *mut c_void, length: usize, vector: *mut u8) -> i32;
    }

    pub(crate) fn mapping_at(address: usize) -> Mapping {
        let mut vector = [0u8; 1];
        // SAFETY: caller supplies a formerly mapped, page-aligned token;
        // length 1 needs one writable vector byte. Linux mincore accepts an
        // unmapped numeric address and reports ENOMEM without dereferencing it.
        let result = unsafe {
            mincore(
                core::ptr::without_provenance_mut::<c_void>(address),
                1,
                vector.as_mut_ptr(),
            )
        };
        let errno = if result == -1 {
            std::io::Error::last_os_error().raw_os_error()
        } else {
            None
        };
        match (result, errno) {
            (0, _) => Mapping::Mapped,
            (-1, Some(ENOMEM)) => Mapping::Unmapped,
            _ => panic!("mincore failed at {address:#x}: {errno:?}"),
        }
    }
}

#[cfg(any(target_os = "linux", all(windows, target_pointer_width = "64")))]
pub(crate) use platform::mapping_at;
