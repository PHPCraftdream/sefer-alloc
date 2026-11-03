#![no_main]

use libfuzzer_sys::fuzz_target;
use sefer_alloc::SeferAlloc;
use std::sync::mpsc;
use std::thread;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const SLOTS: usize = 8;
const MAX_LEN: usize = 4096;
const MAX_OPS: usize = 256;

fn pattern(id: u64, i: usize) -> u8 {
    let x = id.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (i as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    (x >> 29) as u8
}

struct Item {
    id: u64,
    bytes: Vec<u8>,
}
impl Item {
    fn verify(&self) {
        for (i, &byte) in self.bytes.iter().enumerate() {
            assert_eq!(byte, pattern(self.id, i), "id {} byte {i}", self.id);
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let (tx, rx) = mpsc::sync_channel::<Item>(SLOTS);
    let worker = thread::spawn(move || {
        let mut retired = Vec::new();
        while let Ok(item) = rx.recv() {
            item.verify();
            let id = item.id;
            drop(item);
            retired.push(id);
        }
        retired
    });
    let mut slots: Vec<Option<Item>> = (0..SLOTS).map(|_| None).collect();
    let mut created = Vec::new();
    let mut local_retired = Vec::new();
    for (op, chunk) in data.chunks_exact(4).take(MAX_OPS).enumerate() {
        let slot = usize::from(chunk[1]) % SLOTS;
        let requested = usize::from(u16::from_le_bytes([chunk[2], chunk[3]]));
        match chunk[0] % 4 {
            0 if slots[slot].is_none() => {
                let id = op as u64 + 1;
                let size = requested.min(MAX_LEN);
                created.push(id);
                slots[slot] = Some(Item {
                    id,
                    bytes: (0..size).map(|i| pattern(id, i)).collect(),
                });
            }
            1 if slots[slot].is_some() => {
                let item = slots[slot].as_mut().unwrap();
                item.verify();
                let old = item.bytes.len();
                let extra = requested.min(MAX_LEN - old);
                item.bytes.reserve(extra);
                item.bytes
                    .extend((old..old + extra).map(|i| pattern(item.id, i)));
                item.verify();
            }
            2 if slots[slot].is_some() => {
                let item = slots[slot].take().unwrap();
                let id = item.id;
                item.verify();
                drop(item);
                local_retired.push(id);
            }
            3 if slots[slot].is_some() => {
                let item = slots[slot].take().unwrap();
                item.verify();
                tx.send(item).expect("worker closed");
            }
            _ => {}
        }
    }
    for item in slots.into_iter().flatten() {
        let id = item.id;
        item.verify();
        drop(item);
        local_retired.push(id);
    }
    drop(tx);
    let retired = match worker.join() {
        Ok(retired) => retired,
        Err(payload) => std::panic::resume_unwind(payload),
    };
    let mut all_retired = retired;
    all_retired.extend(local_retired);
    all_retired.sort_unstable();
    let mut expected = created;
    expected.sort_unstable();
    assert_eq!(all_retired, expected, "missing or duplicate retirement");
    for pair in all_retired.windows(2) {
        assert_ne!(pair[0], pair[1], "duplicate retirement");
    }
});
