//! NL-3: randomized cross-thread churn driven only by safe code (`Vec<u8>` moved
//! between threads over channels, grown in place, dropped by whichever thread
//! holds it). The allocator contract is never violated here, so a mode that
//! trusts its callers (no double-free / foreign-pointer checks) must behave
//! exactly like the checking mode.
//!
//! Oracles:
//! - every buffer carries a pattern derived from its unique id over its whole
//!   length; the pattern is verified on every handoff, before every growth and at
//!   the drop. Two live blocks that overlap, or a block recycled while still owned,
//!   clobber it;
//! - the created and verified-and-dropped counts match (no buffer is lost);
//! - the live segment count after warm-up grows by at most a small slack: a remote
//!   free that is lost is never recycled and keeps the owner reserving segments.
//!
//! Excluded under `numa-aware` for the reason given in `g1_ingress_remote_free_reuse`.
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    not(feature = "numa-aware")
))]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const THREADS: usize = 4;
const ITERS: usize = 30_000;
const WARMUP: usize = ITERS / 4;
const POOL_CAP: usize = 48;
const SEGMENT_SLACK: i64 = 8;
const SEEDS: [u64; 3] = [
    0x9E37_79B9_7F4A_7C15,
    0xD1B5_4A32_D192_ED03,
    0x2545_F491_4F6C_DD1D,
];
/// Mostly small classes, a few medium sizes; no size reaches the large path.
const SIZES: [usize; 12] = [8, 16, 24, 48, 64, 100, 256, 512, 1000, 2048, 4096, 20_000];

static CREATED: AtomicU64 = AtomicU64::new(0);
static RETIRED: AtomicU64 = AtomicU64::new(0);

struct Item {
    id: u64,
    buf: Vec<u8>,
}

fn pattern(id: u64, i: usize) -> u8 {
    let x = id.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (i as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    (x >> 29) as u8
}

impl Item {
    fn new(id: u64, len: usize) -> Item {
        CREATED.fetch_add(1, Ordering::Relaxed);
        let buf = (0..len).map(|i| pattern(id, i)).collect();
        Item { id, buf }
    }

    fn verify(&self, what: &str) {
        for (i, &b) in self.buf.iter().enumerate() {
            assert_eq!(
                b,
                pattern(self.id, i),
                "{what}: buffer {} (len {}) clobbered at byte {i}",
                self.id,
                self.buf.len()
            );
        }
    }

    fn grow(&mut self, extra: usize) {
        self.verify("before grow");
        let from = self.buf.len();
        self.buf.reserve(extra);
        for i in from..from + extra {
            self.buf.push(pattern(self.id, i));
        }
        self.verify("after grow");
    }

    fn retire(self, what: &str) {
        self.verify(what);
        RETIRED.fetch_add(1, Ordering::Relaxed);
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn live_segments() -> i64 {
    let s = GLOBAL.stats();
    s.segments_reserved_total as i64 - s.segments_released_total as i64
}

fn worker(tid: usize, seed: u64, peers: Vec<Sender<Item>>, inbox: Receiver<Item>) -> (i64, i64) {
    let mut rng = Rng(seed ^ ((tid as u64 + 1) << 32) | 1);
    let mut pool: Vec<Item> = Vec::new();
    let mut next_id = (tid as u64) << 40;
    let (mut at_warmup, mut at_end) = (0i64, 0i64);
    for it in 0..ITERS {
        if tid == 0 && it == WARMUP {
            at_warmup = live_segments();
        }
        while let Ok(item) = inbox.try_recv() {
            item.retire("remote drop");
        }
        match rng.below(100) {
            0..=44 => {
                next_id += 1;
                let len = SIZES[rng.below(SIZES.len())] + rng.below(16);
                let item = Item::new(next_id, len);
                if pool.len() >= POOL_CAP {
                    pool.swap_remove(rng.below(pool.len())).retire("own drop");
                }
                pool.push(item);
            }
            45..=74 if !pool.is_empty() => {
                let item = pool.swap_remove(rng.below(pool.len()));
                let to = (tid + 1 + rng.below(THREADS - 1)) % THREADS;
                // The peer for thread `to` sits at this index in the list without our own slot.
                let slot = if to > tid { to - 1 } else { to };
                peers[slot].send(item).expect("peer inbox closed early");
            }
            75..=89 if !pool.is_empty() => {
                let k = rng.below(pool.len());
                pool[k].grow(1 + rng.below(300));
            }
            _ if !pool.is_empty() => {
                pool.swap_remove(rng.below(pool.len())).retire("own drop");
            }
            _ => {}
        }
    }
    if tid == 0 {
        at_end = live_segments();
    }
    drop(peers);
    for item in pool {
        item.retire("pool drain");
    }
    for item in inbox {
        item.retire("final remote drop");
    }
    (at_warmup, at_end)
}

fn run(seed: u64) -> (i64, i64) {
    let mut senders = Vec::new();
    let mut receivers = Vec::new();
    for _ in 0..THREADS {
        let (tx, rx) = channel::<Item>();
        senders.push(tx);
        receivers.push(rx);
    }
    let mut handles = Vec::new();
    for (tid, inbox) in receivers.into_iter().enumerate() {
        let peers: Vec<Sender<Item>> = senders
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != tid)
            .map(|(_, s)| s.clone())
            .collect();
        handles.push(thread::spawn(move || worker(tid, seed, peers, inbox)));
    }
    drop(senders);
    let mut seg = (0, 0);
    for (tid, h) in handles.into_iter().enumerate() {
        match h.join() {
            Ok(v) if tid == 0 => seg = v,
            Ok(_) => {}
            Err(p) => std::panic::resume_unwind(p),
        }
    }
    seg
}

#[test]
fn safe_cross_thread_churn_keeps_every_buffer_intact_and_recycled() {
    let mut growth = Vec::new();
    for seed in SEEDS {
        let created0 = CREATED.load(Ordering::Relaxed);
        let retired0 = RETIRED.load(Ordering::Relaxed);
        let (at_warmup, at_end) = run(seed);
        let created = CREATED.load(Ordering::Relaxed) - created0;
        let retired = RETIRED.load(Ordering::Relaxed) - retired0;
        assert_eq!(created, retired, "seed {seed:#x}: a buffer was lost");
        assert!(
            created > 1000,
            "seed {seed:#x}: only {created} buffers made"
        );
        growth.push((seed, at_end - at_warmup));
    }
    let bad: Vec<_> = growth.iter().filter(|g| g.1 > SEGMENT_SLACK).collect();
    assert!(
        bad.is_empty(),
        "segment growth after warm-up exceeds {SEGMENT_SLACK}: {growth:?}"
    );
}
