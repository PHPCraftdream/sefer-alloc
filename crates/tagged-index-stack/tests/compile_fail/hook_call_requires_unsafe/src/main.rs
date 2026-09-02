//! Compile-fail fixture — the `unsafe fn` caller-side boundary on the three
//! `StackStorage` hooks. The hooks (`head`/`load_next`/`store_next`) are
//! `unsafe fn` with per-method caller-side `# Safety` contracts, so even a
//! CORRECT implementor's own crate cannot call them from safe code: every
//! call outside an `unsafe` block is **E0133** ("call to unsafe function is
//! unsafe").
//!
//! This fixture REPLACES the retired
//! `tests/compile_fail/hook_token_unconstructible/`: the `&Hook` witness
//! design was removed because fabricating a witness value involved no unsafe
//! operation, so its prose-only closure was unenforceable; `unsafe fn` is the
//! `GlobalAlloc` shape — `unsafe trait` + `unsafe fn` — and gives a
//! compiler-checked caller-side contract.
//!
//! The compile-PASS counterpart: the hooks are a barrier to MISUSE, not to
//! legitimate use — a correct `unsafe impl` driven only through the safe
//! `push_index`/`pop_index` compiles and behaves correctly, pinned by
//! `vec_backed_storage_push_pop_round_trips` +
//! `push_pop_through_dyn_storage` in `tests/custom_storage_impl.rs`. Pinned
//! failing by `tests/compile_fail.rs`.
use core::sync::atomic::{AtomicU32, Ordering};

use tagged_index_stack::{StackHead, StackOps, StackStorage, TAIL};

struct Pool {
    head: StackHead<16>,
    next: [AtomicU32; 8],
}

// SAFETY: this impl is CORRECT — it upholds every `# Safety` clause (privately
// owned head, dedicated link cells, the stack driven only via
// push_index/pop_index). The defects this fixture pins are the three BARE
// CALLS in `main` below, not the impl.
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
    unsafe { pool.push_index(1) };
    unsafe { pool.push_index(2) };
    // Each call below is contract-shaped (index 2 was pushed through this
    // binding), so the ONLY compile error is the unsafe-call error itself.
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
