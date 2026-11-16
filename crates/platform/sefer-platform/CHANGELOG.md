# Changelog

All notable changes to `sefer-platform` are documented here.

## [Unreleased]

- `0.1.0` — initial extraction from `sefer-alloc` by plan step P1.4
  (`docs/plans/2026-11-15-workspace-restructure-plan.md`): a pure move of
  `alloc_core::platform::{fatal, os}`'s SEGMENT-aligned OS aperture half and
  `global::process_exit` into this platform-seam crate. The root crate
  re-exports the whole surface through its `alloc_core::os` / `alloc_core::fatal`
  / `global::process_exit` conduits, so call-site paths are unchanged.
