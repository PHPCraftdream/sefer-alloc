//! Диагностический scalar-carve gate; не измеряет установленный GlobalAlloc.
use sefer_alloc::registry::segment_route::RouteRegistration;
use sefer_alloc::AllocCore;
use std::alloc::Layout;
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .expect("N")
        .parse()
        .expect("integer N");
    assert!((1..=65_536).contains(&n));
    let mut core = AllocCore::dbg_new_routed_for_test().expect("routed core");
    let mut blocks = Vec::with_capacity(n);
    let start = Instant::now();
    for _ in 0..n {
        let ptr = core.dbg_r11_scalar_carve_for_test(1).expect("scalar carve");
        blocks.push(black_box(ptr));
    }
    let ns = start.elapsed().as_nanos();
    // Oracle вне timed region: каждый вызов выше непосредственно scalar carve.
    for &ptr in &blocks {
        assert_eq!(
            RouteRegistration::class_at_global_address_for_test(ptr.addr()),
            Some(1)
        );
        assert_eq!(
            RouteRegistration::class_at_global_address_for_test(ptr.addr() + 16),
            None
        );
    }
    let layout = Layout::from_size_align(32, 16).unwrap();
    for ptr in blocks {
        // SAFETY: уникальный живой блок core; исходный layout, одно освобождение.
        unsafe { core.dealloc(ptr, layout) };
    }
    drop(core);
    println!("{{\"scenario\":\"scalar_carve_32b\",\"n\":{n},\"internal_ns\":{ns},\"scalar_calls\":{n},\"oracle_ok\":true}}");
}
