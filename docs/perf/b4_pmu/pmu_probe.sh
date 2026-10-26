#!/bin/bash
P=/usr/lib/linux-tools-6.8.0-147/perf
$P stat -e cycles,instructions,branches,branch-misses,L1-dcache-loads,L1-dcache-load-misses -- dd if=/dev/zero of=/dev/null bs=1M count=200 2>&1 | tail -12
$P list hw cache 2>/dev/null | head -30
