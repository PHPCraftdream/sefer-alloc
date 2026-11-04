//! NL-7: safe-code `Vec<u8>` growth and shrink across every size band (small,
//! medium, large), and large buffers dropped by another thread. The allocator
//! contract is never violated here, so a mode that trusts its callers (no
//! interior-pointer / oversized-layout checks on realloc and large free) must
//! behave exactly like the checking mode.
//!
//! Oracles: a pattern derived from the buffer id covers the whole length and is
//! verified after every resize and at the drop; every realloc keeps the prefix
//! it promised and every buffer sent to the dropping thread arrives.
#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]

use std::sync::mpsc::channel;
use std::thread;

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// Under miri the same ladder is bounded so the interpreter finishes.
#[cfg(not(miri))]
const LADDER: &[usize] = &[
    24, 100, 1000, 5000, 70_000, 300_000, 2_000_000, 5_000_000, 600_000, 9_000, 120, 40,
];
#[cfg(miri)]
const LADDER: &[usize] = &[24, 100, 1000, 5000, 70_000, 300_000, 20_000, 120, 40];
#[cfg(not(miri))]
const ROUNDS: u64 = 24;
#[cfg(miri)]
const ROUNDS: u64 = 2;

fn pattern(id: u64, i: usize) -> u8 {
    let x = id.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (i as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    (x >> 29) as u8
}

fn fill(buf: &mut [u8], id: u64, from: usize) {
    for (i, b) in buf.iter_mut().enumerate().skip(from) {
        *b = pattern(id, i);
    }
}

fn verify(buf: &[u8], id: u64, what: &str) {
    for (i, b) in buf.iter().enumerate() {
        assert_eq!(
            *b,
            pattern(id, i),
            "{what}: id {id} byte {i} of {}",
            buf.len()
        );
    }
}

/// Walks one buffer along the ladder (grow and shrink through every band).
fn walk(id: u64) -> Vec<u8> {
    let mut v: Vec<u8> = Vec::with_capacity(LADDER[0]);
    v.resize(LADDER[0], 0);
    fill(&mut v, id, 0);
    for &next in &LADDER[1..] {
        let old = v.len();
        if next > old {
            v.resize(next, 0);
            fill(&mut v, id, old);
        } else {
            v.truncate(next);
            v.shrink_to_fit();
        }
        verify(&v, id, "after resize");
    }
    v
}

#[test]
fn single_thread_ladder_keeps_every_prefix() {
    for id in 0..ROUNDS {
        let v = walk(id);
        verify(&v, id, "final");
        drop(v);
    }
}

#[test]
fn large_buffers_dropped_by_another_thread() {
    let (tx, rx) = channel::<(u64, Vec<u8>)>();
    let dropper = thread::spawn(move || {
        let mut n = 0u64;
        while let Ok((id, v)) = rx.recv() {
            verify(&v, id, "on the dropping thread");
            drop(v);
            n += 1;
        }
        n
    });
    let mut sent = 0u64;
    for id in 1_000..1_000 + ROUNDS {
        let mut v = vec![0u8; LADDER[5]];
        fill(&mut v, id, 0);
        // Grow it once more on the producing side so the moved buffer is the
        // result of a realloc, not of the first allocation.
        let old = v.len();
        v.resize(LADDER[LADDER.len() / 2 + 1].max(old + 1), 0);
        fill(&mut v, id, old);
        tx.send((id, v)).unwrap();
        sent += 1;
    }
    drop(tx);
    let got = dropper.join().unwrap();
    assert_eq!(got, sent, "every large buffer reached the dropping thread");
}
