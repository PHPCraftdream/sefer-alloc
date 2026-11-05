#!/bin/bash
# A0': paired bench:table main (g1-c1r = d2028870 tree) vs A0' (two ABBA sets, each after a discarded warm run). Aligned code.
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
R=<repo-root>; O=<scratch>/a0pbt; mkdir -p $O
git -C $R/worktrees/g1-c1r rev-parse HEAD > $O/ident.txt; git -C $R/worktrees/g1-a0p rev-parse HEAD >> $O/ident.txt
run() { (cd $R/worktrees/$1 && node scripts/bench-table-paired.mjs --features production --raw $O/$2 --csv $O/$2.csv > $O/$2.md 2> $O/$2.err); echo "$2 exit $?"; }
run g1-c1r warm1
run g1-c1r s1base_r1; run g1-a0p s1a0p_r1; run g1-a0p s1a0p_r2; run g1-c1r s1base_r2
run g1-a0p warm2
run g1-a0p s2a0p_r1; run g1-c1r s2base_r1; run g1-c1r s2base_r2; run g1-a0p s2a0p_r2
echo done
