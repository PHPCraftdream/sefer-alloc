//! `ghost-lint check` over this repository: every catalog rule holds on the tree.
//! The EV-1 evidence recipes run this test as their baseline and again after a
//! production mutant, which must turn it red with the rule's id in the output.

use std::process::Command;

#[test]
fn catalog_rules_hold_on_the_repository() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let out = Command::new(env!("CARGO_BIN_EXE_ghost-lint"))
        .args(["check", "--root", root])
        .output()
        .expect("run ghost-lint");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    println!("{stdout}{stderr}");
    assert!(
        out.status.success(),
        "ghost-lint check found violations:\n{stdout}{stderr}"
    );
    assert!(
        stdout.contains(" 0 violation(s)"),
        "unexpected check summary:\n{stdout}"
    );
}
