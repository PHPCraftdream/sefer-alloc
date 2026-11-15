//! WJ-1: on Windows a fully committed span with `align > 64 KiB` is one
//! `MEM_RESERVE | MEM_COMMIT` region of exactly `size` bytes at the aligned
//! base (released in ~4 µs), not an over-reserved region with a separately
//! committed window (~30–270 µs). One test per binary: the bench-internals
//! counters are process-global.
#![cfg(all(windows, not(miri), not(aligned_vmem_mock)))]

use aligned_vmem::reserve_aligned;

const MIB: usize = 1024 * 1024;

#[test]
fn aligned_eager_reserve_is_one_exact_region() {
    #[cfg(feature = "bench-internals")]
    aligned_vmem::reset_bench_internals_counters();
    let sizes = [4 * MIB, 8 * MIB, 36 * MIB];
    for &size in &sizes {
        let r = reserve_aligned(size, 4 * MIB).expect("reserve");
        let base = r.as_ptr();
        assert_eq!(base.addr() % (4 * MIB), 0, "{size}: base aligned to 4 MiB");
        assert_eq!(r.len(), size, "{size}: usable length");
        // The old shape reserved size + align at a lower base.
        assert_eq!(
            r.reservation_ptr(),
            base,
            "{size}: region starts at the base"
        );
        assert_eq!(
            r.reservation_len(),
            size,
            "{size}: region is exactly the span"
        );
        // Every page is committed read-write.
        let mut o = 0;
        while o < size {
            // SAFETY: `base..base+size` is the committed span of `r`.
            unsafe {
                base.add(o).write(0x5A);
                assert_eq!(base.add(o).read(), 0x5A);
            }
            o += 64 * 1024;
        }
        drop(r);
    }
    #[cfg(feature = "bench-internals")]
    {
        assert_eq!(
            aligned_vmem::windows_reserve_commit_single_calls(),
            sizes.len() as u64,
            "every span took the one-region path"
        );
        assert_eq!(aligned_vmem::windows_reserve_commit_two_call_pairs(), 0);
    }
}
