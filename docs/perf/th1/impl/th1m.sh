#!/bin/bash
# TH-1 measurement: base 89c1fa58 vs TH-1 70e5943f (same judge source), Linux, ABBA x2 per scenario, + mi; MT control.
# Inputs in $WORK: base/x source tarballs (git archive of the two SHAs), the judge source and g1_mt_one.rs; $SCRATCH is a Linux-local build directory.
source ~/.cargo/env
P=${PERF:-perf}
G=${WORK:?set WORK to the inputs directory}; O=${TH1M_OUT:-$G/th1m}; mkdir -p $O; : > $O/raw.jsonl; : > $O/mt.txt
for L in base x; do
  rm -rf ${SCRATCH:-$HOME/ab}/th1m_$L; mkdir -p ${SCRATCH:-$HOME/ab}/th1m_$L; tar -xf $G/th1m_$L.tar -C ${SCRATCH:-$HOME/ab}/th1m_$L
  cp $G/th1m_judge.rs ${SCRATCH:-$HOME/ab}/th1m_$L/examples/g1_wild_judge.rs
  cp $G/g1_mt_one.rs ${SCRATCH:-$HOME/ab}/th1m_$L/examples/
  python3 - ${SCRATCH:-$HOME/ab}/th1m_$L <<'PY'
import sys
p=sys.argv[1]+'/Cargo.toml'; s=open(p).read()
a='[[example]]\nname = "malloc_macro"'
s=s.replace(a,'[[example]]\nname = "g1_mt_one"\nrequired-features = ["alloc-global", "alloc-xthread"]\n\n'+a,1)
open(p,'w').write(s)
PY
  (cd ${SCRATCH:-$HOME/ab}/th1m_$L && CARGO_TARGET_DIR=${SCRATCH:-$HOME/ab}/tgt-th1m_$L cargo build -q --release --example g1_wild_judge --example g1_mt_one --features production)
  echo "$L $(sha256sum ${SCRATCH:-$HOME/ab}/tgt-th1m_$L/release/examples/g1_wild_judge | cut -c1-16) $(sha256sum ${SCRATCH:-$HOME/ab}/tgt-th1m_$L/release/examples/g1_mt_one | cut -c1-16)" >> $O/bin.txt
done
one() { # tag arm scen
  B=${SCRATCH:-$HOME/ab}/tgt-th1m_$1/release/examples/g1_wild_judge; [ "$1" = mi ] && B=${SCRATCH:-$HOME/ab}/tgt-th1m_base/release/examples/g1_wild_judge
  out=$($P stat -e page-faults -x, -o /tmp/th1m.stat $B $3 $2 1 0.5 2>/dev/null)
  pf=$(grep -o '^[0-9]*' /tmp/th1m.stat | head -1)
  echo "{\"tag\":\"$1\",\"page_faults\":$pf,\"r\":$out}" >> $O/raw.jsonl
}
for s in ${TH1M_SCEN:-grow64 grow1k growmix regrow large wild phase}; do
  for r in $(seq 1 ${TH1M_REP:-2}); do one base sefer $s; one x sefer $s; one mi mi $s; one x sefer $s; one base sefer $s; done
done
for i in 1 2 3 4; do for L in base x x base; do for w in mstress larson; do
  t0=$(date +%s%N); ${SCRATCH:-$HOME/ab}/tgt-th1m_$L/release/examples/g1_mt_one $w sefer 1 2000000 >/dev/null; t1=$(date +%s%N)
  echo "$L $w $(( (t1-t0)/1000000 ))" >> $O/mt.txt
done; done; done
echo done
