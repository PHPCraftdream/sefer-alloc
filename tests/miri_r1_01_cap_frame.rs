//! Installed-Sefer Miri witness for the R1-01 cap branch of own free
//! (`cnt == cap < TCACHE_CAP`, fastbin magazine): the block being freed must
//! not be written while its `Box::drop` frame is live. One scenario per
//! process; no libtest is linked.

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

/// Class of an 8 KiB block: `FREE_PARK_CAP == 8 < TCACHE_CAP == 16`.
const BLOCK: usize = 8192;
const CAP: usize = 8;

fn main() {
    let before = SeferAlloc::dbg_r1_01_cap_route_frees();
    let mut boxes: Vec<Box<[u8; BLOCK]>> = Vec::with_capacity(CAP + 1);
    for i in 0..=(CAP as u8) {
        boxes.push(Box::new([i; BLOCK]));
    }
    // At most `cap` frees fit the magazine; a later one takes the cap branch
    // while its own frame is live.
    while let Some(b) = boxes.pop() {
        drop(b);
    }
    // Path-activation oracle, miri-only: natively on Windows these blocks come
    // from Large-kind segments (lazy reserve), so the branch is unreachable.
    #[cfg(miri)]
    assert!(
        SeferAlloc::dbg_r1_01_cap_route_frees() > before,
        "R1-01 cap branch must have executed"
    );
    let _ = before;
    println!("[miri_r1_01_cap_frame] COMPLETE r1_01_cap_frame");
}
