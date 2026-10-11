//! PRF01 workload bodies shared by thin iai wrappers and the oracle test.
#![allow(unsafe_code)] // Seam: uniquely owned GlobalAlloc blocks, exact layouts and exposed-provenance handoff.

use sefer_alloc::SeferAlloc;
use std::alloc::{GlobalAlloc, Layout};
use std::hint::black_box;

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
        black_box(&owner.addresses);
        // All allocations are complete before ANY free; Drop owns teardown.
        owner.free_owned();
    }

    #[cfg(all(test, r19_sidecar_summary_oracle))]
    pub fn multiseg(rounds: usize) {
        assert!([2, 6].contains(&rounds));
        let allocator = SeferAlloc::new();
        let layout = Layout::from_size_align(258_752, 8).unwrap();
        for _ in 0..rounds {
            let mut blocks = [core::ptr::null_mut(); 34];
            for block in &mut blocks {
                // SAFETY: nonzero valid layout; allocator remains alive through every free.
                *block = unsafe { allocator.alloc(layout) };
                assert!(!block.is_null());
            }
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
        owner.remote_free();
        owner
    }

    pub fn publication_window(self) {
        // Same 2x34 owner region as R18; joined publications precede discovery.
        // No separate allocator instance/heap is substituted here.
        for _ in 0..2 {
            let mut blocks = [core::ptr::null_mut(); 34];
            for block in &mut blocks {
                // SAFETY: valid layout; same retained allocator owns all blocks through teardown.
                *block = unsafe { self.allocator.alloc(self.layout) };
                assert!(!block.is_null());
            }
            black_box(&blocks);
            for block in blocks {
                // SAFETY: unique live mapped allocation and exact original layout, freed exactly once.
                unsafe { self.allocator.dealloc(block, self.layout) };
            }
        }
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
                .spawn(move || {
                    Self::producer_free_window(allocator, &addresses, layout);
                    // Vec teardown is deliberately outside the custom child boundary.
                    drop(addresses);
                })
                .join()
                .unwrap();
        });
    }

    #[inline(never)]
    fn producer_free_window(allocator: &SeferAlloc, addresses: &[usize], layout: Layout) {
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
