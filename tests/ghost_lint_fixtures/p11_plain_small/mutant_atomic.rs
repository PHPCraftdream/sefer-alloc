// expect: does not match
#[repr(C)]
pub(crate) struct ActiveKindIndex {
    small: [AtomicU64; WORDS],
    large: [u64; WORDS],
    small_summary: u64,
    large_summary: u64,
}

pub(crate) struct SegmentTable {
    active_kind: *mut ActiveKindIndex,
}

impl SegmentTable {
    pub(crate) fn register_payload(&mut self, base: *mut u8, kind: SegmentKind) -> Option<u32> {
        if let Some(i) = self.free_list_pop() {
            ActiveKindIndex::set(self.active_kind, i as usize, kind);
            return Some(i);
        }
        ActiveKindIndex::set(self.active_kind, idx, kind);
        Some(idx as u32)
    }

    pub(crate) fn unregister(&mut self, base: *mut u8) {
        ActiveKindIndex::clear(self.active_kind, slot_id, header.kind);
    }

    pub(crate) fn recycle(&mut self, base: *mut u8) {
        ActiveKindIndex::clear(self.active_kind, slot_id, hdr.kind);
    }

    pub(crate) fn next_active(&self, kind: SegmentKind, from: usize) -> Option<usize> {
        ActiveKindIndex::next(self.active_kind, kind, from)
    }
}

impl SegmentHashHarness {
    pub fn active_set(&mut self, slot: usize, large: bool) {
        ActiveKindIndex::set(self.index, slot, kind);
    }

    pub fn active_clear(&mut self, slot: usize, large: bool) {
        ActiveKindIndex::clear(self.index, slot, kind);
    }
}
