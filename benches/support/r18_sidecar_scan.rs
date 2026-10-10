use sefer_alloc::registry::HeapCore;
use sefer_alloc::SeferAlloc;
use std::alloc::{GlobalAlloc, Layout};

pub fn round(name: &str, rounds: usize, publish: bool) {
    measure(SeferAlloc::new(), name, rounds, publish);
}

// IAI evaluates this setup before entering its counted benchmark wrapper.
// A joined foreign producer publishes real GlobalAlloc deallocations; no
// producer creation, synchronization, or deallocation is in the owner window.
pub fn publication_setup() -> SeferAlloc {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(258_752, 8).unwrap();
    let mut addresses = [0usize; 34];
    for address in &mut addresses {
        // SAFETY: valid nonzero layout; ownership transfers to the producer.
        let block = unsafe { sefer.alloc(layout) };
        assert!(!block.is_null(), "r18 setup allocation failed");
        *address = block.expose_provenance();
    }
    std::thread::scope(|scope| {
        let allocator = &sefer;
        scope
            .spawn(move || {
                for address in addresses {
                    let block = core::ptr::with_exposed_provenance_mut(address);
                    // SAFETY: unique live block and exact original layout.
                    unsafe { allocator.dealloc(block, layout) };
                }
            })
            .join()
            .unwrap();
    });
    sefer
}

pub fn publication_owner_window(sefer: SeferAlloc) {
    measure(sefer, "r18_scan_publication", 2, true);
}

fn measure(sefer: SeferAlloc, name: &str, rounds: usize, publish: bool) {
    let layout = Layout::from_size_align(258_752, 8).unwrap();
    let oracle = std::env::var("SEFER_R18_SCAN_ORACLE").as_deref() == Ok("1");
    let baseline = match std::env::var("SEFER_R18_SCAN_MODE").as_deref() {
        Ok("base") => true,
        Ok("candidate") => false,
        _ => panic!("r18 scan mode missing"),
    };
    HeapCore::dbg_sidecar_scan_measurement(oracle, baseline);
    for _ in 0..rounds {
        let mut blocks = [core::ptr::null_mut(); 34];
        for block in &mut blocks {
            // SAFETY: nonzero valid layout; each returned block is freed once below.
            *block = unsafe { sefer.alloc(layout) };
            assert!(!block.is_null(), "r18 allocation failed");
        }
        for block in blocks {
            // SAFETY: unique allocation from this allocator with this exact layout.
            unsafe { sefer.dealloc(block, layout) };
        }
    }
    let counts = HeapCore::dbg_sidecar_scan_measurement(false, true);
    if oracle {
        assert!(
            counts[4] > 0,
            "r18 allocation-discovery routed scan inactive"
        );
        assert!(counts[0] > 0 && counts[1] > 0, "r18 empty scan inactive");
        if baseline {
            assert_eq!(counts[0], counts[2], "baseline must exchange every word");
        } else {
            assert_eq!(
                counts[0],
                counts[1] + counts[2],
                "candidate must skip empty words"
            );
            assert_eq!(
                counts[2], counts[3],
                "single owner must exchange nonempty words"
            );
        }
        if publish {
            assert!(counts[3] > 0, "r18 publication drain inactive");
        }
    }
    if oracle {
        let path = std::env::var_os("SEFER_R18_SAMPLES").expect("r18 oracle sample path missing");
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        writeln!(file, "{{\"name\":\"{name}\",\"layer\":\"SeferAlloc::GlobalAlloc->HeapCore\",\"rounds\":{rounds},\"publish\":{publish},\"baseline\":{baseline},\"oracle\":{oracle},\"visited\":{},\"empty_skipped\":{},\"exchanges\":{},\"nonempty_exchanges\":{},\"routed_scans\":{}}}", counts[0], counts[1], counts[2], counts[3], counts[4]).unwrap();
    }
}
