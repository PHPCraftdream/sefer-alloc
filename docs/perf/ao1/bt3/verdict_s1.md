| Cell (AO-1 v2b set 1) | ratio d pair 1 / 2 | ratio agreed | sefer time pair 1 / 2 | sefer time agreed | sefer/mi before → after |
|---|---:|---:|---:|---:|---|
| direct 16 B | -3.6 % / +0.0 % | +0.0 % | -2.6 % / +0.0 % | +0.0 % | 1.275 → 1.251 |
| direct 64 B | -4.5 % / +0.0 % | +0.0 % | -3.7 % / +0.0 % | +0.0 % | 0.854 → 0.834 |
| direct 256 B | +2.7 % / +3.5 % | +2.7 % | +3.4 % / +3.5 % | +3.4 % | 0.752 → 0.775 |
| direct 1024 B | +0.6 % / -1.0 % | +0.0 % | +0.0 % / +0.0 % | +0.0 % | 0.523 → 0.522 |
| churn 16 B | -11.9 % / -1.0 % | -1.0 % | -12.7 % / -1.0 % | -1.0 % | 0.936 → 0.872 |
| churn 64 B | +0.6 % / -3.5 % | +0.0 % | -1.0 % / -1.1 % | -1.0 % | 0.799 → 0.788 |
| churn 256 B | -3.8 % / -2.4 % | -2.4 % | -1.0 % / -3.0 % | -1.0 % | 0.580 → 0.562 |
| churn 1024 B | -0.3 % / +0.7 % | +0.0 % | +0.0 % / +1.0 % | +0.0 % | 0.277 → 0.278 |
| churn_write 16 B | -2.0 % / -2.1 % | -2.0 % | -2.0 % / -3.9 % | -2.0 % | 0.923 → 0.904 |
| churn_write 64 B | +1.0 % / +3.0 % | +1.0 % | +1.0 % / +3.0 % | +1.0 % | 0.880 → 0.898 |
| churn_write 256 B | +2.3 % / +21.0 % | +2.3 % | +0.0 % / +12.8 % | +0.0 % | 0.575 → 0.642 |
| churn_write 1024 B | +3.6 % / +1.6 % | +1.6 % | +0.9 % / +0.0 % | +0.0 % | 0.282 → 0.289 |

Sefer time (A/B of the two builds): geometric mean -0.4 %; agreed wins 5.1 pp, agreed losses 4.4 pp; largest agreed loss +3.4 %.
Ratio to mimalloc: geometric mean over 12 cells: +0.0 %; agreed wins 5.5 pp, agreed losses 7.6 pp (ratio 0.72); largest agreed loss +2.7 %.

Magnitude rule: gm -0.4 % (<= -0.5 %), wins/losses 5.1/4.4 pp (>= 2x), largest loss +3.4 % (<= 3 %), new LOSS cells 0 (0) => **NO-GO**
