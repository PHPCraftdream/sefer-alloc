//! Compile-fail fixture — caller-side hook forgery
//! against a downstream implementor): the three `StackStorage` hooks each
//! take a first `_: &Hook` witness parameter, and `Hook` is
//! `pub struct Hook(())` — public type, PRIVATE field — so the witness can
//! only be constructed inside this crate. Even the implementor's OWN crate
//! therefore cannot call `head`/`load_next`/`store_next` from safe code:
//!
//! - Attack route (a): the bare call `pool.store_next(1, 3)` omits the
//!   required `&Hook` argument (E0061-family arity error).
//! - Attack route (b): `pool.store_next(&Hook(()), 1, 3)` attempts to forge
//!   the witness via tuple-struct construction (E0423-family private-field
//!   error; the struct-literal spelling `Hook { 0: () }` is E0451).
//!
//! This reproduces, as a compile-fail oracle, the attack this closure makes
//! UNEXPRESSIBLE: a bare `p.store_next(1, 3)` splices a cycle and
//! double-issues. Pinned failing by
//! `tests/compile_fail.rs`.
//!
//! Note: the witness is `&Hook` (a reference), NOT an owned token — this is
//! load-bearing. An owned non-Copy token could be stashed by a cooperating
//! implementor into a `Cell<Option<Hook>>` and re-exposed through the
//! implementor's own safe method; the reference form makes that a lifetime
//! error. Full rationale: the audit report + the repository ADR (both
//! repository files, not published).
use core::sync::atomic::{AtomicU32, Ordering};

use tagged_index_stack::{Hook, StackHead, StackOps, StackStorage};

struct Pool {
    head: StackHead<16>,
    next: [AtomicU32; 8],
}

// SAFETY: this impl is CORRECT — it upholds every `# Safety` clause (privately
// owned head, dedicated link cells, the stack driven only via
// push_index/pop_index). The defects this fixture pins are the two ATTACK
// calls in `main` below, not the impl.
unsafe impl StackStorage<16> for Pool {
    fn head(&self, _: &Hook) -> &StackHead<16> {
        &self.head
    }
    fn load_next(&self, _: &Hook, index: u32) -> u32 {
        self.next[index as usize].load(Ordering::Acquire)
    }
    fn store_next(&self, _: &Hook, index: u32, next: u32) {
        self.next[index as usize].store(next, Ordering::Release);
    }
}

fn main() {
    let pool = Pool {
        head: StackHead::new(),
        next: [const { AtomicU32::new(0) }; 8],
    };
    pool.push_index(1);
    pool.push_index(2);
    // Attack route (a): the bare call — the required `&Hook` witness argument
    // is simply missing. ERROR: E0061 — this method takes 3 arguments but 2
    // arguments were supplied ("argument #1 of type `&Hook` is missing").
    pool.store_next(1, 3);
    // Attack route (b): forge the witness itself via tuple-struct
    // construction. ERROR: E0423 — cannot initialize a tuple struct which
    // contains private fields.
    pool.store_next(&Hook(()), 1, 3);
}
