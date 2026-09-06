//! `ArrayIndexStack::new()` must reject a capacity larger than the index mask.
//! The fixture deliberately contains no other invalid instantiation.
use tagged_index_stack::ArrayIndexStack;

fn main() {
    let _ = ArrayIndexStack::<4, 16>::new();
}
