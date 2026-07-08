//! Full-fidelity object ingest — walks every commit's tree and stores the `trees`,
//! `tree_entries`, and `blobs` (with raw `content`) needed to reconstruct a repo byte-for-byte
//! (`entl rebuild`; see notes/design/testing.md). Opt-in (heavy for large repos): the default pull
//! stores commit metadata + diffs only. Runs *after* the commit pass and reads the distinct
//! `tree_oid`s from the `commits` table, so it walks each unique tree once.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context, Result};
// Builders for change-stream batches → entl's OWN arrow (the batches are emitted, not appended).
use arrow::array::{ArrayRef, BinaryBuilder, BooleanBuilder, Int64Builder, StringBuilder};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use duckdb::params;
use gix::bstr::ByteSlice;

use crate::db::Db;
use crate::ingest::compute_repo_id;
use crate::stream::{ChangeBatch, ChangeOp, ChangeSink};

#[derive(Default)]
pub struct ObjIngest {
    pub trees: usize,
    pub tree_entries: usize,
    pub blobs: usize,
}

struct TreeRow {
    oid: gix::ObjectId,
}
struct EntryRow {
    tree_oid: gix::ObjectId,
    name: String,
    mode: &'static str,
    entry_type: &'static str,
    child_oid: gix::ObjectId,
}
struct BlobRow {
    oid: gix::ObjectId,
    size: i64,
    is_binary: bool,
    content: Vec<u8>,
}

const CHUNK: usize = 1000;

/// Ingest the object graph (trees/tree_entries/blobs) reachable from the commits already in `db`.
pub fn ingest_git_objects(db: &Db, path: &str, sink: Option<&ChangeSink>) -> Result<ObjIngest> {
    let repo = gix::discover(path).context("discover git repo")?;
    let (repo_id, _) = compute_repo_id(&repo);

    // Distinct commit trees to walk (already ingested by the commit pass).
    let roots: Vec<gix::ObjectId> = {
        let mut stmt = db
            .conn
            .prepare("SELECT DISTINCT tree_oid FROM commits WHERE repo_id = ?")?;
        let rows = stmt.query_map([&repo_id], |r| r.get::<_, Vec<u8>>(0))?;
        rows.collect::<duckdb::Result<Vec<_>>>()?
            .into_iter()
            .filter_map(|b| gix::ObjectId::try_from(b.as_slice()).ok())
            .collect()
    };

    let (mut trees, mut entries, mut blobs) = (Vec::new(), Vec::new(), Vec::new());
    let (mut seen_tree, mut seen_blob) = (HashSet::new(), HashSet::new());
    for root in roots {
        walk(
            &repo,
            root,
            &mut trees,
            &mut entries,
            &mut blobs,
            &mut seen_tree,
            &mut seen_blob,
        )?;
    }

    let stats = ObjIngest {
        trees: trees.len(),
        tree_entries: entries.len(),
        blobs: blobs.len(),
    };

    // Write DuckDB (appenders) + emit change batches for the sinks.
    {
        let mut ta = db.conn.appender("trees")?;
        for t in &trees {
            ta.append_row(params![t.oid.as_bytes(), repo_id])?;
        }
        ta.flush()?;
        let mut ea = db.conn.appender("tree_entries")?;
        for e in &entries {
            ea.append_row(params![
                e.tree_oid.as_bytes(),
                e.name,
                e.name,
                e.mode,
                e.entry_type,
                e.child_oid.as_bytes()
            ])?;
        }
        ea.flush()?;
        let mut ba = db.conn.appender("blobs")?;
        for b in &blobs {
            ba.append_row(params![
                b.oid.as_bytes(),
                repo_id,
                b.size,
                b.is_binary,
                Option::<&str>::None,
                Option::<&str>::None,
                b.content.as_slice()
            ])?;
        }
        ba.flush()?;
    }

    if let Some(sink) = sink {
        for chunk in trees.chunks(CHUNK) {
            sink.emit(ChangeBatch::new(
                "trees",
                ChangeOp::Insert,
                trees_batch(chunk, &repo_id)?,
            ));
        }
        for chunk in entries.chunks(CHUNK) {
            sink.emit(ChangeBatch::new(
                "tree_entries",
                ChangeOp::Insert,
                entries_batch(chunk)?,
            ));
        }
        for chunk in blobs.chunks(CHUNK) {
            sink.emit(ChangeBatch::new(
                "blobs",
                ChangeOp::Insert,
                blobs_batch(chunk, &repo_id)?,
            ));
        }
    }

    Ok(stats)
}

