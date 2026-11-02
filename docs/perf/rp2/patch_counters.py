import sys, re, os
root = sys.argv[1]

def edit(path, pairs):
    s = open(path, encoding='utf-8').read()
    for a, b in pairs:
        if s.count(a) != 1:
            raise SystemExit('anchor count %d for %r in %s' % (s.count(a), a[:60], path))
        s = s.replace(a, b)
    open(path, 'w', encoding='utf-8').write(s)

# global counters at the crate root (measurement-only scratch patch)
lib = os.path.join(root, 'src', 'lib.rs')
s = open(lib, encoding='utf-8').read()
s += '''
#[doc(hidden)]
pub static RPC: [core::sync::atomic::AtomicU64; 12] = [const { core::sync::atomic::AtomicU64::new(0) }; 12];
#[doc(hidden)]
#[inline(always)]
pub fn rpc_add(i: usize, n: usize) {
    RPC[i].fetch_add(n as u64, core::sync::atomic::Ordering::Relaxed);
}
#[doc(hidden)]
pub static SNAP: [core::sync::atomic::AtomicU64; 512] = [const { core::sync::atomic::AtomicU64::new(0) }; 512];
#[doc(hidden)]
#[inline(always)]
pub fn rpc_snap() {
    use core::sync::atomic::Ordering::Relaxed;
    let k = RPC[0].load(Relaxed) as usize;
    if k % 2048 == 0 {
        let i = k / 2048;
        if i < 128 {
            SNAP[i * 4].store(RPC[7].load(Relaxed), Relaxed);
            SNAP[i * 4 + 1].store(RPC[2].load(Relaxed) + RPC[3].load(Relaxed) + RPC[6].load(Relaxed), Relaxed);
            SNAP[i * 4 + 2].store(RPC[8].load(Relaxed), Relaxed);
            SNAP[i * 4 + 3].store(RPC[4].load(Relaxed), Relaxed);
        }
    }
}
'''
open(lib, 'w', encoding='utf-8').write(s)

m = os.path.join(root, 'src', 'alloc_core', 'small', 'alloc_core_small_magazine.rs')
edit(m, [
    ('        let block_size = SizeClasses::block_size(class_idx);\n        debug_assert!(block_size >= NODE_SIZE);\n        let want = out.len();\n',
     '        let block_size = SizeClasses::block_size(class_idx);\n        debug_assert!(block_size >= NODE_SIZE);\n        let want = out.len();\n        crate::rpc_add(0, 1);\n        crate::rpc_add(1, want);\n        crate::rpc_snap();\n'),
    ('                if n != 0 {\n                    filled += n;\n                    continue;\n                }\n                // B4 §11: probe',
     '                if n != 0 {\n                    crate::rpc_add(2, n);\n                    filled += n;\n                    continue;\n                }\n                // B4 §11: probe'),
    ('                if n != 0 {\n                    filled += n;\n                    continue;\n                }\n                // `find_segment_with_free` consumes',
     '                if n != 0 {\n                    crate::rpc_add(3, n);\n                    filled += n;\n                    continue;\n                }\n                // `find_segment_with_free` consumes'),
    ('                let found_seg = self.find_segment_with_free(class_idx);\n                if let Some(seg) = found_seg {\n',
     '                let found_seg = self.find_segment_with_free(class_idx);\n                crate::rpc_add(4, 1);\n                if let Some(seg) = found_seg {\n                    crate::rpc_add(5, 1);\n'),
    ('                    if n != 0 {\n                        filled += n;\n                        continue;\n                    }\n                }\n                // Discovery found no free block in this pass',
     '                    if n != 0 {\n                        crate::rpc_add(6, n);\n                        filled += n;\n                        continue;\n                    }\n                }\n                // Discovery found no free block in this pass'),
    ('            if n != 0 {\n                #[cfg(feature = "virgin-zero-skip")]\n                if let (Some(mask), Some(true)) = (virgin_out.as_deref_mut(), cur_virgin) {',
     '            if n != 0 {\n                crate::rpc_add(7, n);\n                #[cfg(feature = "virgin-zero-skip")]\n                if let (Some(mask), Some(true)) = (virgin_out.as_deref_mut(), cur_virgin) {'),
    ('            match self.reserve_small_segment() {\n                Some(_) => {\n',
     '            match self.reserve_small_segment() {\n                Some(_) => {\n                    crate::rpc_add(8, 1);\n'),
])
print('patched')

# --- diagnostics round 2: drain outcomes and slots per process_ingress call ---
s = open(lib, encoding='utf-8').read()
assert s.count('[core::sync::atomic::AtomicU64; 12] = [const { core::sync::atomic::AtomicU64::new(0) }; 12]') == 1
s = s.replace('[core::sync::atomic::AtomicU64; 12] = [const { core::sync::atomic::AtomicU64::new(0) }; 12]', '[core::sync::atomic::AtomicU64; 16] = [const { core::sync::atomic::AtomicU64::new(0) }; 16]')
open(lib, 'w', encoding='utf-8').write(s)

sd = os.path.join(root, 'src', 'alloc_core', 'alloc_core', 'sidecar_drain.rs')
edit(sd, [
    ('            let pooled = self.release_or_pool_empty_segment(base);\n            return if pooled {\n',
     '            let pooled = self.release_or_pool_empty_segment(base);\n            crate::rpc_add(if pooled { 9 } else { 10 }, 1);\n            return if pooled {\n'),
    ('        DrainEnd::Live\n    }\n\n    /// Claim one pending Large route',
     '        if changed_classes != 0 {\n            crate::rpc_add(11, 1);\n        }\n        DrainEnd::Live\n    }\n\n    /// Claim one pending Large route'),
])
ig = os.path.join(root, 'src', 'alloc_core', 'small', 'alloc_core_small', 'find_segment', 'ingress_discovery.rs')
edit(ig, [
    ('        let mut hit = None;\n        ingress.take(|i| {\n            if let Some(base) = self.drain_notified_slot(i, class_idx) {',
     '        let mut hit = None;\n        let mut slots_here = 0usize;\n        ingress.take(|i| {\n            slots_here += 1;\n            if let Some(base) = self.drain_notified_slot(i, class_idx) {'),
    ('        if let Some(base) = hit {\n            self.finalize_hit(\n                base,\n                class_idx,\n                DirectoryCandidateContext {\n                    #[cfg(feature = "alloc-stats")]\n                    routed_scan_was_nonempty: None,',
     '        if slots_here != 0 {\n            crate::rpc_add(12, 1);\n            crate::rpc_add(13, slots_here);\n            crate::RPC[14].fetch_max(slots_here as u64, core::sync::atomic::Ordering::Relaxed);\n        }\n        if let Some(base) = hit {\n            self.finalize_hit(\n                base,\n                class_idx,\n                DirectoryCandidateContext {\n                    #[cfg(feature = "alloc-stats")]\n                    routed_scan_was_nonempty: None,'),
])
print('patched round 2')
