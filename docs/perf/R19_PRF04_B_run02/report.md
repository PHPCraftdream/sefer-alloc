# PRF04 gate2b

NO-GO

| axis | arm | k | w | n | commit_idle_median | rss_idle_median | wall_ns_median | first_call_p99_median | numerator | denominator | ratio | noise | bound | pairs | mean_delta | t_stat | p_value | inconclusive | kill |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| commit | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | 5374734336 | 5420199936 | 0.9916118223429313 | 0.000009824043818258212 | unavailable | unavailable | unavailable | unavailable | unavailable | false | false |
| live | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | 1429800 | 7494400 | 0.1907824508966695 | 0.043310022111379645 | 0.05 | 40 | -1229900 | -0.039506125232203315 | 0.9686884179886748 | false | true |
| churn | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | -157062600 | 2851333100 | -0.05508391846606768 | 0.0049853400936290156 | 0.02 | 40 | -124842825 | -0.9094053795814646 | 0.3687222973427694 | false | false |
| wave_p99 | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | -2559200 | 3005200 | -0.8515905763343538 | 0.04212980308896382 | unavailable | unavailable | unavailable | unavailable | unavailable | false | false |

Ratios: (B-A)/A; commit savings (A-B)/A. AA noise: |median(A2)-median(A1)|/median(A1) (commit: per-pair bytes vs max(64MiB,25%*A1)). Diagnostic timings excluded. Secondary cells excluded from verdict.

| arm | k | w | n | commit_idle_median | rss_idle_median | wall_ns_median | first_call_p99_median |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A | 2 | 16 | 5 | 24801280 | 21901312 | 35304100 | 106600 |
| A | 2 | 128 | 5 | 177881088 | 144179200 | 500347500 | 155700 |
| A | 8 | 16 | 5 | 155717632 | 125935616 | 174679100 | 255400 |
| A | 8 | 128 | 5 | 1226403840 | 978276352 | 2861328300 | 969400 |
| A | 32 | 16 | 5 | 680034304 | 542777344 | 1584352000 | 1693400 |
| A | 32 | 128 | 5 | 5420199936 | 2042642432 | 13637800600 | 3005200 |
| B | 2 | 16 | 5 | 4395008 | 5480448 | 39200100 | 66800 |
| B | 2 | 128 | 5 | 4407296 | 5505024 | 381190100 | 69600 |
| B | 8 | 16 | 5 | 12599296 | 12173312 | 101866400 | 232500 |
| B | 8 | 128 | 5 | 12623872 | 12263424 | 2478461100 | 220800 |
| B | 32 | 16 | 5 | 45309952 | 38547456 | 985310200 | 514100 |
| B | 32 | 128 | 5 | 45465600 | 35700736 | 10501423400 | 446000 |
