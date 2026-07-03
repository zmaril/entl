//! Ruby binding for the entl engine (Magnus) — the Rust sync engine in-process in Ruby, mirroring
//! the Python (PyO3) and Node (napi) bindings. First cut: open, sink, query, extract. Ruby's GVL
//! serialises access; each call clones the DuckDB connection for its work (same database).

use magnus::{function, method, prelude::*, Error, Ruby};

use entl_core::{build_sink, extract_json, pull_into, Db, PullOpts, SinkSelect, SinkTarget};

/// An open entl database.
#[magnus::wrap(class = "Entl", free_immediately, size)]
struct Entl {
    db: Db,
}

fn rberr(e: impl std::fmt::Display) -> Error {
    let ruby = magnus::Ruby::get().expect("entl called outside the Ruby GVL");
    Error::new(ruby.exception_runtime_error(), e.to_string())
}

impl Entl {
    /// Open (or create) the .duckdb at `path` and apply the schema.
    fn new(path: String) -> Result<Self, Error> {
        let db = Db::open(&path).map_err(rberr)?;
        db.migrate().map_err(rberr)?;
        Ok(Entl { db })
    }

    /// Pull `repo` (git only) into `target` (`sqlite` | `jsonl` | `postgres`) at `dest`.
    fn sink(&self, repo: String, target: String, dest: String) -> Result<(), Error> {
        let tgt: SinkTarget = target.parse().map_err(rberr)?;
        let d = Db::from_conn(self.db.conn.try_clone().map_err(rberr)?);
        let sink = build_sink(tgt, Some(&dest), SinkSelect::default()).map_err(rberr)?;
        pull_into(&d, &repo, sink, PullOpts { github: false, objects: false }).map_err(rberr)?;
        Ok(())
    }

    /// Run a SQL query → a JSON string (array of row objects).
    fn query(&self, sql: String) -> Result<String, Error> {
        let wrapped = format!(
            "SELECT CAST(COALESCE(json_group_array(to_json(__t)), '[]') AS VARCHAR) FROM ({sql}) AS __t"
        );
        self.db.conn.query_row(&wrapped, [], |r| r.get(0)).map_err(rberr)
    }

    /// Read a store (`sqlite` | `jsonl` | `postgres` | `duckdb`) back into canonical rows (JSON).
    fn extract(&self, source: String, dest: String) -> Result<String, Error> {
        let tables: Vec<String> = entl_core::extract::GIT_TABLES.iter().map(|s| s.to_string()).collect();
        extract_json(&source, &dest, &tables, None).map_err(rberr)
    }
}

#[magnus::init(name = "entl")]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let class = ruby.define_class("Entl", ruby.class_object())?;
    class.define_singleton_method("new", function!(Entl::new, 1))?;
    class.define_method("sink", method!(Entl::sink, 3))?;
    class.define_method("query", method!(Entl::query, 1))?;
    class.define_method("extract", method!(Entl::extract, 2))?;
    Ok(())
}
