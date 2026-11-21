use super::super::depot::DEPOT_K;
use super::{DepotOverflow, ALL_SMALL, DEPOT_CLASSES, TCACHE_CAP};
use crate::alloc_core::size_classes::SIZE_CLASS_TABLE;
use crate::alloc_core::LargeCacheConfig;

const CAP: usize = 3;
const STEPS: usize = 5;

/// И-O1: the ring against a queue model — park appends iff the ring and
/// the budget have room, reload returns the newest record, decay the
/// oldest (the ring wraps once decay moved its head), and the parked
/// byte count is always `len * magazine bytes`. The segment-refill tick
/// flags, driven as `park_overflow` / `depot_segment_tick` drive them
/// (a class's first record arms; a refill of class `k` disarms `k` only
/// on an empty overflow), mirror `armed` exactly and cover every small
/// class while bytes are parked.
#[kani::proof]
#[kani::unwind(18)]
#[allow(unsafe_code)]
fn overflow_ring_matches_a_queue() {
    let mut buf = [[core::ptr::null_mut::<u8>(); TCACHE_CAP]; CAP];
    let c: usize = kani::any();
    kani::assume(c < DEPOT_CLASSES);
    let budget: usize = kani::any();
    kani::assume(budget <= 1 << 30);
    let mb = SIZE_CLASS_TABLE[c] * TCACHE_CAP;
    // SAFETY: `buf` provides CAP initialized, pointer-aligned stack records
    // exclusively to this ring while it is live; `c` is a valid depot class.
    let mut o = unsafe { DepotOverflow::kani_on_buffer(buf.as_mut_ptr().cast(), c, CAP as u16) };
    let mut model = [0usize; STEPS];
    let (mut front, mut back) = (0usize, 0usize);
    let mut out = [core::ptr::null_mut::<u8>(); TCACHE_CAP];
    let mut flags = 0u64;
    let mut step = 0;
    while step < STEPS {
        let n = back - front;
        let op: u8 = kani::any();
        if op % 4 == 3 {
            // A segment refill of class `k` while armed.
            let k: u32 = kani::any();
            kani::assume(k < ALL_SMALL.count_ones());
            if flags & 1 << k != 0 && o.bytes() == 0 {
                o.disarm(k as usize);
                flags &= !(1 << k);
            }
        } else if op % 4 == 0 {
            let id = step + 1;
            let mag = [id as *mut u8; TCACHE_CAP];
            let room = n < CAP && o.bytes() + mb <= budget;
            let parked = o.park(c, n, &mag, budget);
            assert_eq!(parked, room);
            if room {
                model[back] = id;
                back += 1;
                // As `park_overflow`: only a class's first record arms.
                if n == 0 {
                    flags |= o.arm();
                }
            }
        } else if op % 4 == 1 && n > 0 {
            o.reload(c, n, &mut out);
            back -= 1;
            assert!(out == [model[back] as *mut u8; TCACHE_CAP]);
        } else if n > 0 {
            o.pop_oldest(c, n, &mut out);
            assert!(out == [model[front] as *mut u8; TCACHE_CAP]);
            front += 1;
        }
        assert_eq!(o.bytes(), (back - front) * mb);
        assert_eq!(flags, o.armed());
        assert!(o.bytes() == 0 || o.armed() == ALL_SMALL);
        step += 1;
    }
    kani::cover!(front > 0 && back - front == CAP, "the ring wrapped full");
}

/// И-O2: for every request the resolved budget is a multiple of 256 B
/// and floor + overflow ring of any depot class fits `PerClass::limit`.
#[kani::proof]
fn resolved_budget_fits_the_park_limit() {
    let req: usize = kani::any();
    let c: usize = kani::any();
    kani::assume(c < DEPOT_CLASSES);
    let b = LargeCacheConfig::new()
        .depot_budget_bytes(req)
        .resolved_depot_budget_bytes();
    assert_eq!(b % 256, 0);
    assert!(b <= req);
    assert!(DEPOT_K[c] as usize + DepotOverflow::cap_for(b, c) <= u16::MAX as usize);
}

const FCAP: usize = 4;
type FastBuf = [[*mut u8; TCACHE_CAP]; FCAP];

/// A ring buffer whose record `i` holds the marker `i + 1` in every word.
fn marked_buffer() -> FastBuf {
    [
        [1 as *mut u8; TCACHE_CAP],
        [2 as *mut u8; TCACHE_CAP],
        [3 as *mut u8; TCACHE_CAP],
        [4 as *mut u8; TCACHE_CAP],
    ]
}

/// Whether two magazines match at their first and last word. Every word of
/// a record is written by one struct copy and the proofs fill records
/// uniformly, so the ends tell a skipped, doubled or misplaced copy; array
/// `==` would lower to a `memcmp` loop needing an unwind bound of its length.
fn same_mag(a: &[*mut u8; TCACHE_CAP], b: &[*mut u8; TCACHE_CAP]) -> bool {
    a[0] == b[0] && a[TCACHE_CAP - 1] == b[TCACHE_CAP - 1]
}

