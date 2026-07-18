"""The SQLAlchemy read-plane models: query a real sink store through them, and confirm coverage.

python -m pytest tests/test_models.py    (needs the `orm` extra: sqlalchemy)
"""

import os
import tempfile

import entl
import pytest
from conftest import make_repo
from entl import models
from sqlalchemy import create_engine, func, select
from sqlalchemy.orm import Session


def test_query_sink_through_models():
    repo = make_repo()
    sp = os.path.join(tempfile.mkdtemp(), "data.sqlite")
    stats = entl.Entl(":memory:").sink(
        repo, entl.SinkTarget.Sqlite, path=sp, github=False
    )

    engine = create_engine(f"sqlite:///{sp}")
    with Session(engine) as s:
        n = s.scalar(select(func.count()).select_from(models.Commits))
        first = s.scalars(select(models.Commits)).first()
    assert n == stats.new_commits > 0
    assert len(first.oid) == 40 and first.author_name == "Tester"


def test_models_cover_every_sink_table():
    have = {m.__tablename__ for m in models.Base.__subclasses__()}
    for t in [
        "commits",
        "commit_parents",
        "file_changes",
        "refs",
        "blobs",
        "gh_pull_requests",
        "gh_issues",
        "gh_users",
    ]:
        assert t in have, f"no model for {t}"
    assert set(models.ENTL_TABLES) == have


def test_models_never_migrate():
    """The ORM is a pure read projection — the sink owns the schema. create_all/drop_all must
    refuse rather than author a (drifting) schema."""
    engine = create_engine("sqlite:///:memory:")
    with pytest.raises(RuntimeError, match="read-only"):
        models.Base.metadata.create_all(engine)
    with pytest.raises(RuntimeError, match="read-only"):
        models.Base.metadata.drop_all(engine)
