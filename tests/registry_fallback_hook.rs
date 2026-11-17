//! Focused source-including tests for the safe fallback hook boundary.
#![allow(dead_code)]

mod registry {
    pub struct HeapCore {
        pub value: u32,
    }

    pub(crate) mod fallback_hook {
        include!("../src/registry/fallback_hook.rs");
    }
}

use registry::fallback_hook::FallbackHook;
use registry::HeapCore;

fn registered_callback(f: &mut dyn FnMut(&mut HeapCore)) -> bool {
    let mut heap = HeapCore { value: 11 };
    f(&mut heap);
    heap.value == 18
}

fn unavailable_callback(_f: &mut dyn FnMut(&mut HeapCore)) -> bool {
    false
}

#[test]
fn absent_hook_does_not_invoke_the_closure() {
    let hook = FallbackHook::new();
    let mut called = false;
    let result = hook.try_with_heap(&mut |_| called = true);

    assert!(!result);
    assert!(!called);
}

#[test]
fn registered_hook_invokes_exclusive_closure_and_reports_its_result() {
    let hook = FallbackHook::new();
    assert!(hook.register(registered_callback));

    let mut observed_heap_result = 0;
    let result = hook.try_with_heap(&mut |heap| {
        heap.value += 7;
        observed_heap_result = heap.value;
    });

    assert!(result);
    assert_eq!(observed_heap_result, 18);
    assert!(!hook.register(unavailable_callback));
}

#[test]
fn unavailable_fallback_does_not_invoke_closure_or_initialize() {
    let hook = FallbackHook::new();
    assert!(hook.register(unavailable_callback));

    let mut called = false;
    let result = hook.try_with_heap(&mut |_| called = true);

    assert!(!result);
    assert!(!called);
}

#[test]
fn facade_registers_the_hook_before_ready_publication() {
    let source = include_str!("../src/global/fallback.rs");
    let registration = source
        .find(".register(fallback_hook_callback)")
        .expect("facade hook registration must remain present");
    let ready_publication = source
        .find("INIT_STATE.store(FallbackInitState::Ready as u8, Ordering::Release)")
        .expect("fallback READY publication must remain present");
    assert!(registration < ready_publication);

    let callback = source
        .find("fn fallback_hook_callback(f: &mut dyn FnMut(&mut HeapCore)) -> bool {")
        .expect("facade callback must remain present");
    let callback_end = source[callback..].find("\n}").map(|end| callback + end);
    let callback_body = &source[callback..callback_end.expect("callback body")];
    assert!(callback_body.contains("try_with_heap(|heap| f(heap)).is_some()"));
}
