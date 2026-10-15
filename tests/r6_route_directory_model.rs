#![cfg(all(feature = "alloc-global", feature = "internals"))]

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, RoutePin};

const SEGMENT: usize = 4 * 1024 * 1024;

#[test]
#[cfg(not(miri))]
fn terminal_publication_signatures_consume_the_pin() {
    let _: unsafe fn(RoutePin, u32) -> bool = RoutePin::publish_small;
    let _: unsafe fn(RoutePin) -> bool = RoutePin::publish_large;
    // A safe function coerces to an unsafe fn pointer, so those assignments
    // alone cannot detect a regression of the ownership boundary.
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/compile_fail/route_publication_requires_unsafe/Cargo.toml");
    let target =
        std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("route_publication_requires_unsafe");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = std::process::Command::new(cargo)
        .args(["check", "--locked", "--offline", "--manifest-path"])
        .arg(manifest)
        .env("CARGO_TARGET_DIR", target)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("compile route ownership fixture");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "safe publication compiled");
    assert_eq!(stderr.matches("error[E0133]").count(), 2, "{stderr}");
}

#[test]
fn cached_instance_cannot_be_claimed_before_owner_reset() {
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let route = directory
        .register(root, 1, root, 1, RouteKind::Large)
        .unwrap();
    // SAFETY: the test owns this live standalone instance and transfers it once.
    assert!(unsafe { directory.lookup(root).unwrap().publish_large() });
    let generation = route.claim_large_pending().unwrap();
    assert!(route.cache_large_consumed(generation));
    let next = route.begin_large_reuse().unwrap();
    assert!(route.claim_large_pending().is_none());
    // This model checks the phase gate; production owner reset is out of scope.
    assert!(route.finish_large_reuse_after_reset(next));
    // SAFETY: reset completed; the test owns the new instance and transfers once.
    assert!(unsafe { directory.lookup(root).unwrap().publish_large() });
    let second = route.claim_large_pending().unwrap();
    assert!(route.cache_large_consumed(second));
    assert!(route.release_cached_large(second));
}

#[test]
fn all_owner_and_two_pin_release_orders_reclaim_slot() {
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let directory = RouteDirectory::new();
        let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
        let root = reservation.as_ptr();
        let mut owner = Some(
            directory
                .register(root, SEGMENT, root, 8, RouteKind::Small)
                .unwrap(),
        );
        let mut pins = [directory.lookup(root), directory.lookup(root)];
        for action in order {
            match action {
                0 => drop(owner.take()),
                1 | 2 => drop(pins[action - 1].take()),
                _ => unreachable!(),
            }
            assert_eq!(directory.lookup(root).is_some(), owner.is_some());
            for pin in pins.iter().flatten() {
                assert_eq!(pin.owner(), 8);
            }
        }
        assert!(directory.lookup(root).is_none());
        let reused = directory
            .register(root, SEGMENT, root, 9, RouteKind::Small)
            .unwrap();
        assert_eq!(directory.lookup(root).unwrap().owner(), 9);
        drop(reused);
    }
}
