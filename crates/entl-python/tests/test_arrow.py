"""The Arrow handoff, proven in Python: ChangeBatch speaks the Arrow PyCapsule
protocol (``pa.record_batch(batch)`` imports the rows zero-copy — no IPC decode),
``ipc()`` yields the same rows as an IPC stream, and ``query_arrow`` decodes to a
table with native Arrow types (Binary oids, not hex text).

    .venv/bin/pip install pyarrow   (the `arrow` extra)
"""

import json

import entl
import pytest
from conftest import make_repo

pa = pytest.importorskip("pyarrow")


def test_changes_batches_import_via_capsules_and_match_ipc():
    repo = make_repo(commits=2)
    e = entl.Entl(":memory:")

    rows_by_table = {}
    commit_batch = None
    for batch in e.changes(repo, github=False):
        # The zero-copy path: pa.record_batch consumes __arrow_c_array__.
        rb = pa.record_batch(batch)
        rows_by_table[batch.table] = rows_by_table.get(batch.table, 0) + rb.num_rows
        assert batch.op in ("insert", "update", "upsert", "delete", "replace")
        if batch.table == "commits" and commit_batch is None:
            commit_batch = (batch, rb)

    assert rows_by_table["commits"] == 2
    batch, rb = commit_batch
    assert "oid" in rb.schema.names and "author_name" in rb.schema.names

    # The IPC bytes decode to the very same rows the capsules delivered.
    via_ipc = pa.ipc.open_stream(batch.ipc()).read_all()
    assert via_ipc.to_pylist() == pa.Table.from_batches([rb]).to_pylist()

    # And the stream landed in the DB — the JSON plane agrees.
    assert json.loads(e.query("SELECT count(*)::int AS n FROM commits"))[0]["n"] == 2


def test_query_arrow_is_the_dataframe_on_ramp():
    repo = make_repo()
    e = entl.Entl(":memory:")
    e.load_git(repo)

    t = pa.ipc.open_stream(e.query_arrow("SELECT oid, author_name FROM commits")).read_all()
    assert t.num_rows == 1
    assert t.schema.names == ["oid", "author_name"]
    # Native Arrow semantics: oid is Binary (20 raw bytes), not hex text.
    assert len(t["oid"][0].as_py()) == 20
    assert t["author_name"][0].as_py() == "Tester"

    # Zero rows still decodes (schema-only stream).
    empty = pa.ipc.open_stream(e.query_arrow("SELECT 1 AS x WHERE false")).read_all()
    assert empty.num_rows == 0 and empty.schema.names == ["x"]
