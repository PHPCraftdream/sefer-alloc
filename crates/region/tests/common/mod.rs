//! Common test utilities for the `sefer-region` crate.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A test fixture that counts how many times it's dropped via an `Arc<AtomicUsize>`.
///
/// Used for drop-once semantics tests (I5) across multiple test files.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Not used by all test files
pub struct DropCounter {
    pub id: usize,
    pub drop_count: Arc<AtomicUsize>,
}

impl DropCounter {
    #[allow(dead_code)]
    pub fn new(id: usize, drop_count: Arc<AtomicUsize>) -> Self {
        Self { id, drop_count }
    }
}

impl Drop for DropCounter {
    fn drop(&mut self) {
        self.drop_count.fetch_add(1, Ordering::SeqCst);
    }
}
