//! `ring-mpsc` — a bounded, allocation-free, `no_std` MPSC index ring
//! (Vyukov-style CAS-reserve push / single-consumer drain) usable over an
//! **owned array** OR **caller-supplied raw memory**.
//!
//! # The protocol
//!
//! Two monotonic wrapping cursors — `tail` (producers CAS-reserve a slot) and
//! `head` (the single consumer advances past drained slots) — index a fixed
//! `CAP`-slot array of [`RingEntry`] payloads. `CAP` must be a power of two so
//! the `usize`/`u32` cursors wrap continuously across their integer maximum
//! (`2^bits % CAP == 0`); this is pinned at compile time.
//!
//! **Push (multi-producer).**
//! 1. `t = tail.load(Relaxed)`; `h = head.load(Acquire)`. If
//!    `t.wrapping_sub(h) >= CAP` the ring is full → [`push`](MpscRing::push)
//!    returns [`Err(Full)`](Full) (the caller applies its own bounded-loss
//!    policy — see below). The `Acquire` head load observes the consumer's
//!    `Release` head advance, so a slot freed by a drain is visible.
//! 2. CAS `tail: t -> t+1` (`AcqRel` on success — the reservation is the
//!    linearization point, exactly one producer wins each `t`; `Relaxed` on
//!    failure → retry).
//! 3. Publish the payload into `slots[t % CAP]` with a `Release` store of the
//!    entry's **gate** field. For a two-word entry the non-gate word is stored
//!    `Relaxed` FIRST and the gate word `Release` LAST, so a consumer that
//!    `Acquire`-loads a published gate also observes the prior `Relaxed` word
//!    (Release-sequence rule).
//!
//! **Drain (single consumer).**
//! 1. `t = tail.load(Acquire)` (sees every producer's `Release` reservation).
//! 2. While `h != t` (wrap-correct — undrained count is `t.wrapping_sub(h)`,
//!    NEVER `t - h`): load `slots[h % CAP]`'s gate with `Acquire`. If it reads
//!    the **empty sentinel**, the reservation was won but the publish store has
//!    not happened yet → **STOP** (a later drain picks it up; order is preserved
//!    by the cursors, the slot cannot be skipped). Otherwise reclaim the entry,
//!    clear the slot's gate back to the sentinel (`Relaxed` — the next producer
//!    to reserve it `Release`-stores again), and `h = h.wrapping_add(1)`.
//! 3. `head.store(h, Release)` publishes drain progress to producers' full-check
//!    `Acquire` head load.
//! 4. [`drain`](MpscRing::drain) returns the ACTUAL stop position `h` (the value
//!    just published to `head`), NOT the entry-time `tail` snapshot. This is
//!    load-bearing when the drain stopped early at a reserved-but-unpublished
//!    slot (`h < t` at the break): a guard that caches the returned cursor and
//!    the entry-time `tail` would then WRONGLY skip the re-drain that must
//!    observe the slot once its producer finishes publishing. Returning `h`
//!    keeps the cache strictly below `tail` while any reservation remains
//!    unpublished, so the guard keeps re-draining until the publish lands.
//!
//! # Overflow — bounded loss, never corruption
//!
//! A full ring returns [`Err(Full)`](Full); the ring never overwrites an
//! undrained slot. What the caller does with a rejected entry (drop it, retry,
//! route it elsewhere) is the caller's bounded-loss policy — the ring guarantees
//! only that a push that returns `Ok` is drained exactly once and never torn.
//!
//! # The drain guard — [`tail_relaxed`](MpscRing::tail_relaxed) idiom
//!
//! The owner may cache the cursor returned by [`drain`](MpscRing::drain) and
//! skip a full drain while [`tail_relaxed`](MpscRing::tail_relaxed) still equals
//! that cache. This is sound because `tail` is monotonic: a `Relaxed` load can
//! only be STALE (older) or exact, never a value that HIDES a genuine push. A
//! push that races the guard is caught by the next drain — the same "later drain
//! picks it up" liveness the reserved-but-unpublished stop already relies on.
//!
//! # Two tiers
//!
//! - [`MpscRing::new`] — **safe owned storage**: the ring OWNS a `CAP`-slot
//!   array. Ideal for a plain in-process queue.
//! - [`MpscRing::over_raw`] — **`unsafe` in-place view over caller memory**: the
//!   ring is a thin handle over `FOOTPRINT` bytes the caller points it at
//!   (shared-memory IPC, DMA mailboxes, in-arena metadata). See that method's
//!   `# Safety` contract.
//!
//! # Payload entries
//!
//! Two ready-made entries are provided: [`U32Entry`] (a single `u32` payload,
//! one-word publish) and [`UsizeU32Entry`] (a `(usize, u32)` pair, two-word
//! pair-publish). Implement [`RingEntry`] to carry your own `Copy` payload.
//!
//! # DirtyRouter
//!
//! [`DirtyRouter`] is a lost-wakeup-safe "ready-set" over N keys: producers
//! [`mark`](DirtyRouter::mark) a key AFTER publishing into its channel; the
//! consumer [`for_each_dirty`](DirtyRouter::for_each_dirty)`s the set keys. See
//! its docs for the HONEST at-least-once / bounded-deferral contract.

