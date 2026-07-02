-- entl sqlite template for `commit_parents` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "commit_oid" TEXT NOT NULL,
  "parent_oid" TEXT NOT NULL,
  "idx" INTEGER NOT NULL,
  PRIMARY KEY ("commit_oid", "idx")
);
