"""Smoke test: sink a local repo into a temp SQLite file and check the counts.

Runs git-only (``github=False``) so it needs no GitHub token; full-forge parity is covered by
the Rust/Node runs. Point at another repo with ``ENTL_TEST_REPO``.

    maturin develop && python -m pytest crates/entl-python/tests/
    # or, without pytest:
    python crates/entl-python/tests/test_sink.py
"""

import os
import sqlite3
import subprocess
import tempfile

import entl


def _fixture_repo():
    """A self-contained two-commit repo (CI has no ~/projects to point at)."""
    d = tempfile.mkdtemp()
    repo = os.path.join(d, "repo")
    subprocess.run(["git", "init", "-q", repo], check=True)
    subprocess.run(["git", "-C", repo, "config", "user.email", "t@e.com"], check=True)
    subprocess.run(["git", "-C", repo, "config", "user.name", "Tester"], check=True)
    for i, name in enumerate(("a.txt", "b.txt")):
        open(os.path.join(repo, name), "w").write(f"hello {i}\n")
        subprocess.run(["git", "-C", repo, "add", "-A"], check=True)
        subprocess.run(["git", "-C", repo, "commit", "-qm", f"commit {i}"], check=True)
    return repo


REPO = os.environ.get("ENTL_TEST_REPO") or _fixture_repo()


def _counts(db):
    con = sqlite3.connect(db)
    try:
        return {t: con.execute(f"SELECT count(*) FROM {t}").fetchone()[0]
                for t in ("commits", "file_changes", "refs")}
    finally:
        con.close()


def _run(out, repo=REPO):
    e = entl.Entl(":memory:")
    s1 = e.sink(repo, entl.SinkTarget.Sqlite, path=out, github=False)
    assert s1.new_commits > 0, s1
    assert s1.rows > 0, s1
    c1 = _counts(out)
    assert c1["commits"] == s1.new_commits, (c1, s1)

    # Re-run into the same file: PK upsert → counts must not double.
    entl.Entl(":memory:").sink(repo, entl.SinkTarget.Sqlite, path=out, github=False)
    c2 = _counts(out)
    assert c2 == c1, (c1, c2)
    return c1


def test_sink_sqlite_idempotent(tmp_path):
    _run(str(tmp_path / "out.db"))


if __name__ == "__main__":
    import tempfile

    with tempfile.TemporaryDirectory() as d:
        counts = _run(os.path.join(d, "out.db"))
        print("PASS", counts)
