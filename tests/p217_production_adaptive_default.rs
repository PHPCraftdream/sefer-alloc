//! Production-default regression coverage for the adaptive Large cache.
//!
//! The unconditional feature-linkage assertion keeps a demotion from making
//! the behavior tests silently disappear in a `production` build.

#![cfg(feature = "production")]

#[test]
#[allow(clippy::assertions_on_constants)]
fn production_enables_large_cache_adaptive() {
    assert!(
        cfg!(feature = "large-cache-adaptive"),
        "production must imply large-cache-adaptive"
    );
}

#[cfg(all(
    feature = "large-cache-adaptive",
    feature = "internals",
    feature = "alloc-stats"
))]
mod adaptive_behavior {
    use core::alloc::Layout;
    use sefer_alloc::AllocCore;

    const MIB: usize = 1024 * 1024;
    const SPAN: usize = 4 * MIB;

    fn layout() -> Layout {
        Layout::from_size_align(SPAN - aligned_vmem::page_size(), 16).unwrap()
    }

    fn round(core: &mut AllocCore, count: usize) -> usize {
        let layout = layout();
        let hits_before = core.dbg_large_cache_hits();
        let pointers: Vec<_> = (0..count)
            .map(|_| {
                let pointer = core.alloc(layout);
                assert!(!pointer.is_null(), "large allocation must succeed");
                pointer
            })
            .collect();
        let hits = (core.dbg_large_cache_hits() - hits_before) as usize;
        for pointer in pointers.into_iter().rev() {
            // SAFETY: each pointer came from this core with this layout and is freed once.
            unsafe { core.dealloc(pointer, layout) };
        }
        hits
    }

    #[test]
    fn production_learns_and_retains_beyond_base_eight() {
        let mut core = AllocCore::new().expect("core");
        core.dbg_set_decay_config(1000, 1000, 16 * MIB);
        assert_eq!(round(&mut core, 80), 0);
        assert_eq!(core.dbg_large_cache_used(), 8 * SPAN);
        assert_eq!(round(&mut core, 80), 8);
        let (extra, _, _, mistakes, _) = core.dbg_large_cache_adaptive().expect("adaptive state");
        assert!(
            mistakes > 0,
            "re-requested released spans must teach the cache"
        );
        assert!(extra > 0, "learned target must exceed the base slots");
        assert!(core.dbg_large_cache_used() > 8 * SPAN);
        assert_eq!(
            round(&mut core, 80),
            80,
            "learned capacity should serve the round"
        );
    }

    #[test]
    fn explicit_budget_remains_a_hard_byte_cap() {
        let mut core = AllocCore::new().expect("core");
        let budget = 40 * MIB;
        core.dbg_set_large_cache_budget(Some(budget));
        for _ in 0..4 {
            round(&mut core, 80);
            assert!(
                core.dbg_large_cache_used() <= budget,
                "cache usage must stay within explicit budget"
            );
        }
        assert!(core.dbg_large_cache_adaptive().unwrap().0 > 0);
    }

    #[test]
    fn quiet_decay_returns_to_headroom_floor() {
        let mut core = AllocCore::new().expect("core");
        let floor = 16 * MIB;
        for _ in 0..3 {
            round(&mut core, 80);
        }
        assert!(core.dbg_large_cache_adaptive().unwrap().0 > 0);
        core.dbg_set_decay_config(1000, 1000, floor);
        for _ in 0..64 {
            core.dbg_force_decay_tick();
        }
        assert_eq!(core.dbg_large_cache_adaptive().unwrap().0, 0);
        assert!(
            core.dbg_large_cache_used() <= floor,
            "quiet decay should trim to the configured headroom floor"
        );
    }
}
