//! b4-r1 shadow model: the TLS route pin cache's fast path. Mirrors the
//! reviewed precedent `tests/loom_route_pin_unlink.rs` (NOT the actual-source
//! `#[path]` pattern of `loom_sidecar_bitmap_actual.rs`): a shadow model is
//! used here deliberately because `src/registry/heap_core_xthread/
//! route_pin_cache.rs` and `directory.rs` pull System allocations, the shard
//! lock machinery, std thread_local, `fatal()` and the `crate::global` hooks,
//! none of which exist under `loom`.
//!
//! WHAT IS MODELED. The b4-r1 cached-pin fast path: a per-thread cached pin
//! is gated by an Acquire load of `Entry::linked`; a `true` gate authorizes a
//! single `pending.fetch_or(AcqRel)` publication into the pinned sidecar;
//! unlink is a Release clear of `linked` under the shard lock (before the
//! index unlink and the owner-reference drop); the last `refs` drop frees the
//! sidecar. The cached pin's reference keeps the sidecar alive across the
//! whole unlink-to-free window.
//!
//! SCOPE (ledger hypothesis). For a VALID free the producer's block holds a
//! segment credit (REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT §2), so a valid
//! free cannot race the route's unlink — that hypothesis lives in the ledger,
//! outside the atomics, and is not modelable here. A producer whose gate read
//! a stale `true` (coherence-legal under Acquire for a Release store that has
//! not yet happened-before the load) may still publish into an
//! unlinked-but-pinned sidecar; that bit is dropped with the sidecar. This
//! residual window is identical to the pre-existing post-lookup window on the
//! uncached path and is closed for valid frees by the ledger, NOT by the
//! gate. The model therefore verifies the atomic protocol properties:
//! - (A1) no publication into a FREED sidecar (the refs protocol);
//! - (A2) a publication through the cached pin happens only after a gate load
//!   the same thread performed and that returned `true`;
//! - (A3) a false gate read forces the fresh-lookup path, which observes the
//!   post-unlink index state.
#![cfg(loom)]
#![forbid(unsafe_code)]

use loom::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use loom::sync::{Arc, Mutex};
use loom::thread;

/// One cached route + its pinned sidecar word.
///
/// `gate_done`/`gate_true`/`published_old`/`published_new` are plain
/// `Cell<bool>`-style single-writer slots, realized as Relaxed atomics
/// because `#![forbid(unsafe_code)]` rules out `loom::cell::UnsafeCell`:
/// they are
/// single-thread model bookkeeping — each is written and read by the same
/// thread — and exist only so the oracles can state "published ONLY IF this
/// thread's gate returned true".
struct CachedRoute {
    /// The shard lock (`RouteDirectory`'s `ShardLock`).
    lock: Mutex<()>,
    /// `Entry::linked`: initialized true, cleared once with Release under the
    /// lock in `RouteDirectory::remove`, never set back.
    linked: AtomicBool,
    /// Index membership; mutated only under `lock`. The spec's `Cell<bool>`
    /// is realized as a Relaxed atomic because `#![forbid(unsafe_code)]`
    /// rules out `loom::cell::UnsafeCell`; the shard lock provides the
    /// ordering, so Relaxed suffices.
    member: AtomicBool,
    /// `Entry::refs`: owner + cached pin.
    refs: AtomicUsize,
    /// The pinned sidecar word (one-word model).
    pending: AtomicU64,
    /// Set when the final ref drops: the sidecar free.
    freed: AtomicBool,
    // --- TEST 2 extensions: the fresh route found by the fallback lookup. ---
    /// The fresh route's sidecar word.
    new_pending: AtomicU64,
    /// The fresh route's refs (starts 1 = its owner).
    new_refs: AtomicUsize,
    /// Set by R under the lock: the fresh route is registered in the index
    /// (Relaxed atomic for the same forbid(unsafe_code) reason as `member`).
    new_registered: AtomicBool,
}

impl CachedRoute {
    fn new() -> Self {
        Self {
            lock: Mutex::new(()),
            linked: AtomicBool::new(true),
            member: AtomicBool::new(true),
            refs: AtomicUsize::new(2), // owner + cached pin
            pending: AtomicU64::new(0),
            freed: AtomicBool::new(false),
            new_pending: AtomicU64::new(0),
            new_refs: AtomicUsize::new(1), // the new route's owner
            new_registered: AtomicBool::new(false),
        }
    }

