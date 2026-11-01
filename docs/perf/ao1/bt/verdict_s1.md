| Cell (AO-1 v2 set 1) | ratio d pair 1 / 2 | ratio agreed | sefer time pair 1 / 2 | sefer time agreed | sefer/mi before → after |
|---|---:|---:|---:|---:|---|
| direct 16 B | +0.0 % / +2.0 % | +0.0 % | +0.0 % / +0.9 % | +0.0 % | 1.251 → 1.264 |
| direct 64 B | +0.8 % / +5.6 % | +0.8 % | +0.0 % / +4.7 % | +0.0 % | 0.858 → 0.885 |
| direct 256 B | +3.1 % / +1.5 % | +1.5 % | +1.7 % / +0.9 % | +0.9 % | 0.759 → 0.776 |
| direct 1024 B | -2.7 % / +0.1 % | +0.0 % | -2.4 % / -1.2 % | -1.2 % | 0.528 → 0.521 |
| churn 16 B | +0.9 % / +2.7 % | +0.9 % | +0.0 % / +0.0 % | +0.0 % | 0.857 → 0.873 |
| churn 64 B | +0.0 % / +0.8 % | +0.0 % | +0.0 % / +0.0 % | +0.0 % | 0.779 → 0.782 |
| churn 256 B | +1.2 % / -2.3 % | +0.0 % | -1.0 % / +0.0 % | +0.0 % | 0.555 → 0.552 |
| churn 1024 B | +20.6 % / +1.5 % | +1.5 % | +0.0 % / +0.0 % | +0.0 % | 0.283 → 0.312 |
| churn_write 16 B | -2.0 % / -2.9 % | -2.0 % | -2.9 % / -3.8 % | -2.9 % | 0.937 → 0.913 |
| churn_write 64 B | +0.9 % / +0.0 % | +0.0 % | +0.0 % / +0.0 % | +0.0 % | 0.877 → 0.881 |
| churn_write 256 B | -2.3 % / -2.5 % | -2.3 % | -2.9 % / -1.9 % | -1.9 % | 0.583 → 0.569 |
| churn_write 1024 B | -0.9 % / -0.3 % | -0.3 % | -2.7 % / +0.0 % | +0.0 % | 0.290 → 0.288 |

Sefer time (A/B of the two builds): geometric mean -0.5 %; agreed wins 6.1 pp, agreed losses 0.9 pp; largest agreed loss +0.9 %.
Ratio to mimalloc: geometric mean over 12 cells: +1.0 %; agreed wins 4.6 pp, agreed losses 4.8 pp (ratio 0.97); largest agreed loss +1.5 %.

Magnitude rule: gm -0.5 % (<= -0.5 %), wins/losses 6.1/0.9 pp (>= 2x), largest loss +0.9 % (<= 3 %), new LOSS cells 0 (0) => **NO-GO**
