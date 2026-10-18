//! Non-allocating lock for the route directory shards: a short bounded spin
//! phase, then (on Windows and Linux) the thread parks in the OS
//! (`WaitOnAddress` / futex) instead of burning CPU or scheduler quanta.
//!
//! `std::sync::Mutex` boxes a pthread mutex on first lock on some targets
//! (macOS), which re-enters the installed global allocator while a heap is
//! still being constructed. This lock never allocates: the wait word is
//! inline, and the kernel wait/wake primitives allocate nothing.
//!
//! Targets without an inline OS-wait primitive in scope (macOS and other
//! OSes, plus `miri`/`loom` builds) keep the previous spin-then-yield
//! behavior verbatim; on those targets the third state below is never
//! written and the word only ever holds 0 or 1, exactly like the old
//! `AtomicBool`.
#![allow(unsafe_code)]

use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU32, Ordering};

/// Word value: free. A fast acquirer CASes 0 → 1.
const UNLOCKED: u32 = 0;
/// Word value: held, nobody has parked on it. Unlock is a single `fetch_sub`.
const LOCKED: u32 = 1;
/// Word value: held AND at least one waiter has marked the word, so unlock
/// must wake the wait queue. Only ever written on OS-wait targets.
const CONTENDED: u32 = 2;

/// Bounded TTAS spin phase before the waiter falls back to the OS wait —
/// the same bounded spin the b50f04f4 spin lock performed before yielding.
const SPINS_BEFORE_WAIT: u32 = 64;
/// Cap of the exponential `spin_loop()` backoff: `1 << BACKOFF_MAX_PAUSES_LOG2`
/// pauses per TTAS read at most, then the waiter starts yielding (or, on
/// OS-wait targets, parking).
const BACKOFF_MAX_PAUSES_LOG2: u32 = 6;
/// The process-exit probe (an FFI call on Windows) is checked only once per
/// `EXIT_PROBE_YIELDS` yield rounds, never on every wait iteration. Exit
/// refusal is delayed by at most that many scheduler yields. Fallback tier
/// (non-OS-wait targets) only.
///
/// Compiled out on OS-wait targets (its tier does not exist there) — this is
/// the same predicate, negated, spelled out as in `os_wait` above.
#[cfg(not(any(
    all(windows, not(miri), not(loom)),
    all(
        target_os = "linux",
        any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "riscv64"
        ),
        not(miri),
        not(loom)
    )
)))]
const EXIT_PROBE_YIELDS: u32 = 64;
/// OS-wait re-arm period in milliseconds. Wakeups themselves arrive
/// immediately via the paired wake call; the timeout only bounds how long a
/// wait can sit without re-probing process exit (exit-refusal latency is at
/// most one `WAIT_TIMEOUT_MS` past the bounded spin phase).
const WAIT_TIMEOUT_MS: u32 = 4;

/// Direct kernel wait/wake FFI for the contended tier. Both wrappers return
/// to the caller's value-rechecking loop on any outcome; they never spin.
///
/// Gated on the single "OS wait available" predicate, spelled out at each of
/// its four use sites in this file (no cfg aliases on stable):
/// `any(all(windows, not(miri), not(loom)),
/// all(target_os = "linux", any(target_arch = "x86_64",
/// target_arch = "aarch64", target_arch = "riscv64"), not(miri), not(loom)))`
#[cfg(any(
    all(windows, not(miri), not(loom)),
    all(
        target_os = "linux",
        any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "riscv64"
        ),
        not(miri),
        not(loom)
    )
))]
mod os_wait {
    use super::CONTENDED;
    use core::sync::atomic::AtomicU32;

    #[cfg(windows)]
    #[link(name = "kernel32")]
    extern "system" {
        fn WaitOnAddress(
            address: *const core::ffi::c_void,
            compare_address: *const core::ffi::c_void,
            address_size: usize,
            dw_milliseconds: u32,
        ) -> i32;
        fn WakeByAddressAll(address: *const core::ffi::c_void);
    }