fn walk(
    repo: &gix::Repository,
    tree_oid: gix::ObjectId,
    trees: &mut Vec<TreeRow>,
    entries: &mut Vec<EntryRow>,
    blobs: &mut Vec<BlobRow>,
    seen_tree: &mut HashSet<gix::ObjectId>,
    seen_blob: &mut HashSet<gix::ObjectId>,
) -> Result<()> {
    if !seen_tree.insert(tree_oid) {
        return Ok(());
    }
    trees.push(TreeRow { oid: tree_oid });
    let tree = repo.find_object(tree_oid)?.into_tree();
    for e in tree.iter() {
        let e = e?;
        let child = e.oid().to_owned();
        let (entry_type, mode): (&'static str, &'static str) = match e.mode().kind() {
            gix::object::tree::EntryKind::Tree => ("tree", "40000"),
            gix::object::tree::EntryKind::Blob => ("blob", "100644"),
            gix::object::tree::EntryKind::BlobExecutable => ("blob", "100755"),
            gix::object::tree::EntryKind::Link => ("blob", "120000"),
            gix::object::tree::EntryKind::Commit => ("commit", "160000"),
        };
        entries.push(EntryRow {
            tree_oid,
            name: e.filename().to_str_lossy().into_owned(),
            mode,
            entry_type,
            child_oid: child,
        });
        match e.mode().kind() {
            gix::object::tree::EntryKind::Tree => {
                walk(repo, child, trees, entries, blobs, seen_tree, seen_blob)?;
            }
            gix::object::tree::EntryKind::Commit => {} // gitlink/submodule — no blob content
            _ => {
                if seen_blob.insert(child) {
                    let data = repo.find_object(child)?.data.clone();
                    blobs.push(BlobRow {
                        oid: child,
                        size: data.len() as i64,
                        is_binary: is_binary(&data),
                        content: data,
                    });
                }
            }
        }
    }
    Ok(())
}

/// A NUL byte in the first 8 KiB → treat as binary (git's own heuristic).
fn is_binary(data: &[u8]) -> bool {
    data.iter().take(8192).any(|&b| b == 0)
}

fn binary_col<'a>(oids: impl Iterator<Item = &'a gix::ObjectId>) -> ArrayRef {
    let mut b = BinaryBuilder::new();
    for o in oids {
        b.append_value(o.as_bytes());
    }
    Arc::new(b.finish())
}

fn str_col(vals: impl Iterator<Item = String>) -> ArrayRef {
    let mut b = StringBuilder::new();
    for v in vals {
        b.append_value(v);
    }
    Arc::new(b.finish())
}

fn repo_col(n: usize, repo_id: &str) -> ArrayRef {
    str_col(std::iter::repeat_n(repo_id.to_string(), n))
}

fn trees_batch(rows: &[TreeRow], repo_id: &str) -> Result<RecordBatch> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("oid", DataType::Binary, false),
        Field::new("repo_id", DataType::Utf8, false),
    ]));
    Ok(RecordBatch::try_new(
        schema,
        vec![
            binary_col(rows.iter().map(|r| &r.oid)),
            repo_col(rows.len(), repo_id),
        ],
    )?)
}

fn entries_batch(rows: &[EntryRow]) -> Result<RecordBatch> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("tree_oid", DataType::Binary, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("path", DataType::Utf8, false),
        Field::new("mode", DataType::Utf8, false),
        Field::new("entry_type", DataType::Utf8, false),
        Field::new("child_oid", DataType::Binary, false),
    ]));
    Ok(RecordBatch::try_new(
        schema,
        vec![
            binary_col(rows.iter().map(|r| &r.tree_oid)),
            str_col(rows.iter().map(|r| r.name.clone())),
            str_col(rows.iter().map(|r| r.name.clone())),
            str_col(rows.iter().map(|r| r.mode.to_string())),
            str_col(rows.iter().map(|r| r.entry_type.to_string())),
            binary_col(rows.iter().map(|r| &r.child_oid)),
        ],
    )?)
}

fn blobs_batch(rows: &[BlobRow], repo_id: &str) -> Result<RecordBatch> {
    let mut size = Int64Builder::new();
    let mut is_bin = BooleanBuilder::new();
    let mut content = BinaryBuilder::new();
    let mut ctext = StringBuilder::new();
    let mut csha = StringBuilder::new();
    for r in rows {
        size.append_value(r.size);
        is_bin.append_value(r.is_binary);
        content.append_value(&r.content);
        ctext.append_null();
        csha.append_null();
    }
    let schema = Arc::new(Schema::new(vec![
        Field::new("oid", DataType::Binary, false),
        Field::new("repo_id", DataType::Utf8, false),
        Field::new("size", DataType::Int64, false),
        Field::new("is_binary", DataType::Boolean, false),
        Field::new("content_text", DataType::Utf8, true),
        Field::new("content_sha", DataType::Utf8, true),
        Field::new("content", DataType::Binary, true),
    ]));
    Ok(RecordBatch::try_new(
        schema,
        vec![
            binary_col(rows.iter().map(|r| &r.oid)),
            repo_col(rows.len(), repo_id),
            Arc::new(size.finish()),
            Arc::new(is_bin.finish()),
            Arc::new(ctext.finish()),
            Arc::new(csha.finish()),
            Arc::new(content.finish()),
        ],
    )?)
}
