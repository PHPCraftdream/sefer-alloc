//! loom model of the cross-thread-free protocol (task #37, spec
//! `docs/CROSS_THREAD_STATE_MACHINES.md` §6).
//!
//! This models SM-BLOCK + SM-CHANNEL under the *intended* discipline:
//!   - ONE Owner mutates the free list and collects the channel.
//!   - ONE Remote only *publishes* a freed block's offset into the channel.
//!
//! It asserts the load-bearing invariant **I-BLOCK-1**: a block is never
//! simultaneously LIVE (handed to the app) and on the free list. The shadow
//! `handed[i]` tracks "the allocator has handed block i out and it is not yet
//! freed"; the free list tracks "block i is reclaimable". The two sets must be
//! disjoint at every step.
//!
//! ## Why this model
//!
//! Static reasoning repeatedly failed to either find or rule out the
//! within-single-ownership double-hand-out (see RACE_DRAIN_RECLAIM.md §10
//! correction). loom enumerates the interleavings of Owner-drain/reclaim/pop
//! against Remote-publish. If the intended single-owner discipline is correct,
//! this model is GREEN — which itself is the result: it proves the *protocol*
//! sound and points the bug at an implementation deviation (to be bisected).
//! If loom finds a violation, the failing interleaving is the bug.
//!
//! ## Counterfactual (non-vacuity)
//!
//! `broken_reclaim_ignores_handed` models a reclaim that does NOT respect the
//! "a published offset belongs to an already-released block" ordering — it
//! reclaims a block whose `handed` is still set. loom must find the
//! `LIVE ∧ free-listed` interleaving there (`#[should_panic]`); if it passes,
//! this harness is vacuous.
//!
//! Run: `RUSTFLAGS="--cfg loom" cargo test --features alloc-core,alloc-xthread --test loom_xthread_protocol`

#![cfg(loom)]

use loom::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use loom::sync::Arc;
use loom::thread;

const NIL: u32 = u32::MAX;

/// A tiny segment of 2 blocks at offsets 0 and 1 (abstract units, not bytes).
/// The "channel" is a single-slot publish (one in-flight remote free at a time
/// per block is all the model needs to exhibit a resurrection).
struct Seg {
    /// Free-list head (Owner-only writes; modelled atomic so loom can observe
    /// it from the assert, but only the Owner thread ever stores).
    fl_head: AtomicU32,
    /// Each block's first word: when free it holds the next offset; when the
    /// channel published it, the Remote does NOT touch it (non-intrusive).
    word: [AtomicU32; 2],
    /// Channel: published offset (NIL = empty). Single producer (Remote),
    /// single consumer (Owner) in this model.
    chan: AtomicU32,
    /// Shadow: has the allocator handed this block out (LIVE) and not yet freed?
    handed: [AtomicBool; 2],
}

impl Seg {
    fn new() -> Arc<Self> {
        Arc::new(Seg {
            fl_head: AtomicU32::new(NIL),
            word: [AtomicU32::new(NIL), AtomicU32::new(NIL)],
            chan: AtomicU32::new(NIL),
            handed: [AtomicBool::new(false), AtomicBool::new(false)],
        })
    }

    /// Owner: push `off` onto the free list (reclaim or local free).
    /// I-BLOCK-1 guard: the block must NOT be handed out when it joins the
    /// free list.
    fn fl_push(&self, off: u32) {
        assert!(
            !self.handed[off as usize].load(Ordering::Acquire),
            "I-BLOCK-1 VIOLATED: block {off} pushed to free list while LIVE (handed out)"
        );
        let old = self.fl_head.load(Ordering::Relaxed);
        self.word[off as usize].store(old, Ordering::Relaxed);
        self.fl_head.store(off, Ordering::Relaxed);
    }

