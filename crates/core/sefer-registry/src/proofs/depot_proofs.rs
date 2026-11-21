use super::{Depot, DEPOT_CLASSES, DEPOT_K, TCACHE_CAP};

fn any_magazine() -> [*mut u8; TCACHE_CAP] {
    let mut m = [core::ptr::null_mut(); TCACHE_CAP];
    for slot in &mut m {
        *slot = kani::any::<usize>() as *mut u8;
    }
    m
}

/// И-D1: a park succeeds iff the class holds fewer than `DEPOT_K[c]`
/// magazines, and the parked count never exceeds that bound.
#[kani::proof]
#[kani::unwind(17)]
fn depot_park_respects_capacity() {
    let mut d = Depot::empty();
    let c: usize = kani::any();
    kani::assume(c < DEPOT_CLASSES);
    let mut n: u8 = 0;
    let attempts: usize = kani::any();
    kani::assume(attempts <= 9);
    let mag = any_magazine();
    let mut i = 0;
    while i < attempts {
        let before = n;
        let ok = d.depot_park(c, &mut n, &mag);
        assert_eq!(ok, (before as usize) < DEPOT_K[c] as usize);
        assert_eq!(n, if ok { before + 1 } else { before });
        assert!(n as usize <= DEPOT_K[c] as usize);
        i += 1;
    }
}

/// И-D2: reload returns exactly the magazines parked, last-in first-out,
/// then reports empty; destination and count are untouched on an empty
/// reload. Another class's stored magazine is not disturbed.
#[kani::proof]
#[kani::unwind(17)]
fn depot_reload_returns_parked_magazines_lifo() {
    let mut d = Depot::empty();
    let c: usize = kani::any();
    let other: usize = kani::any();
    kani::assume(c < DEPOT_CLASSES && other < DEPOT_CLASSES && other != c);
    let mut n: u8 = 0;
    let mut n_other: u8 = 0;
    let a = any_magazine();
    let b = any_magazine();
    let o = any_magazine();
    assert!(d.depot_park(other, &mut n_other, &o));
    assert!(d.depot_park(c, &mut n, &a));
    let two = DEPOT_K[c] >= 2;
    if two {
        assert!(d.depot_park(c, &mut n, &b));
    }
    let mut out = [core::ptr::null_mut(); TCACHE_CAP];
    if two {
        assert!(d.depot_reload(c, &mut n, &mut out));
        assert!(out == b);
    }
    assert!(d.depot_reload(c, &mut n, &mut out));
    assert!(out == a);
    let sentinel = any_magazine();
    let mut untouched = sentinel;
    assert!(!d.depot_reload(c, &mut n, &mut untouched));
    assert!(untouched == sentinel);
    assert_eq!(n, 0);
    assert!(d.depot_reload(other, &mut n_other, &mut out));
    assert!(out == o);
}
