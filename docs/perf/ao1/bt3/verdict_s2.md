| Cell (AO-1 v2b set 2) | ratio d pair 1 / 2 | ratio agreed | sefer time pair 1 / 2 | sefer time agreed | sefer/mi before → after |
|---|---:|---:|---:|---:|---|
| direct 16 B | +2.6 % / -0.9 % | +0.0 % | +2.6 % / -0.9 % | +0.0 % | 1.253 → 1.264 |
| direct 64 B | +2.6 % / +2.0 % | +2.0 % | +0.9 % / +2.8 % | +0.9 % | 0.874 → 0.894 |
| direct 256 B | +0.0 % / +4.1 % | +0.0 % | +0.0 % / +3.4 % | +0.0 % | 0.755 → 0.770 |
| direct 1024 B | -4.6 % / -2.7 % | -2.7 % | -1.2 % / -3.0 % | -1.2 % | 0.534 → 0.515 |
| churn 16 B | -4.9 % / +0.9 % | +0.0 % | -4.0 % / +0.0 % | +0.0 % | 0.895 → 0.877 |
| churn 64 B | +3.6 % / +1.7 % | +1.7 % | +1.1 % / +0.0 % | +0.0 % | 0.788 → 0.809 |
| churn 256 B | -4.1 % / -1.8 % | -1.8 % | +0.0 % / +0.0 % | +0.0 % | 0.584 → 0.566 |
| churn 1024 B | +3.0 % / +11.6 % | +3.0 % | +0.0 % / +1.0 % | +0.0 % | 0.267 → 0.287 |
| churn_write 16 B | -0.9 % / +0.0 % | +0.0 % | +0.0 % / +0.0 % | +0.0 % | 0.909 → 0.905 |
| churn_write 64 B | +1.9 % / +1.9 % | +1.9 % | +1.0 % / +1.0 % | +1.0 % | 0.868 → 0.885 |
| churn_write 256 B | -5.2 % / -2.1 % | -2.1 % | -4.7 % / -1.0 % | -1.0 % | 0.606 → 0.583 |
| churn_write 1024 B | -8.9 % / +1.4 % | +0.0 % | -3.6 % / -1.8 % | -1.8 % | 0.303 → 0.291 |

Sefer time (A/B of the two builds): geometric mean -0.3 %; agreed wins 4.0 pp, agreed losses 1.9 pp; largest agreed loss +1.0 %.
Ratio to mimalloc: geometric mean over 12 cells: -0.0 %; agreed wins 6.5 pp, agreed losses 8.6 pp (ratio 0.76); largest agreed loss +3.0 %.

Magnitude rule: gm -0.3 % (<= -0.5 %), wins/losses 4.0/1.9 pp (>= 2x), largest loss +1.0 % (<= 3 %), new LOSS cells 0 (0) => **NO-GO**
