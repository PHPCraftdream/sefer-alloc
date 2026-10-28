#!/bin/bash
# B4-25 path-activation oracle: calls into HeapCore::publish_foreign (the
# remote-free entry) per sefer callgrind run. Usage: bash b425_oracle.sh <label>
# from the directory holding <label>.sefer.<wl>.t<T>.out; appends ORACLE lines.
L=$1
for wl in mstress larson; do
  for t in 1 2 4; do
    f=$L.sefer.$wl.t$t.out
    n=$(awk '
      match($0, /^c?fn=\(([0-9]+)\)( (.*))?$/, m) { if (m[3] != "") name[m[1]] = m[3] }
      /^cfn=\(/ { match($0, /^cfn=\(([0-9]+)\)/, c); cur = c[1]; next }
      /^calls=/ { split(substr($0, 7), a, " "); if (index(name[cur], "HeapCore>::publish_foreign")) s += a[1] }
      END { print s + 0 }' "$f")
    echo "ORACLE sefer $wl $t publish_foreign_calls $n"
  done
done
