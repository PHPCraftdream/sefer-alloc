//! Differential property test (Phase 1 conformance check).
//!
//! The `Region` must behave like its reference model — a `Vec` of the live
//! `(handle, value)` pairs — across random sequences of operations. This
//! encodes invariants I1–I5 from `docs/INVARIANTS.md`. The model is the oracle;
//! any divergence is a bug. The op-set covers insert / remove / get / `get_mut` /
//! clear, and the payload counts its drops so I5 is checked under random
//! sequences too (after the run, total drops must equal total inserts).

use std::cell::Cell;
use std::rc::Rc;

use proptest::prelude::*;
use sefer_alloc::{Handle, Region};

/// A drop-counting payload. Two payloads compare by their inner id so the
/// reference model can verify `get`/`get_mut` results by value.
#[derive(Clone, Debug)]
struct Payload {
    id: u64,
    drops: Rc<Cell<usize>>,
}

impl PartialEq for Payload {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Drop for Payload {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

#[derive(Clone, Debug)]
enum Op {
    Insert(u64),
    Remove(usize),
    Get(usize),
    GetMut(usize, u64),
    Clear,
}

proptest! {
    #[test]
    fn region_matches_reference_model(
        ops in prop::collection::vec(
            prop_oneof![
                any::<u64>().prop_map(Op::Insert),
                any::<usize>().prop_map(Op::Remove),
                any::<usize>().prop_map(Op::Get),
                (any::<usize>(), any::<u64>()).prop_map(|(i, v)| Op::GetMut(i, v)),
                Just(Op::Clear),
            ],
            0..300,
        )
    ) {
        let drops = Rc::new(Cell::new(0usize));
        let mut region: Region<Payload> = Region::new();
        // The model: every currently-live (handle, value).
        let mut live: Vec<(Handle<Payload>, u64)> = Vec::new();
        // Total successful inserts, to check I5 (drop-once) at the end.
        let mut total_inserts = 0usize;

        for op in ops {
            match op {
                Op::Insert(v) => {
                    let p = Payload { id: v, drops: drops.clone() };
                    let h = region.insert(p);
                    // I1: a fresh handle resolves to the inserted value.
                    prop_assert_eq!(region.get(h).map(|p| p.id), Some(v));
                    live.push((h, v));
                    total_inserts += 1;
                }
                Op::Remove(n) => {
                    if !live.is_empty() {
                        let i = n % live.len();
                        let (h, v) = live.swap_remove(i);
                        prop_assert_eq!(region.remove(h).map(|p| p.id), Some(v));
                        // I2: removed handle is None, and removing again is a no-op.
                        prop_assert_eq!(region.get(h).map(|p| p.id), None);
                        prop_assert_eq!(region.remove(h).map(|p| p.id), None);
                    }
                }
                Op::Get(n) => {
                    if !live.is_empty() {
                        let i = n % live.len();
                        let (h, v) = live[i];
                        prop_assert_eq!(region.get(h).map(|p| p.id), Some(v));
                    }
                }
                Op::GetMut(n, new_id) => {
                    if !live.is_empty() {
                        let i = n % live.len();
                        let h = live[i].0;
                        if let Some(p) = region.get_mut(h) {
                            p.id = new_id;
                        }
                        // Reflect the mutation in the model and verify it stuck.
                        live[i].1 = new_id;
                        prop_assert_eq!(region.get(h).map(|p| p.id), Some(new_id));
                    }
                }
                Op::Clear => {
                    region.clear();
                    live.clear();
                    // I2/I4: everything gone, region empty and reusable.
                    prop_assert!(region.is_empty());
                    prop_assert_eq!(region.len(), 0);
                }
            }
            // I4: length tracks the model exactly.
            prop_assert_eq!(region.len(), live.len());
        }

        // Every survivor still resolves to its value.
        for (h, v) in &live {
            prop_assert_eq!(region.get(*h).map(|p| p.id), Some(*v));
        }

        // I5: drop-once. At this point every removed value has been dropped
        // exactly once; the survivors are about to be dropped by `region`'s
        // scope exit. Drop the region explicitly and account for all inserts.
        drop(region);
        // `live` holds only `(Handle, u64)` — no `Payload`s — so this drops no
        // payloads; every `Payload` lived in the region until removed, cleared,
        // or dropped with the region just above. After that, each insert must
        // correspond to exactly one drop.
        drop(live);
        prop_assert_eq!(
            drops.get(),
            total_inserts,
            "every inserted value must be dropped exactly once (no double-free, no leak)"
        );
    }
}
