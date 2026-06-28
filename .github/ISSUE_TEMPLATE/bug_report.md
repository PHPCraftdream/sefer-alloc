---
name: Bug report
about: Report a correctness, safety, or behavioural defect in sefer-alloc
title: "[BUG] "
labels: bug
assignees: ''
---

## Description

<!-- A concise, one-paragraph description of the bug. -->


## Steps to reproduce

<!-- Numbered steps to trigger the bug. -->

1.
2.
3.


## Expected behavior

<!-- What should happen. -->


## Actual behavior

<!-- What actually happens (panic message, wrong return value, UB, etc.). -->


## Minimal reproducer

<!-- A self-contained Cargo project or a single test file that demonstrates the
     bug. The smaller the better. -->

```rust
// paste code here
```


## Environment

| Field | Value |
|---|---|
| `rustc --version --verbose` | |
| OS / distro | |
| Target triple | |
| `sefer-alloc` version | |
| Feature flags enabled | |


## Diagnostic output

<!-- Paste the full panic message, stack trace, sanitizer report, or Miri
     output below. -->

```
paste output here
```


## Verification layer that caught this

<!-- Check all that apply. -->

- [ ] Plain `cargo test`
- [ ] `miri`
- [ ] ThreadSanitizer (TSan)
- [ ] Valgrind / memcheck
- [ ] `loom` model check
- [ ] `cargo fuzz`
- [ ] Found in production / benchmarks
- [ ] Other (describe below)


## Additional context

<!-- Any other information: related issues, workaround discovered, bisect
     result, etc. -->