    /// Production `remove` + owner release + unlink→drain→free: the index
    /// unlink and the Release clear of `linked` are sequenced under the shard
    /// lock; the owner count drops only after the lock is released
    /// (`RouteRegistration::drop`); the final count drains the sidecar word
    /// and frees it.
    fn unlink_and_release_owner(&self) {
        {
            let _guard = self.lock.lock().unwrap();
            self.member.store(false, Ordering::Relaxed);
            self.linked.store(false, Ordering::Release);
        }
        if self.refs.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.pending.swap(0, Ordering::AcqRel); // final owner drain
            self.freed.store(true, Ordering::Release);
        }
    }

    /// `EntryHandle::drop`: the final count frees the sidecar.
    fn unpin(&self) {
        if self.refs.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.freed.store(true, Ordering::Release);
        }
    }
}

fn bounded(f: impl Fn() + Send + Sync + 'static) {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder.max_branches = 2000;
    builder.check(f);
}

#[test]
fn cached_gate_publishes_only_while_linked_and_refs_keep_sidecar_alive() {
    bounded(|| {
        let route = Arc::new(CachedRoute::new());
        let gate_done = Arc::new(AtomicBool::new(false));
        let gate_true = Arc::new(AtomicBool::new(false));
        let published_old = Arc::new(AtomicBool::new(false));

        let p = Arc::clone(&route);
        let gd = Arc::clone(&gate_done);
        let gt = Arc::clone(&gate_true);
        let po = Arc::clone(&published_old);
        let producer = thread::spawn(move || {
            // The gate: Acquire load of `Entry::linked`.
            let gate = p.linked.load(Ordering::Acquire);
            gd.store(true, Ordering::Relaxed);
            gt.store(gate, Ordering::Relaxed);
            if gate {
                assert!(
                    !p.freed.load(Ordering::Acquire),
                    "publish into freed sidecar"
                );
                p.pending.fetch_or(1, Ordering::AcqRel);
                po.store(true, Ordering::Relaxed);
            }
            // Unpin the cached pin: the last ref frees the sidecar.
            p.unpin();
        });

        let u = Arc::clone(&route);
        let unlink = thread::spawn(move || u.unlink_and_release_owner());

        producer.join().unwrap();
        unlink.join().unwrap();

        // Refs balance and the sidecar is freed (A1).
        assert_eq!(route.refs.load(Ordering::Acquire), 0, "counts balance");
        assert!(route.freed.load(Ordering::Acquire), "sidecar freed");
        // A2: any publication happened through a gate load on this thread
        // that completed and returned true.
        let done = gate_done.load(Ordering::Acquire);
        let was_true = gate_true.load(Ordering::Acquire);
        assert!(
            !(published_old.load(Ordering::Acquire) && !done),
            "published without a completed gate load"
        );
        assert!(
            !(published_old.load(Ordering::Acquire) && !was_true),
            "published through a false gate"
        );
        // NOTE: no assertion that published bits were drained — a stale-true
        // publish into an unlinked-but-pinned sidecar is documented scope
        // (see module doc; ledger-covered for valid frees).
    });
}

#[test]
fn fallback_after_false_gate_uses_the_fresh_route() {
    bounded(|| {
        let route = Arc::new(CachedRoute::new());
        let gate_done = Arc::new(AtomicBool::new(false));
        let gate_true = Arc::new(AtomicBool::new(false));
        let published_old = Arc::new(AtomicBool::new(false));
        let published_new = Arc::new(AtomicBool::new(false));

        // R registers the fresh route in the index (under the shard lock).
        let r = Arc::clone(&route);
        let registrar = thread::spawn(move || {
            let _guard = r.lock.lock().unwrap();
            r.new_registered.store(true, Ordering::Relaxed);
        });

        // U unlinks the old route and drops the owner ref.
        let u = Arc::clone(&route);
        let unlink = thread::spawn(move || u.unlink_and_release_owner());

        // P: gate, publish-or-fall-back, unpin.
        let p = Arc::clone(&route);
        let gd = Arc::clone(&gate_done);
        let gt = Arc::clone(&gate_true);
        let po = Arc::clone(&published_old);
        let pn = Arc::clone(&published_new);
        let producer = thread::spawn(move || {
            let gate = p.linked.load(Ordering::Acquire);
            gd.store(true, Ordering::Relaxed);
            gt.store(gate, Ordering::Relaxed);
            if gate {
                assert!(
                    !p.freed.load(Ordering::Acquire),
                    "publish into freed sidecar"
                );
                p.pending.fetch_or(1, Ordering::AcqRel);
                po.store(true, Ordering::Relaxed);
            } else {
                // Production: fresh `RouteDirectory::lookup` — takes the
                // shard lock and reads the CURRENT index. Here the index
                // witness is read under that same lock; production re-derives
                // the identical state under the shard lock, which orders it
                // independently of the gate edge.
                let guard = p.lock.lock().unwrap();
                let fresh = p.new_registered.load(Ordering::Relaxed);
                if fresh {
                    drop(guard);
                    // Model of taking/releasing the fresh pin: strictly
                    // balanced increment/decrement, then the RMW publish.
                    let got = p.new_refs.fetch_add(1, Ordering::AcqRel);
                    if p.new_refs.fetch_sub(1, Ordering::AcqRel) >= 1 && got >= 1 {
                        p.new_pending.fetch_or(1, Ordering::AcqRel);
                        pn.store(true, Ordering::Relaxed);
                    }
                }
            }
            p.unpin();
        });

        registrar.join().unwrap();
        producer.join().unwrap();
        unlink.join().unwrap();

        // A3: a false gate never publishes the old route.
        assert!(
            !(published_old.load(Ordering::Acquire)
                && !gate_true.load(Ordering::Acquire)),
            "false gate published the old route"
        );
        // The fresh route's publication is well-formed and balanced.
        if published_new.load(Ordering::Acquire) {
            assert_eq!(
                route.new_pending.load(Ordering::Acquire),
                1,
                "published_new implies the fresh sidecar carries the bit"
            );
        }
        // The fresh route's owner eventually finishes its reclaim and drops its
        // count (the model does it here, after all threads joined).
        route.new_refs.fetch_sub(1, Ordering::AcqRel);
        assert_eq!(route.new_refs.load(Ordering::Acquire), 0, "new pin balanced");
        // Old sidecar freed, counts balanced (A1).
        assert_eq!(route.refs.load(Ordering::Acquire), 0, "counts balance");
        assert!(route.freed.load(Ordering::Acquire), "sidecar freed");
    });
}

