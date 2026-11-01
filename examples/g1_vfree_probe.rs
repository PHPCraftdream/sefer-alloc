//! WJ-1 probe (measurement only, Windows): the cost of returning a span to the
//! OS under different reservation shapes, by direct `VirtualAlloc` /
//! `VirtualFree` calls (no allocator involved).
//!
//! Usage: g1_vfree_probe [iters]
//! Prints one JSON line per (shape, size, touch): median and min ns of the
//! setup call(s) and of the return call, over `iters` (default 200) cycles.
//!
//! Shapes:
//! - `two_call`: `MEM_RESERVE` of size + 4 MiB, `MEM_COMMIT` of the 4 MiB-aligned
//!   window, return = `MEM_RELEASE` (the shape SeferAlloc uses for spans).
//! - `one_call`: `MEM_RESERVE | MEM_COMMIT` of size, return = `MEM_RELEASE`.
//! - `decommit`: two_call setup, return = `MEM_DECOMMIT` of the window (the VA
//!   stays reserved); recommit is timed as the next cycle's setup.
//! - `reset`: two_call setup, return = `VirtualAlloc(MEM_RESET)` of the window
//!   (pages stay committed, contents discardable; what a purge delay uses).
//!
//! Touch: `2` writes the first and last page, `all` writes every page.
#[cfg(windows)]
mod probe {
    use std::time::Instant;

    const MEM_COMMIT: u32 = 0x1000;
    const MEM_RESERVE: u32 = 0x2000;
    const MEM_DECOMMIT: u32 = 0x4000;
    const MEM_RELEASE: u32 = 0x8000;
    const MEM_RESET: u32 = 0x80000;
    const PAGE_READWRITE: u32 = 0x04;
    const PAGE: usize = 4096;
    const ALIGN: usize = 4 << 20;

    #[link(name = "kernel32")]
    extern "system" {
        fn VirtualAlloc(addr: *mut u8, size: usize, kind: u32, protect: u32) -> *mut u8;
        fn VirtualFree(addr: *mut u8, size: usize, kind: u32) -> i32;
        fn GetModuleHandleA(name: *const u8) -> *mut u8;
        fn GetProcAddress(module: *mut u8, name: *const u8) -> *mut u8;
    }

    #[repr(C)]
    struct AddressRequirements {
        lowest: *mut u8,
        highest: *mut u8,
        alignment: usize,
    }
    #[repr(C)]
    struct ExtendedParameter {
        kind: u64,
        pointer: *mut AddressRequirements,
    }
    type VirtualAlloc2Fn = unsafe extern "system" fn(
        process: *mut u8,
        addr: *mut u8,
        size: usize,
        kind: u32,
        protect: u32,
        params: *mut ExtendedParameter,
        count: u32,
    ) -> *mut u8;

    fn virtual_alloc2() -> VirtualAlloc2Fn {
        // SAFETY: kernelbase is always loaded; the symbol has this signature.
        unsafe {
            let m = GetModuleHandleA(c"kernelbase.dll".as_ptr().cast());
            assert!(!m.is_null(), "kernelbase");
            let p = GetProcAddress(m, c"VirtualAlloc2".as_ptr().cast());
            assert!(!p.is_null(), "VirtualAlloc2 missing");
            core::mem::transmute::<*mut u8, VirtualAlloc2Fn>(p)
        }
    }

    /// One `VirtualAlloc2(MEM_RESERVE | MEM_COMMIT)` with a 4 MiB alignment requirement.
    fn va2(size: usize) -> *mut u8 {
        let mut req = AddressRequirements {
            lowest: core::ptr::null_mut(),
            highest: core::ptr::null_mut(),
            alignment: ALIGN,
        };
        let mut p = ExtendedParameter {
            kind: 1,
            pointer: &mut req,
        };
        // SAFETY: valid parameters; result checked.
        let r = unsafe {
            virtual_alloc2()(
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                size,
                MEM_RESERVE | MEM_COMMIT,
                PAGE_READWRITE,
                &mut p,
                1,
            )
        };
        assert!(
            !r.is_null() && (r as usize).is_multiple_of(ALIGN),
            "VirtualAlloc2 failed"
        );
        r
    }

    /// Over-reserve, release, re-reserve the exact aligned window, commit it in a
    /// second call (retried): isolates the separate-commit cost.
    fn rereserve_two(size: usize) -> *mut u8 {
        loop {
            // SAFETY: reserve / release / reserve / commit; checked.
            unsafe {
                let r = VirtualAlloc(
                    core::ptr::null_mut(),
                    size + ALIGN,
                    MEM_RESERVE,
                    PAGE_READWRITE,
                );
                assert!(!r.is_null());
                let w = ((r as usize + ALIGN - 1) & !(ALIGN - 1)) as *mut u8;
                assert!(VirtualFree(r, 0, MEM_RELEASE) != 0);
                let p = VirtualAlloc(w, size, MEM_RESERVE, PAGE_READWRITE);
                if !p.is_null() {
                    assert!(!VirtualAlloc(p, size, MEM_COMMIT, PAGE_READWRITE).is_null());
                    return p;
                }
            }
        }
    }