    #[cfg(target_os = "linux")]
    extern "C" {
        // Variadic: glibc and musl both export `syscall`, so no libc crate
        // dependency is needed.
        fn syscall(number: i64, ...) -> i64;
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    const SYS_FUTEX: i64 = 202;
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "aarch64", target_arch = "riscv64")
    ))]
    const SYS_FUTEX: i64 = 98; // asm-generic
    #[cfg(target_os = "linux")]
    const FUTEX_WAIT_PRIVATE: i32 = 128; // FUTEX_PRIVATE_FLAG | FUTEX_WAIT(0)
    #[cfg(target_os = "linux")]
    const FUTEX_WAKE_PRIVATE: i32 = 129; // FUTEX_PRIVATE_FLAG | FUTEX_WAKE(1)

    /// All three supported arches are LP64, so two `i64` fields are exact.
    #[cfg(target_os = "linux")]
    #[repr(C)]
    struct Timespec {
        tv_sec: i64,
        tv_nsec: i64,
    }

    /// Block the calling thread while `*word == CONTENDED`, at most
    /// `timeout_ms` milliseconds, then return so the caller can re-check the
    /// value (and, on the exit-refusal face, process exit).
    #[allow(unused_variables)]
    pub(super) fn wait_contended(word: &AtomicU32, timeout_ms: u32) {
        // Test-only activation oracle: the counter exists only in test
        // builds (cfg(test) + predicate), exactly matching the static.
        #[cfg(all(
            test,
            any(
                all(windows, not(miri), not(loom)),
                all(
                    target_os = "linux",
                    any(
                        target_arch = "x86_64",
                        target_arch = "aarch64",
                        target_arch = "riscv64"
                    ),
                    not(miri),
                    not(loom)
                )
            )
        ))]
        super::DBG_OS_WAIT_BLOCKS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        #[cfg(windows)]
        {
            // SAFETY: `word` is a field of a live `ShardLock`, i.e. a valid,
            // 4-byte-aligned atomic that stays alive for the whole lock
            // critical section; `WaitOnAddress` re-checks the compare value
            // under its internal wait-lock before blocking, so a store that
            // lands after the compare cannot be missed by the paired
            // `WakeByAddressAll` on the same address; neither call allocates
            // or transfers ownership. The BOOL result is deliberately
            // ignored: FALSE (ERROR_TIMEOUT, or any other failure) simply
            // falls back to the caller's value-rechecking loop.
            let expected = CONTENDED;
            // SAFETY: `word` is a valid, 4-byte-aligned atomic for the whole
            // lock critical section (see the safety note above the ignore);
            // arguments match the declared extern signature.
            let _ = unsafe {
                WaitOnAddress(
                    word as *const AtomicU32 as *const core::ffi::c_void,
                    &expected as *const u32 as *const core::ffi::c_void,
                    core::mem::size_of::<u32>(),
                    timeout_ms,
                )
            };
        }
        #[cfg(target_os = "linux")]
        {
            // SAFETY: `word` is a valid, 4-byte-aligned atomic for the
            // lifetime of the lock; futex(2) re-checks `*word == CONTENDED`
            // under its wait queue lock before sleeping, so a store landing
            // after this check cannot be missed by the paired FUTEX_WAKE on
            // the same address; the syscall allocates nothing. The result is
            // deliberately ignored: 0 = woken; -1 with EAGAIN (the word
            // changed before we slept), EINTR, or ETIMEDOUT all just return
            // to the caller's value-rechecking loop.
            let timeout = Timespec {
                tv_sec: i64::from(timeout_ms) / 1000,
                tv_nsec: (i64::from(timeout_ms) % 1000) * 1_000_000,
            };
            // SAFETY: `syscall` is variadic and matches its `extern "C"
            // fn(i64, ...) -> i64` declaration; arguments (pointer to the
            // aligned atomic u32, op, val, timeout, NULL, 0) match
            // futex(2)'s WAIT signature.
            let _ = unsafe {
                syscall(
                    SYS_FUTEX,
                    word as *const AtomicU32 as *const u32,
                    i64::from(FUTEX_WAIT_PRIVATE),
                    i64::from(CONTENDED),
                    &timeout as *const Timespec,
                    core::ptr::null::<u32>(),
                    0,
                )
            };
        }
    }

    /// Wake EVERY thread waiting on this address. Wake-all (not wake-one) is
    /// required for correctness: see the comment at the wake site in
    /// `ShardGuard::drop`.
    pub(super) fn wake_all(word: &AtomicU32) {
        #[cfg(windows)]
        {
            // SAFETY: same address as the paired `WaitOnAddress` above;
            // valid, aligned, alive for the whole lock critical section; the
            // call allocates nothing and transfers no ownership.
            unsafe {
                WakeByAddressAll(word as *const AtomicU32 as *const core::ffi::c_void);
            }
        }
        #[cfg(target_os = "linux")]
        {
            // SAFETY: same address as the paired futex WAIT above; valid,
            // aligned, alive for the whole lock critical section; the
            // syscall allocates nothing. Woken count is ignored — waiters
            // re-check the value themselves.
            // SAFETY: `syscall` matches its variadic declaration; arguments
            // (fd-pointer to the aligned atomic u32, op, val, NULL, NULL, 0)
            // match futex(2)'s WAKE signature.
            let _ = unsafe {
                syscall(
                    SYS_FUTEX,
                    word as *const AtomicU32 as *const u32,
                    i64::from(FUTEX_WAKE_PRIVATE),
                    i64::from(u32::MAX),
                    core::ptr::null::<Timespec>(),
                    core::ptr::null::<u32>(),
                    0,
                )
            };
        }
    }
}

