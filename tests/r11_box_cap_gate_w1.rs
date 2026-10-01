//! W+1: exact installed System wrapper, producer frees before terminal RMW.

#[path = "support/r11_box_cap_gate.rs"]
mod gate;

#[global_allocator]
static GLOBAL: gate::Gate<false> = gate::Gate;

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("positive") => gate::run::<false>(gate::Scenario::Positive),
        _ => std::process::exit(2),
    }
}
