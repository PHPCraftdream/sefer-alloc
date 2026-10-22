//! Actual-source storage lifecycle для native и Miri SB/TB, без OS reservation.
#![cfg(not(loom))]
#![allow(dead_code)]

pub(crate) use core::sync::atomic::{AtomicU64, Ordering};

mod alloc_core {
    pub mod os {
        pub const SEGMENT: usize = 4 * 1024 * 1024;
    }
    pub mod size_classes {
        pub const MIN_BLOCK: usize = 16;
    }
    pub mod fatal {
        pub fn fatal(_: &'static [u8]) -> ! {
            std::process::abort()
        }
    }
}

#[path = "../src/alloc_core/segment/remote_bitmap/sidecar_bitmap/chunked_bank.rs"]
mod chunked_bank;
use chunked_bank::ChunkedBank;

fn fresh() -> Box<ChunkedBank> {
    let mut backing = Box::<ChunkedBank>::new_uninit();
    // SAFETY: свежий приватный aligned allocation; ссылок на Self ещё нет.
    unsafe { ChunkedBank::initialize_at(backing.as_mut_ptr()) };
    // SAFETY: initialize_at построил все поля, включая MaybeUninit-оболочки.
    unsafe { backing.assume_init() }
}

#[test]
fn unready_dirty_storage_is_never_read_or_materialized_by_readers() {
    let bank = fresh();
    let last = ChunkedBank::WORDS - 1;
    assert_eq!(ChunkedBank::window_end(0, 0), Some(0));
    assert_eq!(ChunkedBank::window_end(0, 1), None);
    assert_eq!(
        ChunkedBank::window_end(alloc_core::os::SEGMENT + 1, 0),
        None
    );
    assert_eq!(ChunkedBank::window_end(usize::MAX, 0), None);
    assert_eq!(ChunkedBank::range_mask(0, 64, 0), u64::MAX);
    assert_eq!(ChunkedBank::range_mask(63, 65, 0), 1u64 << 63);
    assert_eq!(ChunkedBank::range_mask(63, 65, 1), 1);
    assert_eq!(ChunkedBank::range_mask(64, 64, 1), 0);
    assert_eq!(
        ChunkedBank::range_mask(0, usize::MAX, ChunkedBank::WORDS),
        0
    );
    assert_eq!(bank.initialized_chunks(), 0);
    for word in [0, 7, 8, last] {
        assert_eq!(bank.pending_load(word), 0);
        assert_eq!(bank.pending_swap(word), 0);
        assert_eq!(bank.starts_load(word), 0);
        bank.clear_pending(word, u64::MAX);
        bank.clear_starts(word, u64::MAX);
    }
    for offset in [
        0,
        16,
        8192,
        (alloc_core::os::SEGMENT - 16) as u32,
        u32::MAX,
        17,
    ] {
        assert!(!bank.publish(offset, 0));
    }
    assert_eq!(
        bank.initialized_chunks(),
        0,
        "read/clear/publish не конструируют чанк"
    );
}

#[test]
fn distant_issue_constructs_only_two_chunks_and_never_reconstructs_old_pending() {
    let bank = fresh();
    let far = (alloc_core::os::SEGMENT - 16) as u32;
    // SAFETY: fixture сохраняет единственного owner, issue последовательны.
    unsafe { bank.issue(0) };
    assert!(bank.publish(0, 0));
    // SAFETY: тот же единственный owner; producer трогает лишь готовые атомики.
    unsafe { bank.issue(far) };
    assert_eq!(bank.initialized_chunks(), 2);
    assert_eq!(
        bank.pending_load(0),
        1,
        "рост не затирает старую публикацию"
    );
    assert!(
        !bank.publish(16, 0),
        "ready не заменяет индивидуальный start gate"
    );
    assert!(!bank.publish(8192, 0), "дырка остаётся неготовой");
    assert!(!bank.publish(0, 16), "metadata-фильтр раньше pending RMW");
    // SAFETY: последовательное reissue тем же owner, без реконструкции.
    unsafe { bank.issue(16) };
    assert_eq!(bank.pending_load(0), 1);
    assert!(bank.publish(16, 0));
    bank.clear_pending(0, 1);
    assert_eq!(
        bank.pending_swap(0),
        2,
        "masked clear сохраняет соседний бит"
    );
    assert_eq!(bank.pending_swap(0), 0, "cut потребляется ровно один раз");
    assert!(bank.publish(far, 0));
    assert_eq!(bank.pending_swap(ChunkedBank::WORDS - 1), 1u64 << 63);
    assert_eq!(bank.initialized_chunks(), 2);
}

#[test]
fn retain_clear_keeps_constructed_lifetimes_and_reissue_works() {
    let bank = fresh();
    // SAFETY: единственный owner; конкурентных операций нет.
    unsafe { bank.issue(8192) };
    assert!(bank.publish(8192, 0));
    bank.clear_pending(8, u64::MAX);
    bank.clear_starts(8, u64::MAX);
    assert_eq!(bank.initialized_chunks(), 1);
    assert_eq!(bank.pending_load(8), 0);
    assert!(!bank.publish(8192, 0));
    // SAFETY: прежний owner после quiescent reset; lifetime атомиков сохранён.
    unsafe { bank.issue(8208) };
    assert_eq!(bank.initialized_chunks(), 1);
    assert!(bank.publish(8208, 0));
    assert_eq!(bank.pending_swap(8), 2);
}
