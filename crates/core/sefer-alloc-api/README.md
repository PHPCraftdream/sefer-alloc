# sefer-alloc-api

Opt-in Rust 1.100 `Allocator` adapter for SeferAlloc. `SeferAllocator` owns a
small `SeferAlloc` handle and routes allocations through its process-wide
GlobalAlloc path; allocations remain releasable after the allocating thread
exits. It is not `Clone` or `Copy`.

SeferAlloc operations may panic internally. Each nonzero `GlobalAlloc` call is
made synchronously through a private `unsafe extern "C"` trampoline. The call
record is stack-local and exclusively borrowed until callback return; neither
callback nor context pointers are retained. When a callback panics, the process
panic hook runs before unwinding reaches this boundary. Rust's non-unwind ABI
behavior aborts if a panic reaches the boundary. The hook may execute code and
allocate, and a panic in the hook itself can also abort. OOM may abort in the
underlying allocator, so `AllocError` is not a guarantee of recoverable OOM.
Deterministic allocation failure and failure-preserves-old-block behavior are
not tested because no public deterministic failure control exists.

## Contract coverage

The integration tests in `tests/contract.rs` cover:

- `alignment_zst_and_collection_lifetimes`: checks 4096-byte alignment, zero-sized
  layout behavior, `Vec::new_in`, `Vec::with_capacity_in`, and `Box::new_in`;
  without aligned allocation or collection compatibility, its assertions fail.
- `grow_shrink_change_alignment_and_zero_tail`: checks changed alignment in both
  resize directions, preserved prefix, and shrink-to-zero semantics; without
  correct resize copying/alignment/zero-size behavior, its assertions fail.
- `zeroed_allocation_clears_reused_requested_bytes`: dirties and frees a block,
  then asserts address reuse and all requested bytes zero. Without the
  `allocate_zeroed` guarantee on reused storage this assertion fails.
- `grow_zeroed_clears_reused_same_class_extension`: keeps the source live while
  dirtying/freeing a same-class target, then asserts the replacement reuses
  that exact address and the extension is zero. Without zeroing the grow tail,
  or if the freed target is not reused, the assertion fails.
- `cross_thread_drop_after_owner_exit_reclaims_large_block`: an isolated child
  allocates a large boxed slice on a producer thread, ends that thread before
  freeing on the child main thread, drains cross-thread ingress, and checks the
  existing `large_xthread_reclaimed` counter increased. Without foreign-free
  reclamation this counter assertion fails.
- `panic_boundary_child` (`panic-test` only): hidden subprocess target. The
  parent launches this exact test with `SEFER_ALLOC_API_PANIC_CHILD`; the marker
  confirms execution reached the deliberate callback panic. It is inert during
  ordinary parent-suite execution.
- `panic_crossing_trampoline_aborts_child_process` (`panic-test` only): runs
  the exact hidden `panic_boundary_child` test in a subprocess. It requires Linux
  SIGABRT (signal 6), no ordinary exit code, the non-unwinding runtime diagnostic,
  the exact panic payload line (`probe`), and the child-entry
  marker. Missing marker or success fails; the parent never invokes the probe.

`tests/miri_zst.rs` is a separate, isolated Miri target. Its
`zero_sized_layouts_return_aligned_non_null_dangling_pointers` test checks the
aligned non-null dangling representation at several alignments and no-op
zero-sized deallocation; it intentionally avoids platform-backed allocation.
Without aligned dangling pointers or if zero-sized deallocation enters the
backend, its assertions fail.

Unsafe-code allowance audit: `src/lib.rs` has 8 item-level
`#[allow(unsafe_code)]` attributes, `tests/contract.rs` has 5, and
`tests/miri_zst.rs` has 1 (14 total). The canonical check is:

```sh
grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' crates/core/sefer-alloc-api
```

Contract-test allowances are scoped only
to the four functions containing unsafe blocks and the cross-thread child.
There are no runnable doctests.
