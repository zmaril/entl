//! Python (PyO3) binding for the entl engine. The Rust sync engine runs in-process inside
//! CPython, so the one DuckDB connection used for writes is the same database the reads see —
//! no cross-process file-lock fight (mirrors the Node binding, notes/design/multilibrary.md).
//!
//! entl-core stays **synchronous**; async is a per-binding concern. Here each heavy method
//! releases the GIL via [`Python::allow_threads`] (Python's idiomatic equivalent of Node's
//! off-thread `AsyncTask`), so other Python threads run while the pull/sink is in flight. Each
//! call runs on its own `try_clone()`d connection (the same underlying database).

use std::sync::atomic::AtomicU64;

use anyhow::Result as AnyResult;
use entl_core::{Db, GitIngest, GithubIngest, SinkOutcome};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

fn err(e: impl std::fmt::Display) -> PyErr {
    PyRuntimeError::new_err(e.to_string())
}

/// A built-in sink target. Exposed as `entl.SinkTarget.Sqlite` / `.Jsonl`; maps onto the
/// source-of-truth [`entl_core::SinkTarget`].
#[pyclass(eq, eq_int)]
#[derive(Clone, Copy, PartialEq)]
pub enum SinkTarget {
    Sqlite,
    Jsonl,
    Postgres,
}
impl From<SinkTarget> for entl_core::SinkTarget {
    fn from(t: SinkTarget) -> Self {
        match t {
            SinkTarget::Sqlite => entl_core::SinkTarget::Sqlite,
            SinkTarget::Jsonl => entl_core::SinkTarget::Jsonl,
            SinkTarget::Postgres => entl_core::SinkTarget::Postgres,
        }
    }
}

/// An open entl database. Heavy methods release the GIL while they work.
///
/// `unsendable`: the DuckDB connection isn't `Sync`, and the handle is only ever touched by the
/// thread that created it — `allow_threads` releases the GIL on that same thread rather than
/// moving the object. Each call clones the connection into an owned `Db` for its off-GIL work.
#[pyclass(unsendable)]
pub struct Entl {
    db: Db,
}

#[pymethods]
impl Entl {
    /// Open (or create) the .duckdb at `db_path` and apply migrations. Use `":memory:"` to skip
    /// a persistent DuckDB and only write a `sink()` target.
    #[new]
    fn new(db_path: String) -> PyResult<Self> {
        let db = Db::open(&db_path).map_err(err)?;
        db.migrate().map_err(err)?;
        Ok(Self { db })
    }

