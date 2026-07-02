-- entl sqlite template for `trees` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "oid" TEXT PRIMARY KEY NOT NULL,
  "repo_id" TEXT NOT NULL
);
