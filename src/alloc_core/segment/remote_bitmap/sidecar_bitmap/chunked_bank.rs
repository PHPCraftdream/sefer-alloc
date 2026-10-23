//! Ленивая конструкция атомарных банков; ready публикует lifetime, не pending.
#![allow(unsafe_code)]

use super::{AtomicU64, Ordering};
use crate::alloc_core::fatal::fatal;
use crate::alloc_core::os::SEGMENT;
use crate::alloc_core::size_classes::MIN_BLOCK;
#[cfg(not(all(loom, test)))]
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::ptr;
#[cfg(all(loom, test))]
use loom::cell::UnsafeCell;

const WORDS_PER_CHUNK: usize = 8;
const WORDS: usize = SEGMENT / MIN_BLOCK / 64;
const CHUNKS: usize = WORDS.div_ceil(WORDS_PER_CHUNK);
const READY_WORDS: usize = CHUNKS.div_ceil(64);

#[cfg(kani)]
#[path = "chunked_bank_proofs.rs"]
mod proofs;

#[repr(C, align(64))]
struct Chunk {
    pending: [AtomicU64; WORDS_PER_CHUNK],
    starts: [AtomicU64; WORDS_PER_CHUNK],
}

/// Один backing; каждый чанк конструируется единственным owner ровно один раз.
#[repr(C)]
pub(crate) struct ChunkedBank {
    ready: [AtomicU64; READY_WORDS],
    chunks: [UnsafeCell<MaybeUninit<Chunk>>; CHUNKS],
}

// SAFETY: конструкция доступна только unsafe owner-операции. Ready Release/Acquire
// предшествует каждой ссылке на Chunk; после ready payload меняется лишь атомарно.
// Ready не сбрасывается, lifetime чанков заканчивается только при эксклюзивном Drop.
unsafe impl Sync for ChunkedBank {}

impl ChunkedBank {
    pub(crate) const WORDS: usize = WORDS;
    pub(crate) const GRANULES: usize = SEGMENT / MIN_BLOCK;

    /// # Safety
    /// `raw` — приватное выровненное writable-хранилище Self, ещё не наблюдаемое.
    pub(crate) unsafe fn initialize_at(raw: *mut Self) {
        // SAFETY: поля проецируются без создания ссылки на неготовый Self.
        let ready = unsafe { ptr::addr_of_mut!((*raw).ready).cast::<AtomicU64>() };
        for i in 0..READY_WORDS {
            // SAFETY: отдельная инициализация каждого элемента приватного поля.
            unsafe { ready.add(i).write(AtomicU64::new(0)) };
        }
        // SAFETY: chunks — отдельное поле того же приватного allocation.
        let chunks =
            unsafe { ptr::addr_of_mut!((*raw).chunks).cast::<UnsafeCell<MaybeUninit<Chunk>>>() };
        for i in 0..CHUNKS {
            // SAFETY: валидная оболочка не требует инициализации Chunk.
            unsafe { chunks.add(i).write(UnsafeCell::new(MaybeUninit::uninit())) };
        }
    }

    fn is_ready(&self, chunk: usize) -> bool {
        self.ready[chunk / 64].load(Ordering::Acquire) & (1u64 << (chunk % 64)) != 0
    }

    /// # Safety
    /// Только единственный owner; исключены другие конструкторы этого sidecar.
    unsafe fn ensure_chunk(&self, chunk: usize) {
        let ready = &self.ready[chunk / 64];
        let mask = 1u64 << (chunk % 64);
        let old = ready.load(Ordering::Relaxed);
        if old & mask != 0 {
            return;
        }
        let initialize = |raw: *mut MaybeUninit<Chunk>| {
            // SAFETY: единственный owner, ready==0; payload ещё никому не доступен.
            let raw = unsafe { (*raw).as_mut_ptr() };
            // SAFETY: отдельные поля приватного ещё не сконструированного Chunk.
            let pending = unsafe { ptr::addr_of_mut!((*raw).pending).cast::<AtomicU64>() };
            // SAFETY: starts не пересекает pending.
            let starts = unsafe { ptr::addr_of_mut!((*raw).starts).cast::<AtomicU64>() };
            for i in 0..WORDS_PER_CHUNK {
                // SAFETY: каждый элемент обоих полей записывается один раз до ready.
                unsafe {
                    pending.add(i).write(AtomicU64::new(0));
                    starts.add(i).write(AtomicU64::new(0));
                }
            }
        };
        #[cfg(all(loom, test))]
        self.chunks[chunk].with_mut(initialize);
        #[cfg(not(all(loom, test)))]
        initialize(self.chunks[chunk].get());
        ready.store(old | mask, Ordering::Release);
    }

    fn with_chunk<R>(&self, chunk: usize, f: impl FnOnce(&Chunk) -> R) -> Option<R> {
        if !self.is_ready(chunk) {
            return None;
        }
        let read = |raw: *const MaybeUninit<Chunk>| {
            // SAFETY: acquired ready публикует полностью constructed Chunk.
            // Lifetime не заканчивается и reconstruction не происходит до Drop.
            f(unsafe { (*raw).assume_init_ref() })
        };
        #[cfg(all(loom, test))]
        {
            Some(self.chunks[chunk].with(read))
        }
        #[cfg(not(all(loom, test)))]
        {
            Some(read(self.chunks[chunk].get()))
        }
    }

