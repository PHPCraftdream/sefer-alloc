//! Ph4b (task #2092, chunk B): source pin for the owner decision "LOCAL
//! without Drop". `tests/loom_r11_ph4b_publish_recycle_drain.rs` is a shadow
//! model and cannot see a weakened production declaration, so this test pins
//! the `src/global/tls_heap.rs` source itself:
//! (a) `LOCAL` is declared `Cell<*mut HeapCore>` with a `const` initializer
//!     (no destructor registration, no lease stored in it);
//! (b) `AbandonGuard` stores the typed lease as `Cell<Option<HeapLease>>`;
//! (d) `finish_bind`'s rollback leg resets `LOCAL` to null BEFORE dropping
//!     the lease.
//! (The `Drop for AbandonGuard` teardown ordering is pinned by
//! `tests/r11_ph4b_teardown_ordering_pinned.rs` — one responsibility per file.)
//! Token-based, so reformatting cannot break it; a semantic regression does.
#![cfg(all(feature = "alloc-global", feature = "internals"))]

const TLS_SRC: &str = include_str!("../src/global/tls_heap.rs");

/// The body of the `thread_local! { ... }` block.
fn thread_local_block() -> &'static str {
    let start = TLS_SRC
        .find("thread_local! {")
        .expect("tls_heap declares its TLS cells in a thread_local! block");
    let rest = &TLS_SRC[start..];
    let end = rest.find("\n}\n").expect("end of the thread_local! block");
    &rest[..end]
}

/// The declaration of one `static` item inside the thread_local block, up to
/// (and including) its terminating semicolon.
fn static_declaration(block: &'static str, name: &str) -> &'static str {
    let marker = format!("static {name}:");
    let start = block
        .find(&marker)
        .unwrap_or_else(|| panic!("{name} must be declared in the thread_local! block"));
    let rest = &block[start..];
    let end = rest
        .find(';')
        .unwrap_or_else(|| panic!("{name} declaration is terminated"));
    &rest[..=end]
}

#[test]
fn local_is_a_const_initialised_dropless_raw_cell() {
    let declaration = static_declaration(thread_local_block(), "LOCAL");
    assert!(
        declaration.contains("Cell<*mut HeapCore>"),
        "LOCAL must stay a raw Cell<*mut HeapCore>: {declaration}"
    );
    assert!(
        declaration.contains("const {"),
        "LOCAL must be const-initialised so it registers no destructor \
         (regression pin: 'LOCAL without Drop'): {declaration}"
    );
    assert!(
        declaration.contains("Cell::new(core::ptr::null_mut())"),
        "LOCAL's const initializer must be the null pointer: {declaration}"
    );
    assert!(
        !declaration.contains("Option<HeapLease>"),
        "the lease must live in GUARD, never in LOCAL: {declaration}"
    );
    assert!(
        !declaration.contains("RefCell"),
        "LOCAL must keep no borrow state (reentrancy): {declaration}"
    );
}

#[test]
fn guard_stores_the_typed_lease_option() {
    let start = TLS_SRC
        .find("struct AbandonGuard")
        .expect("AbandonGuard is the thread-exit guard");
    let rest = &TLS_SRC[start..];
    let end = rest.find("\n}\n").expect("end of the AbandonGuard struct");
    let body = &rest[..end];
    assert!(
        body.contains("lease: Cell<Option<HeapLease>>"),
        "AbandonGuard must hold the typed HeapLease in a Cell<Option<_>> \
         (take-then-drop teardown, no borrow state): {body}"
    );
}

#[test]
fn finish_bind_rollback_unpublishes_local_before_the_lease_drop() {
    let start = TLS_SRC
        .find("fn finish_bind(")
        .expect("finish_bind is the shared post-claim path");
    let rest = &TLS_SRC[start..];
    let end = rest.find("\n}\n").expect("end of finish_bind");
    let body = &rest[..end];
    let rollback = body
        .find("if !armed {")
        .expect("finish_bind keeps a GUARD-arming rollback leg");
    let leg = &body[rollback..];
    let unpublish = leg
        .find("null_mut()")
        .expect("the rollback leg resets LOCAL to null");
    let lease_drop = leg
        .find("drop(lease)")
        .expect("the rollback leg releases the slot via drop(lease)");
    assert!(
        unpublish < lease_drop,
        "rollback order regression: LOCAL := null must precede drop(lease) \
         so a stale published pointer never outlives the released slot"
    );
}
