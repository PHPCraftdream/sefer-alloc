#!/bin/bash
# PC-2 time: foreign free of m x 2048 x 1 KiB blocks by one other thread, sefer vs mimalloc, orders cyclic / seq / random, interleaved processes
source ~/.cargo/env
W=<scratch dir>
S=$HOME/w2m
python3 - $S/base/examples/g1_rhythm_probe.rs <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding='utf-8').read()
reps = [
 ('fn pincycle(m: usize, order: &str) {\n    let a = heap(false);', 'fn pincycle(m: usize, order: &str, mi: bool) {\n    let a = heap(mi);'),
 ('Some("pincycle") => pincycle(num(2), &args[3]),', 'Some("pincycle") => pincycle(num(2), &args[3], flag(4, "mi")),'),
 ('\\"alloc\\":\\"sefer\\",\\"counters\\":{COUNTERS},\\"m\\":{m}', '\\"alloc\\":\\"{}\\",\\"counters\\":{COUNTERS},\\"m\\":{m}'),
 ('        by_seg.len(),\n        delta("", before, after)\n    );\n}\n\nfn reclaim', '        if mi { "mi" } else { "sefer" },\n        by_seg.len(),\n        delta("", before, after)\n    );\n}\n\nfn reclaim'),
]
for a, b in reps:
    if s.count(a) != 1:
        raise SystemExit('anchor %d: %r' % (s.count(a), a[:50]))
    s = s.replace(a, b)
open(p, 'w', encoding='utf-8').write(s)
print('patched')
PY
(cd $S/base && CARGO_TARGET_DIR=$S/tgt-base cargo build -q --release --example g1_rhythm_probe --features production) || exit 1
BIN=$S/tgt-base/release/examples/g1_rhythm_probe
O=$W/pc2/out; mkdir -p $O; : > $O/time.txt
for i in 1 2 3 4 5 6 7 8 9; do
  for m in 5 8 16 64; do
    for o in cyclic seq random; do
      for who in sefer mi; do
        if [ $who = mi ]; then $BIN pincycle $m $o mi >> $O/time.txt; else $BIN pincycle $m $o >> $O/time.txt; fi
      done
    done
  done
done
echo pc2t-done