    fn checked_word(word: usize) -> (usize, usize) {
        if word >= WORDS {
            fatal(b"chunked_bank: word out of segment\n");
        }
        (word / WORDS_PER_CHUNK, word % WORDS_PER_CHUNK)
    }

    pub(crate) fn granule(offset: u32) -> Option<usize> {
        let offset = offset as usize;
        (offset < SEGMENT && offset.is_multiple_of(MIN_BLOCK)).then_some(offset / MIN_BLOCK)
    }

    #[inline]
    pub(crate) fn window_end(high_water: usize, word: usize) -> Option<usize> {
        if high_water > SEGMENT {
            return None;
        }
        let end = high_water.div_ceil(64 * MIN_BLOCK);
        (word <= end).then_some(end)
    }

    #[inline]
    fn location(offset: u32) -> Option<(usize, usize, u64)> {
        let granule = Self::granule(offset)?;
        let (chunk, word) = Self::checked_word(granule / 64);
        Some((chunk, word, 1u64 << (granule % 64)))
    }

    #[inline]
    pub(crate) fn range_mask(start: usize, end: usize, word: usize) -> u64 {
        if word >= WORDS {
            return 0;
        }
        let base = word * 64;
        let lo = start.saturating_sub(base).min(64);
        let hi = end.saturating_sub(base).min(64);
        if hi <= lo {
            return 0;
        }
        (u64::MAX << lo) & (u64::MAX >> (64 - hi))
    }

    /// # Safety
    /// Вызывающий владеет единственной owner-capability; issue/retain последовательны.
    /// Start ставится в commit-точке блока, до передачи allocation producer.
    pub(crate) unsafe fn issue(&self, offset: u32) {
        let Some((chunk, word, mask)) = Self::location(offset) else {
            fatal(b"sidecar_bitmap: issue offset out of segment or misaligned\n");
        };
        // SAFETY: исключительность конструктора следует из контракта issue.
        unsafe { self.ensure_chunk(chunk) };
        self.with_chunk(chunk, |bank| {
            let old = bank.starts[word].load(Ordering::Acquire);
            bank.starts[word].store(old | mask, Ordering::Release);
        })
        .unwrap_or_else(|| fatal(b"chunked_bank: issued chunk not ready\n"));
    }

    pub(crate) fn publish(&self, offset: u32, payload_start: usize) -> bool {
        let Some((chunk, word, mask)) = Self::location(offset) else {
            return false;
        };
        if (offset as usize) < payload_start {
            return false;
        }
        self.with_chunk(chunk, |bank| {
            if bank.starts[word].load(Ordering::Acquire) & mask == 0 {
                return false;
            }
            bank.pending[word].fetch_or(mask, Ordering::AcqRel);
            true
        })
        .unwrap_or(false)
    }

    pub(crate) fn pending_load(&self, word: usize) -> u64 {
        let (chunk, word) = Self::checked_word(word);
        self.with_chunk(chunk, |bank| bank.pending[word].load(Ordering::Acquire))
            .unwrap_or(0)
    }

    pub(crate) fn pending_swap(&self, word: usize) -> u64 {
        let (chunk, word) = Self::checked_word(word);
        self.with_chunk(chunk, |bank| bank.pending[word].swap(0, Ordering::AcqRel))
            .unwrap_or(0)
    }

    pub(crate) fn starts_load(&self, word: usize) -> u64 {
        let (chunk, word) = Self::checked_word(word);
        self.with_chunk(chunk, |bank| bank.starts[word].load(Ordering::Acquire))
            .unwrap_or(0)
    }

    pub(crate) fn clear_pending(&self, word: usize, mask: u64) {
        let (chunk, word) = Self::checked_word(word);
        self.with_chunk(chunk, |bank| {
            bank.pending[word].fetch_and(!mask, Ordering::AcqRel);
        });
    }

    pub(crate) fn clear_starts(&self, word: usize, mask: u64) {
        let (chunk, word) = Self::checked_word(word);
        self.with_chunk(chunk, |bank| {
            bank.starts[word].fetch_and(!mask, Ordering::AcqRel);
        });
    }

    #[cfg(any(test, feature = "internals"))]
    pub(crate) fn initialized_chunks(&self) -> usize {
        self.ready
            .iter()
            .map(|word| word.load(Ordering::Acquire).count_ones() as usize)
            .sum()
    }
}

impl Drop for ChunkedBank {
    fn drop(&mut self) {
        for ready_word in 0..READY_WORDS {
            let mut bits = self.ready[ready_word].load(Ordering::Acquire);
            while bits != 0 {
                let chunk = ready_word * 64 + bits.trailing_zeros() as usize;
                bits &= bits - 1;
                if chunk >= CHUNKS {
                    fatal(b"chunked_bank: ready beyond backing\n");
                }
                let drop_chunk = |raw: *mut MaybeUninit<Chunk>| {
                    // SAFETY: Drop эксклюзивен; ready отмечает constructed payload.
                    // Ни одной ссылки/пина уже нет; уничтожение ровно один раз.
                    unsafe { (*raw).assume_init_drop() };
                };
                #[cfg(all(loom, test))]
                self.chunks[chunk].with_mut(drop_chunk);
                #[cfg(not(all(loom, test)))]
                drop_chunk(self.chunks[chunk].get());
            }
        }
    }
}
