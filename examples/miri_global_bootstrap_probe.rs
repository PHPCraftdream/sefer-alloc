use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

fn main() {
    let value = Box::new(std::hint::black_box(7_u8));
    assert_eq!(std::hint::black_box(*value), 7);
    drop(value);
}
