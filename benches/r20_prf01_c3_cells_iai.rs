//! P1-4 C''' producer cells. Setup and geometry checks happen before each
//! measured function; only producer_free_window is Callgrind-collected.

#[cfg(target_os = "linux")]
mod gate {
    use iai_callgrind::{
        library_benchmark, library_benchmark_group, main, EntryPoint, LibraryBenchmarkConfig,
    };
    use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
    use sefer_alloc::{SeferAlloc, SegmentLayout};
    use std::alloc::{GlobalAlloc, Layout};
    use std::hint::black_box;

    const COUNT: usize = 4096;

    struct Prepared {
        allocator: SeferAlloc,
        addresses: Vec<usize>,
        layout: Layout,
    }

    fn prepare_distinct() -> Prepared {
        let allocator = SeferAlloc::new();
        let layout = Layout::from_size_align(1024, 8).unwrap();
        let mut addresses = Vec::with_capacity(COUNT);
        let mut indices = std::collections::BTreeSet::new();
        let mut incarnations = std::collections::BTreeSet::new();
        for _ in 0..COUNT {
            // SAFETY: valid layout; allocation remains live until producer join.
            let p = unsafe { allocator.alloc(layout) };
            assert!(!p.is_null());
            let address = p.expose_provenance();
            let key = core::ptr::with_exposed_provenance_mut(address);
            let pin = RouteDirectory::global()
                .lookup(key)
                .expect("live route absent");
            assert!(matches!(
                pin.kind(),
                RouteKind::Small | RouteKind::Primordial
            ));
            assert!(pin.contains_payload(key, layout.size()));
            // SAFETY: exact live owner allocation; no owner mutation/retirement during snapshot.
            let (_, _, root) = unsafe { allocator.dbg_current_reservation_for_test(key) }
                .expect("canonical owner reservation unavailable");
            let offset = address
                .checked_sub(root)
                .expect("allocation before canonical root");
            assert_eq!(offset % 1024, 0, "1 KiB allocation is not granule aligned");
            let pending_index = offset / 1024;
            assert!(
                indices.insert((pin.incarnation(), pending_index)),
                "duplicate pending word geometry"
            );
            incarnations.insert(pin.incarnation());
            addresses.push(address);
        }
        assert_eq!(indices.len(), COUNT);
        assert!(!incarnations.is_empty());
        Prepared {
            allocator,
            addresses,
            layout,
        }
    }

    fn prepare_large() -> Prepared {
        // Exceeding the source-defined largest Small class forces the Large
        // allocation branch; the actual descriptor kind is still asserted.
        let allocator = SeferAlloc::new();
        let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX + 1, 8).unwrap();
        // SAFETY: valid layout; allocation remains live through joined producer.
        let p = unsafe { allocator.alloc(layout) };
        assert!(!p.is_null());
        let address = p.expose_provenance();
        let pin = RouteDirectory::global()
            .lookup(p)
            .expect("Large route absent");
        assert_eq!(
            pin.kind(),
            RouteKind::Large,
            "requested size did not activate Large route"
        );
        assert!(pin.contains_payload(p, layout.size()));
        Prepared {
            allocator,
            addresses: vec![address],
            layout,
        }
    }

    #[inline(never)]
    fn producer_free_window(allocator: &SeferAlloc, addresses: &[usize], layout: Layout) {
        black_box(());
        #[cfg(r20_prf01_c3_producer_control)]
        {
            let _ = (allocator, addresses, layout);
        }
        #[cfg(not(r20_prf01_c3_producer_control))]
        for &address in addresses {
            let p = core::ptr::with_exposed_provenance_mut(address);
            // SAFETY: unique live allocation transferred to this joined producer; exact Layout retained.
            unsafe { allocator.dealloc(p, layout) };
        }
    }

    fn run_remote(prepared: Prepared) {
        let Prepared {
            allocator,
            addresses,
            layout,
        } = prepared;
        std::thread::scope(|scope| {
            scope
                .spawn(|| producer_free_window(&allocator, &addresses, layout))
                .join()
                .unwrap();
        });
        #[cfg(r20_prf01_c3_producer_control)]
        for address in addresses {
            let p = core::ptr::with_exposed_provenance_mut(address);
            // SAFETY: control leaves each unique live block with owner, which retires it exactly once.
            unsafe { allocator.dealloc(p, layout) };
        }
    }
    fn setup_distinct() -> Prepared {
        prepare_distinct()
    }
    fn setup_large() -> Prepared {
        prepare_large()
    }
    fn distinct_window(prepared: Prepared) {
        run_remote(prepared);
    }
    fn large_window(prepared: Prepared) {
        run_remote(prepared);
    }

    #[library_benchmark(
        setup = setup_distinct,
        config = LibraryBenchmarkConfig::default().entry_point(EntryPoint::Custom(
            "r20_prf01_c3_cells_iai::gate::producer_free_window".to_owned()
        ))
    )]
    fn r20_prf01_c3_producer_distinct_words(prepared: Prepared) {
        distinct_window(prepared);
    }

    #[library_benchmark(
        setup = setup_large,
        config = LibraryBenchmarkConfig::default().entry_point(EntryPoint::Custom(
            "r20_prf01_c3_cells_iai::gate::producer_free_window".to_owned()
        ))
    )]
    fn r20_prf01_c3_remote_large_free(prepared: Prepared) {
        large_window(prepared);
    }

    library_benchmark_group!(name = r20_prf01_c3_cells; benchmarks =
        r20_prf01_c3_producer_distinct_words, r20_prf01_c3_remote_large_free);
    main!(library_benchmark_groups = r20_prf01_c3_cells);
    pub fn run() {
        main();
    }
}

#[cfg(target_os = "linux")]
fn main() {
    gate::run();
}
#[cfg(not(target_os = "linux"))]
fn main() {}
