//! Ghost-check infrastructure self-test (docs/GHOST_CHECKS_PLAN.md §3, phase G1):
//! a passing check records its event in the shadow table, a failing one panics
//! naming its rule and message, and `reset` clears the table. Ghost builds only:
//! `RUSTFLAGS="--cfg sefer_ghost" cargo test --features internals --test ghost_infra`.
#![cfg(all(feature = "internals", any(loom, miri, sefer_ghost)))]

use sefer_alloc::ghost;

fn panic_text(p: Box<dyn std::any::Any + Send>) -> String {
    p.downcast_ref::<String>()
        .cloned()
        .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

// One test: the shadow table is process-global, so parallel tests would race on `reset`.
#[test]
fn ghost_macros_record_fail_and_reset() {
    ghost::reset();
    assert_eq!(ghost::count("ghost/probe", 7), 0);
    ghost::probe(true, 7);
    ghost::probe(true, 7);
    ghost::probe(true, 8);
    assert_eq!(ghost::count("ghost/probe", 7), 2, "two events under key 7");
    assert_eq!(ghost::count("ghost/probe", 8), 1, "keys are separate");
    assert_eq!(ghost::count("ghost/other", 7), 0, "rules are separate");

    let r = std::panic::catch_unwind(|| ghost::probe(false, 9));
    let msg = panic_text(r.expect_err("a false ghost_assert! must panic"));
    assert!(
        msg.contains("ghost check ghost/probe failed"),
        "panic names the rule: {msg}"
    );
    assert!(
        msg.contains("key 9"),
        "panic carries the formatted message: {msg}"
    );
    assert_eq!(
        ghost::count("ghost/probe", 9),
        1,
        "the event before the failing check was recorded"
    );

    assert_eq!(ghost::overflowed(), 0, "no event dropped");
    ghost::reset();
    assert_eq!(ghost::count("ghost/probe", 7), 0, "reset clears the table");

    // A zero delta changes no count and takes no slot (per-word scan events).
    for k in 0..2_000u64 {
        ghost::record("ghost/zero", k, 0);
    }
    assert_eq!(ghost::count("ghost/zero", 1), 0);
    assert_eq!(ghost::overflowed(), 0, "zero deltas take no slot");
    // More keys than slots: the excess is dropped and counted, never lost
    // silently, and each event costs a bounded probe.
    const KEYS: u64 = 1_100;
    for k in 0..KEYS {
        ghost::record("ghost/fill", k, 1);
    }
    let kept = (0..KEYS)
        .filter(|&k| ghost::count("ghost/fill", k) == 1)
        .count() as u64;
    assert!(ghost::overflowed() > 0, "a crowded table drops events");
    assert_eq!(
        kept + ghost::overflowed(),
        KEYS,
        "every event is kept or counted as dropped"
    );
    ghost::reset();
}
