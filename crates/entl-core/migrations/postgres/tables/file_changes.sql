-- entl postgres template for `file_changes` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "commit_oid" text NOT NULL,
  "path" text NOT NULL,
  "old_path" text,
  "status" text NOT NULL,
  "additions" integer,
  "deletions" integer,
  "blob_oid" text,
  "old_blob_oid" text,
  PRIMARY KEY ("commit_oid", "path")
);
