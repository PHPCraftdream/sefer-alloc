//! Operation elapsed sums are distinct from outer-window wall time and mapping bytes.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct RunStats {
    pub alloc_ns: u64,
    pub touch_ns: u64,
    pub free_ns: u64,
    /// Executor wall only; the outer controller must time setup/probes/teardown too.
    pub wall_ns: u64,
    pub allocations: u64,
    pub reallocations: u64,
    pub frees: u64,
    pub requested_bytes_issued: u64,
    pub bytes_written: u64,
}

impl RunStats {
    /// Parallel operation sums may exceed wall time. On overflow nothing changes.
    pub(crate) fn merge(&mut self, other: Self) -> std::io::Result<()> {
        let add = |left: u64, right: u64| {
            left.checked_add(right).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "window statistic overflow")
            })
        };
        let next = Self {
            alloc_ns: add(self.alloc_ns, other.alloc_ns)?,
            touch_ns: add(self.touch_ns, other.touch_ns)?,
            free_ns: add(self.free_ns, other.free_ns)?,
            wall_ns: self.wall_ns,
            allocations: add(self.allocations, other.allocations)?,
            reallocations: add(self.reallocations, other.reallocations)?,
            frees: add(self.frees, other.frees)?,
            requested_bytes_issued: add(self.requested_bytes_issued, other.requested_bytes_issued)?,
            bytes_written: add(self.bytes_written, other.bytes_written)?,
        };
        *self = next;
        Ok(())
    }
}
