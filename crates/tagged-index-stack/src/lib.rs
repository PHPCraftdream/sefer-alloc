//! `tagged-index-stack` — a lock-free LIFO free-list of small **indices** (a
//! *slot recycler*) whose head is a single atomic word packing an
//! `(index | tag)` pair, where a wrapping generation **tag** in the high bits
//! structurally defeats the ABA problem for every permitted `INDEX_BITS`.
//! Lock-freedom here describes the stack's own CAS loops; end-to-end it
//! additionally requires a non-blocking [`StackStorage`] implementation —
//! [`ArrayIndexStack`] and the slot-resident one-`AtomicU32`-per-slot shape both
//! qualify, while a hypothetical mutex-backed [`StackStorage`] would make
//! [`push_index`](StackOps::push_index)/[`pop_index`](StackOps::pop_index)
//! blocking again. That is a derived claim, not a slogan: the enforced `1..=16`
//! cap on `INDEX_BITS` guarantees every legal configuration a tag of at least
//! 48 bits, and the "Tag-width budget" section below derives, from
//! cache-coherence throughput on the single head cache line, that such a tag
//! cannot repeat within any physically plausible observation window. (The
//! tag is not strictly monotonic — a strictly monotonic counter never
//! repeats a value, and this one wraps — it just never repeats on a
//! timescale the coherence protocol can deliver.) Allocation-free, `no_std`,
//! `#![forbid(unsafe_code)]`.
//!
//! This is the canonical "recycle a small integer id" primitive that slab
//! allocators, object pools, entity-component stores, and connection tables all
//! reinvent — and routinely reinvent *wrong*. The two subtleties people get
//! wrong (documented below) are the **H-2 empty-transition tag preservation**
//! and the **lazy link discipline** (internally: RAD-1); both are structurally enforced
//! here.
//!
//! # The packed word — [`TaggedIndex`]
//!
//! The stack head is one `AtomicU64` holding a [`TaggedIndex`]`<INDEX_BITS>`:
//! the low `INDEX_BITS` bits carry a slot index, the high `64 - INDEX_BITS` bits
//! carry a wrapping generation **tag** bumped on every successful PUSH. The
//! all-ones value (`(1 << INDEX_BITS) - 1`, the [`empty_index`](TaggedIndex::empty_index))
//! is reserved as the "stack empty" sentinel, so the usable index range is
//! `0 .. (1 << INDEX_BITS) - 1`.
//!
//! The classic ABA scenario — thread A reads `head = X`, thread B pops X then
//! re-pushes X — is defeated because B's re-push bumps the tag, so A's CAS on
//! `(X, old_tag)` observes a changed tag and fails, forcing a retry. A pop
//! preserves the tag; only a push advances it.
//!
//! [`TaggedIndex::pack`]/[`unpack`](TaggedIndex::unpack) convert between an
//! `(index, tag)` pair and the packed word; [`try_pack`](TaggedIndex::try_pack)
//! is `pack`'s checked twin, returning `None` instead of silently truncating
//! an out-of-range index or tag.
//!
//! # Storage — one implementor owns the head AND the links
//!
//! Each pushed index's "next" link lives in the implementor's storage, reached
//! through the [`StackStorage`] trait ([`load_next`](StackStorage::load_next) /
//! [`store_next`](StackStorage::store_next)), alongside the head it exposes via
//! [`head`](StackStorage::head). This is what lets a production allocator
//! keep its links **slot-resident** (an `AtomicU32` field inside each slot it
//! already owns) instead of paying for a second array. For standalone use, the
//! crate provides [`ArrayIndexStack`]`<INDEX_BITS, N>` — head and links fused
//! into one owned object. Slot-resident does NOT mean payload-aliased — see
//! the [`StackStorage`] trait doc's "Storage requirement" section for the full
//! dedicated-storage rule and why violating it defeats
//! [`pop_index`](StackOps::pop_index)'s own corruption-detection guard
//! (release-active, not debug-only — see
//! [`pop_index`](StackOps::pop_index)'s `# Panics`).
//!
//! The head↔links binding is established ONCE, structurally, by a single
//! [`StackStorage`] impl — instead of being re-asserted per call, as the
//! previous design's per-call `&L: Links` parameter required. The
//! CAS-retry-loop bodies themselves are crate-owned: [`StackOps`] is blanket-
//! implemented by the crate for every [`StackStorage`] implementor, and
//! trait coherence makes a downstream override impossible. Consequently the
//! review's "two `ArrayLinks` instances against one head" double-issue repro —
//! in which two independent `push`/`pop` calls each supplied a different
//! backing for the same head — is no longer EXPRESSIBLE against this API: it
//! does not compile (pinned by a compile-fail regression test added
//! separately).
//!
//! [`store_next`](StackStorage::store_next) is the ONLY write the stack ever
//! makes to a link, and it happens during
//! [`push_index`](StackOps::push_index), immediately before the CAS that
//! publishes the index as the new head. The stack NEVER eagerly
//! initialises links — see "The lazy link discipline (RAD-1)" below.
//!
//! [`StackHead::is_empty`] is an advisory, `Relaxed` emptiness check —
//! useful for diagnostics/monitoring, but a concurrent push or pop can make
//! it stale the instant it returns, so
//! [`pop_index`](StackOps::pop_index)'s `None` remains the only authoritative
//! empty check.
//!
//! # The two hard-won subtleties
//!
//! ## H-2: the empty-transition tag MUST be preserved (not reset to 0)
//!
//! When a [`pop_index`](StackOps::pop_index) drains the LAST element, the head
//! transitions to "empty". A naive implementation packs the empty sentinel with
//! **tag 0** (`TaggedIndex::empty()`). **That is a bug.** Resetting the tag to 0
//! reopens the ABA window: a popper parked mid-`pop`, holding a stale
//! `(idx, tag)` snapshot from BEFORE the drain, can have its stale tag
//! spuriously RECUR once the stack drains (→ tag 0) and is immediately refilled
//! by a push of the SAME index (→ tag `0 + 1 = 1`); if the parked snapshot's tag
//! was `1`, the head word recurs EXACTLY and the stale CAS succeeds — a genuine
//! ABA collision that corrupts the free-list. The fix
//! ([`pop_index`](StackOps::pop_index) here) packs the empty sentinel's index
//! half with the RUNNING tag the draining pop just observed, so the tag keeps
//! climbing across the empty transition exactly as it would across any other
//! pop. [`is_empty`](TaggedIndex::is_empty) inspects only the index half, so a
//! non-zero tag on the empty word is still unambiguously "empty". The
//! [`push_index`](StackOps::push_index) side already reads the tag out of the
//! current head (empty or not) and bumps it, so it composes with no other
//! change. The shipped loom counterfactual
//! `counterfactual_empty_transition_tag_reset_lets_aba_recur` proves this is
//! load-bearing: with tag-reset restored, loom finds the collision.
//!
//! ## The lazy link discipline (RAD-1): links are NEVER eagerly written
//!
//! The stack writes a slot's link ONLY inside
//! [`push_index`](StackOps::push_index) (the
//! [`store_next`](StackStorage::store_next) immediately before publishing that
//! index as head). It performs NO bulk/eager initialisation of the link storage
//! at construction. A caller whose link backing is OS-zeroed memory (a fresh
//! mmap, a zeroed slot array) therefore never first-touches those pages merely
//! to set up the free-list — the pages are committed lazily, on first push of
//! each index, so the avoided cost is a first-touch commit proportional to the
//! pool size. [`ArrayLinks::new`] likewise starts every link
//! at `0` (the zero value), matching OS-zeroed backing, rather than eagerly
//! chaining a full free-list.
//!
//! Because links are lazy, a freshly-constructed stack is EMPTY — the caller
//! pushes indices in as they become free. This crate does NOT offer a "start
//! with `0..N` all pushed" constructor precisely because that would require an
//! eager link-chaining pass, defeating RAD-1. (A caller that genuinely wants
//! every index free from the start pushes `0..N` itself, or mints fresh
//! indices via a separate monotonic counter and only ever pushes RECYCLED
//! ones onto this stack.)
//!
//! # Tag-width budget — the wrap-time bound behind the ABA guarantee
//!
//! A tag defends against ABA only while it does not recur: a stale CAS can
//! succeed again only if the head word returns to the exact `(index, tag)`
//! pair the victim is holding, which takes a FULL tag wrap — `2^TAG_BITS`
//! successful pushes anywhere in the stack, the last of them re-pushing the
//! victim's own index. The time a wrap takes is
//!
//! ```text
//! wrap_time = 2^TAG_BITS / aggregate_successful_push_rate
//! ```
//!
//! and the rate term is bounded by HARDWARE, not by the workload. The tag is
//! GLOBAL to the whole stack, not per-slot: every successful push — of any
//! index, from any thread — is a compare-exchange (a locked RMW) on the ONE
//! `AtomicU64` head word, so the rate in the formula is the stack's AGGREGATE
//! successful-push rate across ALL slots, and every one of those pushes
//! serializes on a single cache line whose exclusive ownership must transfer
//! between cores. That transfer cost caps the aggregate rate at roughly
//! `10^8` to `10^9` RMWs/sec no matter how many threads contend — more
//! contention only makes the line's ownership transfers slower, never
//! faster. That argument covers the CONTENDED regime; the other bound to
//! check is its opposite — the UNCONTENDED single-threaded case, where the
//! head line stays resident and exclusive in one core's L1 and no
//! cross-core ownership transfer ever happens — a regime governed not by
//! coherence transfer but by the latency of the bare RMW instruction itself
//! (`lock cmpxchg` on x86-64, or the target's equivalent CAS instruction):
//! materially faster, but still bounded. The wrap-time conclusion survives
//! both regimes: this crate's own single-threaded `churn` bench row
//! measures ~`2 × 10^7` successful pushes/sec in that uncontended regime (a
//! pop+push pair per iteration, so one successful push per pair — a
//! push-only rate would run faster still, but the pair rate is already an
//! order of magnitude under the working ceiling the next paragraph adopts,
//! so the argument does not need the tighter push-only number). Committed
//! receipt: the single-threaded `churn` rows in
//! `docs/perf/_raw_tis_backoff_cap_sweep_run1.log` — a file in this crate's
//! REPOSITORY (it is not part of the published package):
//! <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/perf/_raw_tis_backoff_cap_sweep_run1.log>
//! (11th Gen Intel Core
//! i7-11800H, rustc 1.97.0, 2026-08-31) — e.g. 53.89 ns/pair in that log's
//! first arm, its 20 such samples spanning 51.41-64.72 ns/pair; re-run
//! `cargo bench -p tagged-index-stack --bench tagged_index_stack_bench`
//! for a fresh sample — the bound below only
//! needs the order of magnitude, not the exact figure.
//!
//! Taking a generous `2 × 10^8` successful pushes/sec as the working ceiling:
//! at `INDEX_BITS = 16` — the widest permitted index half, 65535 usable
//! indices with the `0xFFFF` empty sentinel reserved above them — the tag
//! gets the other **48 bits**, wrapping at `2^48 ≈ 2.8 × 10^14`, and a wrap
//! takes `2^48 / (2 × 10^8) ≈ 16` days; even at the optimistic top of the
//! hardware range it is still `2^48 / 10^9 ≈ 3.3` days. And a wrap is only
//! the PRECONDITION for a collision: cashing one in further requires that
//! the head line stay saturated at the coherence ceiling continuously for
//! the entire span AND that one specific victim thread sit parked,
//! motionless, holding its stale snapshot the whole time. This bound is why
//! `INDEX_BITS > 16` is REJECTED at compile time
//! (`TaggedIndex::_CHECK_BITS`) rather than merely discouraged: at
//! `INDEX_BITS = 24` the tag would be 40 bits, `2^40 / (2 × 10^8) ≈ 92`
//! minutes at the same ceiling — a long debugger pause or OS scheduling
//! delay defeats that — and the pre-cap `INDEX_BITS = 32` maximum gave only
//! `2^32 / (2 × 10^8) ≈ 21` seconds, within reach of ordinary scheduling
//! jitter. Within the permitted range a caller still trades index range
//! against tag headroom, but never below the 48-bit floor.
//!
//! # Lock-freedom and starvation
//!
//! [`push_index`](StackOps::push_index)/[`pop_index`](StackOps::pop_index)
//! never block on a lock — a losing CAS retries — but lock-freedom is not
//! starvation-freedom: a call can lose arbitrarily many CASes in a row, and
//! the exponential backoff deliberately makes an unlucky call wait longer
//! between retries. The measured trade is a SMALL NUMBER OF VERY LARGE
//! OUTLIERS in exchange for better latency at every percentile through
//! p99.9 AND better aggregate throughput — not "tail latency for
//! throughput" in general. On a 64-element `ArrayLinks` under this crate's
//! own contention discipline (8 threads x 200k pop-then-repush iterations,
//! `--release`; see `examples/backoff_per_call_latency.rs`): the single
//! worst `pop` blocked 41-60 ms across three runs under the shipped
//! backoff cap, vs 0.6-24 ms with the backoff disabled — a handful of
//! extreme outliers is the one axis where disabling the backoff wins —
//! while the same workload finished ~4.9x faster in aggregate under the
//! cap, every percentile through p99.9 was 1-2 orders of magnitude better
//! under the cap (p99.9 ≈ 1 µs vs 54-182 µs at 8-16 threads), and at 16
//! threads the backoff-free build produced 2.2-2.6x MORE pops over 1 ms.
//! A consumer recycling a slot on a latency-sensitive request path should
//! size its tolerance for those rare outliers, not fear a broad tail; the
//! full table is `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` §3.4 — a file
//! in this crate's repository (it is not part of the published package):
//! <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md>
//!
//! # loom — the tests run against THIS type
//!
//! Under `--cfg loom` the stack's atomics alias to `loom::sync::atomic`, so the
//! shipped loom suite (`tests/loom_aba.rs`) model-checks the real
//! [`ArrayIndexStack`] / [`StackHead`] / [`TaggedIndex`] code exhaustively —
//! no `preemption_bound`, so loom explores every interleaving these small
//! models admit. Several models run end-to-end through the shipped
//! [`push`](StackOps::push_index)/[`pop`](StackOps::pop_index); most of
//! the rest drive the real head atomic and the real packing through
//! `cas_head_for_test` so an interleaving can be pinned — the one exception is
//! the untagged-ABA counterfactual, which drives a locally-defined buggy
//! stand-in stack instead of the real type. `#[should_panic]` counterfactuals
//! (untagged corruption, the H-2 empty-transition tag-reset ABA, and a
//! Relaxed-CAS-failure-ordering regression) prove the harness is non-vacuous.
//! See `tests/loom_aba.rs`'s own module doc for the per-model breakdown.
//!
//! # Portability limit — requires 64-bit atomics
//!
//! The stack head is a single `AtomicU64` (the packed `(index | tag)` word — see
//! above); packing both halves into ONE atomic word is the entire mechanism that
//! makes the CAS in
//! [`push_index`](StackOps::push_index)/[`pop_index`](StackOps::pop_index)
//! atomic across index-and-tag together, so this is not an incidental
//! implementation choice. That means this crate needs `target_has_atomic = "64"`
//! and will **not compile** on a target without native 64-bit atomic support —
//! notably `thumbv6m-none-eabi`, `thumbv7em-none-eabi`, `riscv32imc-unknown-none-elf`,
//! and `armv5te-unknown-linux-gnueabi`. This crate is `no_std`-compatible, but
//! `no_std` alone does not imply 64-bit-atomic support: many Cortex-M and
//! RISC-V-without-A-extension targets are `no_std` yet lack `AtomicU64` entirely.
//! A build on an unsupported target fails fast with an explicit
//! [`compile_error!`] naming the requirement, rather than the more cryptic
//! "cannot find function/no `AtomicU64` in `core::sync::atomic`" error a bare
//! unresolved import would otherwise produce.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

