-- entl sqlite template for `gh_check_runs` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "id" INTEGER PRIMARY KEY NOT NULL,
  "repo_id" TEXT NOT NULL,
  "commit_oid" TEXT,
  "name" TEXT,
  "status" TEXT,
  "conclusion" TEXT,
  "started_at" TEXT,
  "completed_at" TEXT
);
