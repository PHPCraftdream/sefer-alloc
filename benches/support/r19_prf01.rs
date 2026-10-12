//! PRF01 workload bodies shared by thin iai wrappers and the oracle test.
#![allow(unsafe_code)] // Seam: uniquely owned GlobalAlloc blocks, exact layouts and exposed-provenance handoff.

use sefer_alloc::SeferAlloc;
use std::alloc::{GlobalAlloc, Layout};
use std::hint::black_box;

#[cfg(r19_sidecar_summary_oracle)]
std::thread_local! {
    static GEOMETRY: std::cell::RefCell<(std::collections::BTreeSet<u64>, Vec<serde_json::Value>)> = const {
        std::cell::RefCell::new((std::collections::BTreeSet::new(), Vec::new()))
    };
}

/// One responsibility: fixed PRF01 workloads; never installs a global allocator.
pub struct R19Prf01 {
    allocator: SeferAlloc,
    addresses: Vec<usize>,
    layout: Layout,
}

impl R19Prf01 {
    #[cfg_attr(test, allow(dead_code))] // Shared test exercises discovery geometries, not the expensive growth axis.
    pub fn growth16(n: usize) {
        assert!([32_768, 65_536, 131_072].contains(&n));
        let mut owner = Self::allocate(n, 16);
        #[cfg(r19_sidecar_summary_oracle)]
        Self::observe_routes(
            "growth16",
            "final-live-batch-before-free",
            0,
            owner.layout.size(),
            owner.addresses.iter().copied(),
        );
        black_box(&owner.addresses);
        // All allocations are complete before ANY free; Drop owns teardown.
        owner.free_owned();
    }

    #[cfg(r19_sidecar_summary_oracle)]
    pub fn multiseg(rounds: usize) {
        assert!([2, 6].contains(&rounds));
        let allocator = SeferAlloc::new();
        let layout = Layout::from_size_align(258_752, 8).unwrap();
        for round in 0..rounds {
            let mut blocks = [core::ptr::null_mut(); 34];
            for block in &mut blocks {
                // SAFETY: nonzero valid layout; allocator remains alive through every free.
                *block = unsafe { allocator.alloc(layout) };
                assert!(!block.is_null());
            }
            #[cfg(r19_sidecar_summary_oracle)]
            Self::observe_routes(
                if rounds == 2 {
                    "multiseg_cold_256k"
                } else {
                    "seg_cycle_decommit_256k"
                },
                "live-batch-before-free",
                round,
                layout.size(),
                blocks.iter().map(|ptr| ptr.addr()),
            );
            black_box(&blocks);
            for block in blocks {
                // SAFETY: unique live mapped allocation from allocator, exact original Layout; freed once.
                unsafe { allocator.dealloc(block, layout) };
            }
        }
    }

    pub fn producer_setup() -> Self {
        use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
        let mut owner = Self::allocate(8192, 16);
        // Ground truth is the pinned descriptor incarnation, NOT address/4MiB.
        // A recycled owner may issue from several retained segment free lists.
        let mut groups = std::collections::BTreeMap::<u64, Vec<usize>>::new();
        for &address in &owner.addresses {
            let key = core::ptr::with_exposed_provenance_mut(address);
            let pin = RouteDirectory::global()
                .lookup(key)
                .expect("issued allocation route missing");
            assert!(matches!(
                pin.kind(),
                RouteKind::Small | RouteKind::Primordial
            ));
            assert!(pin.contains_payload(key, 16));
            groups.entry(pin.incarnation()).or_default().push(address);
        }
        #[cfg(all(test, r19_sidecar_summary_oracle))]
        for (incarnation, addresses) in &groups {
            let key = core::ptr::with_exposed_provenance_mut(addresses[0]);
            // SAFETY: exact live allocation start in this owner thread; all
            // candidates remain owned and mapped, no conflicting owner borrow
            // or retirement occurs while this existing diagnostic reads its root.
            let (_, _, root) = unsafe { owner.allocator.dbg_current_reservation_for_test(key) }
                .expect("owned canonical route root missing");
            let min = *addresses.iter().min().unwrap();
            let max = *addresses.iter().max().unwrap();
            println!("{{\"producer_route_incarnation\":{incarnation},\"canonical_root\":{root},\"issued_count\":{},\"min_address\":{min},\"max_address\":{max},\"min_offset\":{},\"max_offset\":{}}}", addresses.len(), min.checked_sub(root).unwrap(), max.checked_sub(root).unwrap());
        }
        let selected = groups
            .values()
            .find(|addresses| addresses.len() >= 4096)
            .expect("bounded 8192 candidates did not provide 4096 allocations on ONE actual route");
        let selected: std::collections::BTreeSet<_> = selected.iter().take(4096).copied().collect();
        let mut retained = Vec::with_capacity(4096);
        for address in core::mem::take(&mut owner.addresses) {
            if selected.contains(&address) {
                retained.push(address);
            } else {
                let block = core::ptr::with_exposed_provenance_mut(address);
                // SAFETY: unique live candidate, retained exposed provenance and
                // exact original Layout; excluded setup retires this obligation once.
                unsafe { owner.allocator.dealloc(block, owner.layout) };
            }
        }
        owner.addresses = retained;
        #[cfg(r19_sidecar_summary_oracle)]
        Self::observe_routes(
            "producer",
            "selected-live-batch-before-handoff",
            0,
            owner.layout.size(),
            owner.addresses.iter().copied(),
        );
        assert_eq!(owner.addresses.len(), 4096);
        assert!(
            owner
                .addresses
                .windows(2)
                .any(|a| a[0] / 1024 == a[1] / 1024),
            "selected actual route lacks repeated pending-word traffic"
        );
        owner
    }

