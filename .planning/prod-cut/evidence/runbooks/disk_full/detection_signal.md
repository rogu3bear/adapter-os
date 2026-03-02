# Detection Signal

Signals captured:

- Baseline capacity:
  - `/dev/disk3s5 ... 146Gi avail, 84% capacity`
- Space concentration:
  - `var/models/` at `70G`
- WAL growth indicator:
  - `var/aos-cp.sqlite3-wal` size `4.0M`
- Synthetic write failure:
  - `synthetic_disk_full_exit=153`

Interpretation:

- Storage pressure vectors and write-failure behavior were observed and recorded.
