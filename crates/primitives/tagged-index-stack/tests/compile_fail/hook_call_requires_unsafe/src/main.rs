//! Compile-fail fixture — the `unsafe fn` caller-side boundary on the three
//! `StackStorage` hooks. The hooks (`head`/`load_next`/`store_next`) are
//! `unsafe fn` with per-method caller-side `# Safety` contracts, so even a
//! CORRECT implementor's own crate cannot call them from safe code: every
//! call outside an `unsafe` block is **E0133** ("call to unsafe function is
//! unsafe").
//!
//! The `unsafe fn` boundary gives a compiler-enforced acknowledgement of the
//! caller-side contract, not a compiler-checked one: the compiler forces the
//! `unsafe {}` wrapper, while the contract's substance is verified by the
//! implementor and caller.
//!
//! The compile-PASS counterpart: the hooks are a barrier to MISUSE, not to
//! legitimate use — a correct `unsafe impl` driven through the `StackOps`
//! operations (with unsafe `StackOps::push_index` calls properly justified)
//! compiles and behaves correctly, pinned by
//! `vec_backed_storage_push_pop_round_trips` +
//! `push_pop_through_dyn_storage` in `tests/custom_storage_impl.rs`. Pinned
//! failing by root `tests/tagged_index_stack_compile_fail.rs`.
use core::sync::atomic::{AtomicU32, Ordering};

use tagged_index_stack::{StackHead, StackOps, StackStorage, TAIL};

struct Pool {
    head: StackHead<16>,
    next: [AtomicU32; 8],
}

// SAFETY: these hook bodies are structurally valid for the privately owned
// head and dedicated link cells. `main` deliberately includes a direct
// `store_next` after publication, which violates that hook's caller contract;
// the fixture pins only the compiler boundary at those bare calls below.
unsafe impl StackStorage<16> for Pool {
    unsafe fn head(&self) -> &StackHead<16> {
        &self.head
    }

    unsafe fn load_next(&self, index: u32) -> u32 {
        self.next[index as usize].load(Ordering::Acquire)
    }

    unsafe fn store_next(&self, index: u32, next: u32) {
        self.next[index as usize].store(next, Ordering::Release);
    }
}

fn main() {
    let pool = Pool {
        head: StackHead::new(),
        next: [const { AtomicU32::new(0) }; 8],
    };
    // SAFETY: fresh pool (domain 0..8); indices 1 and 2 are in-domain and pushed exactly once.
    unsafe { pool.push_index(1) }.expect("fresh head has tag budget");
    unsafe { pool.push_index(2) }.expect("fresh head has tag budget");
    // `head()` and `load_next(2)` are contract-shaped for the published index.
    // The direct `store_next` below is deliberate contract misuse: it runs
    // after publication instead of immediately before a publishing CAS. All
    // three calls stay bare so this fixture proves only the E0133 compiler
    // boundary, not contract-valid hook semantics.
    // Bare call outside an `unsafe` block. ERROR: E0133 — call to unsafe
    // function is unsafe.
    let _head = pool.head();
    // Bare call outside an `unsafe` block. ERROR: E0133 — call to unsafe
    // function is unsafe.
    let _next = pool.load_next(2);
    // Bare call outside an `unsafe` block. ERROR: E0133 — call to unsafe
    // function is unsafe.
    pool.store_next(2, TAIL);
}
