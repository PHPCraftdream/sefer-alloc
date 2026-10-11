# C1 baseline measurement tables

| w | n | count_delta | count_raw | commit_idle_median | rss_idle_median | rss_delta_median | wall_ns_median | first_workload_p50_ns | first_workload_p99_ns | diagnostic_routed_scans |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 5 | 32 | 33 | 45256704 | 38211584 | 35024896 | 117024200 | 154100 | 5444300 | 0 |
| 128 | 5 | 3969 | 3970 | 5420085248 | 1864613888 | 1861443584 | 20623978100 | 152800 | 12001400 | 0 |

Commit idle median difference (W128 minus W1), bytes: 5374828544.

Threshold: 67108864 bytes. This table is not a measured verdict; the orchestrator reviews validity and decides the next phase. Diagnostic timings are excluded.
