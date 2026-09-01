//! Group B compile-fail fixture (ADR
//! docs/adr/2026-09-01-tagged-index-stack-storage-binding-closure.md): a
//! storage whose three hook bodies are CORRECT (the impl below upholds every
//! `# Safety` clause — this is NOT a contract-violation repro) but whose
//! declaration omits the `unsafe` keyword MUST NOT COMPILE. This pins Group
//! B's actual mechanism, the compiler-forced per-impl-site acknowledgment:
//! `StackStorage` is an `unsafe trait`, so no implementor can exist anywhere
//! without asserting the contract at the `unsafe impl` site (E0200, "the
//! trait `StackStorage<16>` requires an `unsafe impl` declaration").
//! Counterfactual: under the pre-conversion safe trait this exact file
//! COMPILED with no acknowledgment possible or required. The compile-PASS
//! counterpart — a correct `unsafe impl` compiles and behaves correctly —
//! is pinned by `vec_backed_storage_push_pop_round_trips` +
//! `push_pop_through_dyn_storage` in `tests/custom_storage_impl.rs`. Pinned
//! failing by `tests/compile_fail_unsafe_impl_required.rs`.
use core::sync::atomic::{AtomicU32, Ordering};

use tagged_index_stack::{StackHead, StackOps, StackStorage};

struct PlainStorage {
    head: StackHead<16>,
    next: [AtomicU32; 8],
}

// The impl body is CORRECT (the same shape as the reference-model
// `VecStorage` in tests/custom_storage_impl.rs): the ONLY defect is the
// missing `unsafe` on the declaration. ERROR: E0200 — the trait requires an
// `unsafe impl` declaration.
impl StackStorage<16> for PlainStorage {
    fn head(&self) -> &StackHead<16> {
        &self.head
    }

    fn load_next(&self, index: u32) -> u32 {
        self.next[index as usize].load(Ordering::Acquire)
    }

    fn store_next(&self, index: u32, next: u32) {
        self.next[index as usize].store(next, Ordering::Release);
    }
}

fn main() {
    // Driven only through StackOps, exactly like the crate's own
    // reference-model storage, so the fixture's sole defect stays the
    // missing `unsafe` keyword.
    let storage = PlainStorage {
        head: StackHead::new(),
        next: [const { AtomicU32::new(0) }; 8],
    };
    storage.push_index(1);
    assert_eq!(storage.pop_index(), Some(1));
}