/// R30-8 path-activation oracle: incremented once at the top of
/// `os_wait::wait_contended`, so a test can prove waiters actually left the
/// bounded spin phase into the OS wait. `cfg(test)` keeps this out of every
/// production build; the `pub ... fn`-only scan of
/// `scripts/verify-dbg-hook-safety.mjs` does not see a static.
#[cfg(all(
    test,
    any(
        all(windows, not(miri), not(loom)),
        all(
            target_os = "linux",
            any(
                target_arch = "x86_64",
                target_arch = "aarch64",
                target_arch = "riscv64"
            ),
            not(miri),
            not(loom)
        )
    )
))]
pub(super) static DBG_OS_WAIT_BLOCKS: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);

/// Bounded exponential backoff: `spin_loop()` `1 << min(shift, cap)` times.
/// Pure latency device; correctness never depends on it.
fn spin_backoff(shift: u32) {
    for _ in 0..(1u32 << shift.min(BACKOFF_MAX_PAUSES_LOG2)) {
        core::hint::spin_loop();
    }
}

pub(super) struct ShardLock<T> {
    /// 0 = free, 1 = held, 2 = held and a waiter has parked on it
    /// (`CONTENDED`; only written on OS-wait targets). First field, so the
    /// futex/WaitOnAddress address is at a stable, 4-byte-aligned offset 0.
    state: AtomicU32,
    value: UnsafeCell<T>,
}

// SAFETY: access to `value` is serialized by `state`; `T: Send` lets the
// guard hand `&mut T` to whichever thread holds the lock.
unsafe impl<T: Send> Sync for ShardLock<T> {}
// SAFETY: moving the lock moves the owned `T`.
unsafe impl<T: Send> Send for ShardLock<T> {}

/// Guard for one acquired [`ShardLock`]: exclusive payload access on the
/// single thread that holds it.
///
/// Auto traits, pinned by the `_unique` marker: `Send` requires only
/// `T: Send`; `Sync` additionally requires `T: Sync`, because [`Deref`]
/// hands `&T` to every thread sharing the guard. [`ShardLock`] itself stays
/// `Sync` under `T: Send` alone.
pub(super) struct ShardGuard<'a, T> {
    lock: &'a ShardLock<T>,
    // `&'a mut T` is `Send` iff `T: Send` and `Sync` iff `T: Sync` —
    // exactly the guard's access contract (R13-05).
    _unique: PhantomData<&'a mut T>,
}

impl<T> ShardLock<T> {
    pub(super) const fn new(value: T) -> Self {
        Self {
            state: AtomicU32::new(UNLOCKED),
            value: UnsafeCell::new(value),
        }
    }

