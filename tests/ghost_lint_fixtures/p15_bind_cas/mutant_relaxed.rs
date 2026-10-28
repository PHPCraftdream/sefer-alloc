// expect: rule requires
impl Drop for TokenBlock {
    fn drop(&mut self) {
        TOKEN_BACKING_DEATHS.fetch_add(1, Ordering::Release);
        LIVE_TOKEN_BLOCKS.fetch_sub(1, Ordering::Release);
    }
}

impl Drop for ErasedGuard {
    fn drop(&mut self) {
        for claim in &self.claims {
            if let Some(occupied) = tokens.slots.get(usize::from(claim.shard)) {
                occupied.store(false, Ordering::Release);
            }
        }
    }
}

fn prune_dead_claims(guard: &mut ErasedGuard) {
    let death_epoch = TOKEN_BACKING_DEATHS.load(Ordering::Acquire);
}

impl<T> ShardedRegion<T> {
    fn claim_or_get_shard(&self) -> u16 {
        for (i, occupied) in self.tokens.slots.iter().enumerate() {
            if occupied
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return i as u16;
            }
        }
        0
    }

    pub fn bind_current_thread_to_shard(&self, shard: u16) -> bool {
        self.tokens.slots[usize::from(shard)]
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }
}
