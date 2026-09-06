//! `ArrayIndexStack` must reject a capacity larger than the index mask when a
//! constructor is instantiated. The primary diagnostic must be the crate's
//! named `_CHECK_N` assertion, not an unrelated type or dependency error.
use tagged_index_stack::ArrayIndexStack;

fn main() {
    let _ = ArrayIndexStack::<4, 16>::new();
    let _: ArrayIndexStack<4, 16> = Default::default();
}
