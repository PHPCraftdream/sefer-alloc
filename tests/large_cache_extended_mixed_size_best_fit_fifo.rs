//! Best-fit crosses the base/extension boundary; FIFO uses deposit sequence,
//! not slot index. All capacity fillers are simultaneously live before deposit.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "large-cache-extended",
    feature = "internals"
))]

#[path = "support/r6_bounded_large_cache.rs"]
mod bounded;

use sefer_alloc::{AllocCore, SegmentLayout};

fn physical_size(bytes: usize) -> usize {
    let mut scratch = AllocCore::new().expect("scratch core");
    scratch.dbg_set_large_cache_budget(None);
    let live = bounded::allocate_live(&mut scratch, &[bytes]);
    bounded::deposit_all(&mut scratch, live);
    scratch.dbg_large_cache_slot_sizes()[0].expect("scratch deposit")
}

fn check_best_fit(tight_in_extension: bool) {
    let tight = bounded::large_request();
    let tight_physical = physical_size(tight);
    let loose = tight_physical + SegmentLayout::PAGE;
    let loose_physical = physical_size(loose);
    assert!(tight_physical < loose_physical);
    assert!(loose_physical <= 2 * tight_physical);
    let filler = 2 * tight_physical + SegmentLayout::PAGE;
    assert!(physical_size(filler) > 2 * tight_physical);

    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);
    let mut sizes = vec![filler; 7];
    sizes.extend([tight, loose]);
    let mut live = bounded::allocate_live(&mut ac, &sizes);
    let loose_entry = live.pop().expect("loose entry");
    let tight_entry = live.pop().expect("tight entry");
    let tight_ptr = tight_entry.0;
    bounded::deposit_all(&mut ac, live);
    if tight_in_extension {
        bounded::deposit_all(&mut ac, vec![loose_entry]);
        bounded::deposit_all(&mut ac, vec![tight_entry]);
    } else {
        bounded::deposit_all(&mut ac, vec![tight_entry]);
        bounded::deposit_all(&mut ac, vec![loose_entry]);
    }
    assert_eq!(bounded::occupied(&ac), (8, 1));

    let hit = bounded::allocate_live(&mut ac, &[tight]);
    assert_eq!(
        hit[0].0, tight_ptr,
        "best-fit must select the tighter physical span"
    );
    assert_eq!(
        bounded::occupied(&ac),
        if tight_in_extension { (8, 0) } else { (7, 1) }
    );
    bounded::deposit_all(&mut ac, hit);
}

#[test]
fn best_fit_picks_tightest_slot_across_base_extension_boundary() {
    check_best_fit(false);
    check_best_fit(true);
}

fn check_fifo(oldest_in_extension: bool) {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);
    let bytes = bounded::large_request();
    let mut live = bounded::allocate_live(&mut ac, &[bytes; 41]);
    let newcomer = live.pop().expect("41st live span");
    let original: Vec<_> = live.iter().map(|(p, _)| *p).collect();
    bounded::deposit_all(&mut ac, live);
    assert_eq!(bounded::occupied(&ac), (8, 32));

    if oldest_in_extension {
        let base = bounded::allocate_live(&mut ac, &[bytes; 8]);
        for (i, (p, _)) in base.iter().enumerate() {
            assert_eq!(*p, original[i]);
        }
        bounded::deposit_all(&mut ac, base);
    }
    let victim = if oldest_in_extension {
        original[8]
    } else {
        original[0]
    };
    bounded::deposit_all(&mut ac, vec![newcomer]);
    assert_eq!(bounded::occupied(&ac), (8, 32));
    bounded::assert_used(&ac);

    let cached = bounded::allocate_live(&mut ac, &[bytes; 40]);
    assert_eq!(bounded::occupied(&ac), (0, 0));
    let pointers: Vec<_> = cached.iter().map(|(p, _)| *p).collect();
    assert!(!pointers.contains(&victim), "FIFO-oldest must be evicted");
    assert!(pointers.contains(&newcomer.0));
    for p in original.into_iter().filter(|p| *p != victim) {
        assert!(pointers.contains(&p), "non-oldest reservation must survive");
    }
    bounded::deposit_all(&mut ac, cached);
}

#[test]
fn fifo_eviction_targets_true_oldest_across_combined_index_space() {
    check_fifo(false);
    check_fifo(true);
}
