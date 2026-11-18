# sefer-segment

`sefer-segment` owns the sefer allocator's segment metadata, segment table,
remote-free bitmap, route directory, terminal publication protocol, and the
small platform-independent helpers those implementations require. Segment
and route intentionally share one crate: their source modules depend on each
other, so splitting them would create a reverse crate dependency.

The crate is independently publishable (`0.1.0`) and carries the root
allocator feature names as default-off toggles. `sefer-alloc` enables it only
with `alloc-core` and forwards the moved source gates without altering its
production bundle. Platform VM operations come from `aligned-vmem` and
`sefer-platform`; size classification comes from `size-classes`.

## Safety

`#![deny(unsafe_code)]` applies crate-wide. The moved pointer-manipulation
seams retain their narrowly scoped `#![allow(unsafe_code)]` attributes and
existing documented invariants. No crate depends back on `sefer-alloc`.

## License

Licensed under either MIT or Apache-2.0, at your option. See `LICENSE-MIT`
and `LICENSE-APACHE`.
