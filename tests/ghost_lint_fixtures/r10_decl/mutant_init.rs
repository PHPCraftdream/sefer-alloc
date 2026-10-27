// expect: initializer
thread_local! {
    static SLOT: Cell<usize> = Cell::new(0);
}

struct Holder {
    value: Cell<u32>,
}
