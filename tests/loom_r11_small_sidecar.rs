//! R11 reduced mixed-leaf publication and pin-lifetime model.
#![cfg(loom)]
#![allow(unsafe_code)]

use loom::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use loom::sync::Arc;
use loom::thread;

struct Route {
    leaf: AtomicPtr<AtomicU8>,
    pending: AtomicU64,
    refs: AtomicUsize,
    freed: AtomicBool,
}

fn model_route(early_free: bool) {
    loom::model(move || {
        let route = Arc::new(Route {
            leaf: AtomicPtr::new(core::ptr::null_mut()),
            pending: AtomicU64::new(0),
            refs: AtomicUsize::new(2), // registration plus producer pin
            freed: AtomicBool::new(false),
        });
        let leaf = Arc::new(AtomicU8::new(4));
        route
            .leaf
            .store(Arc::as_ptr(&leaf).cast_mut(), Ordering::Release);
        let producer_route = Arc::clone(&route);
        let producer = thread::spawn(move || {
            producer_route.pending.fetch_or(1, Ordering::AcqRel);
            thread::yield_now();
            assert!(
                !producer_route.freed.load(Ordering::Acquire),
                "leaf freed while pin alive"
            );
            if producer_route.refs.fetch_sub(1, Ordering::AcqRel) == 1 {
                producer_route.freed.store(true, Ordering::Release);
            }
        });
        let mut producer = Some(producer);
        let mut bits = route.pending.swap(0, Ordering::AcqRel);
        if bits == 0 {
            producer.take().unwrap().join().unwrap();
            bits = route.pending.swap(0, Ordering::AcqRel);
        }
        assert_eq!(bits, 1);
        let seen = route.leaf.load(Ordering::Acquire);
        assert!(!seen.is_null(), "terminal bit preceded leaf publication");
        // SAFETY: the model retains leaf's Arc through every cut and pin drop.
        assert_eq!(unsafe { &*seen }.load(Ordering::Acquire), 4);
        if early_free {
            route.freed.store(true, Ordering::Release);
        }
        if route.refs.fetch_sub(1, Ordering::AcqRel) == 1 {
            route.freed.store(true, Ordering::Release);
        }
        if let Some(producer) = producer {
            producer.join().unwrap();
        }
        assert!(route.freed.load(Ordering::Acquire));
    });
}

struct PublishedLeaf {
    code: AtomicU8,
    pointer: AtomicPtr<AtomicU8>,
}

fn model_visibility(relaxed_publication: bool) {
    loom::model(move || {
        let leaf = Arc::new(PublishedLeaf {
            code: AtomicU8::new(0),
            pointer: AtomicPtr::new(core::ptr::null_mut()),
        });
        let initialized = Arc::clone(&leaf);
        let writer = thread::spawn(move || {
            initialized.code.store(4, Ordering::Relaxed);
            initialized.pointer.store(
                core::ptr::from_ref(&initialized.code).cast_mut(),
                if relaxed_publication {
                    Ordering::Relaxed
                } else {
                    Ordering::Release
                },
            );
        });
        let seen = leaf.pointer.load(Ordering::Acquire);
        if !seen.is_null() {
            // SAFETY: leaf's retained Arc owns the initialized atomic; the
            // assertion models visibility, not the allocation's lifetime.
            assert_eq!(
                unsafe { &*seen }.load(Ordering::Relaxed),
                4,
                "mixed pointer exposed uninitialized class"
            );
        }
        // Joining only after observation prevents join from hiding ordering defects.
        writer.join().unwrap();
    });
}

#[test]
fn pointer_publication_and_last_pin_retention() {
    model_route(false);
    model_visibility(false);
}

#[test]
#[should_panic(expected = "mixed pointer exposed uninitialized class")]
fn negative_relaxed_pointer_publication() {
    model_visibility(true);
}

#[test]
#[should_panic(expected = "leaf freed while pin alive")]
fn negative_early_leaf_release() {
    model_route(true);
}