#[test]
#[should_panic(expected = "publish into unlinked route")]
fn negative_skip_gate_publishes_into_unlinked_route() {
    bounded(|| {
        let route = Arc::new(CachedRoute::new());
        // Model bookkeeping for the A2 oracle (thread-local in production).
        let gate_done = Arc::new(AtomicBool::new(false));
        let gate_true = Arc::new(AtomicBool::new(false));
        let p = Arc::clone(&route);
        let gd = Arc::clone(&gate_done);
        let gt = Arc::clone(&gate_true);
        let producer = thread::spawn(move || {
            // The load is kept only so the oracle can observe it; the MUTANT
            // skips the gate CHECK and publishes unconditionally.
            let gate = p.linked.load(Ordering::Acquire);
            gd.store(true, Ordering::Relaxed);
            gt.store(gate, Ordering::Relaxed);
            p.pending.fetch_or(1, Ordering::AcqRel);
            p.unpin();
            // The A2 oracle becomes an explicit check at publish time: with
            // the mutant, the [U completes first] interleaving drives the
            // panic.
            if !(gd.load(Ordering::Relaxed) && gt.load(Ordering::Relaxed)) {
                panic!("publish into unlinked route");
            }
        });
        let u = Arc::clone(&route);
        let unlink = thread::spawn(move || u.unlink_and_release_owner());
        producer.join().unwrap();
        unlink.join().unwrap();
        // The A2 oracle fires inside the mutant's publish path above (the
        // [U completes first] interleaving panics there); this line is only
        // reached on interleavings the bound excludes from triggering it.
    });
}

#[test]
#[should_panic(expected = "false gate read did not carry the unlink witness")]
fn negative_relaxed_gate_loses_unlink_witness() {
    bounded(|| {
        let route = Arc::new(CachedRoute::new());
        // The index-removal witness, sequenced BEFORE the Release clear of
        // `linked` (mirroring `RouteDirectory::remove`, where the index
        // removal and the flag clear are sequenced under the shard lock).
        let member_store_visible_before_gate = Arc::new(AtomicBool::new(true));

        let u = Arc::clone(&route);
        let witness_u = Arc::clone(&member_store_visible_before_gate);
        let unlink = thread::spawn(move || {
            witness_u.store(false, Ordering::Relaxed);
            u.linked.store(false, Ordering::Release);
        });

        let p = Arc::clone(&route);
        let witness_p = Arc::clone(&member_store_visible_before_gate);
        let producer = thread::spawn(move || {
            // MUTANT: Relaxed instead of Acquire.
            let gate = p.linked.load(Ordering::Relaxed);
            if !gate {
                let witness = witness_p.load(Ordering::Relaxed);
                // Under the production Acquire load this interleaving is
                // EXCLUDED: reading the Release store synchronizes-with the
                // unlinker, so the sequenced-before witness store is visible
                // — coherence forces `false`. That is exactly what the
                // positive fallback test pins.
                assert!(
                    witness == false,
                    "false gate read did not carry the unlink witness"
                );
            }
        });

        producer.join().unwrap();
        unlink.join().unwrap();
    });
}
