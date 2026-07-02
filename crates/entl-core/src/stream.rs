//! The change stream — the engine's `poll` primitive.
//!
//! See notes/design/engine.md, "The change stream — one primitive, every
//! language." The engine pulls git + forge and emits **change batches** (Arrow
//! record batches + a small envelope) into a bounded buffer; a consumer drains
//! them with a blocking, batched `poll(timeout)`. This is the one sync primitive
//! every binding dresses in its own idiom (async iterator / channel / callback),
//! and every sink is just a consumer of it.
//!
//! First cut: a single bounded channel (one consumer). Per-subscriber fan-out
//! and durable cursors come later; the buffer is transport, not a log.

use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender, TryRecvError};
use duckdb::arrow::record_batch::RecordBatch;

/// What happened to the rows in a batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeOp {
    Insert,
    Update,
    /// Insert-or-update by natural key — the forge resources (upsert on re-fetch).
    Upsert,
    Delete,
    /// The table's rows for this repo were wholesale replaced (e.g. `refs`).
    Replace,
}

impl ChangeOp {
    /// Lowercase wire name (for NDJSON / logs).
    pub fn as_str(&self) -> &'static str {
        match self {
            ChangeOp::Insert => "insert",
            ChangeOp::Update => "update",
            ChangeOp::Upsert => "upsert",
            ChangeOp::Delete => "delete",
            ChangeOp::Replace => "replace",
        }
    }
}

/// One unit of the change stream: rows for a single table, carried as Arrow.
#[derive(Debug, Clone)]
pub struct ChangeBatch {
    /// The table these rows belong to (`commits`, `file_changes`, `refs`, …).
    pub table: String,
    /// How the rows changed.
    pub op: ChangeOp,
    /// The rows, columnar. Arrow so it crosses FFI cheaply and every target speaks it.
    pub batch: RecordBatch,
}

impl ChangeBatch {
    pub fn new(table: impl Into<String>, op: ChangeOp, batch: RecordBatch) -> Self {
        Self {
            table: table.into(),
            op,
            batch,
        }
    }

    /// Number of rows in this batch.
    pub fn len(&self) -> usize {
        self.batch.num_rows()
    }

    pub fn is_empty(&self) -> bool {
        self.batch.num_rows() == 0
    }

    /// Pretty-print the rows as a text table (debugging / verification).
    pub fn pretty(&self) -> String {
        duckdb::arrow::util::pretty::pretty_format_batches(std::slice::from_ref(&self.batch))
            .map(|d| d.to_string())
            .unwrap_or_default()
    }
}

/// The result of a `poll`.
#[derive(Debug)]
pub enum Poll {
    /// A batch of changes.
    Batch(ChangeBatch),
    /// Timed out with nothing ready — the producer is still alive.
    Idle,
    /// The producer is gone and the buffer is drained — the stream is over.
    Closed,
}

/// The producer side: the engine emits change batches here.
#[derive(Clone)]
pub struct ChangeSink {
    tx: Sender<ChangeBatch>,
}

impl ChangeSink {
    /// Emit a batch. Blocks if the buffer is full — that block *is* backpressure,
    /// pacing the pull to the slowest consumer. Returns `false` once the consumer
    /// has hung up, so the producer can stop early.
    pub fn emit(&self, batch: ChangeBatch) -> bool {
        self.tx.send(batch).is_ok()
    }
}

/// The consumer side: drain change batches with a blocking, batched poll.
pub struct ChangeStream {
    rx: Receiver<ChangeBatch>,
}

impl ChangeStream {
    /// Block up to `timeout` for the next batch. This is the primitive every
    /// binding wraps: sync languages loop on it in a thread, async ones offload
    /// it and yield a Promise/future.
    pub fn poll(&self, timeout: Duration) -> Poll {
        match self.rx.recv_timeout(timeout) {
            Ok(b) => Poll::Batch(b),
            Err(RecvTimeoutError::Timeout) => Poll::Idle,
            Err(RecvTimeoutError::Disconnected) => Poll::Closed,
        }
    }

    /// Non-blocking: take a batch if one is ready, else report `Idle`/`Closed`.
    pub fn try_poll(&self) -> Poll {
        match self.rx.try_recv() {
            Ok(b) => Poll::Batch(b),
            Err(TryRecvError::Empty) => Poll::Idle,
            Err(TryRecvError::Disconnected) => Poll::Closed,
        }
    }
}

/// Create a bounded change channel: `(producer, consumer)`. `capacity` bounds
/// the in-flight batches (backpressure); the buffer is in-memory transport, not
/// a durable log.
pub fn change_channel(capacity: usize) -> (ChangeSink, ChangeStream) {
    let (tx, rx) = bounded(capacity);
    (ChangeSink { tx }, ChangeStream { rx })
}

#[cfg(test)]
mod tests {
    use super::*;
    use duckdb::arrow::array::Int32Array;
    use duckdb::arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;

    fn tiny_batch(n: i32) -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![Field::new("x", DataType::Int32, false)]));
        let arr = Int32Array::from((0..n).collect::<Vec<_>>());
        RecordBatch::try_new(schema, vec![Arc::new(arr)]).unwrap()
    }

    #[test]
    fn emit_then_poll_returns_the_batch() {
        let (sink, stream) = change_channel(4);
        assert!(sink.emit(ChangeBatch::new("commits", ChangeOp::Insert, tiny_batch(3))));
        match stream.poll(Duration::from_millis(100)) {
            Poll::Batch(b) => {
                assert_eq!(b.table, "commits");
                assert_eq!(b.op, ChangeOp::Insert);
                assert_eq!(b.len(), 3);
            }
            other => panic!("expected a batch, got {other:?}"),
        }
    }

    #[test]
    fn poll_times_out_to_idle_when_nothing_ready() {
        let (_sink, stream) = change_channel(4);
        assert!(matches!(stream.poll(Duration::from_millis(10)), Poll::Idle));
    }

    #[test]
    fn poll_reports_closed_once_the_sink_is_dropped() {
        let (sink, stream) = change_channel(4);
        drop(sink);
        assert!(matches!(stream.poll(Duration::from_millis(10)), Poll::Closed));
    }

    #[test]
    fn producer_on_another_thread_streams_to_a_blocking_poll() {
        let (sink, stream) = change_channel(2);
        let h = std::thread::spawn(move || {
            for _ in 0..5 {
                if !sink.emit(ChangeBatch::new("file_changes", ChangeOp::Insert, tiny_batch(1))) {
                    break;
                }
            }
        });
        let mut got = 0usize;
        loop {
            match stream.poll(Duration::from_millis(500)) {
                Poll::Batch(_) => got += 1,
                Poll::Closed => break,
                Poll::Idle => panic!("producer stalled"),
            }
        }
        h.join().unwrap();
        assert_eq!(got, 5);
    }
}
