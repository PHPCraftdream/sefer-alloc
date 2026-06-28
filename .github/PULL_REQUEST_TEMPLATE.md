# Pull request

## Summary

<!-- 1-3 sentences describing what this PR changes and why. -->


## Related issue(s)

<!-- Closes #NNN  /  Fixes #NNN  /  Part of #NNN — or "N/A". -->


## Change type

- [ ] Bug fix (non-breaking)
- [ ] New feature (non-breaking)
- [ ] Breaking change (API or behaviour change)
- [ ] Refactor / cleanup (no behaviour change)
- [ ] Documentation / comments only
- [ ] CI / tooling


---

## Checklist

### Required for every PR

- [ ] `cargo fmt --all` — no formatting diff
- [ ] `cargo build --all-targets --all-features` — zero warnings
- [ ] `cargo clippy --features production -- -D warnings` — clean
- [ ] `cargo test --features production` — all tests green
- [ ] `CHANGELOG.md` updated under `[Unreleased]`
- [ ] New public items have doc-comments (`///`)

### Required when touching core data structures

- [ ] `cargo test --features alloc-core --test alloc_core_differential`
  (proptest differential) — green, or N/A

### Required when touching concurrent / atomic paths

- [ ] Loom model check (`tests/loom_*.rs`) — green, or N/A
- [ ] ThreadSanitizer run (`RUSTFLAGS="-Z sanitizer=thread"`) — clean, or N/A
- [ ] Cross-arch build (`aarch64-unknown-linux-gnu`) — builds, or N/A

### Required when adding or modifying `unsafe`

- [ ] Every `unsafe` block has a `// SAFETY:` comment naming the invariants
      upheld
- [ ] `cargo +nightly miri test` on relevant invariant tests — clean, or N/A
- [ ] Unsafe code is confined to the allowed modules (`concurrent::hand`,
      `byte::byte_region`, `byte::byte_allocator`)

### Performance

- [ ] Benchmark added / updated in `benches/` (if performance-sensitive), or N/A
- [ ] No measurable regression vs. `main` on affected workloads, or regression
      is justified in the summary above


---

## Notes for reviewers

<!-- Anything that will help the reviewer understand tricky parts, design
     trade-offs, or areas where you are less confident. -->
