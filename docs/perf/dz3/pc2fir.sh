# DZ-3: callgrind Ir of publish_foreign (inclusive), robust extraction (the function line only, largest value), final binaries
S=$HOME/w2m; W=<scratch dir>; O=$W/pc2/final; C=$S/cg; cd $C
BASE=$S/tgt-base/release/examples/g1_rhythm_probe; NEW=$S/tgt-dz3/release/examples/g1_rhythm_probe
: > $O/ir2.txt
for arm in base new; do
  if [ $arm = base ]; then B=$BASE; else B=$NEW; fi
  for m in 4 5 8 16 64; do
    for o in cyclic seq random; do
      valgrind --tool=callgrind --callgrind-out-file=$C/pg.out $B pincycle $m $o > /dev/null 2>&1
      tot=$(callgrind_annotate $C/pg.out 2>/dev/null | grep 'PROGRAM TOTALS' | awk '{print $1}')
      pf=$(callgrind_annotate --inclusive=yes --tree=caller $C/pg.out 2>/dev/null | grep -E '^ *[0-9,]+ \(.*\)\s+\*\s+.*HeapCore>::publish_foreign\s*$' | awk '{gsub(",","",$1); print $1}' | sort -n | tail -1)
      echo "$arm $m $o pf=$pf total=$(echo $tot | tr -d ',')" >> $O/ir2.txt
    done
  done
done
echo done
