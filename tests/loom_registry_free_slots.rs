//! Bounded Loom model of the Registry-style free-slot binding.
//!
//! Scope: the production global `Registry` uses a const-capable `cfg(loom)`
//! shim for its static initialization. This test models that same
//! head↔slot-resident-links/state binding with the REAL
//! `tagged_index_stack::{StackHead, StackStorage, StackOps}` implementation.
//! The model is intentionally separate from the production shim: it checks
//! the Registry claim/recycle protocol through the crate stack itself.
//!
//! Run with:
//!
//! ```text
//! RUSTFLAGS="--cfg loom" cargo test --test loom_registry_free_slots \
//!     --features alloc-global,tagged-index-stack/loom
//! ```

#![cfg(all(loom, feature = "alloc-global"))]

use loom::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use loom::sync::Arc;
use loom::thread;

use tagged_index_stack::{StackHead, StackOps, StackStorage, TAIL};

const INDEX_BITS: u32 = 16;
const SLOT_COUNT: usize = 2;
const STATE_FREE: u32 = 0;
const STATE_LIVE: u32 = 1;

struct Slot {
    state: AtomicU32,
    owner: AtomicU32,
    next_free: AtomicU32,
}

/// A miniature Registry: one stable head is bound to the slot-resident links
/// for the whole value's lifetime, while `state` is the claim/recycle gate.
struct RegistryModel {
    free_slots: StackHead<INDEX_BITS>,
    slots: [Slot; SLOT_COUNT],

    // These fields are model-only rendezvous/activation oracles. They never
    // participate in ownership decisions or replace a stack operation.
    pop_load_arrivals: AtomicU32,
    pop_gate_open: AtomicBool,
    push_store_arrivals: AtomicU32,
    push_gate_open: AtomicBool,

    claimed_arrivals: AtomicU32,
    claimed_gate_open: AtomicBool,
    activation_enabled: AtomicBool,
}

impl RegistryModel {
    fn empty() -> Arc<Self> {
        Arc::new(Self {
            free_slots: StackHead::new(),
            slots: core::array::from_fn(|_| Slot {
                // Like the zeroed production slot backing, links are lazy:
                // the real StackOps push writes a link before publication.
                state: AtomicU32::new(STATE_FREE),
                owner: AtomicU32::new(0),
                next_free: AtomicU32::new(0),
            }),
            pop_load_arrivals: AtomicU32::new(0),
            pop_gate_open: AtomicBool::new(false),
            push_store_arrivals: AtomicU32::new(0),
            push_gate_open: AtomicBool::new(false),
            claimed_arrivals: AtomicU32::new(0),
            claimed_gate_open: AtomicBool::new(false),
            activation_enabled: AtomicBool::new(false),
        })
    }

    fn new() -> Arc<Self> {
        let registry = Self::empty();

        // Seed exactly the two FREE slots through the real StackOps surface.
        // SAFETY: each index is in this storage's 0..SLOT_COUNT link domain,
        // has never been reachable or pushed, and has a unique initial
        // publish authority. The RegistryModel owns the sole head↔links
        // binding used by these calls.
        unsafe { registry.push_index(0) }.expect("initial slot push must fit");
        // SAFETY: index 1 has the same fresh-domain and unique-authority
        // proof as index 0; the preceding push made index 0 the admitted
        // head link and did not make index 1 reachable.
        unsafe { registry.push_index(1) }.expect("initial slot push must fit");

        // The seed pushes also pass through store_next. Start the activation
        // oracles at the first operation performed by the worker threads.
        registry.pop_load_arrivals.store(0, Ordering::Relaxed);
        registry.pop_gate_open.store(false, Ordering::Relaxed);
        registry.push_store_arrivals.store(0, Ordering::Relaxed);
        registry.push_gate_open.store(false, Ordering::Relaxed);
        registry.activation_enabled.store(true, Ordering::Release);
        registry
    }

    fn slot(&self, index: u32) -> &Slot {
        let index = usize::try_from(index).expect("stack index must fit usize");
        self.slots
            .get(index)
            .expect("stack index must be in the model's slot domain")
    }

