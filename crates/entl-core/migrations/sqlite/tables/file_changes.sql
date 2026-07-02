-- entl sqlite template for `file_changes` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "commit_oid" TEXT NOT NULL,
  "path" TEXT NOT NULL,
  "old_path" TEXT,
  "status" TEXT NOT NULL,
  "additions" INTEGER,
  "deletions" INTEGER,
  "blob_oid" TEXT,
  "old_blob_oid" TEXT,
  PRIMARY KEY ("commit_oid", "path")
);
