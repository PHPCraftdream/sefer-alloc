//! Actual-source: конструкция нового чанка, ready visibility и terminal cut.
#![cfg(loom)]
#![allow(dead_code)]

pub(crate) use loom::sync::atomic::{AtomicU64, Ordering};
use loom::sync::{atomic::AtomicBool, Arc};
use loom::thread;

mod alloc_core {
    pub mod os {
        pub const SEGMENT: usize = 2 * 8192;
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

fn fresh() -> Arc<Box<ChunkedBank>> {
    let mut backing = Box::<ChunkedBank>::new_uninit();
    // SAFETY: приватный aligned allocation; все оболочки строятся до Arc.
    unsafe { ChunkedBank::initialize_at(backing.as_mut_ptr()) };
    // SAFETY: все поля Self валидны; payload разрешённо неинициализирован.
    Arc::from(unsafe { backing.assume_init() })
}

#[test]
fn new_chunk_handoff_and_two_cuts_preserve_exact_publication() {
    loom::model(|| {
        let bank = fresh();
        // SAFETY: единственный owner — текущий поток; workers не вызывают issue.
        unsafe { bank.issue(0) };
        assert!(bank.publish(0, 0));
        let handoff = Arc::new(AtomicBool::new(false));
        let producer = {
            let bank = Arc::clone(&bank);
            let handoff = Arc::clone(&handoff);
            thread::spawn(move || {
                while !handoff.load(Ordering::Acquire) {
                    thread::yield_now();
                }
                assert!(bank.publish(8192, 0), "новый issued блок не отвергается");
                assert!(!bank.publish(8208, 0), "ready не выдаёт соседнюю гранулу");
            })
        };
        // SAFETY: тот же единственный owner, конструкция последовательна с drain.
        unsafe { bank.issue(8192) };
        handoff.store(true, Ordering::Release);
        assert_eq!(bank.pending_swap(0), 1, "рост не обнуляет старый чанк");
        let early = bank.pending_swap(8);
        producer.join().unwrap();
        let late = bank.pending_swap(8);
        assert_eq!(early | late, 1);
        assert_eq!(early & late, 0, "одна публикация не попадает в два cut");
        assert_eq!(bank.pending_swap(8), 0);
    });
}

#[test]
fn ready_acquire_protects_observer_without_allocation_handoff() {
    loom::model(|| {
        let bank = fresh();
        let reader = {
            let bank = Arc::clone(&bank);
            thread::spawn(move || {
                // Read-only observer может смотреть ещё не выданный диапазон.
                // При ready=0 payload не читается; при ready=1 конструкция видима.
                assert_eq!(bank.pending_load(8), 0);
            })
        };
        // SAFETY: лишь текущий поток конструирует; reader только читает.
        unsafe { bank.issue(8192) };
        reader.join().unwrap();
        assert_eq!(bank.starts_load(8), 1);
    });
}

#[test]
fn two_producers_and_masked_clear_keep_neighbour_publication() {
    loom::model(|| {
        let bank = fresh();
        // SAFETY: единственный owner, до запуска обоих producers.
        unsafe {
            bank.issue(0);
            bank.issue(16);
        };
        let first = {
            let bank = Arc::clone(&bank);
            thread::spawn(move || assert!(bank.publish(0, 0)))
        };
        let second = {
            let bank = Arc::clone(&bank);
            thread::spawn(move || assert!(bank.publish(16, 0)))
        };
        bank.clear_pending(0, 1);
        let early = bank.pending_swap(0);
        first.join().unwrap();
        second.join().unwrap();
        let late = bank.pending_swap(0);
        assert_eq!((early | late) & 2, 2, "соседний бит не теряется");
        assert_eq!(
            early & late & 2,
            0,
            "соседняя публикация потреблена один раз"
        );
    });
}
