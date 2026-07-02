"""Cross-language round-trip matrix (Phase 4, notes/design/testing.md): for each corpus world,
sink the repo through the Python binding into each store, extract it back, and assert it equals the
reference snapshot the Rust corpus generator produced. Set ENTL_CORPUS to a `gen_corpus` output dir.

    ENTL_CORPUS=/path/to/corpus python -m pytest tests/test_matrix.py
"""

import os
import tempfile

import pytest

import entl

CORPUS = os.environ.get("ENTL_CORPUS")


@pytest.mark.skipif(not CORPUS, reason="set ENTL_CORPUS to a gen_corpus output dir")
def test_cross_language_matrix():
    for name in sorted(os.listdir(CORPUS)):
        d = os.path.join(CORPUS, name)
        repo = os.path.join(d, "repo")
        expected = open(os.path.join(d, "expected.json")).read()

        # SQLite
        sp = os.path.join(tempfile.mkdtemp(), "s.db")
        e = entl.Entl(":memory:")
        e.sink(repo, entl.SinkTarget.Sqlite, path=sp, github=False)
        assert e.extract("sqlite", sp) == expected, f"{name} sqlite"

        # JSONL
        jd = tempfile.mkdtemp()
        e2 = entl.Entl(":memory:")
        e2.sink(repo, entl.SinkTarget.Jsonl, path=jd, github=False)
        assert e2.extract("jsonl", jd) == expected, f"{name} jsonl"
