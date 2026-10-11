//! Hardened corruption is fatal, not a recoverable null/empty allocation.
//! Subprocess markers prove that setup completed and the trigger did not return;
//! they do not prove that no allocator metadata was mutated before termination.
//! Without the abort change, corruption cases return and print SURVIVED.

#![cfg(all(feature = "hardened", feature = "internals"))]

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};
use std::io::Write;

fn oracle(case: &str) {
    for healthy in [true, false] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "corruption_child", "--nocapture"])
            .env("R19_CASE", case)
            .env("R19_HEALTHY", if healthy { "1" } else { "0" })
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stdout.contains("ARMED"), "{case}: {stdout} {stderr}");
        assert!(!stderr.contains("panicked"), "{case}: {stderr}");
        assert_eq!(
            output.status.success(),
            healthy,
            "{case}: {stdout} {stderr}"
        );
        assert_eq!(stdout.contains("SURVIVED"), healthy, "{case}: {stdout}");
        let diagnostic = "sefer-alloc: hardened: Small free-list corruption detected; aborting\n";
        if healthy {
            assert!(!stderr.contains(diagnostic), "{case}: {stderr}");
        } else {
            assert_ne!(output.status.code(), Some(101), "{case}: {stderr}");
            assert!(
                stderr.contains(diagnostic),
                "{case}: missing corruption diagnostic: {stderr}"
            );
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                assert_eq!(output.status.signal(), Some(6), "{case}: {stderr}");
            }
            #[cfg(windows)]
            eprintln!("{case}: abort exit code {:?}", output.status.code());
        }
    }
}

#[test]
fn corruption_child() {
    let Ok(case) = std::env::var("R19_CASE") else {
        return;
    };
    let healthy = std::env::var("R19_HEALTHY").unwrap() == "1";
    let batch = case.starts_with("drain");
    let multi_node = matches!(case.as_str(), "drain_cycle" | "drain_head_loop");
    let mut ac = AllocCore::new().expect("primordial reservation");
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut blocks = [core::ptr::null_mut(); 4];
    assert_eq!(ac.dbg_carve_batch(0, &mut blocks), 4);
    let [anchor, a, b, c] = blocks;
    let base = SegmentLayout::segment_base_of(anchor.addr());
    assert!(blocks
        .iter()
        .all(|p| SegmentLayout::segment_base_of(p.addr()) == base));
    // SAFETY: allocator-issued raw pointers retain segment permissions; a, b,
    // c are live, owned, matching-layout allocations freed exactly once here.
    unsafe {
        ac.dealloc(a, layout);
        ac.dealloc(b, layout);
        ac.dealloc(c, layout);
    }
    assert_eq!(
        ac.dbg_freelist_head_for(anchor, 0),
        (c.addr() - base) as u32
    );
    assert!([a, b, c].iter().all(|&p| ac.dbg_is_free_for(p)));
    assert!(!ac.dbg_is_free_for(anchor));
    let credits = ac.dbg_live_count_for(anchor).unwrap();
    if !healthy {
        if multi_node {
            // `drain_cycle`: c -> b -> a -> b exercises Floyd, not only the
            // return-to-head check. `drain_head_loop`: c -> b -> a -> c
            // exercises the return-to-head check within a short batch.
            let back = if case == "drain_head_loop" { c } else { b };
            // SAFETY: this standalone exclusively owned mapped segment retains
            // its allocator provenance. This deliberate metadata corruption
            // writes only the initialized next word of its free block a.
            unsafe { a.cast::<*mut u8>().write(back) };
        } else {
            let next = if case.ends_with("out_of_segment") {
                core::ptr::without_provenance_mut(c.addr() ^ SegmentLayout::SEGMENT)
            } else if case.ends_with("allocated") {
                anchor
            } else {
                c
            };
            // SAFETY: anchor is live with original segment-wide permissions;
            // the exclusively owned mapped head c is free, class 0. The hook
            // only stores next; the hardened trigger validates before trusting it.
            assert!(unsafe { ac.dbg_corrupt_freelist_head_next(anchor, 0, next) });
        }
    }
    println!("ARMED");
    std::io::stdout().flush().unwrap();
    let mut issued = vec![anchor];
    if batch {
        let mut out = [core::ptr::null_mut(); 8];
        let want = if multi_node { 3 } else { 1 };
        // SAFETY: anchor retains segment-wide permissions in this exclusively
        // owned mapped core. Invalid chains deliberately test hardened rejection.
        let got = unsafe { ac.dbg_drain_freelist_batch(anchor, 0, &mut out[..want]) };
        if healthy {
            assert_eq!(got, want.min(3));
            assert_eq!(&out[..got], &[c, b, a][..got]);
            assert_eq!(ac.dbg_live_count_for(anchor), Some(credits + got as u32));
            issued.extend_from_slice(&out[..got]);
        }
    } else {
        let p = ac.alloc(layout);
        if healthy {
            assert_eq!(p, c);
            assert_eq!(ac.dbg_live_count_for(anchor), Some(credits + 1));
            issued.push(p);
        }
    }
    // Print immediately after a returned corrupt trigger, before any assertion
    // could accidentally substitute a panic for the requested abort oracle.
    println!("SURVIVED");
    std::io::stdout().flush().unwrap();
    if healthy {
        for _ in 0..64 {
            let p = ac.alloc(layout);
            assert!(!p.is_null());
            assert!(!ac.dbg_is_free_for(p));
            issued.push(p);
        }
        let unique: std::collections::HashSet<_> = issued.iter().map(|p| p.addr()).collect();
        assert_eq!(unique.len(), issued.len());
        for p in issued {
            // SAFETY: distinct live allocations issued by this core, exact layout.
            unsafe { ac.dealloc(p, layout) };
        }
    }
}

#[test]
fn pop_out_of_segment_aborts() {
    oracle("pop_out_of_segment");
}
#[test]
fn drain_out_of_segment_aborts() {
    oracle("drain_out_of_segment");
}
#[test]
fn pop_allocated_continuation_aborts() {
    oracle("pop_allocated");
}
#[test]
fn drain_allocated_continuation_aborts() {
    oracle("drain_allocated");
}
#[test]
fn pop_self_cycle_aborts() {
    oracle("pop_self_cycle");
}
#[test]
fn drain_self_cycle_aborts() {
    oracle("drain_self_cycle");
}
#[test]
fn drain_multi_node_cycle_aborts() {
    oracle("drain_cycle");
}
#[test]
fn drain_cycle_back_to_head_aborts() {
    oracle("drain_head_loop");
}
