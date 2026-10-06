//! R13-05 (src review round 13) — `ShardGuard` auto-trait regression oracles.
//!
//! The guard derefs to `&T`, so a shared guard requires `T: Sync`, but its
//! lone `&'a ShardLock<T>` field auto-derived `Sync` under `T: Send` alone.
//! The `PhantomData<&'a mut T>` marker pins the contract: guard `Send`
//! requires `T: Send`, guard `Sync` additionally requires `T: Sync`, and
//! `ShardLock<T>` itself stays `Send + Sync` under `T: Send` only.
//!
//! `ShardLock`/`ShardGuard` are `pub(super)` internals, so every oracle
//! includes the actual private source by path (the `#[path]` convention of
//! `tests/r6_sidecar_bitmap.rs`). The negative case lives in
//! `tests/compile_fail/shard_guard_send_only_payload_not_sync/` and is
//! asserted by `send_only_payload_guard_shared_between_threads_must_not_compile`.

#[path = "../src/registry/segment_route/shard_lock.rs"]
mod shard_lock;

use core::cell::Cell;
use shard_lock::{ShardGuard, ShardLock};

fn require_sync<T: ?Sized + Sync>(_: &T) {}
fn require_send_sync<T: Send + Sync>() {}

/// Positive retention: a `Sync` payload keeps both guard auto traits and the
/// lock keeps `Send + Sync`. The guard is pinned `Sync` by value, then moved
/// to another thread, which mutates through `DerefMut` and releases the
/// lock — the mutate → release → read invariant of the spin lock.
#[test]
fn sync_payload_guard_remains_sync_and_send() {
    require_send_sync::<ShardLock<u32>>();
    require_send_sync::<ShardGuard<'static, u32>>();

    let lock = ShardLock::new(0u32);
    let mut guard = lock.lock();
    require_sync(&guard);
    std::thread::scope(|scope| {
        scope.spawn(move || {
            *guard = 7;
        });
    });
    assert_eq!(*lock.lock(), 7);
}

/// Positive retention: a `Send + !Sync` payload stays legal inside the lock.
/// `ShardLock<Cell<u32>>` remains `Send + Sync`, so `&lock` may be shared
/// across threads while each thread acquires and uses the guard only on the
/// thread that holds it. The two acquire → mutate → release rounds must
/// serialize: the second holder observes the first holder's released write.
#[test]
fn send_only_payload_stays_legal_inside_the_lock() {
    require_send_sync::<ShardLock<Cell<u32>>>();

    let lock = ShardLock::new(Cell::new(0u32));
    std::thread::scope(|scope| {
        for _ in 0..2 {
            scope.spawn(|| {
                let guard = lock.lock();
                guard.set(guard.get() + 1);
            });
        }
    });
    assert_eq!(lock.lock().get(), 2);
}

/// A moved guard keeps `Send` under `T: Send` alone — `Cell<u32>` is
/// `Send + !Sync` and the receiving thread is its sole owner while the lock
/// is held. Mutate on the owner thread, release, read back on the parent.
#[test]
fn moved_guard_of_send_only_payload_is_usable_on_its_owner_thread() {
    let lock = ShardLock::new(Cell::new(0u32));
    let guard = lock.lock();
    std::thread::scope(|scope| {
        scope.spawn(move || {
            guard.set(5);
        });
    });
    assert_eq!(lock.lock().get(), 5);
}

/// Negative compile-fail oracle: compiles the fixture (the actual
/// shard_lock.rs plus a `Sync` requirement on an acquired
/// `ShardGuard<'_, Cell<u32>>`) with process-local rustc and requires
/// exactly one E0277 whose diagnostic names `Sync` and `Cell` and whose
/// primary span sits in the fixture's own file. Removing the
/// `PhantomData<&'a mut T>` marker lets the fixture compile and fails this
/// test.
#[test]
#[cfg(not(miri))]
fn send_only_payload_guard_shared_between_threads_must_not_compile() {
    let as_str = serde_json::Value::as_str;
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/compile_fail/shard_guard_send_only_payload_not_sync/src/main.rs");
    assert!(
        fixture.is_file(),
        "compile-fail fixture missing from checkout: {}",
        fixture.display()
    );
    let out_dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("shard_guard_send_only_payload_not_sync");
    std::fs::create_dir_all(&out_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", out_dir.display()));
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    let output = std::process::Command::new(rustc)
        .arg(&fixture)
        .args(["--edition=2021", "--emit=metadata", "--error-format=json"])
        .arg("--out-dir")
        .arg(&out_dir)
        .output()
        .expect("failed to spawn rustc for the R13-05 fixture");

    let context = format!(
        "fixture: {}\nstatus: {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        fixture.display(),
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !output.status.success(),
        "the R13-05 fixture COMPILED — ShardGuard<Cell<u32>> satisfies `Sync` \
         again (the marker in shard_lock.rs regressed):\n{context}"
    );

    let errors: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .unwrap_or_else(|error| panic!("invalid rustc JSON line {line:?}: {error}"))
        })
        .filter(|diagnostic| {
            diagnostic.get("level").and_then(as_str) == Some("error")
                && diagnostic.pointer("/code/code").and_then(as_str).is_some()
        })
        .collect();
    assert_eq!(
        errors.len(),
        1,
        "expected exactly one error diagnostic for the unsatisfied `Sync` \
         bound, got {}:\n{context}",
        errors.len()
    );
    let diagnostic = &errors[0];
    assert_eq!(
        diagnostic.pointer("/code/code").and_then(as_str),
        Some("E0277"),
        "the fixture must fail on the unsatisfied `Sync` bound, not some \
         other error:\n{context}"
    );
    let rendered = diagnostic
        .get("rendered")
        .and_then(as_str)
        .unwrap_or_default();
    assert!(
        rendered.contains("Sync") && rendered.contains("Cell"),
        "the diagnostic must name the `Sync` trait and the `Cell` payload \
         type:\n{rendered}\n{context}"
    );

    let primary = diagnostic
        .pointer("/spans")
        .and_then(serde_json::Value::as_array)
        .expect("diagnostic carries a spans array")
        .iter()
        .find(|span| span.get("is_primary").and_then(serde_json::Value::as_bool) == Some(true))
        .unwrap_or_else(|| panic!("no primary span in the diagnostic:\n{context}"));
    assert!(
        primary
            .get("file_name")
            .and_then(as_str)
            .unwrap_or_default()
            .replace('\\', "/")
            .ends_with("shard_guard_send_only_payload_not_sync/src/main.rs"),
        "the primary span must sit in the fixture's own main.rs:\n{context}"
    );
}
