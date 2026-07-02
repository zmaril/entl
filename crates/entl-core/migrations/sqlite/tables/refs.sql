-- entl sqlite template for `refs` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "repo_id" TEXT NOT NULL,
  "name" TEXT NOT NULL,
  "kind" TEXT NOT NULL,
  "target_oid" TEXT NOT NULL,
  "is_symbolic" INTEGER NOT NULL,
  "upstream" TEXT,
  PRIMARY KEY ("repo_id", "name")
);
