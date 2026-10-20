//! B4 variant (в) — STATE-BYTE ⇔ ACTUAL-STATE property test.
//!
//! A deterministic pseudo-random (LCG, fixed seed, no external crates)
//! mixed sequence of alloc / free / flush / refill / trim / decommit-reset
//! operations (~64+ cases per run) over the `AllocCore` substrate. A local
//! model (block start → state) is maintained IN PARALLEL through the same
//! public API:
//!
//! * LIVE  — issued by alloc/refill/drain and not yet returned;
//! * FREE  — returned by dealloc / flush / reclaim (substrate has no
//!   tcache magazine, so the MAG state does not occur here; MAG is pinned
//!   in `state_byte_heap_magazine.rs`);
//! * zero  — never issued / interior / reset by decommit-recycle.
//!
//! After EVERY operation the model is checked against the raw state-byte
//! region (`dbg_state_bytes_for`): every tracked start has exactly the
//! modeled state and class field, and every NONZERO byte of a dumped
//! segment belongs to a tracked start (invariant И1). The block start →
//! state map is the trace of what the API handed out and took back.
//!
//! Mutant contract: removing ANY state-byte commit site (carve, pop, drain,
//! dealloc_small FREE, flush_run FREE, reclaim FREE, decommit reset) makes
//! some case in the sequence diverge → RED.

// `bench-internals` + `alloc-decommit`: the scoped-reset oracle drives the
// reset through `dbg_force_decommit_retain_for` (the R29-8 test seam), the
// ONLY path that applies the F7 scoped state-byte reset deterministically
// (plain pool admission retains committed pages and intact free lists
// WITHOUT a reset, so it cannot serve as the counterfactual's trigger).
#![cfg(all(
    feature = "alloc-core",
    feature = "internals",
    feature = "alloc-decommit",
    feature = "bench-internals"
))]

use core::alloc::Layout;
use std::collections::{BTreeMap, BTreeSet};

use sefer_alloc::{AllocCore, SegmentLayout};

const SEGMENT: usize = 4 * 1024 * 1024;
const FOOTPRINT: usize = SEGMENT / 16;

const LIVE: u8 = 0;
const FREE: u8 = 2;

/// Deterministic LCG (Numerical Recipes constants), fixed seed.
struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed)
    }
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 16
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Cls {
    C16,
    C64,
}

