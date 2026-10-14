//! L2 activation oracle: actual installed GlobalAlloc first-allocation observation.
use sefer_alloc::registry::{config_conflicts_total, heaps_claimed_high_water, HeapRegistry};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
#[path = "../benches/support/r20_prf04_heap_reuse.rs"]
mod hr;
#[global_allocator]
static GLOBAL: hr::FirstAlloc<true> = hr::FirstAlloc;

#[cfg(prf04_heap_reuse_oracle)]
fn candidate_observers() -> (Option<u64>, Option<u64>) {
    (
        Some(u64::from(HeapRegistry::dbg_free_slot_hint_popcount())),
        Some(HeapRegistry::dbg_registry_free_set_claims()),
    )
}
#[cfg(not(prf04_heap_reuse_oracle))]
fn candidate_observers() -> (Option<u64>, Option<u64>) {
    (None, None)
}

struct State {
    ready: bool,
    release: bool,
    calls: u64,
}
struct Child {
    handle: JoinHandle<u64>,
    gate: Arc<(Mutex<State>, Condvar)>,
    claim_delta: u64,
}
fn observed_target() -> u64 {
    let layout = std::alloc::Layout::from_size_align(8, 8).expect("valid target layout");
    // Through the installed wrapper: the latch fires here unless std's thread start
    // already took this thread's first allocation; either way target_calls() is 1.
    // SAFETY: non-zero valid layout; the allocation is retained to process exit.
    let ptr = unsafe { std::alloc::alloc(layout) };
    assert!(!ptr.is_null(), "observed target allocation failed");
    hr::target_calls()
}
impl Child {
    fn spawn() -> Self {
        let gate = Arc::new((
            Mutex::new(State {
                ready: false,
                release: false,
                calls: 0,
            }),
            Condvar::new(),
        ));
        let child_gate = Arc::clone(&gate);
        let before = HeapRegistry::dbg_registry_claim_sources();
        let free_before = candidate_observers().1;
        let handle = std::thread::spawn(move || {
            let calls = observed_target();
            let (lock, cv) = &*child_gate;
            let mut state = lock.lock().expect("child gate");
            state.calls = calls;
            state.ready = true;
            cv.notify_all();
            while !state.release {
                state = cv.wait(state).expect("child gate");
            }
            calls
        });
        let mut child = Self {
            handle,
            gate,
            claim_delta: 0,
        };
        let (lock, cv) = &*child.gate;
        let mut state = lock.lock().expect("ready gate");
        while !state.ready {
            state = cv.wait(state).expect("ready gate");
        }
        drop(state);
        let after = HeapRegistry::dbg_registry_claim_sources();
        let free_after = candidate_observers().1;
        let source_delta = after
            .iter()
            .zip(before)
            .map(|(a, b)| a.checked_sub(b).expect("source counter monotonic"))
            .sum::<u64>();
        let free_delta = match (free_before, free_after) {
            (Some(a), Some(b)) => b.checked_sub(a).expect("free counter monotonic"),
            _ => 0,
        };
        let claim_delta = source_delta + free_delta;
        let actual_calls = child.observe();
        if claim_delta != 1 || actual_calls != 1 {
            child.unblock_and_join();
            panic!(
                "first-allocation witness invalid: claims={claim_delta}, target calls={actual_calls}"
            );
        }
        child.claim_delta = claim_delta;
        child
    }
    fn observe(&self) -> u64 {
        self.gate.0.lock().expect("observe gate").calls
    }
    fn unblock_and_join(self) {
        let (lock, cv) = &*self.gate;
        lock.lock().expect("failure release gate").release = true;
        cv.notify_one();
        let _ = self.handle.join().expect("child panic while unblocking");
    }
    fn release(self) -> u64 {
        assert_eq!(self.claim_delta, 1, "stored first-allocation claim witness");
        let before_sources = HeapRegistry::dbg_registry_claim_sources();
        let before_free = candidate_observers().1;
        let (lock, cv) = &*self.gate;
        lock.lock().expect("release gate").release = true;
        cv.notify_one();
        let calls = self.handle.join().expect("child panic");
        assert_eq!(calls, 1, "child witness mismatch");
        let after_sources = HeapRegistry::dbg_registry_claim_sources();
        let after_free = candidate_observers().1;
        let source_delta = after_sources
            .iter()
            .zip(before_sources)
            .map(|(a, b)| a.checked_sub(b).expect("source counter monotonic"))
            .sum::<u64>();
        let free_delta = match (before_free, after_free) {
            (Some(a), Some(b)) => b.checked_sub(a).expect("free counter monotonic"),
            _ => 0,
        };
        assert_eq!(
            source_delta + free_delta,
            0,
            "release window must not add claims"
        );
        calls
    }
}
#[derive(Clone, Copy)]
struct Snapshot {
    sources: [u64; 4],
    high: u64,
    conflicts: u64,
    candidate: (Option<u64>, Option<u64>),
}
fn snapshot() -> Snapshot {
    Snapshot {
        sources: HeapRegistry::dbg_registry_claim_sources(),
        high: u64::from(heaps_claimed_high_water()),
        conflicts: config_conflicts_total(),
        candidate: candidate_observers(),
    }
}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert_eq!(
        args.len(),
        3,
        "usage: <cold|recycled_alloc|recycled_release|wave|wave_release> <N> <A|B>"
    );
    let family = args[0].as_str();
    let n = args[1].parse::<usize>().expect("N");
    let arm = args[2].as_str();
    assert!(n > 0 && ["A", "B"].contains(&arm));
    hr::warm_parent_and_arm();
    match family {
        "cold" => {
            let before = snapshot();
            let children: Vec<_> = (0..n).map(|_| Child::spawn()).collect();
            let calls: Vec<_> = children.iter().map(Child::observe).collect();
            let after = snapshot();
            assert_eq!(calls.iter().sum::<u64>(), n as u64);
            std::mem::forget(children);
            emit(family, n, arm, before, after, calls.iter().sum());
        }
        "recycled_alloc" | "recycled_release" => {
            drop(HeapRegistry::dbg_claim_lease().expect("recycled setup"));
            let before = snapshot();
            let mut calls = Vec::with_capacity(n);
            for _ in 0..n {
                let c = Child::spawn();
                let observed = c.observe();
                let joined = c.release();
                assert_eq!(joined, observed);
                calls.push(observed);
            }
            let after = snapshot();
            assert_eq!(calls.iter().sum::<u64>(), n as u64);
            emit(family, n, arm, before, after, calls.iter().sum());
        }
        "wave" => {
            hr::l2_wave_setup();
            let before = snapshot();
            let children: Vec<_> = (0..n).map(|_| Child::spawn()).collect();
            let calls: Vec<_> = children.iter().map(Child::observe).collect();
            let after = snapshot();
            assert_eq!(calls.iter().sum::<u64>(), n as u64);
            std::mem::forget(children);
            emit(family, n, arm, before, after, calls.iter().sum());
        }
        "wave_release" => {
            let mut children: Vec<_> = (0..hr::WAVE_SLOTS).map(|_| Child::spawn()).collect();
            assert!(n <= children.len());
            let before = snapshot();
            let calls: Vec<_> = children.iter().map(Child::observe).collect();
            for c in children.drain(..n) {
                assert_eq!(c.release(), 1);
            }
            let joins = calls.iter().take(n).sum();
            let after = snapshot();
            assert_eq!(calls.iter().sum::<u64>(), hr::WAVE_SLOTS as u64);
            let source_delta = after
                .sources
                .iter()
                .zip(before.sources)
                .map(|(a, b)| a.checked_sub(b).expect("source counter monotonic"))
                .sum::<u64>();
            let free_delta = match (before.candidate.1, after.candidate.1) {
                (Some(a), Some(b)) => b.checked_sub(a).expect("free-set counter monotonic"),
                _ => 0,
            };
            assert_eq!(
                source_delta + free_delta,
                0,
                "release window must not add claims"
            );
            std::mem::forget(children);
            emit(family, n, arm, before, after, joins);
        }
        _ => panic!("unknown family"),
    }
}
fn emit(family: &str, n: usize, arm: &str, before: Snapshot, after: Snapshot, actual_calls: u64) {
    let observer = after.candidate.0.is_some() && after.candidate.1.is_some();
    let conflicts_delta = after.conflicts - before.conflicts;
    assert_eq!(conflicts_delta, 0);
    let delta = |i: usize| after.sources[i] - before.sources[i];
    let candidate_delta =
        |index: usize| match (before.candidate_at(index), after.candidate_at(index)) {
            (Some(a), Some(b)) if index == 0 => Some(
                i64::try_from(b).expect("popcount fits i64")
                    - i64::try_from(a).expect("popcount fits i64"),
            ),
            (Some(a), Some(b)) => Some(
                i64::try_from(b.checked_sub(a).expect("take monotonic"))
                    .expect("take delta fits i64"),
            ),
            _ => None,
        };
    let target_bind_claims = [delta(0), delta(1), delta(2), delta(3)].iter().sum::<u64>();
    let spill_delta = candidate_delta(0).map_or_else(|| "null".to_owned(), |v| v.to_string());
    let take_delta = candidate_delta(1).map_or_else(|| "null".to_owned(), |v| v.to_string());
    println!(
        concat!(
            "{{\"arm\":\"{}\",\"family\":\"{}\",\"n\":{},",
            "\"hint\":{},\"bump\":{},\"capScan\":{},\"recovery\":{},",
            "\"highWaterDelta\":{},\"heap_reuse_spill_bits_delta\":{},",
            "\"heap_reuse_take_hits\":{},\"observer\":{},",
            "\"targetBindClaims\":{},\"actualTargetCalls\":{},",
            "\"configConflictsDelta\":{}}}"
        ),
        arm,
        family,
        n,
        delta(0),
        delta(1),
        delta(2),
        delta(3),
        after.high - before.high,
        spill_delta,
        take_delta,
        observer,
        target_bind_claims,
        actual_calls,
        conflicts_delta
    );
    if arm == "B" {
        assert!(observer, "candidate oracle hooks unavailable");
    }
}
impl Snapshot {
    fn candidate_at(self, index: usize) -> Option<u64> {
        if index == 0 {
            self.candidate.0
        } else {
            self.candidate.1
        }
    }
}