// Single-file seam crate: `unsafe` is confined to this one module, lifted by the
// crate-level `#![allow(unsafe_code)]`. There is a SINGLE documented reason to
// hold `unsafe`: the `over_raw` in-place tier materialises `&AtomicUN`
// references at caller-chosen byte offsets into memory the crate does not own,
// and hands out the ring handle over it — the validity/size/alignment/lifetime/
// exclusivity of that memory is unverifiable by the callee, so the contract
// lives in the `unsafe fn over_raw` signature (its `# Safety` clause) and every
// internal raw-pointer materialisation carries a `// SAFETY:` comment justified
// by that contract. The owned-array tier and the whole `DirtyRouter` are plain
// safe code.
#![allow(unsafe_code)]
#![cfg_attr(not(test), no_std)]

use core::marker::PhantomData;

// The atomics are aliased so loom can shadow the REAL ring/router types: under
// `--cfg loom` they are built on `loom::sync::atomic`, so the shipped loom tests
// (in `tests/`) model-check the actual implementation, not a hand-copied
// transcription. Under normal builds it is `core::sync::atomic`, keeping the
// crate `no_std` and allocation-free.
#[cfg(not(loom))]
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
#[cfg(loom)]
use loom::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

/// A push failed because the ring is full. The caller applies its own
/// bounded-loss policy (drop, retry, route elsewhere). Never a corruption: an
/// undrained slot is never overwritten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Full;

// ===========================================================================
// RingEntry — the per-slot payload protocol (one- or two-word publish).
// ===========================================================================

/// The per-slot payload protocol for an [`MpscRing`].
///
/// An entry occupies a fixed set of atomic words per slot. The trait supplies:
/// the atomic **storage** type for one slot ([`Slot`](RingEntry::Slot)), the
/// `Copy` **payload** the caller pushes/receives ([`Value`](RingEntry::Value)),
/// how to construct an empty (sentinel) slot, and the publish/read/clear
/// protocol whose orderings the module docs pin.
///
/// # The gate discipline
///
/// Every entry designates one word as the **gate**: the LAST word published
/// (`Release`) and the FIRST word read (`Acquire`). Its empty-sentinel value is
/// what a drain sees for a reserved-but-unpublished slot. A one-word entry's
/// gate is its only word; a two-word entry stores the non-gate word `Relaxed`
/// first and the gate `Release` last, so the Release-sequence rule carries the
/// non-gate word along to any consumer that observes the gate.
///
/// # Safety
///
/// This trait is **not** `unsafe` to implement — it is plain safe atomics. The
/// only obligation is a *correctness* one, enforced by [`MpscRing::new`]'s and
/// [`MpscRing::over_raw`]'s invariants: no real [`Value`](RingEntry::Value) may
/// serialise to the same gate word as the empty sentinel, or a drain would treat
/// a genuine entry as unpublished. Both provided entries document how they
/// guarantee this.
pub trait RingEntry {
    /// The `Copy` payload a producer pushes and the consumer receives.
    type Value: Copy;

    /// One slot's atomic storage (all words for one entry).
    type Slot;

    /// Construct one empty slot (gate == sentinel). Used to fill the owned
    /// array; the raw tier writes the same via [`init_slot`](RingEntry::init_slot).
    fn empty_slot() -> Self::Slot;

    /// The byte size of one slot's storage, for the raw tier's `FOOTPRINT`.
    const SLOT_SIZE: usize;

    /// The alignment of one slot's storage, for the raw tier.
    const SLOT_ALIGN: usize;

    /// Materialise `&Slot` at `base + off` (raw tier). `off` is a byte offset.
    ///
    /// # Safety
    ///
    /// `base + off` must point to at least [`SLOT_SIZE`](RingEntry::SLOT_SIZE)
    /// writable, [`SLOT_ALIGN`](RingEntry::SLOT_ALIGN)-aligned bytes that are
    /// valid for the ring's lifetime (the caller's `over_raw` contract).
    unsafe fn slot_at<'a>(base: *mut u8, off: usize) -> &'a Self::Slot;

    /// Initialise a fresh slot in place to empty (gate == sentinel), for the raw
    /// tier. The segment is single-writer at init time, so a plain store suffices.
    fn init_slot(slot: &Self::Slot);

    /// Publish `value` into a reserved (owned by this producer) slot: non-gate
    /// words `Relaxed` FIRST, then the gate word `Release` LAST.
    fn publish(slot: &Self::Slot, value: Self::Value);

