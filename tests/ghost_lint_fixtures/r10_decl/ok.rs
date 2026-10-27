thread_local! {
    static SLOT: Cell<usize> = const { Cell::new(0) };
}

struct Holder {
    value: Cell<u32>,
}
