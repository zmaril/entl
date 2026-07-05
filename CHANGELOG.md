# Changelog

Notable changes to this project, newest first, by date.

## 2026-07-04

- **git notes + forge repo metadata**: new `git_notes` table (every `refs/notes/*` ref,
  bulk-replaced per sync like refs, streamed to sinks), and the `repos` row now gets
  `owner`/`name`/`host`/`default_branch`/`homepage_url` from GitHub on active syncs
  (new `homepage_url` column). Also fixed: rebuilding an existing store across a schema
  change no longer trips on the `oid()` macro (extras.sql is now fully idempotent).
- **Arrow handoff**: the change stream crosses the binding boundary columnar instead of as
  JSON strings. `ChangeBatch` now holds the Arrow `RecordBatch`: `ipc` yields it as one
  Arrow IPC stream everywhere, and the Python class speaks the Arrow PyCapsule interface
  (`pa.record_batch(batch)` imports zero-copy; entl ships no pyarrow). New `queryArrow(sql)`
  returns a whole result set as one IPC stream — the dataframe on-ramp. **Breaking**:
  `ChangeBatch.rowsJson` is gone, and the stream now carries native Arrow types (binary
  oids, µs timestamps) with no per-row `_op` — `query()`/`extract()` keep the canonical
  JSON form (hex oids, RFC3339).
- started keeping a changelog
