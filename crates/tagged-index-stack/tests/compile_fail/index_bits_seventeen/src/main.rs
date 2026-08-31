//! Sol-codex run-3 P3-6 negative oracle, upper bound: `INDEX_BITS = 17` must
//! NOT compile. This file MUST FAIL TO COMPILE with the `_CHECK_BITS`
//! assertion (E0080) naming the `1..=16` range requirement — the 16 cap is
//! what guarantees every legal configuration at least the documented
//! minimum 48-bit ABA tag. Pinned failing by
//! `tests/compile_fail_index_bits_bounds.rs`.
use tagged_index_stack::TaggedIndex;

fn main() {
    // INDEX_MASK's initializer evaluates `Self::_CHECK_BITS` directly (see
    // src/imp.rs), so a bare const read is the earliest, most direct route to
    // the guard — independent of pack()/try_pack().
    let _ = TaggedIndex::<17>::INDEX_MASK;
}