    /// One-shot two-party rendezvous used only to force a real stack CAS
    /// overlap. The first two calls cannot pass until both participants have
    /// reached the hook; later calls (including the losing CAS retry) are
    /// still counted, then pass directly.
    fn rendezvous(arrivals: &AtomicU32, gate_open: &AtomicBool) {
        let arrival = arrivals.fetch_add(1, Ordering::AcqRel) + 1;
        if gate_open.load(Ordering::Acquire) {
            return;
        }

        if arrival == 2 {
            gate_open.store(true, Ordering::Release);
        } else {
            while !gate_open.load(Ordering::Acquire) {
                thread::yield_now();
            }
        }
    }

    /// Registry claim: pop is only an internal candidate operation. The
    /// caller-visible result is returned only after FREE→LIVE succeeds.
    fn claim(&self) -> Option<u32> {
        loop {
            let index = self.pop_index()?;
            if self
                .slot(index)
                .state
                .compare_exchange(STATE_FREE, STATE_LIVE, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Some(index);
            }
            // This is the production claim retry for a stale/invalid
            // candidate. A valid binding makes this branch unreachable: a
            // successful stack pop transfers the candidate's sole authority.
        }
    }

    /// Registry recycle: release LIVE→FREE first, then publish the index via
    /// the real unsafe StackOps push. No caller may push a still-live slot.
    fn recycle(&self, index: u32, owner: u32) {
        let slot = self.slot(index);
        assert_eq!(
            slot.state.compare_exchange(
                STATE_LIVE,
                STATE_FREE,
                Ordering::Release,
                Ordering::Relaxed,
            ),
            Ok(STATE_LIVE),
            "recycle must own the slot's LIVE state"
        );
        assert_eq!(
            slot.owner
                .compare_exchange(owner, 0, Ordering::Release, Ordering::Relaxed,),
            Ok(owner),
            "recycle must release exactly its owner"
        );

        // SAFETY: `index` was returned by this model's successful claim,
        // remains in-domain, is no longer reachable after that pop, and this
        // caller just won its LIVE→FREE transition, giving it the unique
        // recycle/publish authority required by StackOps::push_index.
        unsafe { self.push_index(index) }.expect("recycle push must fit");
    }
}

// SAFETY: this is the single head↔links binding. `free_slots` is never
// replaced; every hook addresses the same immovable slot's `next_free` cell;
// all link accesses are atomic; and every index used by the model is in the
// two-slot domain. The StackOps blanket implementation is therefore the only
// stack algorithm operating on this binding.
unsafe impl StackStorage<INDEX_BITS> for RegistryModel {
    unsafe fn head(&self) -> &StackHead<INDEX_BITS> {
        &self.free_slots
    }

    unsafe fn load_next(&self, index: u32) -> u32 {
        // Both worker pops reach this hook after loading the same head word.
        // Holding them here forces the subsequent REAL StackOps CASes to
        // contend; the hook does not implement or substitute the CAS loop.
        if self.activation_enabled.load(Ordering::Acquire) {
            Self::rendezvous(&self.pop_load_arrivals, &self.pop_gate_open);
        }
        self.slot(index).next_free.load(Ordering::Acquire)
    }

    unsafe fn store_next(&self, index: u32, next: u32) {
        // Both worker pushes have loaded the same head before this hook. This
        // forces the subsequent REAL StackOps head CASes to contend as well.
        if self.activation_enabled.load(Ordering::Acquire) {
            Self::rendezvous(&self.push_store_arrivals, &self.push_gate_open);
        }
        if next != TAIL {
            let next = usize::try_from(next).expect("non-tail link must fit usize");
            assert!(next < SLOT_COUNT, "StackOps emitted an out-of-domain link");
        }
        self.slot(index).next_free.store(next, Ordering::Release);
    }
}

fn worker(registry: Arc<RegistryModel>, owner: u32) {
    let index = registry
        .claim()
        .expect("two seeded FREE slots must satisfy two claims");
    assert_eq!(
        registry
            .slot(index)
            .owner
            .compare_exchange(0, owner, Ordering::AcqRel, Ordering::Acquire),
        Ok(0),
        "two LIVE claims must never acquire the same slot"
    );

    // Keep both owners LIVE until both claims have completed. This rules out
    // a sequential recycle/reclaim from making an accidental same-index
    // result look like concurrent double ownership.
    RegistryModel::rendezvous(&registry.claimed_arrivals, &registry.claimed_gate_open);
    thread::yield_now();
    registry.recycle(index, owner);
}