    /// Over-reserve, release, re-reserve+commit the aligned window in one call (retried).
    fn rereserve(size: usize) -> *mut u8 {
        loop {
            // SAFETY: reserve / release / reserve+commit; checked.
            unsafe {
                let r = VirtualAlloc(
                    core::ptr::null_mut(),
                    size + ALIGN,
                    MEM_RESERVE,
                    PAGE_READWRITE,
                );
                assert!(!r.is_null());
                let w = ((r as usize + ALIGN - 1) & !(ALIGN - 1)) as *mut u8;
                assert!(VirtualFree(r, 0, MEM_RELEASE) != 0);
                let p = VirtualAlloc(w, size, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
                if !p.is_null() {
                    return p;
                }
            }
        }
    }

    fn touch(p: *mut u8, size: usize, all: bool) {
        // SAFETY: `p..p+size` is committed read-write memory owned by the probe.
        unsafe {
            if all {
                let mut o = 0;
                while o < size {
                    p.add(o).write_volatile(1);
                    o += PAGE;
                }
            } else {
                p.write_volatile(1);
                p.add(size - PAGE).write_volatile(1);
            }
        }
    }

    /// `(reservation, window)`: reserve size + ALIGN, commit the aligned window.
    fn two_call(size: usize) -> (*mut u8, *mut u8) {
        // SAFETY: plain reserve / commit; failures are checked.
        unsafe {
            let r = VirtualAlloc(
                core::ptr::null_mut(),
                size + ALIGN,
                MEM_RESERVE,
                PAGE_READWRITE,
            );
            assert!(!r.is_null(), "reserve failed");
            let w = ((r as usize + ALIGN - 1) & !(ALIGN - 1)) as *mut u8;
            let c = VirtualAlloc(w, size, MEM_COMMIT, PAGE_READWRITE);
            assert!(!c.is_null(), "commit failed");
            (r, w)
        }
    }

    fn stats(mut v: Vec<u128>) -> (u128, u128) {
        v.sort_unstable();
        (v[v.len() / 2], v[0])
    }

    fn cycle(shape: &str, size: usize, all: bool, iters: usize) {
        let (mut setup, mut ret) = (Vec::with_capacity(iters), Vec::with_capacity(iters));
        match shape {
            "two_call" | "one_call" | "va2" | "rereserve" | "rereserve_two" => {
                for _ in 0..iters {
                    let t = Instant::now();
                    let (r, w) = if shape == "two_call" {
                        two_call(size)
                    } else if shape == "va2" {
                        let p = va2(size);
                        (p, p)
                    } else if shape == "rereserve_two" {
                        let p = rereserve_two(size);
                        (p, p)
                    } else if shape == "rereserve" {
                        let p = rereserve(size);
                        (p, p)
                    } else {
                        // SAFETY: plain reserve+commit; checked.
                        let p = unsafe {
                            VirtualAlloc(
                                core::ptr::null_mut(),
                                size,
                                MEM_RESERVE | MEM_COMMIT,
                                PAGE_READWRITE,
                            )
                        };
                        assert!(!p.is_null(), "reserve+commit failed");
                        (p, p)
                    };
                    setup.push(t.elapsed().as_nanos());
                    touch(w, size, all);
                    let t = Instant::now();
                    // SAFETY: `r` is the base of a reservation this cycle made.
                    assert!(unsafe { VirtualFree(r, 0, MEM_RELEASE) } != 0);
                    ret.push(t.elapsed().as_nanos());
                }
            }
            "decommit" | "reset" => {
                let (r, w) = two_call(size);
                for _ in 0..iters {
                    touch(w, size, all);
                    let t = Instant::now();
                    // SAFETY: `w..w+size` is committed inside reservation `r`.
                    unsafe {
                        if shape == "decommit" {
                            assert!(VirtualFree(w, size, MEM_DECOMMIT) != 0);
                        } else {
                            assert!(!VirtualAlloc(w, size, MEM_RESET, PAGE_READWRITE).is_null());
                        }
                    }
                    ret.push(t.elapsed().as_nanos());
                    let t = Instant::now();
                    if shape == "decommit" {
                        // SAFETY: recommit of the window just decommitted.
                        assert!(
                            !unsafe { VirtualAlloc(w, size, MEM_COMMIT, PAGE_READWRITE) }.is_null()
                        );
                    }
                    setup.push(t.elapsed().as_nanos());
                }
                // SAFETY: `r` is the reservation base.
                assert!(unsafe { VirtualFree(r, 0, MEM_RELEASE) } != 0);
            }
            _ => unreachable!(),
        }
        let ((sm, s0), (rm, r0)) = (stats(setup), stats(ret));
        println!(
            "{{\"shape\":\"{shape}\",\"size_mib\":{},\"touch\":\"{}\",\"iters\":{iters},\"setup_ns_median\":{sm},\"setup_ns_min\":{s0},\"return_ns_median\":{rm},\"return_ns_min\":{r0}}}",
            size >> 20,
            if all { "all" } else { "2" }
        );
    }

    pub fn main() {
        let iters: usize = std::env::args()
            .nth(1)
            .map_or(200, |s| s.parse().expect("iters"));
        for size in [4usize << 20, 36 << 20] {
            for all in [false, true] {
                for shape in [
                    "two_call",
                    "one_call",
                    "va2",
                    "rereserve",
                    "rereserve_two",
                    "decommit",
                    "reset",
                ] {
                    cycle(shape, size, all, iters);
                }
            }
        }
    }
}

fn main() {
    #[cfg(windows)]
    probe::main();
    #[cfg(not(windows))]
    eprintln!("g1_vfree_probe: Windows only");
}