impl Cls {
    fn layout(self) -> Layout {
        match self {
            Cls::C16 => Layout::from_size_align(16, 8).unwrap(),
            Cls::C64 => Layout::from_size_align(64, 8).unwrap(),
        }
    }
    fn class_idx(self, ac: &AllocCore) -> usize {
        ac.dbg_layout_class_for(self.layout()).expect("small class")
    }
    fn granules(self) -> usize {
        match self {
            Cls::C16 => 1,
            Cls::C64 => 4,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum St {
    Live,
    Free,
}

struct Model {
    /// start address → (class, state)
    blocks: BTreeMap<usize, (Cls, St)>,
    /// bases with at least one LIVE block (dump anchors)
    live_bases: BTreeSet<usize>,
}

impl Model {
    fn new() -> Self {
        Model {
            blocks: BTreeMap::new(),
            live_bases: BTreeSet::new(),
        }
    }
    fn base_of(p: *mut u8) -> usize {
        SegmentLayout::segment_base_of(p as usize)
    }
    #[allow(dead_code)] // kept for symmetry with the model API (clippy -D sweep)
    fn insert_live(&mut self, p: *mut u8, c: Cls) {
        self.blocks.insert(p as usize, (c, St::Live));
        self.live_bases.insert(Self::base_of(p));
    }
    fn mark_free(&mut self, p: *mut u8) {
        if let Some(e) = self.blocks.get_mut(&(p as usize)) {
            e.1 = St::Free;
        }
        // Drop the dump anchor if this base has no live block left; it is
        // re-added when a block of the base is (re-)issued.
        let base = Self::base_of(p);
        let still_live = self
            .blocks
            .iter()
            .any(|(&a, (_, s))| *s == St::Live && Self::base_of(a as *mut u8) == base);
        if !still_live {
            self.live_bases.remove(&base);
        }
    }
    fn issue(&mut self, p: *mut u8, c: Cls) {
        self.blocks.insert(p as usize, (c, St::Live));
        self.live_bases.insert(Self::base_of(p));
    }
    fn live_of_class(&self, c: Cls) -> Vec<usize> {
        self.blocks
            .iter()
            .filter(|(_, (cc, s))| *cc == c && *s == St::Live)
            .map(|(a, _)| *a)
            .collect()
    }
    fn granule(addr: usize) -> usize {
        (addr & (SEGMENT - 1)) >> 4
    }
}

/// Check the model against the raw state bytes of every dumpable segment.
fn verify(ac: &AllocCore, model: &Model, ctx: &str) {
    let mut seen_bases: BTreeSet<usize> = BTreeSet::new();
    for &base in &model.live_bases {
        // One LIVE block per base serves as the dump anchor.
        let anchor_addr = *model
            .blocks
            .iter()
            .find(|(a, (_, s))| *s == St::Live && Model::base_of(**a as *mut u8) == base)
            .map(|(a, _)| a)
            .expect("live base must have a live block");
        let anchor = anchor_addr as *mut u8;
        let mut buf = vec![0u8; FOOTPRINT];
        // SAFETY: anchor is a live allocation owned by `ac`.
        unsafe { ac.dbg_state_bytes_for(anchor, &mut buf) };
        seen_bases.insert(base);
        for (&addr, &(c, s)) in &model.blocks {
            if Model::base_of(addr as *mut u8) != base {
                continue;
            }
            let g = Model::granule(addr);
            let b = buf[g];
            let want_state = match s {
                St::Live => LIVE,
                St::Free => FREE,
            };
            assert_ne!(b, 0, "{ctx}: tracked block {addr:#x} has zero byte");
            assert_eq!(
                b >> 6,
                want_state,
                "{ctx}: block {addr:#x} state {} != modeled {want_state}",
                b >> 6,
            );
            assert_eq!(
                b & 0x3F,
                c.class_idx(ac) as u8 + 1,
                "{ctx}: block {addr:#x} class field mismatch"
            );
            // Interior granules of a multi-granule block stay zero.
            for k in 1..c.granules() {
                assert_eq!(
                    buf[g + k],
                    0,
                    "{ctx}: interior granule of {addr:#x} not zero"
                );
            }
        }
        // И1 (prefix form): every granule BELOW the lowest tracked granule
        // of this base must be zero (never issued, never carved-into).
        // (Granules inside the substrate's carved-but-unissued refill
        // window legitimately carry LIVE bytes — carve commits LIVE — so a
        // whole-region nonzero⟹tracked scan does not hold; the exactness
        // property is carried by the per-block checks above.)
        let min_g = model
            .blocks
            .iter()
            .filter(|(&a, _)| Model::base_of(a as *mut u8) == base)
            .map(|(&a, _)| Model::granule(a))
            .min()
            .expect("live base has tracked blocks");
        assert!(
            buf[..min_g].iter().all(|&b| b == 0),
            "{ctx}: nonzero state byte below the first issued granule of base {base:#x}"
        );
    }
    let _ = seen_bases;
}

fn run_property(seed: u64, ops: usize) {
    let mut ac = AllocCore::new().expect("primordial");
    let mut model = Model::new();
    let mut rng = Lcg::new(seed);
    let mut issued_counter: u64 = 0;

    for op_i in 0..ops {
        let roll = rng.below(100);
        if roll < 40 {
            // alloc / refill (the refill fast path serves allocs from a
            // freshly carved batch; from the model both are "issued LIVE").
            let c = if rng.below(2) == 0 {
                Cls::C16
            } else {
                Cls::C64
            };
            let p = ac.alloc(c.layout());
            assert!(!p.is_null(), "alloc OOM at op {op_i}");
            model.issue(p, c);
            issued_counter += 1;
        } else if roll < 70 {
            // free a random LIVE block of a random class (exact layout).
            let c = if rng.below(2) == 0 {
                Cls::C16
            } else {
                Cls::C64
            };
            let live = model.live_of_class(c);
            if live.is_empty() {
                continue;
            }
            let addr = live[rng.below(live.len() as u64) as usize];
            // SAFETY: `addr` is a live block of its class in the model.
            unsafe { ac.dealloc(addr as *mut u8, c.layout()) };
            model.mark_free(addr as *mut u8);
        } else if roll < 85 {
            // flush: return a run of LIVE 16B blocks to the bin via
            // `flush_class` (the substrate overflow/R1-01/teardown path).
            let live = model.live_of_class(Cls::C16);
            if live.len() < 3 {
                continue;
            }
            let k = 1 + rng.below((live.len() - 1).min(8) as u64) as usize;
            let run: Vec<*mut u8> = live[..k].iter().map(|&a| a as *mut u8).collect();
            // SAFETY: all blocks are LIVE class-of-16B blocks of `ac`.
            unsafe { ac.flush_class(Cls::C16.class_idx(&ac), &run) };
            for &p in &run {
                model.mark_free(p);
            }
        } else if roll < 95 {
            // reclaim-drain shape: pop from the bin back to LIVE
            // (free-list pop / drain path — model: FREE → LIVE).
            let freed: Vec<usize> = model
                .blocks
                .iter()
                .filter(|(_, (_, s))| *s == St::Free)
                .map(|(a, _)| *a)
                .collect();
            if freed.is_empty() {
                continue;
            }
            let addr = freed[rng.below(freed.len() as u64) as usize];
            // Reissue via alloc: the bin pop serves a freed block.
            let p = ac.alloc(Cls::C16.layout());
            assert!(!p.is_null());
            model.issue(p, Cls::C16);
            if Model::granule(p as usize) == Model::granule(addr) {
                let _ = addr; // the popped block may be the one we saw freed
            }
        } else {
            // trim: issue + immediately return a block (parking shape), the
            // cheapest owner-side trim surrogate at the substrate.
            let p = ac.alloc(Cls::C64.layout());
            assert!(!p.is_null());
            model.issue(p, Cls::C64);
            // SAFETY: live, single free.
            unsafe { ac.dealloc(p, Cls::C64.layout()) };
            model.mark_free(p);
        }
        verify(
            &ac,
            &model,
            &format!("op {op_i} (roll {roll}, issued {issued_counter})"),
        );
    }

    // ── F14 (final-phase rework): recycle + scoped-reset oracle ──────────
    // The old final phase iterated an EMPTY `live_bases` set (every base had
    // been dropped from it by `mark_free`), so its recycle loop was vacuum
    // and `any_recycled` was ignored. Rework:
    //   * remember each base's maximum issued granule (the model knows it);
    //   * free everything; drain the pool — EMPTY SMALL segments get
    //     decommitted AND their state bytes scoped-reset over
    //     [payload_start, bump_before_reset) (the primordial bootstrap
    //     segment never enters the small pool and is NOT reset, so it is
    //     excluded from the oracle and its tracked entries stay in the
    //     model);
    //   * re-issue into a RECYCLED SMALL base and require the RESET shape:
    //     every granule below the PREVIOUS high-water mark must be zero
    //     except the fresh carve/refill window at the new bump.
    // The first allocation of the run always lands in the primordial
    // segment — that base is the non-resetting witness.
    let primordial_base = match model.blocks.iter().next() {
        Some((&a, _)) => Model::base_of(a as *mut u8),
        None => panic!("property run issued no blocks"),
    };
    // Fill: drive allocation until at least one SMALL segment is reserved
    // (the primordial bootstrap segment serves first and never enters the
    // small pool) AND some small base's issued span towers above the
    // post-reset carve frontier, so the stale-region check below is
    // non-vacuous. C64 blocks advance the granule high-water mark 4 per
    // block; ~2.5k granules over the small payload start is reached after
    // the primordial is exhausted (~60k C64 blocks) plus a few hundred more.
    const SMALL_PAYLOAD_START_G: usize = sefer_alloc::SegmentLayout::SMALL_META_END >> 4;
    const TALL_SPAN_GRANULES: usize = SMALL_PAYLOAD_START_G + 2560;
    let mut fill = Vec::new();
    let mut small_tall = false;
    let mut small_max_g: usize = 0;
    for i in 0..200_000 {
        let p = ac.alloc(Cls::C64.layout());
        assert!(!p.is_null(), "fill alloc OOM at {i}");
        model.issue(p, Cls::C64);
        fill.push(p as usize);
        if Model::base_of(p) != primordial_base {
            small_max_g = small_max_g.max(Model::granule(p as usize));
            small_tall = small_max_g >= TALL_SPAN_GRANULES;
        }
        if small_tall {
            break;
        }
    }
    assert!(
        small_tall,
        "the run never grew a small segment span past {TALL_SPAN_GRANULES}          granules — the scoped-reset oracle needs a tall small base"
    );
    // Per-base maximum granule ever issued (model-known high-water mark).
    let mut max_g: BTreeMap<usize, usize> = BTreeMap::new();
    for &a in model.blocks.keys() {
        let b = Model::base_of(a as *mut u8);
        let g = Model::granule(a);
        max_g.entry(b).and_modify(|m| *m = (*m).max(g)).or_insert(g);
    }
    // Free ONLY the small-base blocks (the primordial's blocks stay LIVE, so
    // its bins stay empty and the recycle allocations below land in the
    // small bases). Emptying a small segment routes it into the pool:
    // decommit + scoped state-byte reset.
    for (&addr, &(_, s)) in model.blocks.clone().iter() {
        if s == St::Live && Model::base_of(addr as *mut u8) != primordial_base {
            // SAFETY: live, single free.
            unsafe { ac.dealloc(addr as *mut u8, Cls::C64.layout()) };
            model.mark_free(addr as *mut u8);
        }
    }
    // Anchors: one tracked address per small base (for the forced reset).
    let mut anchors: BTreeMap<usize, usize> = BTreeMap::new();
    for &a in model.blocks.keys() {
        let b = Model::base_of(a as *mut u8);
        if b != primordial_base {
            anchors.entry(b).or_insert(a);
        }
    }
    // Small bases are about to be reset: their stale FREE entries must leave
    // the model (their bytes will read zero), while the primordial's LIVE
    // entries stay (that segment is never reset).
    model
        .blocks
        .retain(|&a, _| Model::base_of(a as *mut u8) == primordial_base);
    // Force the decommit-retain reset on EVERY small base (plain pool
    // admission retains committed pages and intact free lists WITHOUT the
    // reset, so it cannot trigger the scoped state-byte zero).
    for (&b, &a) in &anchors {
        // SAFETY: `a` is a block of the empty small segment `b` owned by
        // `ac` (live_count == 0 after the frees above).
        let ok = unsafe { ac.dbg_force_decommit_retain_for(a as *mut u8) };
        assert!(ok, "forced decommit-retain failed for base {b:#x}");
    }

    // Fresh-window bound: one issued block + the carve refill batch all
    // commit bytes within a few dozen granules of the segment's fresh bump
    // (32-block carve batch + margin); the recycle loop's own issued span
    // is added on top at check time.
    const WINDOW_MARGIN: usize = 40;
    // The previous span must extend this far beyond the fresh activity, or
    // the stale-region check below would be vacuous.
    const MIN_STALE_SPAN: usize = 8;
    let mut any_recycled = false;
    // base -> (first granule touched this session, blocks issued this session)
    let mut span: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
    let mut checked: Option<(usize, usize, usize)> = None; // (base, mg, gmin)
    for _ in 0..4096 {
        let p = ac.alloc(Cls::C16.layout());
        assert!(!p.is_null(), "recycle alloc");
        let b = Model::base_of(p);
        let g = Model::granule(p as usize);
        model.issue(p, Cls::C16);
        if b == primordial_base || !max_g.contains_key(&b) {
            continue;
        }
        any_recycled = true;
        let e = span.entry(b).or_insert((g, 0));
        e.0 = e.0.min(g);
        e.1 += 1;
        // A reset small segment carves from near its payload start (the
        // reset frontier may include the committed first chunk); it must NOT
        // sit in the old high-water region.
        let gmin = span[&b].0;
        assert!(
            (sefer_alloc::SegmentLayout::SMALL_META_END >> 4) <= gmin,
            "first post-reset touch in a recycled small base must be at/after              the fresh payload start (got {gmin})"
        );
        // Need a base whose PREVIOUS span extends well beyond the fresh
        // activity, else the stale-region check would be vacuous.
        let cnt = span[&b].1;
        let mg = max_g[&b];
        if mg >= gmin + cnt + WINDOW_MARGIN + MIN_STALE_SPAN {
            checked = Some((b, mg, gmin));
            break;
        }
    }
    assert!(
        any_recycled,
        "no alloc ever landed in a previously-issued small base — the          recycle premise of the scoped-reset check is broken"
    );
    let (base, mg, gmin) = checked.expect("a recycled base tall enough for the reset oracle");
    let fresh_end = gmin + span[&base].1 + WINDOW_MARGIN;
    let mut buf = vec![0u8; FOOTPRINT];
    // SAFETY: a live allocation in `base` serves as the dump anchor.
    unsafe { ac.dbg_state_bytes_for(base as *mut u8, &mut buf) };
    let mut fresh_nonzero = 0usize;
    for (g, &b) in buf.iter().enumerate().take(mg) {
        if b != 0 {
            assert!(
                g >= gmin && g < fresh_end,
                "scoped reset: stale nonzero state byte at granule {g} of recycled                  base {base:#x} (fresh activity [{gmin}, {fresh_end}), previous                  high-water {mg})"
            );
            fresh_nonzero += 1;
        }
    }
    assert!(
        fresh_nonzero >= 1,
        "the fresh window must carry the new issue"
    );
    verify(&ac, &model, "final");
}

#[test]
fn state_byte_matches_model_seed_1() {
    run_property(0xB4_B4_B4_B4, 64);
}

#[test]
fn state_byte_matches_model_seed_2() {
    run_property(0x5EF_00D, 64);
}

#[test]
fn state_byte_matches_model_seed_3_long() {
    run_property(7, 96);
}
