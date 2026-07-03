"""entl — git + forge activity as queryable data, in Python.

The engine (sink/extract/rebuild/query/changes) is the compiled `_entl` module. The SQLAlchemy
read-plane projection lives in `entl.models` — import it explicitly (needs the `orm` extra:
`pip install entl[orm]`); the engine itself has no SQLAlchemy dependency.
"""
from ._entl import Entl, SinkTarget, Changes  # noqa: F401

__all__ = ["Entl", "SinkTarget", "Changes"]
