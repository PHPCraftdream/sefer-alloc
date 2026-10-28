// expect: rule requires
struct StartingGuard;
impl Drop for StartingGuard {
    fn drop(&mut self) {
        STATE.store(IDLE, Ordering::Release);
    }
}

impl MaintenanceService {
    pub(crate) fn start() -> Result<(), MaintenanceStartError> {
        match STATE.compare_exchange(IDLE, STARTING, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {}
            Err(RUNNING) => return Ok(()),
            Err(_) => return Err(MaintenanceStartError::InProgress),
        }
        let rollback = StartingGuard;
        let handle = Self::spawn_worker().map_err(MaintenanceStartError::Spawn)?;
        core::mem::forget(rollback);
        let guard = Self::control();
        let timed_out = CHANGED
            .wait_timeout_while(guard, STARTUP_CONFIRM_BOUND, |_| {
                STATE.load(Ordering::Acquire) != RUNNING
            })
            .unwrap()
            .1
            .timed_out();
        Ok(())
    }

    pub(crate) fn running() -> bool {
        STATE.load(Ordering::Relaxed) == RUNNING
    }

    fn worker() {
        let _terminal = WorkerGuard;
        {
            let _guard = Self::control();
            STATE.store(RUNNING, Ordering::Release);
            CHANGED.notify_all();
        }
        loop {
            HeapRegistry::maintenance_pass(&mut cursor, SLOT_BUDGET);
        }
    }
}
