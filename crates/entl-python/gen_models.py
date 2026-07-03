#!/usr/bin/env python3
"""Generate SQLAlchemy models from the entl schema (the DuckDB per-table DDL templates) — the
Python read-plane projection, the analog of the Node Drizzle models. One class per table, typed
generically so the same models work against any store the data was sunk into (SQLite/Postgres):
object-ids come back as hex strings, timestamps as RFC3339 strings.

    python gen_models.py     # → python/entl/models.py  (run from crates/entl-python/)
"""

import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
TABLES = os.path.join(HERE, "..", "entl-core", "migrations", "duckdb", "tables")
OUT = os.path.join(HERE, "python", "entl", "models.py")

# DuckDB type -> SQLAlchemy generic type. oids/timestamps are text in the portable stores.
TYPE = {
    "blob": "String",
    "text": "String",
    "timestamp": "String",
    "timestamptz": "String",
    "integer": "Integer",
    "bigint": "Integer",
    "boolean": "Boolean",
}


def parse(path):
    """(table_name, [(col, sqltype, pk, notnull)]) from one per-table DDL file."""
    lines = open(path).read().split("\n")
    table = os.path.basename(path)[: -len(".sql")]
    cols, pk = [], set()
    inside = False
    for ln in lines:
        t = ln.strip()
        if t.startswith("CREATE TABLE"):
            inside = True
            continue
        if not inside or t.startswith("--") or not t:
            continue
        if t.startswith(");"):
            break
        # strip a trailing -- comment
        t = re.sub(r"\s*--.*$", "", t).rstrip(",").strip()
        m = re.match(r"(?:CONSTRAINT\s+\S+\s+)?PRIMARY KEY\s*\(([^)]*)\)", t, re.I)
        if m:
            for c in m.group(1).split(","):
                pk.add(c.strip().strip('"'))
            continue
        c = re.match(r'"?([a-z_]+)"?\s+([A-Za-z]+)(.*)$', t, re.I)
        if not c:
            continue
        name, dtype, rest = c.group(1), c.group(2).lower(), c.group(3)
        cols.append((name, TYPE.get(dtype, "String"), "PRIMARY KEY" in rest.upper() or name in pk, "NOT NULL" in rest.upper()))
    # re-flag composite PKs discovered after the columns
    cols = [(n, ty, (p or n in pk), nn) for (n, ty, p, nn) in cols]
    return table, cols


def cls_name(table):
    return "".join(p.capitalize() for p in table.split("_"))


def main():
    tables = sorted(parse(os.path.join(TABLES, f)) for f in os.listdir(TABLES) if f.endswith(".sql"))
    out = [
        "# AUTO-GENERATED from crates/entl-core/migrations/duckdb/tables/*.sql by gen_models.py.",
        "# The SQLAlchemy read-plane projection of the entl schema. Do not edit by hand.",
        "#",
        "# READ-ONLY: these models never manage schema. entl's sink owns every table (one",
        "# mechanism: per-table DDL templates, drop-and-rebuild — see AGENTS.md), so calling",
        "# create_all()/drop_all() here is a mistake — the column types below are a lossy read",
        "# projection (oids/timestamps are text, bigint->Integer) and would author a schema that",
        "# DISAGREES with the sink. The guard below turns that mistake into a clear error.",
        "#",
        "# Bind to any store the data was sunk into, e.g.:",
        "#   from sqlalchemy import create_engine, select",
        "#   from sqlalchemy.orm import Session",
        "#   from entl.models import Commits",
        "#   e = create_engine('sqlite:///data.sqlite')",
        "#   with Session(e) as s: rows = s.scalars(select(Commits)).all()",
        "",
        "from sqlalchemy import Boolean, Column, Integer, String, event",
        "from sqlalchemy.orm import declarative_base",
        "",
        "Base = declarative_base()",
        "",
        "",
        "def _read_only(action):",
        '    """entl owns the schema via its sink; the ORM is a pure read projection."""',
        "    def _guard(*_args, **_kw):",
        "        raise RuntimeError(",
        '            f"entl.models are read-only: the entl sink owns the schema, so {action} is "',
        '            "disallowed here. Create tables by sinking data with entl (see AGENTS.md), "',
        '            "not through SQLAlchemy."',
        "        )",
        "    return _guard",
        "",
        "",
        "# Abort any attempt to emit DDL from these models (create_all / drop_all) before it runs,",
        "# so the models can never author a schema that drifts from what the sink writes.",
        'event.listen(Base.metadata, "before_create", _read_only("create_all()"))',
        'event.listen(Base.metadata, "before_drop", _read_only("drop_all()"))',
        "",
        "#: Every table entl writes, by table name.",
        "ENTL_TABLES = [",
    ]
    out += [f'    "{t}",' for t, _ in tables]
    out += ["]", ""]
    for table, cols in tables:
        out.append(f"class {cls_name(table)}(Base):")
        out.append(f'    __tablename__ = "{table}"')
        for name, ty, pk, nn in cols:
            args = [ty]
            if pk:
                args.append("primary_key=True")
            elif nn:
                args.append("nullable=False")
            out.append(f"    {name} = Column({', '.join(args)})")
        out.append("")
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    open(OUT, "w").write("\n".join(out))
    print(f"wrote {len(tables)} models -> {OUT}")


if __name__ == "__main__":
    main()
