//! Self-test entry for the ghost macros (`internals` + a ghost cfg only):
//! records one `ghost/probe` event under `key`, then asserts `cond`.

pub fn probe(cond: bool, key: u64) {
    crate::ghost::ghost_event!("ghost/probe", key, 1);
    crate::ghost::ghost_assert!("ghost/probe", cond, "probe condition false (key {key})");
}