/// Whether two ring buffers hold the same records (see [`same_mag`]).
fn same_records(a: &FastBuf, b: &FastBuf) -> bool {
    let mut i = 0;
    while i < FCAP {
        if !same_mag(&a[i], &b[i]) {
            return false;
        }
        i += 1;
    }
    true
}

/// И-F1: the WB-3 park fast lane against the generic chain, over an
/// arbitrary steady-state overflow (`DepotOverflow::kani_symbolic`: ring
/// position and length, commit watermark, bytes, decay epoch and debt,
/// latch and ingress counters all symbolic). When `park_simple` accepts,
/// the class already had a record (no arming: every class is armed), the
/// ring keeps room after the new record (no latch), and the result is
/// exactly `park` followed by a `tick` that finds no decay work — same
/// controls, same record bytes. When it refuses, nothing changes.
#[kani::proof]
#[kani::unwind(20)]
#[allow(unsafe_code)]
fn park_simple_is_park_then_tick() {
    let c: usize = kani::any();
    kani::assume(c < DEPOT_CLASSES);
    let budget: usize = kani::any();
    kani::assume(budget <= 1 << 30);
    let mut buf_a = marked_buffer();
    let mut buf_b = marked_buffer();
    let base_a = buf_a.as_mut_ptr().cast();
    // SAFETY: `buf_a` provides FCAP live, initialized, pointer-aligned
    // stack records exclusive to `a` during accesses; `c` is valid.
    let mut a = unsafe { DepotOverflow::kani_symbolic(base_a, c, FCAP as u16) };
    // SAFETY: `buf_b` is a separate live, initialized, aligned FCAP-record
    // array, exclusive to the comparison ring during record accesses.
    let mut b = unsafe { a.kani_copy(buf_b.as_mut_ptr().cast()) };
    // SAFETY: the saved copy is only used to compare control fields, never
    // to access records, so `a` retains exclusive record access to `buf_a`.
    let before = unsafe { a.kani_copy(base_a) };
    let n = a.len(c);
    let mag = [kani::any::<usize>() as *mut u8; TCACHE_CAP];
    if a.park_simple(c, &mag, budget) {
        assert!(n >= 1 && n + 1 < FCAP);
        assert_eq!(a.armed(), ALL_SMALL);
        let parked = b.park(c, n, &mag, budget);
        assert!(parked);
        assert!(!b.tick());
        assert!(a.kani_same_controls(&b));
        assert!(same_records(&buf_a, &buf_b));
        kani::cover!(true, "the park fast lane accepted");
    } else {
        assert!(a.kani_same_controls(&before));
        assert!(same_records(&buf_a, &marked_buffer()));
    }
}

/// И-F2: the WB-3 reload fast lane against the generic chain, over the
/// same arbitrary steady-state overflow. When `reload_simple` accepts it
/// is exactly `reload`, then an ingress count that is not due, an
/// unlatching that finds no latched class, and a `tick` that finds no
/// decay work; `more` is "records remain". When it refuses, neither the
/// controls nor the destination magazine change.
#[kani::proof]
#[kani::unwind(20)]
#[allow(unsafe_code)]
fn reload_simple_is_reload_then_tick() {
    let c: usize = kani::any();
    kani::assume(c < DEPOT_CLASSES);
    let budget: usize = kani::any();
    kani::assume(budget <= 1 << 30);
    let mut buf_a = marked_buffer();
    let mut buf_b = marked_buffer();
    let base_a = buf_a.as_mut_ptr().cast();
    // SAFETY: `buf_a` provides FCAP live, initialized, pointer-aligned
    // stack records exclusive to `a` during accesses; `c` is valid.
    let mut a = unsafe { DepotOverflow::kani_symbolic(base_a, c, FCAP as u16) };
    // SAFETY: `buf_b` is a separate live, initialized, aligned FCAP-record
    // array, exclusive to the comparison ring during record accesses.
    let mut b = unsafe { a.kani_copy(buf_b.as_mut_ptr().cast()) };
    // SAFETY: the saved copy is only used to compare control fields, never
    // to access records, so `a` retains exclusive record access to `buf_a`.
    let before = unsafe { a.kani_copy(base_a) };
    let n = a.len(c);
    let mut out_a = [core::ptr::null_mut::<u8>(); TCACHE_CAP];
    let mut out_b = [core::ptr::null_mut::<u8>(); TCACHE_CAP];
    match a.reload_simple(c, &mut out_a) {
        Some(more) => {
            assert!(n >= 1);
            b.reload(c, n, &mut out_b);
            assert!(!b.ingress_due());
            assert_eq!(b.take_unstarved(budget), 0);
            assert!(!b.tick());
            assert_eq!(more, n > 1);
            assert!(a.kani_same_controls(&b));
            assert!(same_mag(&out_a, &out_b));
            assert!(same_records(&buf_a, &marked_buffer()));
            kani::cover!(true, "the reload fast lane accepted");
        }
        None => {
            assert!(a.kani_same_controls(&before));
            assert!(same_mag(&out_a, &[core::ptr::null_mut::<u8>(); TCACHE_CAP]));
            assert!(same_records(&buf_a, &marked_buffer()));
        }
    }
}