    /// Read a slot's gate with `Acquire`. Returns `Some(value)` if the slot is
    /// published (gate != sentinel), reading any non-gate word `Relaxed` after
    /// the gate `Acquire` (Release-sequence). `None` iff the slot is
    /// reserved-but-unpublished (gate == sentinel).
    fn read(slot: &Self::Slot) -> Option<Self::Value>;

    /// Clear a drained slot's gate back to the sentinel (`Relaxed` — the next
    /// producer to reserve it `Release`-stores again).
    fn clear(slot: &Self::Slot);
}

// ---------------------------------------------------------------------------
// U32Entry — single-`u32` payload (the RemoteFreeRing shape).
// ---------------------------------------------------------------------------

/// A single-`u32`-payload [`RingEntry`]: one atomic word per slot. The empty
/// sentinel is [`U32Entry::EMPTY`] (`u32::MAX`); a caller MUST never push
/// `u32::MAX` as a real value (debug-asserted). One-word publish: the value's
/// store IS the gate.
pub struct U32Entry;

impl U32Entry {
    /// The empty sentinel (`u32::MAX`) — "reserved but not published, or drained".
    pub const EMPTY: u32 = u32::MAX;
}

impl RingEntry for U32Entry {
    type Value = u32;
    type Slot = AtomicU32;

    #[cfg(not(loom))]
    fn empty_slot() -> AtomicU32 {
        AtomicU32::new(Self::EMPTY)
    }
    #[cfg(loom)]
    fn empty_slot() -> AtomicU32 {
        AtomicU32::new(Self::EMPTY)
    }

    const SLOT_SIZE: usize = core::mem::size_of::<u32>();
    const SLOT_ALIGN: usize = core::mem::align_of::<AtomicU32>();

    unsafe fn slot_at<'a>(base: *mut u8, off: usize) -> &'a AtomicU32 {
        // SAFETY: the caller's `over_raw` contract guarantees `base + off` is a
        // valid, aligned, exclusively-owned `SLOT_SIZE`-byte region live for the
        // ring's use; `AtomicU32` is layout-compatible with a `u32` at that
        // address. The `'a` lifetime is bounded by the ring handle, itself bounded
        // by the caller's liveness guarantee.
        &*(base.add(off).cast::<AtomicU32>())
    }

    fn init_slot(slot: &AtomicU32) {
        slot.store(Self::EMPTY, Ordering::Relaxed);
    }

    fn publish(slot: &AtomicU32, value: u32) {
        debug_assert_ne!(
            value,
            Self::EMPTY,
            "U32Entry value must not be the sentinel"
        );
        // One word: its Release store IS the publish gate.
        slot.store(value, Ordering::Release);
    }

    fn read(slot: &AtomicU32) -> Option<u32> {
        let v = slot.load(Ordering::Acquire);
        if v == Self::EMPTY {
            None
        } else {
            Some(v)
        }
    }

