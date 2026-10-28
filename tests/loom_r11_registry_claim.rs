//! R11 shadow model of the selector's state-CAS, hint, and negative-scan
//! publication composition. All interleaved atoms and threads are Loom's.
#![cfg(all(loom, feature = "alloc-global", feature = "internals"))]

use loom::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use loom::sync::Arc;
use loom::thread;

// ghost-bind: saturation/mark-cas
const SAT_MARK: (Ordering, Ordering) = (Ordering::AcqRel, Ordering::Acquire);
// ghost-bind: saturation/publish-acqrel
const SAT_PUBLISH: (Ordering, Ordering) = (Ordering::AcqRel, Ordering::Acquire);

const FREE: u8 = 0;
const LIVE: u8 = 1;
const NO_HINT: u64 = 2;

struct RegistryModel {
    states: [AtomicU8; 2],
    hint: AtomicU64,
    saturation: AtomicU64,
}

impl RegistryModel {
    fn claim(&self) -> bool {
        // ghost-bind: registry/hint-take-spill
        let hinted = self.hint.swap(NO_HINT, Ordering::AcqRel);
        if hinted < NO_HINT
            && self.states[hinted as usize]
                // ghost-bind: heap-lease/claim-free-live
                .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            return true;
        }
        // ghost-bind: saturation/rmw-read
        if self.saturation.fetch_or(0, Ordering::Acquire) & 1 != 0 {
            return false;
        }
        let snapshot = self.saturation.load(Ordering::Acquire);
        for slot in &self.states {
            if slot
                // ghost-bind: heap-lease/claim-free-live
                .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return true;
            }
        }
        if snapshot & 1 == 0 {
            let _ =
                self.saturation
                    .compare_exchange(snapshot, snapshot | 1, SAT_MARK.0, SAT_MARK.1);
        }
        false
    }

    fn recycle_one(&self) {
        assert!(self.states[1]
            // ghost-bind: heap-lease/drop-release
            .compare_exchange(LIVE, FREE, Ordering::Release, Ordering::Relaxed)
            .is_ok());
        self.hint.store(1, Ordering::Relaxed);
        let mut old = self.saturation.load(Ordering::Acquire);
        loop {
            let next = (old + 2) & !1;
            match self
                .saturation
                .compare_exchange_weak(old, next, SAT_PUBLISH.0, SAT_PUBLISH.1)
            {
                Ok(_) => return,
                Err(actual) => old = actual,
            }
        }
    }
}

#[test]
fn completed_free_publication_cannot_leave_false_full_or_two_owners() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder.check(|| {
        let reg = Arc::new(RegistryModel {
            states: [AtomicU8::new(LIVE), AtomicU8::new(LIVE)],
            hint: AtomicU64::new(NO_HINT),
            saturation: AtomicU64::new(0),
        });
        let scanner = Arc::clone(&reg);
        let scan = thread::spawn(move || scanner.claim());
        let recycler = Arc::clone(&reg);
        let publish = thread::spawn(move || recycler.recycle_one());
        let first = scan.join().expect("scanner");
        publish.join().expect("publisher");
        // Model a displaced/consumed hint: completed availability publication
        // must independently defeat any stale negative-scan certification.
        reg.hint.store(NO_HINT, Ordering::Relaxed);
        // ghost-bind: saturation/rmw-read
        assert_eq!(reg.saturation.fetch_or(0, Ordering::Acquire) & 1, 0);
        assert_eq!(reg.states[0].load(Ordering::Acquire), LIVE);
        if first {
            assert_eq!(reg.states[1].load(Ordering::Acquire), LIVE);
        } else {
            assert_eq!(reg.states[1].load(Ordering::Acquire), FREE);
            assert!(reg.claim(), "completed publication cannot stay saturated");
        }
        assert_eq!(reg.states[1].load(Ordering::Acquire), LIVE);
    });
}
