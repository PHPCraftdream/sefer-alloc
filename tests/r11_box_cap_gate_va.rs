//! W+2-like VA reuse / incarnation (ABA) observation; System-only, not Sefer.

#[path = "support/r11_box_cap_reissue.rs"]
mod gate;

#[global_allocator]
static GLOBAL: gate::Gate = gate::Gate;

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("va-overlap") => gate::run(gate::Scenario::VaOverlap),
        _ => std::process::exit(2),
    }
}
