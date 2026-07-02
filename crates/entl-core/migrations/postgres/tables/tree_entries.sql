-- entl postgres template for `tree_entries` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "tree_oid" text NOT NULL,
  "name" text NOT NULL,
  "path" text NOT NULL,
  "mode" text NOT NULL,
  "entry_type" text NOT NULL,
  "child_oid" text NOT NULL,
  PRIMARY KEY ("tree_oid", "name")
);
