//! Persistent (structurally shared) page table for `LockFreeRegion`.
//!
//! A left-packed 16-ary trie of `Arc`-shared nodes. A snapshot copy is O(1)
//! (one root `Arc` bump); replacing or appending one element copies one node
//! per level — at most `FANOUT` entries each, `O(log_16 P)` nodes — instead of
//! all `P` elements. Readers walk immutable nodes: no locks, no `unsafe`.

use std::sync::Arc;

/// Bits of page index consumed per level.
const BITS: u32 = 4;
/// Entries per node.
const FANOUT: usize = 1 << BITS;

/// A trie node: leaves hold elements, inner nodes hold children. Both are
/// left-packed (`len` entries at indices `0..len`).
enum Node<E> {
    Leaf(Vec<E>),
    Inner(Vec<Arc<Node<E>>>),
}

/// Immutable persistent sequence of `E`, indexed by page number.
pub(crate) struct PageTable<E> {
    root: Option<Arc<Node<E>>>,
    /// Root height; a leaf root has height 0. Capacity is `FANOUT^(height+1)`.
    height: u32,
    len: usize,
}

impl<E> Clone for PageTable<E> {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            height: self.height,
            len: self.len,
        }
    }
}

/// Child/element index of `idx` at `level` (0 = leaf).
fn digit(idx: usize, level: u32) -> usize {
    (idx >> (BITS * level)) & (FANOUT - 1)
}

impl<E: Clone> PageTable<E> {
    /// An empty table.
    pub(crate) fn new() -> Self {
        Self {
            root: None,
            height: 0,
            len: 0,
        }
    }

    /// Builds a table from `items` bottom-up, in O(n).
    pub(crate) fn from_vec(items: Vec<E>) -> Self {
        let len = items.len();
        if len == 0 {
            return Self::new();
        }
        let mut leaves: Vec<Arc<Node<E>>> = Vec::with_capacity(len.div_ceil(FANOUT));
        let mut it = items.into_iter().peekable();
        while it.peek().is_some() {
            let chunk: Vec<E> = it.by_ref().take(FANOUT).collect();
            leaves.push(Arc::new(Node::Leaf(chunk)));
        }
        let mut level = leaves;
        let mut height = 0;
        while level.len() > 1 {
            let mut up: Vec<Arc<Node<E>>> = Vec::with_capacity(level.len().div_ceil(FANOUT));
            let mut it = level.into_iter().peekable();
            while it.peek().is_some() {
                let chunk: Vec<Arc<Node<E>>> = it.by_ref().take(FANOUT).collect();
                up.push(Arc::new(Node::Inner(chunk)));
            }
            level = up;
            height += 1;
        }
        Self {
            root: level.pop(),
            height,
            len,
        }
    }

    /// Number of elements.
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// The element at `idx`, or `None` if out of range. Pure lookup.
    pub(crate) fn get(&self, idx: usize) -> Option<&E> {
        if idx >= self.len {
            return None;
        }
        let mut node: &Node<E> = self.root.as_deref()?;
        let mut level = self.height;
        loop {
            match node {
                Node::Inner(children) => {
                    node = children.get(digit(idx, level))?;
                    level -= 1;
                }
                Node::Leaf(items) => return items.get(digit(idx, 0)),
            }
        }
    }

    /// Replaces the element at `idx` (which must exist), copying one node per
    /// level. Returns `false` and leaves `self` unchanged if out of range.
    pub(crate) fn set(&mut self, idx: usize, item: E) -> bool {
        if idx >= self.len {
            return false;
        }
        let root = self.root.take().expect("non-empty table has a root");
        self.root = Some(put(Some(&root), self.height, idx, item));
        true
    }

    /// Appends `item` at index `len`, growing the root height when full.
    pub(crate) fn push(&mut self, item: E) {
        let idx = self.len;
        let mut root = self.root.take();
        if let Some(old) = root.take() {
            // Capacity of the current root: FANOUT^(height+1).
            if idx >= 1usize << (BITS * (self.height + 1)) {
                self.height += 1;
                root = Some(Arc::new(Node::Inner(vec![old])));
            } else {
                root = Some(old);
            }
        }
        self.root = Some(put(root.as_ref(), self.height, idx, item));
        self.len += 1;
    }
}

/// Path-copies `node` (absent for a fresh branch) setting/appending `item` at
/// `idx`. Only the nodes on the path are copied; siblings stay shared.
fn put<E: Clone>(node: Option<&Arc<Node<E>>>, level: u32, idx: usize, item: E) -> Arc<Node<E>> {
    let d = digit(idx, level);
    if level == 0 {
        let old: &[E] = match node.map(|n| &**n) {
            Some(Node::Leaf(v)) => v,
            _ => &[],
        };
        let mut v: Vec<E> = Vec::with_capacity(old.len().max(d + 1));
        let mut item = Some(item);
        for (i, e) in old.iter().enumerate() {
            if i == d {
                v.push(item.take().expect("replaced once"));
            } else {
                v.push(e.clone());
            }
        }
        if let Some(it) = item {
            debug_assert_eq!(v.len(), d, "appends are dense");
            v.push(it);
        }
        return Arc::new(Node::Leaf(v));
    }
    let old: &[Arc<Node<E>>] = match node.map(|n| &**n) {
        Some(Node::Inner(v)) => v,
        _ => &[],
    };
    let mut v: Vec<Arc<Node<E>>> = Vec::with_capacity(old.len().max(d + 1));
    v.extend(old.iter().take(d).cloned());
    v.push(put(old.get(d), level - 1, idx, item));
    v.extend(old.iter().skip(d + 1).cloned());
    Arc::new(Node::Inner(v))
}
