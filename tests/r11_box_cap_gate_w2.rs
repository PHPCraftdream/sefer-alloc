//! W+2: owner-cap System candidate only; not Sefer acceptance.

#[path = "support/r11_box_cap_gate.rs"]
mod gate;

#[global_allocator]
static GLOBAL: gate::Gate<true> = gate::Gate;

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("positive") => gate::run::<true>(gate::Scenario::Positive),
        #[cfg(miri)]
        Some("root-one") => gate::run::<true>(gate::Scenario::RootOne),
        #[cfg(miri)]
        Some("root-eight") => gate::run::<true>(gate::Scenario::RootEight),
        _ => std::process::exit(2),
    }
}
