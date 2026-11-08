// expect: expected 5 matching site(s), found 4
impl ThpTupleCell {
    pub(crate) fn publish(&self, t: &ThpTuple) -> bool {
        match self
            .state
            .compare_exchange(UNSET, PUBLISHING, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => {
                let packed = t.pack();
                self.w[0].store(packed[0], Ordering::Relaxed);
                self.w[1].store(packed[1], Ordering::Relaxed);
                self.w[2].store(packed[2], Ordering::Relaxed);
                self.w[3].store(packed[3], Ordering::Relaxed);
                self.w[4].store(packed[4], Ordering::Relaxed);
                self.state.store(PUBLISHED, Ordering::Release);
                true
            }
            Err(PUBLISHED) => ThpTuple::try_unpack(self.load_words()) == Some(*t),
            Err(_) => false,
        }
    }

    pub(crate) fn read(&self) -> ThpTuple {
        if self.state.load(Ordering::Acquire) == PUBLISHED {
            ThpTuple::unpack(self.load_words())
        } else {
            ThpTuple::DEFAULT
        }
    }

    fn load_words(&self) -> [u64; 5] {
        [
            self.w[0].load(Ordering::Relaxed),
            self.w[1].load(Ordering::Relaxed),
            self.w[2].load(Ordering::Relaxed),
            self.w[3].load(Ordering::Relaxed),
        ]
    }
}