    pub fn producer_window(mut self) {
        self.remote_free();
    }

    pub fn publication_setup() -> Self {
        let mut owner = Self::allocate(34, 258_752);
        #[cfg(r19_sidecar_summary_oracle)]
        Self::observe_routes(
            "publication",
            "prepared-live-batch-before-handoff",
            0,
            owner.layout.size(),
            owner.addresses.iter().copied(),
        );
        owner.remote_free();
        owner
    }

    pub fn publication_window(self) {
        // Same 2x34 owner region as R18; joined publications precede discovery.
        // No separate allocator instance/heap is substituted here.
        for _round in 0..2 {
            let mut blocks = [core::ptr::null_mut(); 34];
            for block in &mut blocks {
                // SAFETY: valid layout; same retained allocator owns all blocks through teardown.
                *block = unsafe { self.allocator.alloc(self.layout) };
                assert!(!block.is_null());
            }
            #[cfg(r19_sidecar_summary_oracle)]
            Self::observe_routes(
                "publication",
                "discovery-live-batch-before-free",
                _round,
                self.layout.size(),
                blocks.iter().map(|ptr| ptr.addr()),
            );
            black_box(&blocks);
            for block in blocks {
                // SAFETY: unique live mapped allocation and exact original layout, freed exactly once.
                unsafe { self.allocator.dealloc(block, self.layout) };
            }
        }
    }

    // Checkpoints inspect only the caller's still-owned live batch. No process
    // census, address bucket or registration attempt substitutes for route pins.
    #[cfg(r19_sidecar_summary_oracle)]
    fn observe_routes(
        name: &str,
        phase: &str,
        round: usize,
        block_bytes: usize,
        addresses: impl Iterator<Item = usize>,
    ) {
        use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
        let mut counts = std::collections::BTreeMap::<u64, usize>::new();
        let mut block_count = 0usize;
        for address in addresses {
            let ptr = core::ptr::with_exposed_provenance_mut::<u8>(address);
            let pin = RouteDirectory::global()
                .lookup(ptr)
                .expect("live workload route missing");
            assert!(matches!(
                pin.kind(),
                RouteKind::Small | RouteKind::Primordial
            ));
            assert!(pin.contains_payload(ptr, block_bytes));
            let incarnation = pin.incarnation();
            assert_ne!(incarnation, 0);
            let count = counts.entry(incarnation).or_default();
            *count = count
                .checked_add(1)
                .expect("incarnation block count overflow");
            block_count = block_count
                .checked_add(1)
                .expect("live block count overflow");
        }
        assert!(block_count > 0);
        let incarnations: Vec<_> = counts.keys().copied().collect();
        let per_incarnation_counts: Vec<_> = counts.into_iter().map(|(incarnation, count)| {
            serde_json::json!({"incarnation":incarnation,"blockCount":count})
        }).collect();
        GEOMETRY.with(|geometry| {
            let mut geometry = geometry.borrow_mut();
            geometry.0.extend(incarnations.iter().copied());
            geometry.1.push(serde_json::json!({
                "name":name,"phase":phase,"point":phase,"round":round,"roundIndexBase":0,
                "blockCount":block_count,"blockBytes":block_bytes,
                "incarnations":incarnations,"perIncarnationCounts":per_incarnation_counts,
                "method":Self::segment_count_method(),
            }));
        });
    }

