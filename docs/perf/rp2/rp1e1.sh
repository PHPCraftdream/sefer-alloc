#!/bin/bash
# RP-2 experiment E1 (scratch, measurement only): a drain that empties a segment does NOT release/pool it (keeps its blocks on the freelist)
source ~/.cargo/env
W=<scratch dir>
S=$HOME/w2m
rm -rf $S/e1; mkdir -p $S/e1; tar -xf $W/base.tar -C $S/e1
node_free_patch() { :; }
python3 - $S/e1 <<'PY'
import sys, os
root = sys.argv[1]
p = os.path.join(root, 'src', 'alloc_core', 'alloc_core', 'sidecar_drain.rs')
s = open(p, encoding='utf-8').read()
a = 'if changed_classes != 0 && Self::dec_live_and_maybe_decommit(base, self.small_cur) {'
assert s.count(a) == 1
s = s.replace(a, 'if false && changed_classes != 0 && Self::dec_live_and_maybe_decommit(base, self.small_cur) {')
open(p, 'w', encoding='utf-8').write(s)
c = os.path.join(root, 'Cargo.toml')
t = open(c).read()
m = '[[example]]\nname = "malloc_macro"'
if 'name = "g1_rp1_probe"' not in t:
    t = t.replace(m, '[[example]]\nname = "g1_rp1_probe"\nrequired-features = ["alloc-global", "alloc-xthread"]\n\n' + m, 1)
open(c, 'w').write(t)
print('patched')
PY
cp $W/rp1/g1_rp1_probe.rs $S/e1/examples/
(cd $S/e1 && CARGO_TARGET_DIR=$S/tgt-e1 cargo build -q --release --example g1_rp1_probe --features production) || exit 1
cp $W/rp1/g1_rp1_probe.rs $S/base/examples/
(cd $S/base && CARGO_TARGET_DIR=$S/tgt-base cargo build -q --release --example g1_rp1_probe --features production) || exit 1
B=$S/tgt-base/release/examples/g1_rp1_probe; E=$S/tgt-e1/release/examples/g1_rp1_probe
O=$W/rp1/out5; mkdir -p $O; : > $O/raw.txt
for cfg in "4 64 3" "16 64 3" "16 1024 3"; do
  set -- $cfg
  n=$(( $1 * (4194304 / $2) * 9 / 10 ))
  for i in 1 2 3 4 5 6 7; do
    $B sefer $1 $2 $3 $n | sed 's/^rp1 sefer/rp1 base/' >> $O/raw.txt
    $E sefer $1 $2 $3 $n | sed 's/^rp1 sefer/rp1 e1/' >> $O/raw.txt
    $B mi $1 $2 $3 $n >> $O/raw.txt
  done
done
echo e1-done
