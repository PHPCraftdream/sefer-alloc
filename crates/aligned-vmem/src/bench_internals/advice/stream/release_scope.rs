//! Thread-affine inert source classification; never mapped-memory ownership.
#![forbid(unsafe_code)]
use core::cell::Cell;
use core::marker::PhantomData;
use std::rc::Rc;

std::thread_local! {
    static LARGE_RELEASE: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
}

/// Source-classification guard only. Not Send/Sync: restoration belongs to the
/// emitting thread. Keys are inert integers and authorize no memory operation.
pub struct AdviceReleaseScope {
    previous: (usize, usize),
    _thread: PhantomData<Rc<()>>,
}

impl AdviceReleaseScope {
    pub(crate) fn enter(reservation: usize, len: usize) -> Result<Self, &'static str> {
        let previous = LARGE_RELEASE
            .try_with(|context| context.replace((reservation, len)))
            .map_err(|_| "release source context unavailable")?;
        Ok(Self {
            previous,
            _thread: PhantomData,
        })
    }
    pub(crate) fn matches(reservation: usize, len: usize) -> Result<bool, &'static str> {
        LARGE_RELEASE
            .try_with(|context| context.get() == (reservation, len))
            .map_err(|_| "release source context unavailable")
    }
    pub const fn scalar_storage_bytes_per_thread() -> usize {
        core::mem::size_of::<(usize, usize)>()
    }
}

impl Drop for AdviceReleaseScope {
    fn drop(&mut self) {
        if LARGE_RELEASE
            .try_with(|context| context.set(self.previous))
            .is_err()
        {
            super::super::capture::AdviceCapture::context_lost();
        }
    }
}
