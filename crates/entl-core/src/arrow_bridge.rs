//! The Arrow version bridge — duckdb's arrow → entl's arrow.
//!
//! entl-core carries its change stream in its OWN arrow (`entl_core::RecordBatch`), which floats
//! independently of the arrow the `duckdb` crate bundles (see notes/design/arrow-ipc.md and the
//! `arrow`/`arrow58` deps in Cargo.toml). Every batch that ORIGINATES inside entl (the in-memory
//! builders in `ingest`/`objects`) is built natively in entl's arrow — no conversion. The only
//! batches born in duckdb's arrow are the ones DuckDB hands back from `query_arrow(...)`; this
//! module converts those to entl's arrow at the read boundary.
//!
//! Mechanism: **Arrow IPC**. The Arrow IPC stream format is stable across arrow major versions,
//! so serializing a batch with duckdb's arrow (`arrow58`) and reading it back with entl's arrow
//! (`arrow`) is a well-defined, safe, no-`unsafe` round-trip. The read sites are all bounded and
//! one-shot (a table backfill, a delta emit), so the one copy the IPC buffer costs is negligible;
//! the hot in-memory builder path never comes here.

use anyhow::Result;

/// Serialize DuckDB-produced Arrow batches (duckdb's arrow, v58) to an Arrow IPC stream (`schema` +
/// the batches). `arrow58` is the same crate instance duckdb links, with `ipc` unioned in, so its
/// StreamWriter accepts these batches. Shared by the bridge below and `Db::query_arrow_ipc` (the
/// one place we serialize duckdb batches straight to bytes).
pub fn duck_batches_to_ipc(
    schema: &duckdb::arrow::datatypes::Schema,
    batches: &[duckdb::arrow::record_batch::RecordBatch],
) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut w = arrow58::ipc::writer::StreamWriter::try_new(&mut buf, schema)?;
    for b in batches {
        w.write(b)?;
    }
    w.finish()?;
    Ok(buf)
}

/// Convert DuckDB-produced Arrow batches (duckdb's arrow, `arrow58`) into entl's Arrow batches
/// (`arrow`), via an Arrow IPC round-trip. Empty input yields an empty vec (nothing to serialize).
pub fn duckdb_batches_to_entl(
    batches: Vec<duckdb::arrow::record_batch::RecordBatch>,
) -> Result<Vec<arrow::record_batch::RecordBatch>> {
    if batches.is_empty() {
        return Ok(Vec::new());
    }
    // Serialize with duckdb's arrow (v58), then read back with entl's arrow (v59). The IPC stream
    // carries the schema, so the round-trip is self-contained and version-independent.
    let ipc = duck_batches_to_ipc(batches[0].schema().as_ref(), &batches)?;
    let reader = arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(ipc), None)?;
    Ok(reader.collect::<std::result::Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_duckdb_batch_into_entl_arrow() {
        // Build a batch in duckdb's arrow (v58), bridge it, and check the entl-arrow (v59) result.
        use duckdb::arrow::array::{Int32Array, StringArray};
        use duckdb::arrow::datatypes::{DataType, Field, Schema};
        use std::sync::Arc;

        let schema = Arc::new(Schema::new(vec![
            Field::new("n", DataType::Int32, false),
            Field::new("s", DataType::Utf8, false),
        ]));
        let n = Int32Array::from(vec![1, 2, 3]);
        let s = StringArray::from(vec!["a", "b", "c"]);
        let duck: duckdb::arrow::record_batch::RecordBatch =
            duckdb::arrow::record_batch::RecordBatch::try_new(
                schema,
                vec![Arc::new(n), Arc::new(s)],
            )
            .unwrap();

        let entl = duckdb_batches_to_entl(vec![duck]).unwrap();
        assert_eq!(entl.len(), 1);
        assert_eq!(entl[0].num_rows(), 3);
        assert_eq!(entl[0].schema().field(0).name(), "n");
        assert_eq!(entl[0].schema().field(1).name(), "s");
    }

    #[test]
    fn empty_input_is_empty_output() {
        assert!(duckdb_batches_to_entl(Vec::new()).unwrap().is_empty());
    }
}
