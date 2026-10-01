//! W+3: re-issue of the retained exact-layout cap without physical dealloc;
//! System-only candidate, not Sefer acceptance.

#[path = "support/r11_box_cap_reissue.rs"]
mod gate;

#[global_allocator]
static GLOBAL: gate::Gate = gate::Gate;

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("reissue-live") => gate::run(gate::Scenario::ReissueLive),
        Some("reissue-live-across") => gate::run(gate::Scenario::ReissueLiveAcross),
        Some("reissue-after") => gate::run(gate::Scenario::ReissueAfter),
        #[cfg(miri)]
        Some("reissue-live-root") => gate::run(gate::Scenario::ReissueLiveRoot),
        _ => std::process::exit(2),
    }
}
