//! Narrow-provenance Miri cell for the b4-r1 per-thread route pin cache
//! (round-2 P2 rewrite). The round-1 cell was rejected: it carried full
//! pointers through `usize` (no `Box` ownership, no narrow reborrow) and its
//! only oracle (foreign-or-unroutable counter == 0) is equally satisfied by a
//! DISABLED cache. This cell fixes both.
//!
//! What is proven, with REAL cross-thread frees of REAL `Box` allocations
//! through the installed `#[global_allocator]`:
//! - the first foreign free of a segment base is a cache MISS + INSERT;
//! - the second foreign free of the SAME base takes the cached fast path
//!   (HIT);
//! - a delivered publication is observable: after the terminal RMW the
//!   route's pending bit reads set. `TerminalPublicationGate`
//!   deterministically pauses the production routing path right after a
//!   successful publication of the armed block (the same accepted pattern as
//!   `tests/support/r8_global_box_witness.rs::paused_terminal_owner_retirement`);
//! - ownership transport is provenance-clean: owner-allocated `Box`es are
//!   moved across the thread boundary by value inside a `Send` wrapper (the
//!   reviewed `tests/r8_global_box_provenance.rs` pattern) — NO full pointers
//!   through `usize`, NO pointer reconstruction: every free is the real
//!   `Box` destructor;
//! - worker exit releases the retained pins through the TLS cache dtor
//!   (`PIN_CACHE_THREAD_RELEASES` delta);
//! - the owner deterministically drains every pending bit with same-class
//!   own alloc/free cycles (the production drain trigger, as in
//!   `tests/b4_r1_pin_cache.rs::foreign_free_delivery_reclaim_and_totals`).
//!
//! ACTIVATION ORACLE (R30-8) — counterfactual mutant: "publish_cached always
//! returns Miss" (cache effectively disabled: the second same-base free
//! re-runs the cold path every time). Under that mutant `PIN_CACHE_HITS`
//! stays flat, so the `hits delta >= 1` assertion below turns this cell red
//! on the CI miri-core steps.
//!
//! Concurrency is NOT claimed: owner allocations complete BEFORE the foreign
//! frees start (a deterministic schedule). Only the delivery observation is
//! made deterministic, via the publication gate.
//!
//! Every free here must resolve through the routed paths:
//! `AllocCore::dbg_foreign_or_unroutable_frees` delta must be exactly 0.
//!
//! Runs without libtest (`harness = false`, like `miri_global_box_acceptance`):
//! under the installed allocator on non-fastbin rows, libtest's own teardown
//! drops a `Box` by value on the main thread, and that free hits the accepted
//! P1-box defect (card 164, `dealloc_small` writes the link while the frame is
//! live) after this scenario has already passed.

#[cfg(all(
    miri,
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]
mod cell {
    use std::alloc::GlobalAlloc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use sefer_alloc::alloc_core::AllocCore;
    use sefer_alloc::registry::bootstrap;
    use sefer_alloc::registry::segment_route::{
        RouteDirectory, RoutePin, TerminalPublicationGate as Gate,
    };
    use sefer_alloc::SeferAlloc;

    #[global_allocator]
    static GLOBAL: SeferAlloc = SeferAlloc::new();

    /// One small size class, 256 B / align 8 — the same class the native
    /// `tests/b4_r1_pin_cache.rs` fixtures free through the routed path.
    #[repr(align(8))]
    struct Block([u8; 256]);

    impl Block {
        fn addr(&self) -> usize {
            self.0.as_ptr().addr()
        }
    }

    const SEGMENT: usize = 4 * 1024 * 1024;

    fn base_of(addr: usize) -> usize {
        addr & !(SEGMENT - 1)
    }

    /// Address-only observation of a route's pending bit: the pointer is only a
    /// numeric lookup key, exactly like every production foreign free. No
    /// provenance is reconstructed, which keeps the observation valid under
    /// `-Zmiri-strict-provenance`.
    fn pending_at(addr: usize) -> bool {
        // SAFETY: `lookup` reads only the address; the returned pin observes the
        // route's sidecar, never dereferences the address.
        match RouteDirectory::global().lookup(core::ptr::without_provenance_mut::<u8>(addr)) {
            Some(pin) => pin.pending_for_test(core::ptr::without_provenance_mut::<u8>(addr)),
            None => false,
        }
    }

