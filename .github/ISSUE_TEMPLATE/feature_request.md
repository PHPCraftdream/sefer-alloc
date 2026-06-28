---
name: Feature request
about: Propose a new capability or API for sefer-alloc
title: "[FEATURE] "
labels: enhancement
assignees: ''
---

## Use case

<!-- Describe the concrete problem or scenario this feature addresses.
     "I want X" is less useful than "When doing Y, I cannot achieve Z because..." -->


## Proposed API

<!-- If you have a specific public API in mind, sketch it here. Otherwise leave
     blank. -->

```rust
// proposed signatures / types / trait impls
```


## Alternatives considered

<!-- What workarounds exist today? Why are they insufficient? -->


## Safety and verification impact

<!-- sefer-alloc is a verification-first project. Please answer: -->

- Will this feature require adding `unsafe` code?
  - [ ] No
  - [ ] Yes — in which module(s)?

- Which verification layers will need new coverage?
  - [ ] `cargo test` (unit / integration)
  - [ ] `proptest` differential
  - [ ] `miri`
  - [ ] `loom` model check
  - [ ] ThreadSanitizer
  - [ ] `cargo fuzz`
  - [ ] Cross-arch (aarch64 weak-memory smoke)
  - [ ] None / unclear

<!-- Any concern about performance impact? Should a bench be added? -->


## Additional context

<!-- References, prior art in other allocators, links to related issues, etc. -->
