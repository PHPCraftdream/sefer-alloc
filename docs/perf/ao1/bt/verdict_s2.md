| Cell (AO-1 v2 set 2) | ratio d pair 1 / 2 | ratio agreed | sefer time pair 1 / 2 | sefer time agreed | sefer/mi before → after |
|---|---:|---:|---:|---:|---|
| direct 16 B | +0.9 % / +5.3 % | +0.9 % | +0.9 % / +5.3 % | +0.9 % | 1.247 → 1.286 |
| direct 64 B | +3.2 % / -7.0 % | +0.0 % | +4.9 % / -12.0 % | +0.0 % | 0.868 → 0.850 |
| direct 256 B | +1.9 % / +8.7 % | +1.9 % | +2.6 % / +2.6 % | +2.6 % | 0.738 → 0.777 |
| direct 1024 B | -1.8 % / -2.4 % | -1.8 % | -1.2 % / -2.4 % | -1.2 % | 0.533 → 0.522 |
| churn 16 B | +1.0 % / +1.0 % | +1.0 % | +1.0 % / +1.0 % | +1.0 % | 0.873 → 0.882 |
| churn 64 B | +0.8 % / +2.1 % | +0.8 % | +0.0 % / +2.1 % | +0.0 % | 0.779 → 0.790 |
| churn 256 B | +4.1 % / +0.6 % | +0.6 % | +1.1 % / +0.0 % | +0.0 % | 0.557 → 0.570 |
| churn 1024 B | -10.2 % / -2.1 % | -2.1 % | +1.0 % / +0.0 % | +0.0 % | 0.302 → 0.284 |
| churn_write 16 B | +1.9 % / +3.0 % | +1.9 % | +1.0 % / +2.0 % | +1.0 % | 0.895 → 0.918 |
| churn_write 64 B | +0.0 % / +0.9 % | +0.0 % | +0.0 % / +0.0 % | +0.0 % | 0.877 → 0.880 |
| churn_write 256 B | -0.6 % / -1.0 % | -0.6 % | +0.0 % / -1.0 % | +0.0 % | 0.585 → 0.580 |
| churn_write 1024 B | +4.6 % / -1.8 % | +0.0 % | +1.9 % / +0.0 % | +0.0 % | 0.285 → 0.288 |

Sefer time (A/B of the two builds): geometric mean +0.4 %; agreed wins 1.2 pp, agreed losses 5.5 pp; largest agreed loss +2.6 %.
Ratio to mimalloc: geometric mean over 12 cells: +0.5 %; agreed wins 4.5 pp, agreed losses 7.2 pp (ratio 0.62); largest agreed loss +1.9 %.

Magnitude rule: gm +0.4 % (<= -0.5 %), wins/losses 1.2/5.5 pp (>= 2x), largest loss +2.6 % (<= 3 %), new LOSS cells 0 (0) => **NO-GO**
