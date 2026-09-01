//! Compile-fail successor of the former `array_index_stack_head_still_double_issue`
//! runtime test (Group A of
//! docs/adr/2026-09-01-tagged-index-stack-storage-binding-closure.md): building a
//! competing binding around a standalone `ArrayIndexStack`'s head MUST NOT compile.
//! The type deliberately does not implement the public `StackStorage` trait — the
//! ADR's Group A closure — so `head()` is neither callable on it (no trait method)
//! nor reachable via any generic over `StackStorage` (no impl to satisfy). What
//! used to be a compiling, double-issuing runtime demonstration is now
//! UNEXPRESSIBLE in safe code; the compile errors below ARE the structural fix.
//! Pinned failing by `tests/compile_fail_array_index_stack_head.rs`.
use tagged_index_stack::{ArrayIndexStack, StackHead, StackStorage};

fn steal_head<S: StackStorage<16>>(s: &S) -> &StackHead<16> {
    s.head()
}

fn main() {
    let owned = ArrayIndexStack::<16, 64>::new();
    owned.push(1); // the inherent push/pop API still works
    let _head = steal_head(&owned); // ERROR: E0277 — StackStorage not implemented
    let _direct = owned.head(); // ERROR: E0599 — no method named `head`

    // Third route a competing binding would need: coercion to the trait
    // object (same bound, same E0277). With no public impl, no route to a
    // &StackHead<16> — generic, inherent, or dyn — exists from this type,
    // so no StackOps-callable competing binding can be built around its
    // head.
    let _dyn: &dyn StackStorage<16> = &owned; // ERROR: E0277 — same unsatisfied bound
}