    #[cfg(r19_sidecar_summary_oracle)]
    pub fn segment_count_method() -> &'static str {
        "cardinality of union of live Small/Primordial RouteDirectory pin.incarnation() sets at retained checkpoints; every block checked by contains_payload(ptr, blockBytes) while owned; not peak concurrency, process census or registration attempts"
    }

    #[cfg(r19_sidecar_summary_oracle)]
    #[cfg_attr(test, allow(dead_code))]
    pub fn observed_geometry() -> serde_json::Value {
        GEOMETRY.with(|geometry| {
            let geometry = geometry.borrow();
            let mut checkpoint_union = std::collections::BTreeSet::new();
            for checkpoint in &geometry.1 {
                let incarnations: std::collections::BTreeSet<_> = checkpoint["incarnations"]
                    .as_array()
                    .expect("checkpoint incarnations missing")
                    .iter()
                    .map(|id| id.as_u64().expect("invalid incarnation"))
                    .collect();
                assert_eq!(
                    incarnations.len(),
                    checkpoint["incarnations"].as_array().unwrap().len()
                );
                let mut counted_incarnations = std::collections::BTreeSet::new();
                let mut blocks = 0u64;
                for count in checkpoint["perIncarnationCounts"]
                    .as_array()
                    .expect("checkpoint counts missing")
                {
                    let incarnation = count["incarnation"]
                        .as_u64()
                        .expect("invalid counted incarnation");
                    let count = count["blockCount"].as_u64().expect("invalid block count");
                    assert!(incarnation > 0 && count > 0);
                    assert!(counted_incarnations.insert(incarnation));
                    blocks = blocks
                        .checked_add(count)
                        .expect("checkpoint count overflow");
                }
                assert_eq!(counted_incarnations, incarnations);
                assert_eq!(Some(blocks), checkpoint["blockCount"].as_u64());
                checkpoint_union.extend(incarnations);
            }
            assert_eq!(
                checkpoint_union, geometry.0,
                "checkpoint union differs from observed union"
            );
            serde_json::json!({
                "segmentCount":geometry.0.len(),"incarnations":geometry.0,
                "checkpoints":geometry.1,"segmentCountMethod":Self::segment_count_method(),
            })
        })
    }

    #[cfg(r19_sidecar_summary_oracle)]
    #[cfg_attr(test, allow(dead_code))]
    pub fn observed_segment_count() -> usize {
        usize::try_from(Self::observed_geometry()["segmentCount"].as_u64().unwrap())
            .expect("segment count does not fit usize")
    }

    fn allocate(n: usize, bytes: usize) -> Self {
        let mut owner = Self {
            allocator: SeferAlloc::new(),
            addresses: Vec::with_capacity(n),
            layout: Layout::from_size_align(bytes, 8).unwrap(),
        };
        for _ in 0..n {
            // SAFETY: valid nonzero layout; owner retains allocator and each live allocation until free/join.
            let block = unsafe { owner.allocator.alloc(owner.layout) };
            assert!(!block.is_null());
            owner.addresses.push(block.expose_provenance());
        }
        owner
    }

    fn remote_free(&mut self) {
        let addresses = core::mem::take(&mut self.addresses);
        let allocator = &self.allocator;
        let layout = self.layout;
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    Self::producer_free_window(allocator, &addresses, layout);
                })
                .join()
                .unwrap();
        });
        #[cfg(r19_prf01_producer_control)]
        for &address in &addresses {
            let block = core::ptr::with_exposed_provenance_mut(address);
            // SAFETY: control child removed every free; the joined owner retains
            // all live allocations and retires each once outside the child boundary.
            unsafe { self.allocator.dealloc(block, layout) };
        }
        drop(addresses);
    }

    #[inline(never)]
    fn producer_free_window(allocator: &SeferAlloc, addresses: &[usize], layout: Layout) {
        // Amendment B: identical unit-valued anti-DCE anchor in both variants.
        // Symbol/call retention and overhead require nm, disassembly and profiles.
        std::hint::black_box(());
        #[cfg(r19_prf01_producer_control)]
        let _ = (allocator, addresses, layout);
        #[cfg(not(r19_prf01_producer_control))]
        for &address in addresses {
            let block = core::ptr::with_exposed_provenance_mut(address);
            // SAFETY: private caller transfers the only free obligation for each
            // live mapped allocation, with retained exposed provenance and exact
            // original layout. Owner allocator stays alive until explicit join.
            unsafe { allocator.dealloc(block, layout) };
        }
    }

    fn free_owned(&mut self) {
        for address in self.addresses.drain(..) {
            let block = core::ptr::with_exposed_provenance_mut(address);
            // SAFETY: retained exposed provenance, uniquely owned live allocation;
            // exact original layout and allocator, obligation removed once by drain.
            unsafe { self.allocator.dealloc(block, self.layout) };
        }
    }
}

impl Drop for R19Prf01 {
    fn drop(&mut self) {
        self.free_owned();
    }
}
