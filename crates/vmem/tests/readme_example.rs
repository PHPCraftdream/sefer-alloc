//! README example code, verified as a runnable test.

use aligned_vmem::{release, reserve_aligned};

#[test]
fn readme_example() {
    // Reserve 4 MiB aligned to 4 MiB — e.g. one allocator segment.
    let span = 4 * 1024 * 1024;
    let r = reserve_aligned(span, span).expect("OOM");
    let base = r.as_ptr();
    assert_eq!(base.addr() % span, 0);

    // SAFETY: base is valid for r.len() bytes, owned exclusively.
    unsafe {
        base.write(0xAB);
        assert_eq!(base.read(), 0xAB);
    }

    // RAII release on drop — or take the parts for self-hosted manual release:
    let (raw, raw_len, raw_align) = r.into_parts();
    // SAFETY: the triple came from `into_parts` and is released exactly once.
    unsafe { release(raw, raw_len, raw_align) };
}