    #[allow(dead_code)] // Standalone path-included auto-trait tests use only lock().
    pub(super) fn lock_unless_exiting(
        &self,
        exiting: impl Fn() -> bool,
        giveup: impl Fn(),
    ) -> Option<ShardGuard<'_, T>> {
        let exiting: &dyn Fn() -> bool = &exiting;
        let giveup: &dyn Fn() = &giveup;
        self.lock_slow(Some(exiting), Some(giveup))
    }

    // R6-T16: its compiled lib consumers are the bench-gated directory
    // probes; the standalone auto-trait tests include this file in their own
    // crate and do not lint this one.
    #[cfg_attr(not(feature = "bench-internals"), allow(dead_code))]
    pub(super) fn lock(&self) -> ShardGuard<'_, T> {
        self.lock_slow(None, None)
            .expect("lock() has no exit-refusal face")
    }

    /// Shared slow path of both acquire faces. `lock()` passes `None`/`None`
    /// and expects acquisition (no exit-refusal face); the fallback face
    /// probes `exiting` and calls `giveup` on refusal.
    fn lock_slow(
        &self,
        exiting: Option<&dyn Fn() -> bool>,
        giveup: Option<&dyn Fn()>,
    ) -> Option<ShardGuard<'_, T>> {
        let mut spins = 0u32;
        #[cfg(not(any(
            all(windows, not(miri), not(loom)),
            all(
                target_os = "linux",
                any(
                    target_arch = "x86_64",
                    target_arch = "aarch64",
                    target_arch = "riscv64"
                ),
                not(miri),
                not(loom)
            )
        )))]
        let mut yield_rounds = 0u32;
        loop {
            // Fast path and retry: a single CAS; no read precedes the first
            // attempt, so the uncontended path is unchanged.
            if self
                .state
                .compare_exchange_weak(UNLOCKED, LOCKED, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return Some(ShardGuard {
                    lock: self,
                    _unique: PhantomData,
                });
            }
            // TTAS: read-only spin while the lock looks held — the cache line
            // stays shared instead of ping-ponging RMW between waiters.
            while self.state.load(Ordering::Relaxed) != UNLOCKED {
                if spins < SPINS_BEFORE_WAIT {
                    spins += 1;
                    spin_backoff(spins);
                } else {
                    break;
                }
            }
            if spins < SPINS_BEFORE_WAIT {
                continue; // the word looked free — retry the CAS
            }
            // Spin phase exhausted.
            #[cfg(not(any(
                all(windows, not(miri), not(loom)),
                all(
                    target_os = "linux",
                    any(
                        target_arch = "x86_64",
                        target_arch = "aarch64",
                        target_arch = "riscv64"
                    ),
                    not(miri),
                    not(loom)
                )
            )))]
            {
                // Fallback tier, unchanged from b50f04f4: yield and probe
                // process exit once per EXIT_PROBE_YIELDS yield rounds.
                yield_rounds = yield_rounds.wrapping_add(1);
                if let Some(exiting) = exiting {
                    if yield_rounds & (EXIT_PROBE_YIELDS - 1) == 0 && exiting() {
                        giveup.expect("exiting face always supplies giveup")();
                        return None;
                    }
                }
                std::thread::yield_now();
                continue;
            }
            #[cfg(any(
                all(windows, not(miri), not(loom)),
                all(
                    target_os = "linux",
                    any(
                        target_arch = "x86_64",
                        target_arch = "aarch64",
                        target_arch = "riscv64"
                    ),
                    not(miri),
                    not(loom)
                )
            ))]
            {
                // Checked BEFORE every blocking wait: exit refusal never
                // waits more than one WAIT_TIMEOUT_MS.
                if let Some(exiting) = exiting {
                    if exiting() {
                        giveup.expect("exiting face always supplies giveup")();
                        return None;
                    }
                }
                match self.state.load(Ordering::Relaxed) {
                    UNLOCKED => {
                        // A slow-path acquire ALWAYS takes the word with the
                        // waiter bit set: its own unlock must wake anyone
                        // still queued. A plain UNLOCKED→LOCKED acquire here
                        // could strand a queued waiter behind a plain-store
                        // unlock that sees old == LOCKED and skips the wake.
                        if self
                            .state
                            .compare_exchange(
                                UNLOCKED,
                                CONTENDED,
                                Ordering::Acquire,
                                Ordering::Relaxed,
                            )
                            .is_ok()
                        {
                            return Some(ShardGuard {
                                lock: self,
                                _unique: PhantomData,
                            });
                        }
                    }
                    LOCKED => {
                        // Mark the word so the holder's unlock wakes the
                        // queue; failure just means the state moved and the
                        // next iteration re-reads it.
                        let _ = self.state.compare_exchange(
                            LOCKED,
                            CONTENDED,
                            Ordering::Acquire,
                            Ordering::Relaxed,
                        );
                        os_wait::wait_contended(&self.state, WAIT_TIMEOUT_MS);
                    }
                    _ => {
                        os_wait::wait_contended(&self.state, WAIT_TIMEOUT_MS);
                    }
                }
            }
        }
    }
}

impl<T> Deref for ShardGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: the guard holds the lock.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for ShardGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the guard holds the lock exclusively.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for ShardGuard<'_, T> {
    fn drop(&mut self) {
        // Exactly one atomic op while no waiter ever marked the word — the
        // uncontended path keeps the b50f04f4 operation COUNT (one release
        // RMW instead of the old release store). A marked word publishes
        // UNLOCKED first, then wakes the queue: the wake's value re-check
        // must observe the store.
        if self.lock.state.fetch_sub(1, Ordering::Release) != LOCKED {
            self.lock.state.store(UNLOCKED, Ordering::Release);
            #[cfg(any(
                all(windows, not(miri), not(loom)),
                all(
                    target_os = "linux",
                    any(
                        target_arch = "x86_64",
                        target_arch = "aarch64",
                        target_arch = "riscv64"
                    ),
                    not(miri),
                    not(loom)
                )
            ))]
            {
                // Wake-ALL, not wake-one: with wake-one, an unlock that
                // leaves a second waiter queued while the value reads
                // UNLOCKED lets a fresh fast-path acquirer take the word as
                // LOCKED; its unlock then sees old == LOCKED and never wakes
                // — the queued waiter strands until an unrelated future
                // contention. Wake-all makes every unlock that moves the
                // value away from CONTENDED wake every queued waiter;
                // surplus woken threads just re-check the value and go back
                // to sleep.
                os_wait::wake_all(&self.lock.state);
            }
        }
    }
}