    /// Owner: pop a block from the free list → hand it out (LIVE). Returns the
    /// offset, or NIL if empty.
    fn fl_pop(&self) -> u32 {
        let h = self.fl_head.load(Ordering::Relaxed);
        if h == NIL {
            return NIL;
        }
        let next = self.word[h as usize].load(Ordering::Relaxed);
        self.fl_head.store(next, Ordering::Relaxed);
        self.handed[h as usize].store(true, Ordering::Release);
        h
    }

    /// Owner: drain the channel → reclaim the published offset to the free list.
    fn drain(&self) {
        let off = self.chan.swap(NIL, Ordering::AcqRel);
        if off != NIL {
            self.fl_push(off);
        }
    }

    /// Remote: free block `off` it currently holds — release it (no longer
    /// LIVE), then publish its offset into the channel. The release must be
    /// ordered before the publish (the app is done with it before the freer
    /// hands it back).
    fn remote_free(&self, off: u32) {
        self.handed[off as usize].store(false, Ordering::Release);
        // publish (single producer in this model): only if empty, to keep the
        // 1-slot channel honest; a real ring is bounded MPSC (modelled clean
        // in loom_remote_ring.rs).
        let _ = self
            .chan
            .compare_exchange(NIL, off, Ordering::AcqRel, Ordering::Relaxed);
    }
}

/// The intended discipline: Owner allocates block 0, hands it to the Remote;
/// Remote frees it (publish); Owner drains+reclaims+reallocates — concurrently.
/// I-BLOCK-1 must hold on every interleaving.
#[test]
fn protocol_single_owner_never_resurrects() {
    let mut b = loom::model::Builder::new();
    b.preemption_bound = Some(3);
    b.check(|| {
        let seg = Seg::new();

        // Pre-state: block 0 has been handed out to the Remote (LIVE).
        seg.handed[0].store(true, Ordering::Release);

        let owner_seg = Arc::clone(&seg);
        let owner = thread::spawn(move || {
            // The Owner runs its alloc-slow path a couple of times: drain the
            // channel, reclaim, then try to re-pop (re-hand-out).
            for _ in 0..2 {
                owner_seg.drain();
                let got = owner_seg.fl_pop();
                if got != NIL {
                    // "use" the block, then free it locally so the cycle can
                    // continue (models the owner reusing then releasing).
                    owner_seg.handed[got as usize].store(false, Ordering::Release);
                    owner_seg.fl_push(got);
                }
            }
        });

        let remote_seg = Arc::clone(&seg);
        let remote = thread::spawn(move || {
            // The Remote frees block 0 (the one it was handed).
            remote_seg.remote_free(0);
        });

        owner.join().unwrap();
        remote.join().unwrap();
    });
}

/// COUNTERFACTUAL: a reclaim that pushes the published offset WITHOUT the
/// release-before-publish ordering — i.e. the Remote publishes before releasing
/// (handed still true), and the Owner drains in that window. loom must find the
/// interleaving where `fl_push` fires the I-BLOCK-1 assert. If this test does
/// NOT panic, the harness is vacuous.
#[test]
#[should_panic(expected = "I-BLOCK-1 VIOLATED")]
fn broken_publish_before_release_resurrects() {
    let mut b = loom::model::Builder::new();
    b.preemption_bound = Some(3);
    b.check(|| {
        let seg = Seg::new();
        seg.handed[0].store(true, Ordering::Release);

        let owner_seg = Arc::clone(&seg);
        let owner = thread::spawn(move || {
            for _ in 0..2 {
                owner_seg.drain();
                let _ = owner_seg.fl_pop();
            }
        });

        let remote_seg = Arc::clone(&seg);
        let remote = thread::spawn(move || {
            // BUG: publish BEFORE releasing — the block is still handed out when
            // its offset enters the channel, so a concurrent drain reclaims a
            // LIVE block.
            let _ = remote_seg
                .chan
                .compare_exchange(NIL, 0, Ordering::AcqRel, Ordering::Relaxed);
            remote_seg.handed[0].store(false, Ordering::Release);
        });

        owner.join().unwrap();
        remote.join().unwrap();
    });
}
