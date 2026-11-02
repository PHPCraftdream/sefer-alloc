#!/bin/bash
# RP-2 experiment E2 (scratch, UPPER BOUND only, not semantically safe): after one failed discovery, skip discovery for the next 63 refills.
source ~/.cargo/env
W=<scratch dir>
S=$HOME/w2m
rm -rf $S/e2; mkdir -p $S/e2; tar -xf $W/base.tar -C $S/e2
python3 - $S/e2 <<'PY'
import sys, os
root = sys.argv[1]
p = os.path.join(root, 'src', 'alloc_core', 'small', 'alloc_core_small_magazine.rs')
s = open(p, encoding='utf-8').read()
a = '                let found_seg = self.find_segment_with_free(class_idx);\n'
assert s.count(a) == 1
b = ('                static SKIP: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);\n'
     '                let found_seg = if SKIP.load(core::sync::atomic::Ordering::Relaxed) > 0 {\n'
     '                    SKIP.fetch_sub(1, core::sync::atomic::Ordering::Relaxed);\n'
     '                    None\n'
     '                } else {\n'
     '                    let r = self.find_segment_with_free(class_idx);\n'
     '                    if r.is_none() {\n'
     '                        SKIP.store(63, core::sync::atomic::Ordering::Relaxed);\n'
     '                    }\n'
     '                    r\n'
     '                };\n')
s = s.replace(a, b)
open(p, 'w', encoding='utf-8').write(s)
c = os.path.join(root, 'Cargo.toml')
t = open(c).read()
m = '[[example]]\nname = "malloc_macro"'
if 'name = "g1_rp1_probe"' not in t:
    t = t.replace(m, '[[example]]\nname = "g1_rp1_probe"\nrequired-features = ["alloc-global", "alloc-xthread"]\n\n' + m, 1)
open(c, 'w').write(t)
print('patched')
PY
cp $W/rp1/g1_rp1_probe.rs $S/e2/examples/
(cd $S/e2 && CARGO_TARGET_DIR=$S/tgt-e2 cargo build -q --release --example g1_rp1_probe --example g1_wild_judge --features "production internals") || exit 1
(cd $S/base && CARGO_TARGET_DIR=$S/tgt-base cargo build -q --release --example g1_rp1_probe --example g1_wild_judge --features "production internals") || exit 1
O=$W/rp1/out6; mkdir -p $O; C=$S/cg; cd $C
: > $O/totals.txt
export RP1_NOSTATS=1
for tree in base e2; do
  T=$S/tgt-$tree/release/examples
  for n in 0 943718; do
    valgrind --tool=callgrind --callgrind-out-file=$C/e2.$tree.rp1.$n.out $T/g1_rp1_probe sefer 16 64 3 $n > /dev/null 2>&1
    echo "$tree rp1 allocs=$n $(callgrind_annotate $C/e2.$tree.rp1.$n.out 2>/dev/null | grep 'PROGRAM TOTALS' | awk '{print $1}')" >> $O/totals.txt
  done
  for sc in grow64 grow1k; do
    valgrind --tool=callgrind --callgrind-out-file=$C/e2.$tree.$sc.out $T/g1_wild_judge $sc sefer 1 0.3 > /dev/null 2>&1
    echo "$tree $sc $(callgrind_annotate $C/e2.$tree.$sc.out 2>/dev/null | grep 'PROGRAM TOTALS' | awk '{print $1}')" >> $O/totals.txt
  done
done
# wall-clock of the probe, interleaved
unset RP1_NOSTATS
: > $O/raw.txt
for i in 1 2 3 4 5 6 7; do
  $S/tgt-base/release/examples/g1_rp1_probe sefer 16 64 3 943718 | sed 's/^rp1 sefer/rp1 base/' >> $O/raw.txt
  $S/tgt-e2/release/examples/g1_rp1_probe sefer 16 64 3 943718 | sed 's/^rp1 sefer/rp1 e2/' >> $O/raw.txt
done
echo e2-done
