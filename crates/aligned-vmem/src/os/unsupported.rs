// Unsupported target: no `reserve_aligned_raw` / `release_reservation` implementation.
//
// The crate currently provides backends only for:
// - Windows (not miri): uses `VirtualAlloc` / `VirtualFree`.
// - Unix (not miri): uses `mmap` / `munmap`.
// - Miri: uses `std::alloc` for testing.
//
// This target has `std` but matches none of the above (e.g. `wasm32-wasip1`,
// `x86_64-fortanix-unknown-sgx`). Adding support requires a new
// `reserve_aligned_raw` / `release_reservation` implementation for this
// target family.
#[cfg(all(not(windows), not(unix), not(miri)))]
compile_error!(
    "aligned-vmem does not currently support this target because no \
     `reserve_aligned_raw` / `release_reservation` implementation exists \
     for it. The crate provides backends only for Windows, Unix, and miri. \
     Adding support requires implementing those two functions for this \
     target family."
);
