//! Miri-only one-shot controller for an actual terminal publication frame.
//! The module wiring excludes ordinary builds entirely. All state is static
//! and independent of every allocator reservation and publication descriptor.

use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};

struct State {
    controller_ready: bool,
    prepared: bool,
    allow_publication: bool,
    published: bool,
    resume: bool,
}

static TARGET: AtomicUsize = AtomicUsize::new(0);
static STATE: Mutex<State> = Mutex::new(State {
    controller_ready: false,
    prepared: false,
    allow_publication: false,
    published: false,
    resume: true,
});
static CHANGED: Condvar = Condvar::new();

/// Test-only static synchronization, never an allocation/free capability.
/// One controller arms one current allocation and joins its producer before
/// arming another. The numeric target prevents pausing unrelated std frees.
pub struct TerminalPublicationGate;

impl TerminalPublicationGate {
    pub fn arm(address: usize) {
        let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
        *state = State {
            controller_ready: false,
            prepared: false,
            allow_publication: false,
            published: false,
            resume: false,
        };
        TARGET.store(address, Ordering::Release);
    }

    /// Called BEFORE Box Drop. Both participants must enter a condvar wait
    /// before publication is allowed, initializing synchronization/parking
    /// machinery outside the terminal commit and retirement regions.
    pub fn prepare_producer() {
        let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
        while !state.controller_ready {
            state = CHANGED
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
        state.prepared = true;
        CHANGED.notify_all();
        while !state.allow_publication {
            state = CHANGED
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    }

    pub fn allow_prepared_producer() {
        let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
        state.controller_ready = true;
        CHANGED.notify_all();
        while !state.prepared {
            state = CHANGED
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
        state.allow_publication = true;
        CHANGED.notify_all();
    }

    pub fn wait_for_publication() {
        let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
        while !state.published {
            state = CHANGED
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    }

    pub fn resume_producer() {
        TARGET.store(0, Ordering::Release);
        let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
        state.resume = true;
        CHANGED.notify_all();
    }

    /// Called only after a successful terminal RMW. No reservation, callback,
    /// allocation API, or logging is touched here; the producer's condvar wait
    /// has already been initialized by prepare_producer outside Box Drop.
    pub(crate) fn pause_after_publication(address: usize) {
        if TARGET.load(Ordering::Acquire) != address {
            return;
        }
        let mut state = STATE.lock().unwrap_or_else(|error| error.into_inner());
        // Disable matching immediately: same-VA reissue cannot accidentally
        // pause a second free while the first producer's frame is suspended.
        TARGET.store(0, Ordering::Release);
        state.published = true;
        CHANGED.notify_all();
        while !state.resume {
            state = CHANGED
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    }
}
