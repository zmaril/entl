-- entl sqlite template for `gh_workflow_runs` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "id" INTEGER PRIMARY KEY NOT NULL,
  "repo_id" TEXT NOT NULL,
  "workflow_id" INTEGER,
  "head_oid" TEXT,
  "head_branch" TEXT,
  "event" TEXT,
  "status" TEXT,
  "conclusion" TEXT,
  "run_number" INTEGER,
  "run_attempt" INTEGER,
  "created_at" TEXT,
  "updated_at" TEXT,
  "run_started_at" TEXT
);
