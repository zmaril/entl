# Changelog

Notable changes to this project, newest first, by date.

## 2026-07-04

- **Arrow handoff**: the change stream crosses the binding boundary columnar instead of as
  JSON strings. `ChangeBatch` now holds the Arrow `RecordBatch`: `ipc` yields it as one
  Arrow IPC stream everywhere, and the Python class speaks the Arrow PyCapsule interface
  (`pa.record_batch(batch)` imports zero-copy; entl ships no pyarrow). New `queryArrow(sql)`
  returns a whole result set as one IPC stream — the dataframe on-ramp. **Breaking**:
  `ChangeBatch.rowsJson` is gone, and the stream now carries native Arrow types (binary
  oids, µs timestamps) with no per-row `_op` — `query()`/`extract()` keep the canonical
  JSON form (hex oids, RFC3339).
- started keeping a changelog