    fn clear(slot: &AtomicU32) {
        slot.store(Self::EMPTY, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// UsizeU32Entry — (usize, u32) pair payload (the HeapOverflow shape).
// ---------------------------------------------------------------------------

/// Two atomic words for the [`UsizeU32Entry`] pair: `base` is the gate.
pub struct UsizeU32Slot {
    /// Gate word — `0` (== [`UsizeU32Entry::EMPTY_BASE`]) when unpublished.
    base: AtomicUsize,
    /// Payload word — published `Relaxed` BEFORE `base`.
    packed: AtomicU32,
}

/// A `(usize, u32)`-pair [`RingEntry`]: two atomic words per slot, with the
/// two-word pair-publish protocol. The `usize` (call it `base`) is the **gate**;
/// its empty sentinel is [`UsizeU32Entry::EMPTY_BASE`] (`0`). A caller MUST never
/// push `base == 0` (debug-asserted). Publish order: `packed` (`Relaxed`) then
/// `base` (`Release`); read order: `base` (`Acquire`) then `packed` (`Relaxed`).
pub struct UsizeU32Entry;

impl UsizeU32Entry {
    /// The empty-gate sentinel (`0`) — "reserved but not published, or drained".
    pub const EMPTY_BASE: usize = 0;
}

impl RingEntry for UsizeU32Entry {
    type Value = (usize, u32);
    type Slot = UsizeU32Slot;

    fn empty_slot() -> UsizeU32Slot {
        UsizeU32Slot {
            base: AtomicUsize::new(Self::EMPTY_BASE),
            packed: AtomicU32::new(0),
        }
    }

    const SLOT_SIZE: usize =
        core::mem::size_of::<AtomicUsize>() + core::mem::size_of::<AtomicU32>();
    const SLOT_ALIGN: usize = core::mem::align_of::<UsizeU32Slot>();

    unsafe fn slot_at<'a>(base: *mut u8, off: usize) -> &'a UsizeU32Slot {
        // SAFETY: see `U32Entry::slot_at` — `UsizeU32Slot` is a `#[repr(Rust)]`
        // aggregate of two atomics; the `over_raw` contract guarantees a valid,
        // aligned, exclusively-owned region of `SLOT_SIZE` bytes at `base + off`.
        // The raw tier lays slots out at `SLOT_STRIDE`-aligned offsets (see
        // `over_raw`), so the alignment obligation holds by construction.
        &*(base.add(off).cast::<UsizeU32Slot>())
    }

    fn init_slot(slot: &UsizeU32Slot) {
        slot.packed.store(0, Ordering::Relaxed);
        slot.base.store(Self::EMPTY_BASE, Ordering::Relaxed);
    }

    fn publish(slot: &UsizeU32Slot, value: (usize, u32)) {
        let (base, packed) = value;
        debug_assert_ne!(
            base,
            Self::EMPTY_BASE,
            "UsizeU32Entry base must not be the sentinel"
        );
        // Pair-publish: `packed` Relaxed FIRST, then `base` Release LAST. A
        // consumer that Acquire-loads a non-empty `base` also observes this
        // `packed` store (Release sequence). This is the ordering the
        // wrong-publish-order torn-read counterfactual breaks.
        slot.packed.store(packed, Ordering::Relaxed);
        slot.base.store(base, Ordering::Release);
    }

    fn read(slot: &UsizeU32Slot) -> Option<(usize, u32)> {
        let base = slot.base.load(Ordering::Acquire);
        if base == Self::EMPTY_BASE {
            None
        } else {
            let packed = slot.packed.load(Ordering::Relaxed);
            Some((base, packed))
        }
    }

    fn clear(slot: &UsizeU32Slot) {
        slot.base.store(Self::EMPTY_BASE, Ordering::Relaxed);
    }
}

// ===========================================================================
// Cursor storage — owned vs raw.
// ===========================================================================

/// The two cursors an [`MpscRing`] needs, plus a way to reach each slot, plus
/// the entry protocol and capacity. This trait is what lets ONE ring type sit
/// over EITHER owned storage or raw memory without duplicating the push/drain
/// protocol.
///
/// It is a **sealed** implementation seam: only the crate's own [`OwnedStore`]
/// and [`RawStore`] implement it (downstream code cannot). `Entry`/`CAP` are
/// carried here so the ring's entry-agnostic methods have no free type/const
/// parameters. It is `pub` only so it can appear in [`MpscRing`]'s method
/// signatures; it exposes nothing a consumer needs to name.
pub trait RingStore: sealed::Sealed {
    /// The per-slot entry protocol.
    type Entry: RingEntry;
    /// The ring capacity (power of two).
    const CAP: usize;
    fn head(&self) -> &AtomicUsize;
    fn tail(&self) -> &AtomicUsize;
    /// The slot at reservation index `raw` (the impl reduces `raw % CAP`).
    fn slot(&self, raw: usize) -> &<Self::Entry as RingEntry>::Slot;
}

mod sealed {
    /// Sealing supertrait: only the crate's own store types implement it, so no
    /// downstream code can implement [`super::RingStore`].
    pub trait Sealed {}
    impl<T: super::RingEntry, const CAP: usize> Sealed for super::OwnedStore<T, CAP> {}
    impl<T: super::RingEntry, const CAP: usize> Sealed for super::RawStore<T, CAP> {}
}

/// Owned-array storage: the ring OWNS its cursors and `CAP` slots.
pub struct OwnedStore<T: RingEntry, const CAP: usize> {
    head: AtomicUsize,
    tail: AtomicUsize,
    slots: [T::Slot; CAP],
}

impl<T: RingEntry, const CAP: usize> RingStore for OwnedStore<T, CAP> {
    type Entry = T;
    const CAP: usize = CAP;
    #[inline]
    fn head(&self) -> &AtomicUsize {
        &self.head
    }
    #[inline]
    fn tail(&self) -> &AtomicUsize {
        &self.tail
    }
    #[inline]
    fn slot(&self, raw: usize) -> &T::Slot {
        &self.slots[raw % CAP]
    }
}

/// Raw-memory storage: a thin view over caller-supplied bytes. `head`/`tail`
/// live at fixed byte offsets; each slot is at `SLOTS_OFF + (idx % CAP) *
/// SLOT_STRIDE`.
pub struct RawStore<T: RingEntry, const CAP: usize> {
    base: *mut u8,
    _marker: PhantomData<fn() -> T>,
}

impl<T: RingEntry, const CAP: usize> RawStore<T, CAP> {
    /// Byte offset of the `head` cursor (own word).
    const HEAD_OFF: usize = 0;
    /// Byte offset of the `tail` cursor (own word, separated from `head`).
    const TAIL_OFF: usize = core::mem::size_of::<AtomicUsize>();
    /// Byte offset of the first slot: past both cursors, rounded up to
    /// `SLOT_ALIGN`.
    const SLOTS_OFF: usize = {
        let cursors = 2 * core::mem::size_of::<AtomicUsize>();
        let a = T::SLOT_ALIGN;
        // round `cursors` up to a multiple of `a`.
        cursors.next_multiple_of(a)
    };
    /// Per-slot byte stride: `SLOT_SIZE` rounded up to `SLOT_ALIGN` so every
    /// slot lands on its own aligned address.
    const SLOT_STRIDE: usize = T::SLOT_SIZE.next_multiple_of(T::SLOT_ALIGN);
    /// Total byte footprint of the raw layout for this `T`/`CAP`.
    const FOOTPRINT: usize = Self::SLOTS_OFF + CAP * Self::SLOT_STRIDE;
}

impl<T: RingEntry, const CAP: usize> RingStore for RawStore<T, CAP> {
    type Entry = T;
    const CAP: usize = CAP;
    #[inline]
    fn head(&self) -> &AtomicUsize {
        // SAFETY: the `over_raw` contract guarantees `base` points to at least
        // `FOOTPRINT` valid, aligned, exclusively-owned bytes live for the ring's
        // use; `HEAD_OFF == 0` is `AtomicUsize`-aligned by the contract's base
        // alignment. The `&AtomicUsize`'s lifetime is bounded by `self`, itself
        // bounded by the caller's liveness guarantee.
        unsafe { &*(self.base.add(Self::HEAD_OFF).cast::<AtomicUsize>()) }
    }
    #[inline]
    fn tail(&self) -> &AtomicUsize {
        // SAFETY: as `head` above; `TAIL_OFF` is a `size_of::<AtomicUsize>()`
        // multiple, so the word is `AtomicUsize`-aligned given the aligned base.
        unsafe { &*(self.base.add(Self::TAIL_OFF).cast::<AtomicUsize>()) }
    }
    #[inline]
    fn slot(&self, raw: usize) -> &T::Slot {
        let idx = raw % CAP;
        let off = Self::SLOTS_OFF + idx * Self::SLOT_STRIDE;
        // SAFETY: `off` is within `FOOTPRINT` (idx < CAP) and `SLOT_STRIDE` is a
        // multiple of `SLOT_ALIGN`, so each slot address is `SLOT_ALIGN`-aligned
        // given the aligned base — exactly `T::slot_at`'s obligation.
        unsafe { T::slot_at(self.base, off) }
    }
}

// ===========================================================================
// MpscRing.
// ===========================================================================

/// A bounded MPSC index ring. See the [crate docs](crate) for the full protocol,
/// orderings, the overflow bounded-loss policy, and the drain-guard idiom.
///
/// `CAP` must be a power of two (pinned at compile time). Two tiers:
/// [`MpscRing::new`] (owned array) and [`MpscRing::over_raw`] (raw memory view).
pub struct MpscRing<S> {
    store: S,
}

// --- Compile-time CAP pin, shared by both tiers. ---
struct CapPin<const CAP: usize>;
impl<const CAP: usize> CapPin<CAP> {
    const POW2: () = assert!(
        CAP.is_power_of_two(),
        "MpscRing CAP must be a power of two so the wrapping cursors wrap \
         continuously across their integer maximum (2^bits % CAP == 0); a \
         non-power-of-two CAP jumps the slot index at the wrap and corrupts FIFO",
    );
}

impl<T: RingEntry, const CAP: usize> MpscRing<OwnedStore<T, CAP>> {
    /// Construct a ring that OWNS a fresh `CAP`-slot array (all slots empty,
    /// cursors zero).
    ///
    /// Under `--cfg loom` this cannot be `const` (loom's atomics have no `const`
    /// constructor); on normal builds it is `const` so the ring can live in a
    /// `static`.
    #[cfg(not(loom))]
    #[must_use]
    pub fn new() -> Self {
        let () = CapPin::<CAP>::POW2;
        Self {
            store: OwnedStore {
                head: AtomicUsize::new(0),
                tail: AtomicUsize::new(0),
                slots: core::array::from_fn(|_| T::empty_slot()),
            },
        }
    }
    /// Construct a ring that OWNS a fresh `CAP`-slot array (loom build — non-`const`).
    #[cfg(loom)]
    #[must_use]
    pub fn new() -> Self {
        let () = CapPin::<CAP>::POW2;
        Self {
            store: OwnedStore {
                head: AtomicUsize::new(0),
                tail: AtomicUsize::new(0),
                slots: core::array::from_fn(|_| T::empty_slot()),
            },
        }
    }
}

impl<T: RingEntry, const CAP: usize> Default for MpscRing<OwnedStore<T, CAP>> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: RingEntry, const CAP: usize> MpscRing<RawStore<T, CAP>> {
    /// The byte footprint the raw tier requires at `base`. Carve exactly this
    /// many bytes (aligned to at least [`RAW_ALIGN`](Self::RAW_ALIGN)) before
    /// calling [`over_raw`](Self::over_raw).
    pub const FOOTPRINT: usize = RawStore::<T, CAP>::FOOTPRINT;

    /// The minimum base alignment the raw tier requires: the max of the cursor
    /// (`AtomicUsize`) and slot alignments.
    pub const RAW_ALIGN: usize = {
        let cu = core::mem::align_of::<AtomicUsize>();
        let cs = T::SLOT_ALIGN;
        if cu > cs {
            cu
        } else {
            cs
        }
    };

    /// Construct an in-place ring view over `FOOTPRINT` caller-supplied bytes at
    /// `base`, initialising the cursors to zero and every slot to empty.
    ///
    /// A release-surviving `assert!` (not `debug_assert!`) checks `base` is
    /// non-null and [`RAW_ALIGN`](Self::RAW_ALIGN)-aligned, so a null/misaligned
    /// base panics in every build.
    ///
    /// # Safety
    ///
    /// The caller MUST guarantee that `base` points to at least
    /// [`FOOTPRINT`](Self::FOOTPRINT) writable, [`RAW_ALIGN`](Self::RAW_ALIGN)-
    /// aligned bytes that are **exclusively owned** by this ring handle and live
    /// for the ring's entire use (e.g. a leaked owned buffer, a segment-metadata
    /// carve-out, or an mmap the caller keeps mapped). The
    /// footprint/alignment/liveness/exclusivity contract cannot be checked at
    /// runtime beyond the null/alignment guard; passing a too-short, dangling,
    /// shared, or non-`FOOTPRINT`-valid base is undefined behaviour. The ring
    /// writes the cursors and every slot at construction, so the bytes must be
    /// writable.
    #[must_use = "the ring handle must be kept to use the ring"]
    pub unsafe fn over_raw(base: *mut u8) -> Self {
        let () = CapPin::<CAP>::POW2;
        assert!(
            !base.is_null() && (base as usize).is_multiple_of(Self::RAW_ALIGN),
            "MpscRing::over_raw: base must be non-null and RAW_ALIGN-aligned",
        );
        let store = RawStore {
            base,
            _marker: PhantomData,
        };
        // Initialise in place: cursors zero, every slot empty. Single-writer at
        // construction (the caller's exclusivity contract), so plain stores.
        store.head().store(0, Ordering::Relaxed);
        store.tail().store(0, Ordering::Relaxed);
        for i in 0..CAP {
            T::init_slot(store.slot(i));
        }
        Self { store }
    }

    /// Construct an in-place ring view over ALREADY-INITIALISED caller memory
    /// (cursors + slots previously written by [`over_raw`](Self::over_raw) or an
    /// out-of-band single-writer init), WITHOUT re-initialising. Same `# Safety`
    /// contract as [`over_raw`](Self::over_raw), plus: the bytes must already
    /// hold a valid ring state (cursors consistent, unpublished slots at the
    /// empty sentinel).
    ///
    /// # Safety
    ///
    /// See [`over_raw`](Self::over_raw#safety). This variant additionally
    /// requires the memory to already be a valid ring image; it performs no
    /// initialisation.
    #[must_use = "the ring handle must be kept to use the ring"]
    pub unsafe fn view_raw(base: *mut u8) -> Self {
        let () = CapPin::<CAP>::POW2;
        assert!(
            !base.is_null() && (base as usize).is_multiple_of(Self::RAW_ALIGN),
            "MpscRing::view_raw: base must be non-null and RAW_ALIGN-aligned",
        );
        Self {
            store: RawStore {
                base,
                _marker: PhantomData,
            },
        }
    }
}

impl<S> MpscRing<S>
where
    S: RingStore,
{
    /// Push `value` onto the ring (any producer). Returns [`Err(Full)`](Full) if
    /// the ring is full (the caller applies its bounded-loss policy). A push that
    /// returns `Ok` is drained exactly once and never torn.
    ///
    /// `value` must not serialise to the entry's empty sentinel (debug-asserted
    /// by the entry).
    #[inline]
    pub fn push(&self, value: <S::Entry as RingEntry>::Value) -> Result<(), Full> {
        loop {
            let t = self.store.tail().load(Ordering::Relaxed);
            let h = self.store.head().load(Ordering::Acquire);
            if t.wrapping_sub(h) >= S::CAP {
                return Err(Full);
            }
            match self.store.tail().compare_exchange_weak(
                t,
                t.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    S::Entry::publish(self.store.slot(t), value);
                    return Ok(());
                }
                Err(_) => continue,
            }
        }
    }

    /// Drain every published entry, invoking `reclaim` for each, and return the
    /// ACTUAL stop position (the new `head` cursor). Called ONLY by the single
    /// consumer (the owner). Stops at the first reserved-but-unpublished slot; a
    /// later drain picks it up (order preserved by the cursors).
    ///
    /// The returned cursor is the drain-guard cache value — see the crate docs'
    /// "drain returns actual stop position" contract and
    /// [`tail_relaxed`](Self::tail_relaxed).
    #[inline]
    pub fn drain<F: FnMut(<S::Entry as RingEntry>::Value)>(&self, mut reclaim: F) -> usize {
        let t = self.store.tail().load(Ordering::Acquire);
        // Relaxed head load is sound: the single consumer is the sole writer of
        // `head`. See the crate docs' single-consumer note (a new owner across a
        // handle handoff is fenced by the handoff, not by a per-load Acquire).
        let mut h = self.store.head().load(Ordering::Relaxed);
        while h != t {
            let slot = self.store.slot(h);
            match S::Entry::read(slot) {
                None => break, // reserved but not yet published — stop.
                Some(value) => {
                    reclaim(value);
                    S::Entry::clear(slot);
                    h = h.wrapping_add(1);
                }
            }
        }
        self.store.head().store(h, Ordering::Release);
        h
    }

    /// Drain-guard primitive: a single `Relaxed` load of `tail`. Compare it
    /// against a cached cursor (from a prior [`drain`](Self::drain) return value)
    /// to decide whether a full drain is needed. Sound because `tail` is
    /// monotonic — see the crate docs' drain-guard section.
    #[inline]
    #[must_use]
    pub fn tail_relaxed(&self) -> usize {
        self.store.tail().load(Ordering::Relaxed)
    }

    /// Progress-detection primitive: a single `Relaxed` load of `head` (the
    /// drain cursor). A stalled producer can compare two loads one probe-round
    /// apart to detect whether the owner drained anything. Sound by the same
    /// monotonicity argument as [`tail_relaxed`](Self::tail_relaxed).
    #[inline]
    #[must_use]
    pub fn head_relaxed(&self) -> usize {
        self.store.head().load(Ordering::Relaxed)
    }

    /// `true` iff the ring is momentarily observed empty (`tail == head`, both
    /// `Acquire`). Heuristic — a producer may push immediately after.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.store.tail().load(Ordering::Acquire) == self.store.head().load(Ordering::Acquire)
    }

    /// **Test surface**: advance `tail` by one reservation WITHOUT publishing —
    /// reproducing the window between a winning producer's tail CAS and its
    /// publish store, so a single-threaded test can exercise the
    /// drain-stops-at-unpublished-slot path deterministically. MUST be called on
    /// a quiescent ring (no concurrent push/drain).
    #[doc(hidden)]
    pub fn dbg_reserve_unpublished(&self) {
        let t = self.store.tail().load(Ordering::Relaxed);
        self.store
            .tail()
            .store(t.wrapping_add(1), Ordering::Relaxed);
        // Intentionally do NOT publish: the slot stays at the empty sentinel.
    }

    /// **Test surface**: read the `(head, tail)` cursor pair (both `Acquire`).
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_cursors(&self) -> (usize, usize) {
        (
            self.store.head().load(Ordering::Acquire),
            self.store.tail().load(Ordering::Acquire),
        )
    }

    /// **Test surface**: preset the cursors directly (`Release` stores) so a test
    /// can drive the ring across the cursor wrap without pushing `2^bits`
    /// entries. MUST be called on a quiescent ring and leave
    /// `tail.wrapping_sub(head) <= CAP`.
    #[doc(hidden)]
    pub fn dbg_set_cursors(&self, head: usize, tail: usize) {
        self.store.head().store(head, Ordering::Release);
        self.store.tail().store(tail, Ordering::Release);
    }
}

// The raw view holds a `*mut u8` it exclusively owns for its lifetime; it is
// `Send`/`Sync` under the same discipline as the atomics it materialises — the
// caller's `over_raw` contract guarantees exclusive ownership of the backing
// bytes, so concurrent access is race-free exactly as for the owned tier.
// SAFETY: all shared state is behind the materialised atomics; the raw `*mut u8`
// is only ever used to derive `&AtomicUN`, never dereferenced as data.
unsafe impl<T: RingEntry, const CAP: usize> Send for RawStore<T, CAP> {}
// SAFETY: see the `Send` impl above.
unsafe impl<T: RingEntry, const CAP: usize> Sync for RawStore<T, CAP> {}

/// Convenience alias: an owned-storage ring of `CAP` entries of type `T`.
/// `MpscRing::<Owned<U32Entry, 256>>::new()` reads as "a 256-slot owned ring".
pub type Owned<T, const CAP: usize> = OwnedStore<T, CAP>;

/// Convenience alias: a raw-memory (in-place) ring of `CAP` entries of type `T`.
pub type Raw<T, const CAP: usize> = RawStore<T, CAP>;

// ===========================================================================
// DirtyRouter.
// ===========================================================================

/// A lost-wakeup-safe "ready-set" over `WORDS * 64` keys: a `[AtomicU64; WORDS]`
/// dirty bitmap. Producers [`mark`](DirtyRouter::mark) a key AFTER publishing
/// into its channel; the single consumer [`for_each_dirty`](
/// DirtyRouter::for_each_dirty)`s exactly the set keys.
///
/// # The HONEST contract — at-least-once wakeup with bounded deferral
///
/// [`mark`](DirtyRouter::mark) is a `fetch_or(bit, Release)` performed AFTER the
/// channel publish; [`for_each_dirty`](DirtyRouter::for_each_dirty) is a
/// `swap(0, Acquire)` per word. This guarantees at-least-once wakeup: a key whose
/// producer completes its `mark` is observed by the next
/// [`for_each_dirty`](DirtyRouter::for_each_dirty) pass, and the `Acquire`/
/// `Release` pair makes that producer's prior channel publish visible to the
/// consumer that observes the bit.
///
/// It does NOT guarantee immediate wakeup. A producer stalled BETWEEN its channel
/// publish and its `mark` is *boundedly deferred* — its entry is in the channel
/// but INVISIBLE to a `for_each_dirty` that runs in that window. It becomes
/// visible via (a) the next pass once the `mark` lands, (b) another producer's
/// `mark` of the SAME key, or (c) — and this is the caller's obligation — the
/// caller's own periodic full sweep of every channel. A producer that CRASHES
/// between publish and `mark` leaves its entry reachable ONLY via that full
/// sweep. **The router therefore requires the consumer to tolerate deferral until
/// the next mark, OR to run a periodic unconditional full sweep as a backstop.**
/// (In `sefer-alloc`, that backstop is a guarded linear scan that drains every
/// ring on a directory miss — the router is the fast path, the sweep is the
/// completeness guarantee.) This is a coherent, useful contract — it is exactly
/// how sparse epoll-style ready-lists and deferred-free queues behave — but it is
/// a contract, not magic.
pub struct DirtyRouter<const WORDS: usize> {
    words: [AtomicU64; WORDS],
}

impl<const WORDS: usize> DirtyRouter<WORDS> {
    /// Number of keys this router covers (`WORDS * 64`).
    pub const CAPACITY: usize = WORDS * 64;

