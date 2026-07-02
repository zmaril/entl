-- entl sqlite template for `tree_entries` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "tree_oid" TEXT NOT NULL,
  "name" TEXT NOT NULL,
  "path" TEXT NOT NULL,
  "mode" TEXT NOT NULL,
  "entry_type" TEXT NOT NULL,
  "child_oid" TEXT NOT NULL,
  PRIMARY KEY ("tree_oid", "name")
);
