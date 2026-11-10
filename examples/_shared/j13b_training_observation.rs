//! Raw selected-control evidence, before any report or floating-point summary.
use super::control_interval::ControlInterval;
use std::thread::ThreadId;

#[derive(Debug)]
pub(crate) struct TrainingObservation {
    pub eligible_position: usize,
    pub ticket: u64,
    pub owner_thread: ThreadId,
    pub touch_thread: ThreadId,
    pub e_q31: u64,
    pub m_q31: u64,
    pub interval: ControlInterval,
}