#[test]
fn registry_free_slots_claim_recycle_is_conservative_under_contention() {
    loom::model(|| {
        let registry = RegistryModel::new();
        let first_registry = Arc::clone(&registry);
        let second_registry = Arc::clone(&registry);

        let first = thread::spawn(move || worker(first_registry, 1));
        let second = thread::spawn(move || worker(second_registry, 2));
        first.join().expect("first worker must not panic");
        second.join().expect("second worker must not panic");

        // Both state transitions completed and both slots were recycled.
        for slot in &registry.slots {
            assert_eq!(slot.state.load(Ordering::Acquire), STATE_FREE);
            assert_eq!(slot.owner.load(Ordering::Acquire), 0);
        }

        // Drain exactly the bounded capacity through the real StackOps API.
        // This is the conservation oracle: no loss, no duplicate, no hidden
        // third publication, and the stack is empty afterward.
        let first_free = registry
            .pop_index()
            .expect("first recycled slot must be reachable");
        let second_free = registry
            .pop_index()
            .expect("second recycled slot must be reachable");
        assert_ne!(first_free, second_free, "free-list returned a duplicate");
        let mut recovered = [first_free, second_free];
        recovered.sort_unstable();
        assert_eq!(recovered, [0, 1], "free-slot conservation failed");
        assert_eq!(registry.pop_index(), None, "free-list has an extra issue");

        // Non-vacuity: the first two worker pop hooks ran before either real
        // pop CAS, so one CAS necessarily lost and invoked StackOps' retry.
        // The same construction applies to the two concurrent recycle pushes.
        // The third counted hook call is the loser's retry, not an auxiliary
        // model CAS.
        assert!(registry.pop_gate_open.load(Ordering::Acquire));
        assert!(registry.pop_load_arrivals.load(Ordering::Acquire) >= 3);
        assert!(registry.push_gate_open.load(Ordering::Acquire));
        assert!(registry.push_store_arrivals.load(Ordering::Acquire) >= 3);
    });
}

#[test]
fn recycled_slot_is_not_published_until_it_is_free() {
    loom::model(|| {
        let registry = RegistryModel::empty();
        registry.slot(0).state.store(STATE_LIVE, Ordering::Relaxed);
        registry.slot(0).owner.store(1, Ordering::Relaxed);

        let recycle_done = Arc::new(AtomicBool::new(false));
        let recycler_registry = Arc::clone(&registry);
        let recycler_done = Arc::clone(&recycle_done);
        let recycler = thread::spawn(move || {
            recycler_registry.recycle(0, 1);
            recycler_done.store(true, Ordering::Release);
        });

        let claimant_registry = Arc::clone(&registry);
        let claimant_done = Arc::clone(&recycle_done);
        let claimant = thread::spawn(move || {
            let index = loop {
                if let Some(index) = claimant_registry.claim() {
                    break index;
                }
                if claimant_done.load(Ordering::Acquire) {
                    break claimant_registry
                        .claim()
                        .expect("recycle completed but the sole slot was lost from the free list");
                }
                thread::yield_now();
            };

            assert_eq!(index, 0, "the only recycled slot must be returned");
            assert_eq!(
                claimant_registry.slot(index).owner.compare_exchange(
                    0,
                    2,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ),
                Ok(0),
                "a published FREE slot must not retain its previous owner"
            );
            index
        });

        recycler.join().expect("recycler must not panic");
        assert_eq!(claimant.join().expect("claimant must not panic"), 0);
        assert_eq!(registry.slot(0).state.load(Ordering::Acquire), STATE_LIVE);
        assert_eq!(registry.slot(0).owner.load(Ordering::Acquire), 2);
        assert_eq!(
            registry.pop_index(),
            None,
            "a claimed slot must not remain reachable from free_slots"
        );
    });
}