// The stack head is one AtomicU64 (see the crate-doc "Portability limit"
// section above) — that requires native 64-bit atomic support from the target.
// Fail fast with an explicit, named reason instead of the cryptic "no
// `AtomicU64` in `core::sync::atomic`" unresolved-import error a naive use
// would otherwise produce on e.g. thumbv6m/thumbv7em/riscv32imc/armv5te.
#[cfg(not(target_has_atomic = "64"))]
compile_error!(
    "tagged-index-stack requires a target with native 64-bit atomics \
     (target_has_atomic = \"64\") because its head is a single AtomicU64 \
     packing the (index | tag) word atomically. This target does not have \
     them (e.g. thumbv6m-none-eabi, thumbv7em-none-eabi, \
     riscv32imc-unknown-none-elf, and armv5te-unknown-linux-gnueabi are all \
     known-unsupported) — see the crate-root doc comment's \"Portability \
     limit\" section."
);

// Loom is an OPTIONAL `cfg(loom)`-gated dependency (feature `loom`): setting
// `--cfg loom` compiles the `#[cfg(loom)]` atomic aliasing below, which
// references the loom crate — but Cargo only resolves and links that
// dependency when the implicit `loom` feature is enabled as well. Fail fast
// with a named reason instead of the cryptic "unresolved import `loom`"
// error a cfg-without-feature build would otherwise produce.
#[cfg(all(loom, not(feature = "loom")))]
compile_error!(
    "building with --cfg loom requires --features loom (loom is now an \
     optional dependency)"
);

// The entire implementation lives in ONE module gated on the exact COMPLEMENT
// of the two `compile_error!` conditions above (Sol-codex run-3 P2-4):
// `compile_error!` does not stop rustc from parsing and name-resolving sibling
// items, so under an invalid configuration the module below is cfg'd out
// entirely and the build fails with ONLY the named error — no secondary
// name-resolution error from the loom-aliasing `use` (nor from `AtomicU64` on
// a target without native 64-bit atomics). Under a valid configuration the
// module compiles and its public items are re-exported here, so every public
// path stays `tagged_index_stack::<Item>` exactly as before the restructure.
#[cfg(all(target_has_atomic = "64", any(not(loom), feature = "loom")))]
mod imp;

#[cfg(all(target_has_atomic = "64", any(not(loom), feature = "loom")))]
pub use imp::*;