    /// Construct a router with every key clean.
    ///
    /// `const` on normal builds (so it can live in a `static`); non-`const` under
    /// `--cfg loom`.
    #[cfg(not(loom))]
    #[must_use]
    pub const fn new() -> Self {
        // `[AtomicU64::new(0); WORDS]` needs a const item because `AtomicU64` is
        // not `Copy`; build via a const block per element.
        Self {
            words: [const { AtomicU64::new(0) }; WORDS],
        }
    }
    /// Construct a router with every key clean (loom build — non-`const`).
    #[cfg(loom)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            words: core::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    /// Mark `key` dirty. Producers MUST call this AFTER publishing into `key`'s
    /// channel — the `fetch_or(bit, Release)` orders the channel publish
    /// before-visible to the consumer's `Acquire` swap.
    ///
    /// # Panics
    ///
    /// Panics (debug) if `key >= CAPACITY`.
    #[inline]
    pub fn mark(&self, key: usize) {
        debug_assert!(key < Self::CAPACITY, "DirtyRouter key out of range");
        let word = key / 64;
        let bit = 1u64 << (key % 64);
        self.words[word].fetch_or(bit, Ordering::Release);
    }

    /// Consume the dirty set: for each word, `swap(0, Acquire)` and invoke
    /// `visit(key)` for every set bit. Called ONLY by the single consumer. After
    /// this returns, every key that was marked BEFORE its word's swap has been
    /// visited exactly once; a mark that races the swap is caught next pass (the
    /// bounded-deferral contract).
    #[inline]
    pub fn for_each_dirty<F: FnMut(usize)>(&self, mut visit: F) {
        for w in 0..WORDS {
            let mut bits = self.words[w].swap(0, Ordering::Acquire);
            while bits != 0 {
                let b = bits.trailing_zeros() as usize;
                visit(w * 64 + b);
                bits &= bits - 1; // clear the lowest set bit.
            }
        }
    }

    /// **Test surface**: `true` iff `key`'s bit is currently set (`Acquire`),
    /// without consuming it.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_is_marked(&self, key: usize) -> bool {
        let word = key / 64;
        let bit = 1u64 << (key % 64);
        self.words[word].load(Ordering::Acquire) & bit != 0
    }
}

impl<const WORDS: usize> Default for DirtyRouter<WORDS> {
    fn default() -> Self {
        Self::new()
    }
}
