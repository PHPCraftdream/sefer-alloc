//! Differential property test (Phase 0 verification harness).
//!
//! The `Region` must behave like its reference model — a `Vec` of the live
//! `(handle, value)` pairs — across random sequences of operations. This
//! encodes invariants I1–I4 from `docs/INVARIANTS.md`. The model is the
//! oracle; any divergence is a bug.

use proptest::prelude::*;
use sefer_alloc::{Handle, Region};

#[derive(Clone, Debug)]
enum Op {
    Insert(u64),
    Remove(usize),
    Get(usize),
}

proptest! {
    #[test]
    fn region_matches_reference_model(
        ops in prop::collection::vec(
            prop_oneof![
                any::<u64>().prop_map(Op::Insert),
                any::<usize>().prop_map(Op::Remove),
                any::<usize>().prop_map(Op::Get),
            ],
            0..300,
        )
    ) {
        let mut region: Region<u64> = Region::new();
        // The model: every currently-live (handle, value).
        let mut live: Vec<(Handle<u64>, u64)> = Vec::new();

        for op in ops {
            match op {
                Op::Insert(v) => {
                    let h = region.insert(v);
                    // I1: a fresh handle resolves to the inserted value.
                    prop_assert_eq!(region.get(h), Some(&v));
                    live.push((h, v));
                }
                Op::Remove(n) => {
                    if !live.is_empty() {
                        let i = n % live.len();
                        let (h, v) = live.swap_remove(i);
                        prop_assert_eq!(region.remove(h), Some(v));
                        // I2: removed handle is None, and removing again is a no-op.
                        prop_assert_eq!(region.get(h), None);
                        prop_assert_eq!(region.remove(h), None);
                    }
                }
                Op::Get(n) => {
                    if !live.is_empty() {
                        let i = n % live.len();
                        let (h, v) = live[i];
                        prop_assert_eq!(region.get(h), Some(&v));
                    }
                }
            }
            // I4: length tracks the model exactly.
            prop_assert_eq!(region.len(), live.len());
        }

        // Every survivor still resolves to its value.
        for (h, v) in &live {
            prop_assert_eq!(region.get(*h), Some(v));
        }
    }
}
