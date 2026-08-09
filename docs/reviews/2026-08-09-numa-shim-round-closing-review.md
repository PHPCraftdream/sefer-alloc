# `numa-shim` round-closing review (read-only, end-to-end)

**Date:** 2026-08-09
**Reviewed range:** `b275480^..94c4a74` (the 9 fix commits) plus the two follow-up docs commits
`3adbc96` (CHANGELOG, task #749) and `b1f35f7` (checkpoint, task #750). Verified the range touches
nothing outside `crates/numa/**` and the root `CHANGELOG.md`:
`git diff --stat b275480~1..94c4a74` → `crates/numa/Cargo.toml`, `crates/numa/README.md`,
`crates/numa/src/lib.rs`, `crates/numa/tests/{cpumap_parser,mock_dispatch,smoke}.rs` only.
**Scope:** `b275480` (#697), `3899ab9` (#720), `c5e013b` (#721), `69045e3` (#722), `2cdb765` (#723),
`2efa70f` (#724), `f989bed` (#725), `53b3ca2` (#726), `94c4a74` (#727), against the original audit
`docs/reviews/2026-08-07-numa-shim-rust-intel-audit.md` (0 critical, 0 high, 12 medium, 10 info).
**Mode:** read-only. No repository file was modified except this report; `git status --porcelain`
was empty before and after every probe. All counterfactuals were run in a **workspace-detached
scratch copy** of `crates/numa` + `crates/vmem` under `%TEMP%` (deleted afterwards), plus one
standalone `%TEMP%` cargo project modelling `OnceLock` reentrancy. Verbatim output is inlined.

---

**Bottom line.** The round's *shipped parsing and API-surface changes are correct*. Every one of
the six counterfactuals that can be mechanically constructed reproduces **exactly** as claimed —
I re-ran all six independently rather than trusting the commit messages (§2), including the two
the task singled out. #697's `maxnode` reasoning is **right**, and I re-derived it from the kernel
ABI rather than accepting it (§3.1). #721's extraction is genuinely behaviour-preserving. #726's
`#[non_exhaustive]` enforcement is real, not vacuous (E0639 + E0638 reproduced). The
verification-honesty distinction (REASONED-FROM-SPEC vs empirically verified) is maintained
**consistently and correctly across all nine commit messages** — I found no commit that implies
execution of Linux-only code without saying it did not.

There is one finding severe enough to name up front.

> **F1 (HIGH): #723 put a heap allocation and a non-reentrant `OnceLock::get_or_init` onto the
> exact allocation path that `sefer-alloc`'s own M5 invariant declares reentrancy-free.** The
> pre-#723 `cpu_to_numa_node` was 100% stack-only (`[u8; 4096]`, no `Vec`); the new `topology()`
> allocates ~65 `Vec`s inside a `OnceLock` initializer, and `numa_shim::current_node()` is called
> from `AllocCore::reserve_small_segment` / `alloc_large_slow` — inside `GlobalAlloc::alloc`.
> Under the crate's own advertised Linux `#[global_allocator]` + `numa-aware` deployment this
> self-recurses into the allocator (aliasing a second `&mut HeapCore`) and then re-enters
> `get_or_init`, which **deadlocks** — demonstrated empirically on this toolchain. The commit
> claims "No production API or observable return value changed."

The second-order problem is the **evidence layer around #724**, the round's only commit claiming
EMPIRICAL verification. Two independent MEDIUMs: its stated mechanism is the *inverse* of
Microsoft's documented `nndPreferred` contract (**F2**), and the "empirical verification" it cites
passes identically against the reverted, double-committing code — I proved it (**F3**).

Three further MEDIUMs are process/bookkeeping: four audit findings closed neither in code nor in
any index (**F4**), a `CORRECTNESS_OPEN_ITEMS.md` card whose trigger fired this round and was not
updated (**F5**).

---

## 0. Current-state green check (re-run personally, not trusted from commit messages)

| Command | Result |
| --- | --- |
| `cargo test -p numa-shim --all-features` | **32 passed**, 0 failed (`cpumap_parser` 17 · `mock_dispatch` 9 · `smoke` 6 · lib 0 · doc 0) |
| `cargo test -p numa-shim --features vmem-integration --test smoke` (#724's cited real-path run) | **6 passed** — and `current_node()` really does return `Some(0)` on this host, so `reserve_aligned_numa` really is reached (not the `NO_NODE` bypass). Claim structurally valid. |
| `cargo fmt -p numa-shim -- --check` | clean (exit 0) |
| `cargo clippy -p numa-shim --all-features --all-targets -- -D warnings` | clean |
| `cargo clippy -p numa-shim --features vmem-integration --all-targets -- -D warnings` | clean |
| `cargo clippy -p numa-shim --features mock --all-targets -- -D warnings` | clean |
| `cargo clippy -p numa-shim --all-targets -- -D warnings` (**DEFAULT features**) | **FAILS** — `error: function GetCurrentProcess is never used`. Pre-existing, acknowledged in #726's message, left in place — **F9** |
| `cargo check -p numa-shim --target x86_64-unknown-linux-gnu --all-features --tests` | clean (the cross-compile every REASONED-FROM-SPEC commit cites) |
| `cargo doc -p numa-shim --features vmem-integration --no-deps` (the **exact docs.rs feature set**) | clean, **0 warnings** |
| `cargo doc -p numa-shim --all-features --no-deps` | clean, **0 warnings** |
| `grep -rn "TODO\|FIXME\|XXX\|unimplemented\|todo!" crates/numa/` | **0 hits** |
| `grep -rn "CALLS\|CURRENT_NODE_SLOT" --include=*.rs .` outside `crates/numa/src/lib.rs` | **0 hits** — #726(1)'s "no code anywhere in this workspace touched them directly" is **true** |
| **0 doctests** in every configuration | CLAUDE.md's no-doctest rule holds |

The 32/32 count in #727's message is correct. No new `unsafe` block anywhere in the round lacks a
`// SAFETY:` comment (the three new ones in `reserve_aligned_numa` and the two new ones in
`read_cpumap_bytes` all carry per-site proofs). No out-of-scope file was touched.

---

## 1. Commit-by-commit: does each diff match its own message and the audit finding it claims?

All nine diffs were read line by line against their messages and against the audit's finding text.
**Nine of nine diffs do what their messages say**, and each names the correct audit finding:

| Commit | Claims | Audit finding | Verdict |
| --- | --- | --- | --- |
| `b275480` #697 | `maxnode` 64 → 65 | §F1 (MEDIUM, lib.rs:527) | correct; reasoning independently re-derived (§3.1) |
| `3899ab9` #720 | loop-to-EOF, 4 KiB | §C4 (MEDIUM, lib.rs:383) | correct; matches the audit's own suggested fix verbatim |
| `c5e013b` #721 | extract 5 parsers + 15 tests | §D1a (MEDIUM, lib.rs:397) | correct; byte-identical logic move, verified below |
| `69045e3` #722 | 4 doc/code divergences | §F2 :146, §F1 :522, §F1 :631, §F2 :154 | all four correct |
| `2cdb765` #723 | `OnceLock` topology cache | §E5 (MEDIUM, lib.rs:309) | closes the finding, **introduces F1** |
| `2efa70f` #724 | two-call reserve-then-commit | §F2 (MEDIUM, lib.rs:694) | net effect correct, **stated mechanism inverted — F2** |
| `f989bed` #725 | scope the `# Safety` contract | §B5 (MEDIUM, lib.rs:188) | correct for the 5 named sites; **6th site left — F6** |
| `53b3ca2` #726 | 5 publish-surface decisions | §A3, §B14, §B25, §C1a, §C10 | all five correct; `#[non_exhaustive]` enforcement proven real |
| `94c4a74` #727 | 3 hygiene residuals | §B7, §B26, §D1 | all three correct |

Specific spot-verifications that carried real risk of divergence:

- **#721's "byte-for-byte identical" parsing-logic claim is true.** Diffed the extracted
  `cpumap::{format_sysfs_path, parse_contains_cpu, trim_end, nth_token, parse_hex_u32}` against
  their pre-move private originals in `git show c5e013b^:crates/numa/src/lib.rs`: the only
  differences are the rename `parse_cpumap_contains_cpu` → `parse_contains_cpu`, `pub` visibility,
  and doc comments. No arithmetic changed.
- **#723 preserves `cpu_to_numa_node`'s iteration order and semantics exactly.** Old:
  `for node in 0u32..64 { if node_contains_cpu(node, cpu_idx) { return node } } 0`. New:
  `for (node, bytes) in topology().iter().enumerate() { if !bytes.is_empty() && parse_contains_cpu(bytes, cpu_idx) { return node as u32 } } 0`.
  `topology()` is built by `(0u32..64).map(...).collect()`, so index `i` is node `i` and the scan
  order is identical. The `!bytes.is_empty()` guard is a strict no-op on behaviour: I traced the
  empty-input path by hand — `trim_end(b"")` → `b""`, `word_count = 0 + 1 = 1`, `target_word 0 < 1`,
  `left_index = 0`, `nth_token(b"", 0, b',')` → `Some(b"")`, `parse_hex_u32(b"")` → `None` → `false`.
  A node whose read failed therefore returned `false` before and is skipped now — same outcome.
  The all-empty case still falls through to `0`, matching the single-node/no-sysfs behaviour.
  **Answering the review question directly: yes, `get_or_init` populates all 64 candidates exactly
  once and `OnceLock` makes concurrent racing threads safe — one runs the closure, the others block
  on the completed value. There is no partial-state race.** The correctness regression is not there;
  it is F1.
- **#722(3)'s Windows MAXUSHORT guard** is `if ok == 0 || node == u16::MAX { return NO_NODE; }` —
  correct per the audit's stated fix, and the corrected SAFETY comment now describes the real
  BOOL-vs-OUT-parameter split.
- **#726(3)'s `ProcessorNumber` layout assertions** compile (`const _: () = { … }`) and are genuinely
  const-evaluated: `size_of == 4`, `align_of == 2`, offsets `0/2/3`. Matches the real
  `PROCESSOR_NUMBER`.

---

## 2. Counterfactuals — all six re-run independently, all six reproduce exactly

Run in a workspace-detached scratch copy (`%TEMP%/numa-cf`, a two-member virtual workspace over
copies of `crates/numa` and `crates/vmem`), restored from the pristine repo file between each.

**2.1 #726(2) — `CALLS_CAP`.** Replaced the `if b.len() < CALLS_CAP { b.push(call); }` body with a
bare `b.push(call);`:

```
test calls_log_is_capped_not_unbounded ... FAILED
panicked at crates\numa\tests\mock_dispatch.rs:150:5:
CALLS must be capped at 4096, got 5000
test result: FAILED. 8 passed; 1 failed
```

Exactly the message #726 claims. **Confirmed.**

**2.2 #727(1) — `format_sysfs_path`'s digit buffer.** Reverted `[0u8; 10]` → `[0u8; 4]`:

```
test format_sysfs_path_does_not_panic_on_five_plus_digit_node ... FAILED
panicked at crates\numa\src\lib.rs:391:17:
index out of bounds: the len is 4 but the index is 4
test result: FAILED. 16 passed; 1 failed
```

Exactly the message #727 claims (and the audit's own §B7 wording). **Confirmed.**

**2.3 #727(2) — `parse_hex_u32`'s length guard.** Deleted `if s.len() > 8 { return None; }`:

```
test parse_hex_u32_rejects_tokens_longer_than_8_digits ... FAILED
  left: Some(4294967295)
 right: None
test result: FAILED. 16 passed; 1 failed
```

Exactly the `Some(4294967295)` silent-wrap value #727 claims. **Confirmed.**

**2.4 #722(4) — the `mock` arm's `NO_NODE` remap.** Reverted to unconditional `Some(n)`:

```
test current_node_scripted_no_node_yields_none ... FAILED
  left: Some(4294967295)
 right: None
test result: FAILED. 8 passed; 1 failed
```

Exactly the `left: Some(4294967295), right: None` #722 claims. **Confirmed.**

**2.5 #721 — the word-order arithmetic.** Replaced `left_index = word_count - 1 - target_word`
with `left_index = target_word`:

```
test cpu_32_is_bit_0_of_the_leftmost_word ... FAILED
test cpu_63_is_bit_31_of_the_leftmost_word ... FAILED
test doc_example_cpus_0_and_1_set ... FAILED
test empty_token_is_rejected ... FAILED
test result: FAILED. 13 passed; 4 failed
```

**Exactly the four test names #721's message lists**, and exactly 4 of the (then-)15. **Confirmed.**
The suite is not vacuous.

**2.6 #726(4) — `#[non_exhaustive]` on the variants is genuinely enforced, not coincidentally
un-triggered.** Added a throwaway integration test in the scratch copy exercising both a
struct-literal construction and a no-`..` field pattern:

```
error[E0639]: cannot create non-exhaustive variant using struct expression
error[E0638]: `..` required with variant marked as non-exhaustive
```

Both errors fire. **Confirmed real enforcement.** The current
`crates/numa/tests/mock_dispatch.rs` does use `..` at every relevant site — `bind_range_records_args`
(`matches!` with `..`, :98-106), `reserve_on_node_chains_and_records` (two `matches!` with `..`,
:117-129), `reserve_on_node_no_node_skips_bind` (`..`, :171-174). The two `assert_eq!(calls, vec![
mock::MockCall::CurrentNode(n)])` sites still compile because `CurrentNode` is a **tuple** variant
that was deliberately not marked (see **F13**).

---

## 3. Independent re-derivation of the two "REASONED-FROM-SPEC" claims the task named

### 3.1 #697's `mbind` `maxnode` — the reasoning is CORRECT

Derived from the kernel ABI rather than the commit message. `mm/mempolicy.c`'s `get_nodes()` opens
with `--maxnode;` **before** computing the addressable-bit range, then:

```
nlongs  = BITS_TO_LONGS(maxnode);
endmask = ((maxnode % BITS_PER_LONG) == 0) ? ~0UL : (1UL << (maxnode % BITS_PER_LONG)) - 1;
```

- With `maxnode = 64` (the pre-fix value): after `--`, `maxnode = 63`; `nlongs = 1`;
  `63 % 64 = 63 ≠ 0` → `endmask = (1 << 63) - 1` = **bits 0..62 only**. Bit 63 is masked off.
  `bind_range(base, len, 63)` therefore passed an effectively empty nodemask.
  **The commit's claim is exactly right.**
- With `maxnode = 65` (the fix): after `--`, `maxnode = 64`; `nlongs = 1`; `64 % 64 == 0` →
  `endmask = ~0UL` = all 64 bits. `nlongs = 1` also means the kernel copies exactly `8` bytes from
  the user pointer — which is precisely the size of the `u64` `nodemask` on the Rust stack, so the
  fix introduces **no over-read**. I checked this specifically; it is the one way a "+1" fix of this
  shape can go wrong, and it does not here.
- `libnuma`'s bitmask-size+1 convention is correctly cited.

No finding. This is a genuinely correct REASONED-FROM-SPEC fix, honestly labelled.

### 3.2 #724's `VirtualAllocExNuma` sequence — valid API usage, but the STATED MECHANISM IS INVERTED

See **F2**. The two-call sequence is a documented, valid pattern ("You can use
**VirtualAllocExNuma** to reserve a block of pages and then make additional calls to
**VirtualAllocExNuma** to commit individual pages from the reserved block"), the `MEM_COMMIT`
prerequisite is satisfied (the whole range is reserved, so the documented
`ERROR_INVALID_ADDRESS` case cannot fire), the arithmetic keeps the commit inside the reservation
(`base - raw ∈ [0, align-1]`, so `base + size < raw + over`), and the cleanup
`VirtualFree(raw, 0, MEM_RELEASE)` is correct and leak-free (`dwSize` must be 0 for `MEM_RELEASE`;
`raw` is the exact base the reserve returned; no `Reservation` exists yet, so no double-free).
**Answering the review question directly: yes, the fix avoids double-charging, there is no Windows
version on which `MEM_RESERVE` implicitly commits, and the failure path is correct.** The defect is
in *why* the code says it works.

---

## 4. Findings

### F1 (HIGH) — #723's `OnceLock` topology cache allocates on, and can deadlock, the allocation path the consuming allocator declares reentrancy-free

`crates/numa/src/lib.rs:563-593`. The pre-#723 `cpu_to_numa_node` was **allocation-free by
construction** — verified against `git show 2cdb765^:crates/numa/src/lib.rs`: a stack `[u8; 4096]`
buffer, `return false` on failure, no `Vec`, no lock, no `static`. #723 replaced it with:

```rust
static TOPOLOGY: std::sync::OnceLock<Vec<Vec<u8>>> = std::sync::OnceLock::new();

fn topology() -> &'static Vec<Vec<u8>> {
    TOPOLOGY.get_or_init(|| {
        (0u32..64).map(|node| { … read_cpumap_bytes(path_str).unwrap_or_default() }).collect()
    })
}
```

The initializer performs **~65 heap allocations** (one `Vec<u8>` per readable node via
`buf[..total].to_vec()`, plus the outer `collect()`), and `std::sync::OnceLock::get_or_init` is
documented as **an error to re-enter**: "It is an error to reentrantly initialize the cell from
`f`. The exact outcome is unspecified. Current implementation deadlocks."

**Reachability is not hypothetical — it is this repository's own primary consumer:**

- `src/alloc_core/numa.rs:35` — `pub fn current_node() -> u32 { numa_shim::current_node().unwrap_or(NO_NODE) }`
- `src/alloc_core/alloc_core.rs:1422` — `current_node_cached()` calls it on a cache miss
- `src/alloc_core/alloc_core_small.rs:1938` — `let my_node = self.current_node_cached();` inside
  `reserve_small_segment`
- `src/alloc_core/alloc_core_large.rs:484` — the same, inside the large-segment reservation path
- Both sit inside `AllocCore::alloc`, which is what `SeferAlloc`'s `unsafe impl GlobalAlloc`
  (`src/global/sefer_alloc.rs:967`) drives.

And that path has an explicit, load-bearing no-allocation invariant:

- `src/alloc_core/alloc_core.rs:9-11` — "`AllocCore` itself contains NO `unsafe` and NO
  `Vec`/`Box`/`HashSet`/`std::alloc` — the alloc path is therefore **reentrancy-free (M5)**: it
  cannot recurse into the global allocator because it allocates no metadata through it."
- `src/global/tls_heap.rs:20-22` — the TLS routing deliberately **dropped its `RefCell` refusal
  guard** because "Reentrancy is structurally excluded by M5 (no `Vec`/`Box`/`std::alloc` on the
  alloc path), so there is no reentrant mutation to guard against." The raw `Cell<*mut HeapCore>`
  now hands out a `&mut HeapCore` on **every** call with no borrow check.

So on Linux, with `sefer-alloc` installed as `#[global_allocator]` and `numa-aware` on (the exact
configuration `src/lib.rs:47` and `README` advertise, and which `ci.yml:629` compiles as
`--features "production numa-aware"`; `production` includes `alloc-global`, `Cargo.toml:413`), the
first allocation that needs a segment does:

```
GlobalAlloc::alloc → tls_heap::current() → &mut HeapCore (#1)
  → reserve_small_segment → current_node_cached()      [cache is None]
    → numa_shim::current_node() → cpu_to_numa_node → topology()
      → OnceLock::get_or_init(closure)
        → Vec::to_vec()/collect() → GlobalAlloc::alloc
          → tls_heap::current() → &mut HeapCore (#2, ALIASES #1 — UB)
          → reserve_small_segment → current_node_cached()  [still None]
            → … → topology() → OnceLock::get_or_init  ← REENTRANT: DEADLOCK
```

I confirmed the terminal step empirically on this exact toolchain (rustc 1.97.0) with a standalone
`%TEMP%` project whose `OnceLock` initializer re-enters `get_or_init`:

```
STILL RUNNING after 3s -> reentrant get_or_init DEADLOCKED
```

Two independent defects on one path: the aliasing `&mut HeapCore` is UB the moment the inner
allocation is served at all, and the `get_or_init` re-entry is a hard hang.

**Why nothing caught it.** No CI job executes the real Linux `platform` module. `numa-shim-mock`
(ubuntu) runs only `--features mock` / `"mock vmem-integration"`, which bypasses `platform`
entirely; `numa-shim-windows` / `numa-shim-macos` run the *other* platform blocks; the root
`--all-features` job enables `numa-aware-mock`, which turns `mock` on. The weekly
`numa-real-kernel` job does exercise real Linux, but its test binaries do **not** install
`#[global_allocator]` (grep-verified: the only `global_allocator` occurrences in `src/` and the
three `tests/numa_*.rs` files are inside doc examples), so it drives `AllocCore` with the system
allocator underneath and cannot reproduce the recursion either.

**The commit's own summary is therefore an overclaim:** "No production API or observable return
value changed — `current_node()` returns the identical node for the identical CPU as before, only
with fewer syscalls after the first call." The *return value* is unchanged; the *allocation
behaviour and reentrancy profile* are not, and that is the property the consumer depends on.

**Suggested closure** (not applied — read-only review): make the populate allocation-free. The
natural shape is a fixed `[[u8; 4096]; 64]`-equivalent or, cheaper, a
`static TOPOLOGY: [AtomicU64; 64]`-style pre-parsed bitmask (the audit's own §E5 fix text says
"hoist the parsed topology into a `std::sync::OnceLock` of per-node **CPU masks**", i.e. parsed
fixed-width masks, not raw byte `Vec`s — #723 deliberately deviated to "preserve full correctness
for hosts with >64 CPUs per node", which is what forced the heap). Alternatives: keep the raw-bytes
design but back it with a `static mut`-free, allocation-free arena, or use a hand-rolled
`AtomicU8` state machine (`UNINIT/INITIALIZING/READY`) that **fails open to the old per-call path**
on re-entry instead of blocking — the same shape `racy-ptr-cell` already implements in this
workspace. At minimum, `current_node()`'s public rustdoc must state that the first Linux call
allocates and must not be called from an allocator.

---

### F2 (MEDIUM) — #724's stated mechanism is the exact inverse of Microsoft's documented `nndPreferred` contract; the fix works only because of a step its own message calls decorative

`crates/numa/src/lib.rs:909-989`. The code, the doc comment, the commit message and the CHANGELOG
all state the same mechanism in four places:

> "`node` has no effect on a reserve-only call (no physical pages are allocated yet); passing it
> through is harmless and keeps this one call site…" (SAFETY comment, `:921-927`)
>
> "This is the call that actually allocates physical pages, so `node` takes effect here."
> (SAFETY comment, `:945-952`)
>
> "…still via `VirtualAllocExNuma` so the NUMA node preference is honored at the point physical
> pages actually get allocated — reservation alone allocates no physical memory, so the node
> argument is immaterial to it." (rustdoc, `:886-894`)

Microsoft's reference page for `VirtualAllocExNuma` says the opposite, twice:

> **`nndPreferred`**: "The NUMA node where the physical memory should reside. **Used only when
> allocating a new VA region (either committed or reserved). Otherwise this parameter is ignored
> when the API is used to commit pages in a region that already exists.**"
>
> Remarks: "Because **VirtualAllocExNuma** does not allocate any physical pages, it will succeed
> whether or not the pages are available on that node… **The physical pages are allocated on
> demand.**"

So: the **`MEM_RESERVE` call** is "allocating a new VA region (reserved)" → `nndPreferred` **is**
used there. The **`MEM_COMMIT` call** commits "pages in a region that already exists" →
`nndPreferred` is **ignored** there. And no `VirtualAllocExNuma` call ever "actually allocates
physical pages"; Windows does that at first touch.

The net shipped behaviour is still correct — the node preference is recorded by the reserve call
and the commit charge is genuinely halved — but it is correct *by accident*. The one thing that
keeps NUMA binding alive is passing `node` on the reserve call, which the commit message
explicitly frames as a cosmetic convenience:

> "reserves address space only, no physical pages allocated, so `node` has no effect on this call
> (kept for API uniformity rather than adding a second non-NUMA `VirtualAlloc` import just for
> the reserve step)."

That is a live regression trap. A future reader who believes the comment has every reason to drop
`node` from the reserve step (it is documented as a no-op) and keep it only on the commit step
(documented as where it matters) — at which point NUMA binding on Windows, this crate's headline
Windows capability, **silently stops working entirely**: both calls still succeed, no error is
returned, and no test would notice (see F3). This is precisely the §F1/§F2 "documented semantics
diverge from real behaviour" class the round existed to close, newly introduced by the round.

**Suggested closure:** correct all four sites to say the preference is established by the
`MEM_RESERVE` call and ignored by the `MEM_COMMIT` call (quoting the `nndPreferred` sentence),
and re-label the `node` argument on the reserve call as **load-bearing, not uniformity**. The
`node` argument on the commit call can stay (harmless, ignored) or be dropped, but that choice
should be stated.

---

### F3 (MEDIUM) — #724's "EMPIRICALLY VERIFIED" evidence passes identically against the pre-fix code; the round's one Windows-native behavioural fix has no regression test

#724 is the round's only commit claiming empirical rather than reasoned verification:

> "**Verification — EMPIRICALLY VERIFIED, not just reasoned-from-spec** … Ran the real (non-mock)
> code path directly: `cargo test -p numa-shim --features vmem-integration --test smoke` … 6/6
> pass, including `reserve_on_node_returns_valid_span` and `reserve_on_node_large_align_round_trip`,
> both of which exercise this exact function on real Windows hardware."

The premise is sound — I verified `current_node()` returns `Some(0)` on this host, so `node != NO_NODE`
and `reserve_aligned_numa` really is reached. But the conclusion does not follow. In the scratch
copy I reverted #724 to the pre-fix single `MEM_RESERVE | MEM_COMMIT` call over `over` bytes
(deleting the second call and its failure branch) and ran the exact cited command:

```
--- pre-#724 (single reserve+commit of `over`) smoke run ---
running 6 tests
test bind_range_no_node_is_noop ... ok
test bind_range_zero_len_is_noop ... ok
test current_node_returns_valid_or_none ... ok
test bind_range_on_owned_memory_does_not_panic ... ok
test reserve_on_node_returns_valid_span ... ok
test reserve_on_node_large_align_round_trip ... ok
test result: ok. 6 passed; 0 failed
```

**6/6 green against the bug.** Both cited tests assert alignment, `len()`, and byte-level
readback — none of which the double-commit changed. The "empirical verification" therefore
establishes only that the *new* code still works; it is zero evidence that the *bug* was fixed,
and it is exactly the "a test that would pass even if the fix were wrong" shape this project's
zero-trust discipline exists to catch. Every other fix in the round that *could* get a
counterfactual got one (§2); this one — the only one running on native hardware, where a
counterfactual is cheapest — got none, and the CHANGELOG's blanket sentence "Every fix that could
be counterfactually tested on this host was" is therefore not quite true.

**Suggested closure:** a Windows-gated regression test asserting the process commit charge grows
by ≈`size`, not ≈`size + align`, across a `reserve_on_node(4 MiB, 4 MiB, node)` — e.g. via
`GetProcessMemoryInfo`'s `PagefileUsage`/`PrivateUsage` delta, or `VirtualQuery` over the
`over`-byte span asserting the tail pages report `MEM_RESERVE` and not `MEM_COMMIT`. `VirtualQuery`
is the cheaper and more deterministic of the two and needs no new crate dependency. This would also
guard the F2 regression path.

---

### F4 (MEDIUM) — four audit findings were closed neither in code nor in any open-items index

CLAUDE.md's "Round start: check BOTH open-items indexes" rule requires that, for each item a
review flags, the round "closes it, defers it (with a one-line reason appended in the relevant
index), or leaves it — none may be silently ignored," and that a newly-flagged open item is added
to the appropriate index **in the same commit**. `git diff --stat b275480~1..94c4a74` shows the
round touched neither `docs/CORRECTNESS_OPEN_ITEMS.md` nor `docs/perf/OPEN_ITEMS.md`. Four audit
findings therefore have no durable record anywhere:

1. **§D1a (MEDIUM), `lib.rs:531` — "the mbind path (the crate's key selling point) has no
   behavioral oracle anywhere."** This is a *separate* MEDIUM from the §D1a cpumap-parser finding
   #721 closed. #721's message is admirably honest about declining it ("Not claiming this half of
   the finding closed"), but that admission lives **only in commit-message prose** — the exact
   failure mode CLAUDE.md's index rule exists to prevent, and exactly what the preceding
   `aligned-vmem` round's F13 fixed for its own deferrals (items 42-43).
   `grep -rn "mbind" docs/CORRECTNESS_OPEN_ITEMS.md docs/perf/OPEN_ITEMS.md` → **0 hits**.
   The audit's own suggested closure is cheap and does not need multi-node hardware: "an env-guarded
   Linux test asserting the syscall return is 0 for a valid single-node bind (a wrong syscall number
   yields -1/ENOSYS and goes red)". A weekly-`numa-real-kernel`-gated version of that would also be
   the only thing capable of catching a future #697-style `maxnode`/marshalling regression.
2. **§A2 (INFO), `lib.rs:100` — `CURRENT_NODE_SLOT: RefCell<u32>` where `Cell<u32>` would do.**
   Untouched by #726 (which narrowed its visibility but left the type) and unmentioned in any
   commit. The audit's argument is not cosmetic: `Cell<u32>` "structurally cannot participate in
   the §B17 reentrant-borrow hazard this very module documents and defends against for its sibling
   `CALLS` cell" — and `set_current_node` still uses a **panicking** `borrow_mut()` (`:149`), not
   the `try_borrow_mut` its sibling `record()` was deliberately given.
3. **§A3 (INFO), `lib.rs:231` — `aligned_vmem::Reservation` in a public signature couples
   `numa-shim`'s semver to `aligned-vmem 0.2.`** The audit called it a
   "Documentation/re-export decision only", explicitly to be settled before first publish
   (task #657, which this round is the gate for). `grep -rn "pub use aligned_vmem" crates/numa/`
   → **0 hits**; no doc note either.
4. **The round-wide REASONED-FROM-SPEC status itself.** #697's `maxnode`, #720's read loop and
   #723's cache are all Linux-only and have never executed anywhere — and, per F1's CI analysis,
   are not scheduled to. `aligned-vmem`'s round filed exactly this shape as item 43
   ("Deferred verification — … REASONED-FROM-SPEC for 4 of 6 affected targets, never empirically
   executed"). `numa-shim`'s has no counterpart.

---

### F5 (MEDIUM) — `CORRECTNESS_OPEN_ITEMS.md` item 42's "Next trigger" fired in #726 and the card was not updated in that commit

`docs/CORRECTNESS_OPEN_ITEMS.md:1848-1888`, item 42, filed by the *previous* round (task #776/F13)
specifically to hand this decision to this round:

> **Status:** OPEN — decision recorded, not yet revisited.
> **Next trigger:** when numa-shim's round reaches its own §C10 finding (mock feature-unification
> hazard), apply the SAME doc-only resolution task #715 chose here, citing this item and task
> #715's own reasoning…

#726 did *exactly* what the item prescribes — doc-only, citing #715 — which is the right call. But
the card was never updated: it still reads "OPEN — decision recorded, not yet revisited" and its
Next trigger still forward-references `numa-shim`'s round as future work, with no mention of
`53b3ca2`. This is the specific defect CLAUDE.md names: "A closed item that still sits in an active
tier with no Status-card update is a structural defect — the round that closes it MUST update the
card to `Status: CLOSED` and move the narrative in the SAME commit". #726's message also does not
cite item 42 (only task #715 directly), so the linkage is one-directional.

**Suggested closure:** update item 42's Status card to record that `numa-shim`'s §C10 was resolved
doc-only in `53b3ca2` consistently with `aligned-vmem`, and either close it (both crates now have
the same recorded policy, and the revisit condition is "gains real external consumers") or restate
the remaining trigger explicitly as the two-crate joint revisit.

---

### F6 (LOW-MEDIUM) — #725's "all five test call sites contract-compliant by construction" leaves a sixth, audit-flagged site still non-compliant

#725 and its CHANGELOG bullet both say the rewording makes "all five existing test call sites
contract-compliant by construction." That is true of the five sites the audit's §B5 MEDIUM named
(`mock_dispatch.rs:45,56,67` — now :68,:79,:90 — and `smoke.rs:59,68` — now :71,:86), all of which
short-circuit. But the audit separately flagged a **sixth** `bind_range` call site as
§B5-INFO (`crates/numa/tests/smoke.rs:46`):

```rust
let mut buf: Vec<u8> = vec![0u8; page];
let node = current_node().unwrap_or(0);          // -> Some(0) on any real host
unsafe { bind_range(base, len, node) };          // node == 0, len == 4096: NOT short-circuited
```

This is the *only* test call site that reaches a real platform backend, and the reworded contract
still demands "a valid **OS reservation** owned exclusively by the caller" — which a `Vec<u8>`
inside the global heap's mapping is not. The audit's own suggested fix was to fold this in during
the same rewording: "require 'a valid **mapped range**' (which a heap buffer satisfies), and note
that mbind policy applies at page granularity so surrounding same-page data may be affected."
#725 kept "OS reservation" verbatim and did not mention the residual, so the finding is neither
closed nor recorded. (Practically harmless — `mbind(MPOL_PREFERRED)` on heap pages is kernel
metadata — but it is the exact contract-letter problem #725 set out to eliminate, and the
page-granularity caveat the audit asked for is genuinely useful to a published crate's readers.)

Two smaller notes on the same doc: the reworded `# Safety` section now embeds ~13 lines of
task-#725 historical narrative *inside* the `# Safety` heading, which renders on docs.rs as part of
the safety contract a consumer must satisfy. Prior rounds in this sweep moved such narratives to a
non-`# Safety` paragraph; worth matching.

---

### F7 (LOW-MEDIUM) — the new `CALLS_CAP` silently truncates, and `drain()`'s public rustdoc still promises it does not

`crates/numa/src/lib.rs:142-145`:

```rust
/// Drain every recorded call since the last drain (or test start).
pub fn drain() -> Vec<MockCall> { … }
```

After #726 this is false past 4096 entries: `record()` silently stops pushing and `drain()` returns
a truncated prefix with **no signal** — no counter, no flag, no `Result`, nothing a caller can
observe. The cap is documented only on `const CALLS_CAP`, which is **private** and therefore does
not render in rustdoc at all; the `mock` module's own doc does not mention it either. A downstream
test that drives >4096 mocked calls and asserts on `drain().len()` would get a silently wrong
answer — the same silent-truncation shape §C4 flagged in the cpumap reader and #720 fixed by
failing closed. (Answering the review question directly: no test in this repo relies on a
full-set `drain()` — all four `drain()`-asserting tests drain immediately — so there is **no
current** breakage; the hazard is the undocumented public contract, not a live bug.)

**Suggested closure:** state the cap and the drop-oldest-`None`/keep-first policy on `drain()`'s
own rustdoc and on the `mock` module doc, and consider making `CALLS_CAP` a `pub const` so a
downstream test can assert against it instead of hardcoding `4096` (which
`mock_dispatch.rs:151` currently must do, with a comment admitting the mirror).

---

### F8 (LOW) — CHANGELOG credits the 15 cpumap tests to task #720; they are task #721's

`CHANGELOG.md:265`, in the #723 bullet:

> "…but the parser correctness of the cached-bytes call site IS empirically verified — it's the
> same `parse_contains_cpu` **`#720`'s 15 tests** already exercise."

`tests/cpumap_parser.rs` and its 15 tests were added by **#721** (`c5e013b`); #720 (`3899ab9`) added
no tests at all and says so explicitly in its own message ("No new automated test was added in this
task"). The immediately-preceding CHANGELOG bullet gets it right ("Task #721 … Added
`tests/cpumap_parser.rs`, 15 tests"). Single-word fix.

*(Not a finding: the #720 bullet's "returns `false`" is accurate for #720's own commit, even though
#723 later changed that helper's signature to return `None`. Per-task historical bullets are
correct as written.)*

---

### F9 (LOW) — `clippy -D warnings` fails in the DEFAULT feature configuration, which is what a published consumer gets, and no CI row covers this crate

```
$ cargo clippy -p numa-shim --all-targets -- -D warnings
error: function `GetCurrentProcess` is never used
    --> crates\numa\src\lib.rs:1017:12
```

Genuinely pre-existing (the symbol predates the round) and #726's message flags it as known and
unrelated. But three things make it worth closing before publish rather than after: (a) *default
features* is the configuration `cargo add numa-shim` produces, so **every** downstream Windows
consumer sees this warning in their build output; (b) `crates/numa` has **no clippy row anywhere in
`ci.yml`** — the repo's six clippy rows are all root-crate rows, and the four `numa-shim-*` jobs run
`cargo test` only, so nothing would catch a regression here either; (c) #726/#727's stated scope was
publish-surface hygiene, and the sibling `aligned-vmem` round treated exactly this class (its #719
"blanket `dead_code`" narrowing) as in-scope. One-line fix:
`#[cfg(feature = "vmem-integration")]` on the `GetCurrentProcess` declaration (its only two call
sites are already so gated), or move it into the `vmem-integration`-gated `extern "system"` block
next to `VirtualAllocExNuma`/`VirtualFree`.

---

### F10 (LOW) — the CHANGELOG's "Runtime improvements: 2" is inconsistent with its own R30-12 prefix taxonomy

The round header counts #697 and #723. But #697 shipped as `fix(perf)`, whose R30-12 definition is
"…**no** runtime algorithm or default's OBSERVABLE behavior changed (only its internal
correctness/consistency did)" — while #697's own message argues the opposite ("this changes REAL
observable behavior on Linux (node 63 binding now genuinely takes effect…)"). Meanwhile #724, also
`fix(perf)`, genuinely halves Windows commit charge for every NUMA reservation — an observable
runtime resource change — and is **not** counted. Either both belong in the count or neither does;
as written a reader reconciling the header against `git log --oneline` gets two different answers.
(Low impact: this is a framing inconsistency in a round that is otherwise scrupulous about the
runtime-vs-measurement distinction, not an overclaim of speed.)

---

### F11 (INFORMATIONAL) — stale hardcoded line-number references in the macOS-stub comment

`crates/numa/src/lib.rs:1056-1058`: "matching the Linux/Windows/fallback blocks at
`:259`/`:608`/`:812` above". Those were written by `dc003c9` (pre-round) and were accurate for two
of the three at the time (Linux 259 ✓, Windows 608 ✓, fallback was actually 822). After this round's
+421-line growth the real values are **502 / 817 / 1115** — all three now wrong. Prior reviews in
this sweep have flagged hardcoded counts/positions in comments (see the `aligned-vmem` round's F10);
replacing them with a `grep`-able description ("the three sibling `mod platform` blocks") would be
drift-proof.

---

### F12 (INFORMATIONAL) — #723's hotplug caveat covers "after the first call" but not "during the populate", and `current_node`'s public doc says nothing about first-call cost

The `TOPOLOGY` doc correctly states "CPU-hotplug changes **after the first call** are NOT
reflected". It does not cover the narrower window the change also opens: `topology()` reads 64
sysfs files sequentially, so a hot-plug landing mid-scan can freeze a **torn** snapshot for the
process's whole lifetime (a CPU present in no cached map → permanent `Some(0)` fallback for that
CPU). The old per-call derivation self-corrected on the next call; the cached one cannot. Separately,
`current_node`'s **public** rustdoc (`:183-203`) still describes only the return-value contract —
nothing about the first Linux call performing up to 64 `open`/`read`/`close` triples and (per F1)
allocating. For a crate whose selling point is "zero dependencies, `forbid(unsafe_code)`-friendly
for consumers", first-call cost and allocation behaviour are contract-level facts.

---

### F13 (INFORMATIONAL) — `MockCall::CurrentNode` was left without variant-level `#[non_exhaustive]`; the §C1a hazard still applies to it

#726(4) added `#[non_exhaustive]` to `BindRange` and `ReserveOnNode` — correctly, those are the two
the audit named. `CurrentNode(u32)` is a tuple variant and was left bare, so adding a second field
to it remains a semver-major break for downstream matches — the identical hazard §C1a describes,
just on the one variant the audit did not enumerate. This may well be the right call (marking it
would force `mock_dispatch.rs`'s two `assert_eq!(calls, vec![MockCall::CurrentNode(n)])` sites into
`matches!` form and cost the equality oracle), but the decision is neither recorded nor visible —
and §C1a's own framing is "decide at first publication — v0.1.0 is the moment." One sentence in the
enum's doc stating that `CurrentNode`'s single-field shape is deliberately frozen would close it.

---

## 5. Areas I checked and found nothing wrong (stated explicitly, per the review contract)

- **Verification-honesty consistency across all nine commits.** I read every commit message in full
  looking for a claim of execution that did not happen. There is none. Every Linux-only commit
  (#697, #720, #723, and #722's items 1-2) says REASONED-FROM-SPEC in so many words and names the
  cross-compile it actually ran; #721 goes further and explicitly *declines* to claim the mbind half
  of §D1a; #722 separates its four items by verification class (item 4 counterfactual, item 3
  reasoned from the Microsoft contract, items 1-2 doc-only); #724 is the only EMPIRICAL claim and
  its premise (the real path is reached on this host) is true. This is the round's strongest
  quality. The problems with #724 (F2/F3) are about *what the empirical run proves*, not about
  dishonesty over *whether it happened*.
- **`OnceLock` thread-safety and population completeness** (the review's question 1). All 64
  candidates are populated exactly once; concurrent callers cannot observe a partial state; the
  cached iteration reproduces the old order and the old node-0 fallback exactly. Verified by hand
  including the empty-`Vec` parse path. The defect is F1's reentrancy, not the concurrency.
- **`VirtualFree(raw, 0, MEM_RELEASE)` cleanup** (the review's question 2, second half). Correct
  and leak-free: `dwSize` must be 0 for `MEM_RELEASE`, `raw` is exactly the reserve's return value,
  and no `aligned_vmem::Reservation` has been constructed yet, so `Drop` cannot double-release.
- **`reserve_aligned_numa` arithmetic.** `over = size.checked_add(align)?` (overflow-safe);
  `base = round_up(raw, align)` gives `base - raw ∈ [0, align-1]`, so `base + size < raw + over` —
  the commit is provably inside the reservation. The now-uncommitted alignment slack is not a
  problem for the in-repo consumer: `sefer-alloc` only ever touches `[base, base+usable)`, and the
  non-NUMA Windows path (`aligned-vmem`'s `win_reserve_commit`) has always behaved this way, so
  #724 *restores* parity rather than breaking it.
- **`CALLS_CAP` under the R11-5 reentrancy hazard** (the review's question 3, first half). The cap
  check sits inside the `try_borrow_mut` guard, so a re-entrant `record()` still fails the borrow
  and drops silently exactly as before; the cap adds no new borrow, no allocation, and no panic
  path. Correct as placed. (The doc gap is F7.)
- **`#[non_exhaustive]` enforcement and every pattern site** (the review's question 4). Proven real
  (§2.6); `mock_dispatch.rs` uses `..` at all four relevant sites.
- **New `unsafe`.** Every new `unsafe` block in the round (`reserve_aligned_numa`'s three,
  `read_cpumap_bytes`'s two) carries its own per-site `// SAFETY:` proof. No new `pub unsafe fn`,
  no new `dbg_*`-shaped safe-`pub fn`-taking-a-raw-pointer hook, no new module-level
  `allow(unsafe_code)`.
- **Vacuous tests.** All 17 `cpumap_parser.rs` tests assert real postconditions and 4+2 of them
  were shown to fail against reverted code (§2.2, §2.3, §2.5). All 9 `mock_dispatch.rs` tests
  assert on drained call logs. The two `smoke.rs` no-op tests *are* structurally unfalsifiable —
  which is exactly what #727(3) documents in their doc comments rather than pretending otherwise;
  that is the audit's own second offered option and an honest resolution.
- **Out-of-scope edits, TODO/FIXME, dead code, doc warnings.** None (see §0). `cargo doc` is clean
  in both the docs.rs feature set and `--all-features`.
- **README / `Cargo.toml` / crate-doc `mock` warnings** added by #726(5). Consistent with each
  other and with `aligned-vmem`'s wording; the `node >= 64` deviation is documented on
  `bind_range`'s rustdoc, at the guard, and in `README.md`, as #722(2) claims.

---

## 6. Recommended disposition

| # | Severity | One-line action |
| --- | --- | --- |
| F1 | **HIGH** | Make `topology()`'s populate allocation-free (or fail-open on re-entry); restore M5 for the `alloc-global` + `numa-aware` Linux path; correct #723's "no observable change" claim. |
| F2 | MEDIUM | Correct the four sites stating `nndPreferred` applies at commit; mark the reserve call's `node` argument load-bearing. |
| F3 | MEDIUM | Add a `VirtualQuery`-based commit-charge regression test for #724; soften the CHANGELOG's blanket counterfactual sentence. |
| F4 | MEDIUM | File four items into `docs/CORRECTNESS_OPEN_ITEMS.md`: §D1a mbind oracle, §A2, §A3, round-wide never-executed-on-Linux status. |
| F5 | MEDIUM | Update item 42's Status card to record `53b3ca2` as the numa-shim-side resolution. |
| F6 | LOW-MED | Adopt the audit's "valid mapped range" wording + page-granularity note; move #725's narrative out of the `# Safety` heading. |
| F7 | LOW-MED | Document `CALLS_CAP` on `drain()`; consider `pub const CALLS_CAP`. |
| F8 | LOW | CHANGELOG:265 — `#720`'s 15 tests → `#721`'s. |
| F9 | LOW | Gate `GetCurrentProcess` behind `vmem-integration`; consider a `numa-shim` clippy row in `ci.yml`. |
| F10 | LOW | Reconcile "Runtime improvements: 2" with the `fix(perf)` definition (include #724 or exclude #697). |
| F11-F13 | INFO | Line-number references; hotplug-during-populate + first-call-cost docs; `CurrentNode` non_exhaustive decision. |

`numa-shim`'s crates.io publish (task #657) should stay gated until at least **F1** is closed —
it is a latent hang in the crate's own headline consumer configuration, and it did not exist before
this round.
