| Cell (AO-1 v2b set 3) | ratio d pair 1 / 2 | ratio agreed | sefer time pair 1 / 2 | sefer time agreed | sefer/mi before → after |
|---|---:|---:|---:|---:|---|
| direct 16 B | +0.0 % / +0.6 % | +0.0 % | +0.0 % / +1.8 % | +0.0 % | 1.247 → 1.251 |
| direct 64 B | -7.6 % / +1.7 % | +0.0 % | -6.1 % / +0.0 % | +0.0 % | 0.894 → 0.866 |
| direct 256 B | +3.2 % / +2.6 % | +2.6 % | +2.6 % / +2.6 % | +2.6 % | 0.756 → 0.778 |
| direct 1024 B | -0.6 % / -1.9 % | -0.6 % | -1.2 % / -0.6 % | -0.6 % | 0.531 → 0.525 |
| churn 16 B | +0.1 % / +0.1 % | +0.1 % | +1.0 % / +1.1 % | +1.0 % | 0.872 → 0.873 |
| churn 64 B | +0.8 % / -1.6 % | +0.0 % | +0.0 % / +0.0 % | +0.0 % | 0.789 → 0.785 |
| churn 256 B | -4.2 % / -7.3 % | -4.2 % | +1.0 % / -1.0 % | +0.0 % | 0.592 → 0.558 |
| churn 1024 B | +5.2 % / +3.4 % | +3.4 % | +1.0 % / +1.0 % | +1.0 % | 0.279 → 0.291 |
| churn_write 16 B | -6.4 % / +3.0 % | +0.0 % | -5.6 % / +3.0 % | +0.0 % | 0.941 → 0.923 |
| churn_write 64 B | +1.1 % / -2.7 % | +0.0 % | +2.0 % / -1.0 % | +0.0 % | 0.885 → 0.878 |
| churn_write 256 B | -0.4 % / -6.0 % | -0.4 % | -1.0 % / -1.0 % | -1.0 % | 0.599 → 0.580 |
| churn_write 1024 B | -3.5 % / +4.8 % | +0.0 % | +1.8 % / -1.8 % | +0.0 % | 0.298 → 0.300 |

Sefer time (A/B of the two builds): geometric mean -0.0 %; agreed wins 1.6 pp, agreed losses 4.6 pp; largest agreed loss +2.6 %.
Ratio to mimalloc: geometric mean over 12 cells: -0.7 %; agreed wins 5.2 pp, agreed losses 6.2 pp (ratio 0.84); largest agreed loss +3.4 %.

Magnitude rule: gm -0.0 % (<= -0.5 %), wins/losses 1.6/4.6 pp (>= 2x), largest loss +2.6 % (<= 3 %), new LOSS cells 0 (0) => **NO-GO**
