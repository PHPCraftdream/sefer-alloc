#!/bin/bash
# RP-2: refill-path counters in a scratch copy of c0ee7dfb (measurement-only patch), probe 16 segments x 64 B, 3 workers
source ~/.cargo/env
W=<scratch dir>
S=$HOME/w2m
rm -rf $S/cnt; mkdir -p $S/cnt; tar -xf $W/base.tar -C $S/cnt
python3 $W/rp1/patch_counters.py $S/cnt || exit 1
cp $W/rp1/g1_rp1_probe.rs $S/cnt/examples/g1_rp1_probe.rs
python3 - $S/cnt <<'PY'
import sys, os
root = sys.argv[1]
p = os.path.join(root, 'examples', 'g1_rp1_probe.rs')
s = open(p).read()
a = '    if std::env::var_os("RP1_NOSTATS").is_some() {'
b = '    if a[1].starts_with("sefer") {\n        let c: Vec<u64> = sefer_alloc::RPC.iter().map(|x| x.load(std::sync::atomic::Ordering::Relaxed)).collect();\n        let sn: Vec<u64> = sefer_alloc::SNAP.iter().map(|x| x.load(std::sync::atomic::Ordering::Relaxed)).collect();\n        for i in 0..128usize { if sn[i * 4] + sn[i * 4 + 1] + sn[i * 4 + 2] + sn[i * 4 + 3] != 0 || i == 0 { println!("snap refills={} carve={} freelist_total={} reserve={} find_calls={}", i * 2048, sn[i * 4], sn[i * 4 + 1], sn[i * 4 + 2], sn[i * 4 + 3]); } }\n        println!("rpc refills={} want_sum={} from_cur_freelist={} from_cursor={} find_calls={} find_some={} from_found_seg={} from_carve={} reserve={} drain_pooled={} drain_released={} drain_live_changed={} ingress_calls={} ingress_slots={} ingress_max_slots={}", c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7], c[8], c[9], c[10], c[11], c[12], c[13], c[14]);\n    }\n'
assert s.count(a) == 1
s = s.replace(a, b + a)
d = '    drop(blocks);\n'
assert s.count(d) == 1
s = s.replace(d, d + '    for x in sefer_alloc::RPC.iter() {\n        x.store(0, std::sync::atomic::Ordering::Relaxed);\n    }\n')
open(p, 'w').write(s)
c = os.path.join(root, 'Cargo.toml')
t = open(c).read()
m = '[[example]]\nname = "malloc_macro"'
if 'name = "g1_rp1_probe"' not in t:
    t = t.replace(m, '[[example]]\nname = "g1_rp1_probe"\nrequired-features = ["alloc-global", "alloc-xthread"]\n\n' + m, 1)
open(c, 'w').write(t)
PY
(cd $S/cnt && CARGO_TARGET_DIR=$S/tgt-cnt cargo build -q --release --example g1_rp1_probe --features production) || exit 1
O=$W/rp1/out3; mkdir -p $O
RP1_NOSTATS=1 $S/tgt-cnt/release/examples/g1_rp1_probe sefer 16 64 3 943718 > $O/cnt_16x64.txt
RP1_NOSTATS=1 $S/tgt-cnt/release/examples/g1_rp1_probe sefer 4 64 3 235929 > $O/cnt_4x64.txt
RP1_NOSTATS=1 $S/tgt-cnt/release/examples/g1_rp1_probe sefer 16 1024 3 58982 > $O/cnt_16x1024.txt
RP1_NOSTATS=1 $S/tgt-cnt/release/examples/g1_rp1_probe sefer8 16 64 3 943718 > $O/cnt_s8_16x64.txt
echo rp1cnt-done
