//! Sol-codex run-3 P3-6 negative oracle, lower bound: `INDEX_BITS = 0` must
//! NOT compile. This file MUST FAIL TO COMPILE with the `_CHECK_BITS`
//! assertion (E0080) naming the `1..=16` range requirement — the guard that
//! carries the documented minimum-48-bit-tag argument. Pinned failing by
//! `tests/compile_fail_index_bits_bounds.rs`.
use tagged_index_stack::TaggedIndex;

fn main() {
    // INDEX_MASK's initializer evaluates `Self::_CHECK_BITS` directly (see
    // src/lib.rs), so a bare const read is the earliest, most direct route to
    // the guard — independent of pack()/try_pack().
    let _ = TaggedIndex::<0>::INDEX_MASK;
}
