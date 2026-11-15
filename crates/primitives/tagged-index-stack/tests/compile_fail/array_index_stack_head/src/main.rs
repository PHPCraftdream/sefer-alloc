//! Building a competing binding around a standalone `ArrayIndexStack`'s head
//! MUST NOT compile. The type deliberately does not implement the public
//! `StackStorage` trait, so `head()` is neither callable on it nor reachable
//! through a generic `StackStorage` bound. No safe route can construct a
//! competing binding around its head; the compile errors below are the
//! structural oracle.
//! Pinned failing by root `tests/tagged_index_stack_compile_fail.rs`.
use tagged_index_stack::{ArrayIndexStack, StackHead, StackStorage};

fn steal_head<S: StackStorage<16>>(s: &S) -> &StackHead<16> {
    // SAFETY: this generic route uses the returned reference only as this
    // binding's head and never creates a competing head↔links binding.
    unsafe { s.head() }
}

fn main() {
    let owned = ArrayIndexStack::<16, 64>::new();
    // SAFETY: fresh stack (domain 0..64); index 1 is in-domain and this is its first push.
    unsafe { owned.push(1) }.expect("fresh head has tag budget"); // the inherent push/pop API still works (push now requires unsafe)
    let _head = steal_head(&owned); // ERROR: E0277 — StackStorage not implemented
    let _direct = owned.head(); // ERROR: E0599 — no method named `head`

    // Third route a competing binding would need: coercion to the trait
    // object (same bound, same E0277). With no public impl, no route to a
    // &StackHead<16> — generic, inherent, or dyn — exists from this type,
    // so no StackOps-callable competing binding can be built around its
    // head.
    let _dyn: &dyn StackStorage<16> = &owned; // ERROR: E0277 — same unsatisfied bound
}
