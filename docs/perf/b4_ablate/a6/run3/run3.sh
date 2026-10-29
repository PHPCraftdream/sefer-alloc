#!/bin/sh
# G1-K7 run 3 (Addendum 6 rule 3): RSS. Trees and the output directory are
# resolved relative to this worktree (siblings under worktrees/). One process
# per (arm, workload, repetition). Refuses to start unless both trees are at
# their expected commits, clean, and carry byte-identical committed copies of
# the harness whose sha256 is EXPECT_SHA; refuses a non-empty output dir.
set -e
cd "$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
K5=../g1-k5-base
K7=../g1-k7-j11-v5
OUT=../../.rush/g1/a6/run3
H=examples/g1_k7_rss_judge.rs
: "${EXPECT_SHA:?set EXPECT_SHA to the committed harness sha256}"
: "${EXPECT_K5:?set EXPECT_K5 to the K5 tree commit}"
: "${EXPECT_K7:?set EXPECT_K7 to the K7 tree commit}"
for t in "$K5:$EXPECT_K5" "$K7:$EXPECT_K7"; do
  d=${t%%:*}; c=${t#*:}
  [ "$(git -C $d rev-parse HEAD)" = "$c" ] || { echo "abort: $d is not at $c"; exit 2; }
  [ -z "$(git -C $d status --porcelain)" ] || { echo "abort: $d is not clean (tracked or untracked)"; exit 2; }
  git -C $d ls-files --error-unmatch $H > /dev/null || { echo "abort: $d $H not committed"; exit 2; }
  [ "$(sha256sum < $d/$H | cut -d' ' -f1)" = "$EXPECT_SHA" ] || { echo "abort: $d $H sha256 mismatch"; exit 2; }
done
if [ -d $OUT/raw ] && [ -n "$(ls -A $OUT/raw)" ]; then echo "abort: $OUT/raw is not empty"; exit 2; fi
ALIGN="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
F="production bench-internals internals"
export RUSTC_WRAPPER=
mkdir -p $OUT/bin $OUT/raw
build() { # arm tree cfg E
  ( cd $2 && G1_K7_E=$4 RUSTFLAGS="$ALIGN $3" CARGO_TARGET_DIR=../../target/k7rss-$1 \
      cargo build --release -q --example g1_k7_rss_judge --features "$F" )
  cp ../../target/k7rss-$1/release/examples/g1_k7_rss_judge.exe $OUT/bin/$1.exe
}
build k5 $K5 "" ""
build mi $K5 "--cfg g1_mi" ""
for e in 0 524288 1048576 2097152; do build k7_$e $K7 "--cfg g1_k7" $e; done
for t in k5:$K5 k7:$K7; do
  n=${t%%:*}; d=${t#*:}
  ( cd $d && RUSTFLAGS="$ALIGN" CARGO_TARGET_DIR=../../target/k7rss-probe-$n \
      cargo build --release -q --example rss_probe --features production )
  cp ../../target/k7rss-probe-$n/release/examples/rss_probe.exe $OUT/bin/rss_probe_$n.exe
done
{ echo "k5_tree $(git -C $K5 rev-parse HEAD)"; echo "k7_tree $(git -C $K7 rev-parse HEAD)";
  echo "harness_sha256 $EXPECT_SHA"; (cd $OUT/bin && sha256sum *.exe); rustc -vV;
  echo "features: $F"; echo "RUSTFLAGS: $ALIGN"; } > $OUT/identity.txt
# One invocation; on failure it is re-run once with identical arguments.
# Every attempt keeps its own stdout/stderr (att/<rep>_<bin>_<w>_<n>.*); a
# second failure ends run 3 as inconclusive (exit 3).
mkdir -p $OUT/raw/att
attempt() { # rep bin workload n -> 0 on success
  $OUT/bin/$2.exe $3 > $OUT/raw/att/$1_$2_$3_$4.out 2> $OUT/raw/att/$1_$2_$3_$4.err
}
once() { # rep bin workload
  n=1
  if ! attempt $1 $2 $3 1; then
    echo "retry rep $1 $2 $3" >> $OUT/raw/retries.log
    n=2; attempt $1 $2 $3 2 || { echo "second failure rep $1 $2 $3: inconclusive"; exit 3; }
  fi
  sed "s/^{/{\"rep\":$1,\"bin\":\"$2\",/" $OUT/raw/att/$1_$2_$3_$n.out >> $OUT/raw/judge.jsonl
}
probe() { # rep tree
  if ! $OUT/bin/rss_probe_$2.exe > $OUT/raw/att/w4_$1_$2_1.out 2> $OUT/raw/att/w4_$1_$2_1.err; then
    echo "retry W4 rep $1 $2" >> $OUT/raw/retries.log
    $OUT/bin/rss_probe_$2.exe > $OUT/raw/att/w4_$1_$2_2.out 2> $OUT/raw/att/w4_$1_$2_2.err       || { echo "second failure W4 rep $1 $2: inconclusive"; exit 3; }
    n=2
  else n=1; fi
  echo "== rep $1 $2" >> $OUT/raw/w4.log
  cat $OUT/raw/att/w4_$1_$2_$n.out >> $OUT/raw/w4.log
}
ARMS="k5 k7_0 k7_524288 k7_1048576 k7_2097152 mi"
for rep in 1 2 3 4 5; do
  # Rotate the arm order by one position per round.
  set -- $ARMS; i=1; while [ $i -lt $rep ]; do a=$1; shift; set -- "$@" $a; i=$((i+1)); done
  order="$*"
  for w in W1 W2 W3 W5i W5c; do
    for a in $order; do once $rep $a $w; done
  done
  if [ $((rep % 2)) = 1 ]; then pr="k5 k7"; else pr="k7 k5"; fi
  for t in $pr; do probe $rep $t; done
done
echo run3-done
