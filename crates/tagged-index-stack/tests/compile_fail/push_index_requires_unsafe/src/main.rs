//! Compile-fail fixture — the `unsafe fn` caller-side boundary on the two
//! push entry points: the blanket-impl [`StackOps::push_index`] and the owned
//! type's inherent [`ArrayIndexStack::push`]. Both became `unsafe fn` carrying
//! the three-clause caller contract (link domain + liveness + exclusive ownership), so even a CORRECT
//! implementor's own crate cannot push from safe code: every bare push outside
//! an `unsafe` block is **E0133** ("call to unsafe function is unsafe").
//!
//! The compile-PASS counterpart: the pushes are a barrier to MISUSE, not to
//! legitimate use — setup pushes here ARE properly wrapped in `unsafe {}` with
//! SAFETY comments and compile fine, and in-domain, live-free pushes through
//! `unsafe` blocks are pinned everywhere else in the suite (e.g.
//! `vec_backed_storage_push_pop_round_trips` +
//! `push_pop_through_dyn_storage` in `tests/custom_storage_impl.rs`). Pinned
//! failing by `tests/compile_fail.rs`.
use core::sync::atomic::{AtomicU32, Ordering};

use tagged_index_stack::{ArrayIndexStack, StackHead, StackOps, StackStorage};

struct Pool {
    head: StackHead<16>,
    next: [AtomicU32; 8],
}

// SAFETY: this impl is CORRECT — it upholds every `# Safety` clause (privately
// owned head, dedicated atomic link cells for the declared domain 0..8, the
// stack driven only via push_index/pop_index). The defects this fixture pins
// are the two BARE CALLS in `main` below, not the impl.
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
    let owned: ArrayIndexStack<16, 8> = ArrayIndexStack::new();
    // SAFETY: fresh stacks (domain 0..8); indices 1 and 2 are in-domain and
    // pushed exactly once on each binding.
    unsafe { pool.push_index(1) }.expect("fresh head has tag budget");
    unsafe { owned.push(2) }.expect("fresh head has tag budget");
    // Index 0 is in each binding's 0..8 domain and was never pushed through
    // either, so the ONLY compile errors are the unsafe-call errors themselves.
    // Bare call outside an `unsafe` block. ERROR: E0133 — call to unsafe
    // function `push_index` is unsafe and requires unsafe function or block.
    pool.push_index(0);
    // Bare call outside an `unsafe` block. ERROR: E0133 — call to unsafe
    // function `ArrayIndexStack::<B, N>::push` is unsafe and requires unsafe
    // function or block.
    owned.push(0);
}
