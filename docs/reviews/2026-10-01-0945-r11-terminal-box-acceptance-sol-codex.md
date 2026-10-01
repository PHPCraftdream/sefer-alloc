# R11 acceptance finding — Sol Codex

Date: 2026-10-01 Europe/Berlin. This is a dynamic acceptance finding, not a
fresh all-src review. Main HEAD is `fe65746d`; the run additionally includes
the uncommitted adaptive Small-sidecar candidate. No release GO.

## P1: owner payload write overlaps a protected real Box Drop frame

Both strict-provenance Miri configurations reject the existing installed
allocator witness `tests/miri_global_box_acceptance.rs -- paused`:

- Stacked Borrows: the allocation-root write would remove a weakly protected
  Unique tag originating at `drop(byte)`.
- Tree Borrows: the allocation-root write is foreign to the protected
  Reserved tag and would disable it.

The controller waits until the actual producer has terminal-published and
is paused inside the actual `Box::drop`/dealloc call chain. The owner then
calls `trim_current_thread`. This is not a test that merely holds a route
pin outside the producer frame. The failing write is
`Node::write_next` at `src/alloc_core/platform/node.rs:90`, called from
`reclaim_sidecar_record` at `src/alloc_core/small/alloc_core_small_reclaim.rs:63`,
through the strict sidecar sweep. It writes the returned payload's intrusive
freelist link via the allocator-origin root while the Box argument is still
protected at `tests/support/r8_global_box_witness.rs:109`.

Task `2ad6e542-852b-481a-97a2-c325fc3d99a6` ran both configurations to exit1;
neither was a timeout. Flags included strict provenance, normal validation
and normal borrowing checks; the second configuration additionally enabled
Tree Borrows. Isolation was disabled for the legitimate synchronization
witness, not validation or borrowing rules. Test source was not weakened.

Miri's aliasing models are experimental; this is not a claim of a separately
observed native crash or an exhaustive language-model proof. Nevertheless,
two real failed acceptance gates must be repaired, not treated as green.
The adaptive leaf map does not change the failing Root-derived payload-write
mechanism; this finding is separate from its System storage savings and its
one-shot OOM/rescue test interaction.

## Required follow-up in this same remediation workflow

XXS is investigating the lawful boundary between terminal publication,
payload mutation/reuse and physical deallocation while a real caller Drop
frame remains active. It must evaluate out-of-band bookkeeping, narrow
pointer permissions and allocation/provenance boundaries before an HS patch.
Removing the pause, delaying it outside the frame, disabling protectors or
switching off validation is not acceptance. Neither System-byte counters nor
the shadow pin-lifetime model closes this finding.

Keep both real installed-allocator witnesses and matching negative controls.
Do not promote the terminal-sidecar allocator to release-ready until the
mechanism and its relevant configurations pass. Pending broader platform and
performance gates remain distinct. The independent XS12 round follows
accepted repairs; it has not started at this finding's filing.