    /// Load git history from `repo` (one-way, incremental). Returns a stats dict.
    fn load_git<'py>(&self, py: Python<'py>, repo: String) -> PyResult<Bound<'py, PyDict>> {
        let db = Db::from_conn(self.db.conn.try_clone().map_err(err)?);
        let g = py
            .allow_threads(move || -> AnyResult<GitIngest> {
                let counter = AtomicU64::new(0);
                entl_core::ingest_git(&db, &repo, &counter)
            })
            .map_err(err)?;
        git_dict(py, &g)
    }

    /// Load GitHub data (events/PRs/issues/Actions). Needs a token (`gh auth token` / GH_TOKEN).
    /// Returns a stats dict.
    fn load_github<'py>(&self, py: Python<'py>, repo: String) -> PyResult<Bound<'py, PyDict>> {
        let db = Db::from_conn(self.db.conn.try_clone().map_err(err)?);
        let gh = py
            .allow_threads(move || entl_core::ingest_github(&db, &repo))
            .map_err(err)?;
        github_dict(py, &gh)
    }

    /// Run a SQL query → a JSON string (array of row objects).
    fn query(&self, py: Python<'_>, sql: String) -> PyResult<String> {
        let db = Db::from_conn(self.db.conn.try_clone().map_err(err)?);
        py.allow_threads(move || -> AnyResult<String> {
            // DuckDB serializes the result; all types come back as JSON.
            let wrapped = format!(
                "SELECT CAST(COALESCE(json_group_array(to_json(__t)), '[]') AS VARCHAR) \
                 FROM ({sql}) AS __t"
            );
            Ok(db.conn.query_row(&wrapped, [], |row| row.get(0))?)
        })
        .map_err(err)
    }

    /// Pull `repo` and sync it into `target` (a SQLite file / JSONL dir / Postgres URL), in one
    /// call. Writes both this handle's DuckDB (the default store) and the chosen target. `path`
    /// is the SQLite file, JSONL directory, or Postgres connection URL; `github` also pulls the
    /// forge (default True). `tables`/`exclude` narrow which tables are written, `rename` maps
    /// `{source: target}` names, and `schema` sets the Postgres target schema. Returns a stats
    /// dict.
    #[pyo3(signature = (repo, target, path=None, github=true, objects=false, tables=None, exclude=None, rename=None, schema=None))]
    #[allow(clippy::too_many_arguments)]
    fn sink<'py>(
        &self,
        py: Python<'py>,
        repo: String,
        target: SinkTarget,
        path: Option<String>,
        github: bool,
        objects: bool,
        tables: Option<Vec<String>>,
        exclude: Option<Vec<String>>,
        rename: Option<std::collections::HashMap<String, String>>,
        schema: Option<String>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let db = Db::from_conn(self.db.conn.try_clone().map_err(err)?);
        let core_target: entl_core::SinkTarget = target.into();
        let select = entl_core::SinkSelect {
            tables,
            exclude: exclude.unwrap_or_default(),
            rename: rename.map(|m| m.into_iter().collect()).unwrap_or_default(),
            schema,
        };
        let outcome = py
            .allow_threads(move || -> AnyResult<SinkOutcome> {
                let sink = entl_core::build_sink(core_target, path.as_deref(), select)?;
                entl_core::pull_into(&db, &repo, sink, entl_core::PullOpts { github, objects })
            })
            .map_err(err)?;
        outcome_dict(py, &outcome)
    }

    /// Read a store back into canonical rows → a JSON string (table → rows, normalized like the
    /// sinks write). The reverse of `sink`. `source` is `duckdb`|`sqlite`|`jsonl`|`postgres`.
    #[pyo3(signature = (source, dest, tables=None, schema=None))]
    fn extract(&self, py: Python<'_>, source: String, dest: String, tables: Option<Vec<String>>, schema: Option<String>) -> PyResult<String> {
        let tables = tables.unwrap_or_else(|| {
            entl_core::extract::GIT_TABLES.iter().map(|s| s.to_string()).collect()
        });
        py.allow_threads(move || entl_core::extract_json(&source, &dest, &tables, schema.as_deref()))
            .map_err(err)
    }
}

fn git_dict<'py>(py: Python<'py>, g: &GitIngest) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("new_commits", g.new_commits)?;
    d.set_item("file_changes", g.file_changes)?;
    d.set_item("refs", g.refs)?;
    Ok(d)
}

fn github_dict<'py>(py: Python<'py>, g: &GithubIngest) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("pull_requests", g.pull_requests)?;
    d.set_item("reviews", g.reviews)?;
    d.set_item("review_comments", g.review_comments)?;
    d.set_item("issues", g.issues)?;
    d.set_item("comments", g.comments)?;
    d.set_item("workflow_runs", g.workflow_runs)?;
    d.set_item("check_runs", g.check_runs)?;
    d.set_item("users", g.users)?;
    d.set_item("events", g.events)?;
    Ok(d)
}

fn outcome_dict<'py>(py: Python<'py>, o: &SinkOutcome) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("new_commits", o.git.new_commits)?;
    d.set_item("file_changes", o.git.file_changes)?;
    d.set_item("refs", o.git.refs)?;
    if let Some(gh) = &o.github {
        d.set_item("pull_requests", gh.pull_requests)?;
        d.set_item("issues", gh.issues)?;
        d.set_item("events", gh.events)?;
        d.set_item("workflow_runs", gh.workflow_runs)?;
        d.set_item("check_runs", gh.check_runs)?;
    }
    d.set_item("rows", o.rows)?;
    Ok(d)
}

#[pymodule]
fn entl(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Entl>()?;
    m.add_class::<SinkTarget>()?;
    Ok(())
}
