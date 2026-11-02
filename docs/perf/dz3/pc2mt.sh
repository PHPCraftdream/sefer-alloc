# DZ-3 control: larson / mstress at T=2 and T=4 (cross-thread frees), base vs change vs mimalloc, interleaved, aggregate Mops
source ~/.cargo/env
W=<scratch dir>; S=$HOME/w2m; O=$W/pc2/final2; mkdir -p $O
cp <scratch dir>/../g1_mt_one.rs $S/dz3/examples/
python3 - $S/dz3/Cargo.toml <<'PY'
import sys
p=sys.argv[1]; s=open(p).read()
a='[[example]]\nname = "malloc_macro"'
if 'name = "g1_mt_one"' not in s:
    s=s.replace(a,'[[example]]\nname = "g1_mt_one"\nrequired-features = ["alloc-global", "alloc-xthread"]\n\n'+a,1)
open(p,'w').write(s)
PY
(cd $S/dz3 && CARGO_TARGET_DIR=$S/tgt-dz3 cargo build -q --release --example g1_mt_one --features production) || exit 1
BASE=$S/tgt-base/release/examples/g1_mt_one; NEW=$S/tgt-dz3/release/examples/g1_mt_one
sha256sum $BASE $NEW | sed "s#$S/##" > $O/mt_ident.txt
: > $O/mt.txt
for i in 1 2 3 4 5 6 7; do
  for w in larson mstress; do
    for t in 2 4; do
      echo "base $($BASE $w sefer $t 1000000)" >> $O/mt.txt
      echo "new $($NEW $w sefer $t 1000000)" >> $O/mt.txt
      echo "mi $($BASE $w mi $t 1000000)" >> $O/mt.txt
    done
  done
done
echo mt-done
