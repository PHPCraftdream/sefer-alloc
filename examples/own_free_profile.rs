//! Production-only GlobalAlloc facade: runtime/const own-thread churn.
use core::alloc::{GlobalAlloc, Layout};
use std::hint::black_box;

#[inline(never)]
fn w_alloc_rt<A: GlobalAlloc>(a: &A, layout: Layout) -> *mut u8 {
    // SAFETY: ненулевой корректный layout; результат проверяется до использования.
    unsafe { a.alloc(layout) }
}

#[inline(never)]
fn w_free_rt<A: GlobalAlloc>(a: &A, ptr: *mut u8, layout: Layout) {
    // SAFETY: run_rt передаёт свой уникальный живой блок с исходным layout.
    unsafe { a.dealloc(ptr, layout) };
    black_box(layout.size());
}

#[inline(never)]
fn w_alloc16<A: GlobalAlloc>(a: &A) -> *mut u8 {
    // SAFETY: корректный layout; результат проверяется до использования.
    unsafe { a.alloc(Layout::from_size_align(16, 8).unwrap()) }
}

#[inline(never)]
fn w_free16<A: GlobalAlloc>(a: &A, ptr: *mut u8) {
    // SAFETY: run_const передаёт уникальный живой блок w_alloc16.
    unsafe { a.dealloc(ptr, Layout::from_size_align(16, 8).unwrap()) };
    black_box(16usize);
}

#[inline(never)]
fn w_alloc64<A: GlobalAlloc>(a: &A) -> *mut u8 {
    // SAFETY: корректный layout; результат проверяется до использования.
    unsafe { a.alloc(Layout::from_size_align(64, 8).unwrap()) }
}

#[inline(never)]
fn w_free64<A: GlobalAlloc>(a: &A, ptr: *mut u8) {
    // SAFETY: run_const передаёт уникальный живой блок w_alloc64.
    unsafe { a.dealloc(ptr, Layout::from_size_align(64, 8).unwrap()) };
    black_box(64usize);
}

fn rnd(x: &mut u64) -> usize {
    *x ^= *x << 13;
    *x ^= *x >> 7;
    *x ^= *x << 17;
    *x as usize
}

fn run_rt<A: GlobalAlloc>(a: &A, steps: usize) {
    let mut x = 0x9e37_79b9_7f4a_7c15;
    let mut slots = Vec::with_capacity(768);
    for _ in 0..768 {
        let layout = Layout::from_size_align(16 + rnd(&mut x) % 497, 8).unwrap();
        let ptr = w_alloc_rt(a, layout);
        assert!(!ptr.is_null());
        slots.push((ptr, layout));
    }
    for _ in 0..steps {
        let r = rnd(&mut x);
        let i = r % slots.len();
        let (ptr, layout) = slots[i];
        w_free_rt(a, ptr, layout);
        let layout = black_box(Layout::from_size_align(16 + (r >> 20) % 497, 8).unwrap());
        let ptr = w_alloc_rt(a, layout);
        assert!(!ptr.is_null());
        slots[i] = (ptr, layout);
    }
    for (ptr, layout) in slots {
        w_free_rt(a, ptr, layout);
    }
}

fn run_const<A: GlobalAlloc>(a: &A, steps: usize, alloc: fn(&A) -> *mut u8, free: fn(&A, *mut u8)) {
    let mut x = 0x9e37_79b9_7f4a_7c15;
    let mut slots = [core::ptr::null_mut(); 16];
    for slot in &mut slots {
        *slot = alloc(a);
        assert!(!slot.is_null());
    }
    for _ in 0..steps {
        let i = rnd(&mut x) % slots.len();
        free(a, slots[i]);
        slots[i] = alloc(a);
        assert!(!slots[i].is_null());
    }
    for ptr in slots {
        free(a, ptr);
    }
}

fn run<A: GlobalAlloc>(a: &A, mode: &str, steps: usize) {
    match mode {
        "rt" => run_rt(a, steps),
        "16" => run_const(a, steps, w_alloc16::<A>, w_free16::<A>),
        "64" => run_const(a, steps, w_alloc64::<A>, w_free64::<A>),
        _ => panic!("mode must be rt/16/64"),
    }
    println!("{{\"mode\":\"{mode}\",\"steps\":{steps},\"completed\":true}}");
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let steps: usize = args[3].parse().expect("integer steps");
    assert!((1..=1_000_000).contains(&steps));
    match args[1].as_str() {
        "sefer" => run(&sefer_alloc::SeferAlloc::new(), &args[2], steps),
        "mimalloc" => run(&mimalloc::MiMalloc, &args[2], steps),
        _ => panic!("arm must be sefer/mimalloc"),
    }
}