    /// Owner allocations finish before the worker starts; the flag only keeps
    /// the schedule explicit (and mirrors the previous cell's start gate).
    static OWNER_DONE: AtomicBool = AtomicBool::new(false);

    pub fn narrow_pin_cache_hit_release_and_delivery_miri() {
        let _ = bootstrap::ensure();

        let totals = || RoutePin::dbg_route_pin_cache_totals();
        // The six counters are six separate Relaxed loads, not a snapshot; wait
        // for two identical consecutive reads before any delta assertion (same
        // protocol as the native `tests/b4_r1_pin_cache.rs` cell).
        let quiescent_totals = || {
            let mut last = totals();
            for _ in 0..1000 {
                let next = totals();
                if next == last {
                    return next;
                }
                last = next;
                std::hint::spin_loop();
            }
            last
        };

        let (h0, m0, i0, _e0, u0, r0, _s0) = quiescent_totals();
        let foreign_before = AllocCore::dbg_foreign_or_unroutable_frees();

        // ------------------------------------------------------------------
        // Track only two bases. Keep all other live Boxes in spill rather than
        // dropping inside the allocation loop; the bounded search is runtime-heavy
        // under Miri (up to 32,768 allocations).
        // ------------------------------------------------------------------
        let mut groups: Vec<(usize, Vec<Box<Block>>)> = Vec::new();
        // Keep surplus owners alive until their Box destructors run at the end
        // of this frame; the selected witness is one owning allocation per base,
        // with two allocations sharing the first base for the cache-hit oracle.
        let mut spill: Vec<Box<Block>> = Vec::new();
        let covered = |groups: &[(usize, Vec<Box<Block>>)]| {
            groups.len() == 2
                && groups.iter().all(|(_, v)| !v.is_empty())
                && groups.iter().any(|(_, v)| v.len() >= 2)
        };
        for _ in 0..32_768 {
            let block = Box::new(Block([0u8; 256]));
            let base = base_of(block.addr());
            let open_slot = groups.len() < 2;
            match groups.iter_mut().find(|(g, _)| *g == base) {
                Some((_, v)) if v.len() < 2 => v.push(block),
                Some(_) => spill.push(block),
                None if open_slot => groups.push((base, vec![block])),
                None => spill.push(block),
            }
            if covered(&groups) {
                break;
            }
        }
        assert!(covered(&groups), "owner could not cover two segment bases");

        let heavy = groups
            .iter()
            .position(|(_, v)| v.len() >= 2)
            .expect("a base with two live blocks");
        let mut x_group = groups.remove(heavy).1;
        let mut y_group = groups.remove(0).1;
        // Two same-base blocks: first free = MISS + INSERT, second free = HIT.
        let mut first = x_group.remove(0);
        let mut hit = x_group.remove(0);
        let mut other_base = y_group.remove(0);
        drop(x_group);
        drop(y_group);
        assert_ne!(
            base_of(first.addr()),
            base_of(other_base.addr()),
            "the second base must be a distinct segment"
        );

        // Narrow reborrows END before the moves (accepted witness pattern):
        // only the addresses are kept, for gate arming and pending reads.
        let first_addr = {
            let narrow: &mut Block = &mut first;
            narrow.0[0] ^= 0x5A;
            narrow.0.as_ptr().addr()
        };
        let hit_addr = {
            let narrow: &mut Block = &mut hit;
            narrow.0[0] ^= 0xA5;
            narrow.0.as_ptr().addr()
        };
        let other_addr = {
            let narrow: &mut Block = &mut other_base;
            narrow.0[0] ^= 0x33;
            narrow.0.as_ptr().addr()
        };
        assert_eq!(base_of(first_addr), base_of(hit_addr));

        // ------------------------------------------------------------------
        // Delivery observation: arm the gate on the FIRST foreign free. The
        // production routing path calls Gate::pause_after_publication right
        // after a successful terminal RMW, so the worker is deterministically
        // suspended in its own Box Drop/dealloc frame with the pending bit set.
        // ------------------------------------------------------------------
        Gate::arm(first_addr);
        struct ResumeProducer;
        impl Drop for ResumeProducer {
            fn drop(&mut self) {
                Gate::resume_producer();
            }
        }
        let resume = ResumeProducer;

        let worker = std::thread::spawn(move || {
            let _ = bootstrap::ensure();
            while !OWNER_DONE.load(Ordering::Acquire) {
                std::hint::spin_loop();
            }
            // Warm the parking machinery OUTSIDE the Box Drop frames.
            Gate::prepare_producer();
            // Real foreign frees through the installed global allocator: the
            // first same-base free is a miss + insert, and its publication is
            // where the gate pauses this worker.
            drop(first);
            // After resume: the same-base free must take the cached fast path
            // (HIT), then a fresh-base miss + insert.
            drop(hit);
            drop(other_base);
            // Worker exit: its TLS pin-cache dtor releases the retained pins.
        });
        OWNER_DONE.store(true, Ordering::Release);
        Gate::allow_prepared_producer();
        Gate::wait_for_publication();

        // The worker is paused after the terminal RMW, inside its own dealloc
        // frame: the delivery is already observable (sidecar-only read).
        assert!(
            pending_at(first_addr),
            "the armed foreign free must publish its pending bit before resuming"
        );
        drop(resume); // release the worker deterministically
        worker.join().expect("foreign-free worker panicked");

        // All three frees published into their (still linked) routes; worker
        // frees never drain publications (same as the native cell).
        for (addr, label) in [
            (first_addr, "first"),
            (hit_addr, "hit"),
            (other_addr, "other-base"),
        ] {
            assert!(pending_at(addr), "{label} free must leave a pending bit");
        }

        // ------------------------------------------------------------------
        // ACTIVATION ORACLE (R30-8). MUTANT "publish_cached always returns
        // Miss" (cache disabled): the second same-base free re-runs the cold
        // path, HITS stay flat and this assertion fails.
        // ------------------------------------------------------------------
        let (h1, m1, i1, _e1, u1, r1, _s1) = quiescent_totals();
        assert!(
            h1 - h0 >= 1,
            "same-base foreign free must HIT the cached pin, got {}",
            h1 - h0
        );
        assert!(
            m1 - m0 >= 2,
            "first touch per base must miss, got {}",
            m1 - m0
        );
        assert!(
            i1 - i0 >= 2,
            "cold foreign free per base must insert a cached pin, got {}",
            i1 - i0
        );
        // TLS thread-exit release of the retained cached pins: the worker held
        // one pin per touched base; its exit released both.
        assert!(
            r1 - r0 >= 2,
            "worker exit must release the cached pins, got {}",
            r1 - r0
        );
        assert_eq!(u1 - u0, 0, "linked routes throughout: no unlink fallback");
        assert_eq!(
            AllocCore::dbg_foreign_or_unroutable_frees() - foreign_before,
            0,
            "every foreign free must be routed, none foreign-or-unroutable"
        );

        // ------------------------------------------------------------------
        // Owner-side deterministic drain: same-class own alloc/free cycles are
        // the production drain trigger for detached foreign publications.
        // ------------------------------------------------------------------
        // Raw alloc/dealloc, not `Box`: on non-fastbin rows a by-value `Box::drop`
        // frame hits the accepted P1-box defect (card 164, `dealloc_small` writes
        // the link while the frame is live), which is not what this test pins.
        let layout = core::alloc::Layout::new::<Block>();
        for _ in 0..10_000 {
            // SAFETY: non-zero layout; the block is freed once with the same layout.
            unsafe {
                let p = GlobalAlloc::alloc(&GLOBAL, layout);
                assert!(!p.is_null(), "owner drain cycle allocation failed");
                GlobalAlloc::dealloc(&GLOBAL, p, layout);
            }
            GLOBAL.trim_current_thread();
            if pending_at(first_addr) || pending_at(hit_addr) || pending_at(other_addr) {
                continue;
            }
            println!("[b4_r1_pin_cache_miri] COMPLETE narrow");
            return;
        }
        panic!("pending bits were not drained within the owner-cycle bound");
    }
}

fn main() {
    #[cfg(all(
        miri,
        feature = "alloc-global",
        feature = "internals",
        feature = "bench-internals"
    ))]
    {
        cell::narrow_pin_cache_hit_release_and_delivery_miri();
    }
}
