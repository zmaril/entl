-- entl postgres template for `gh_check_runs` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "id" bigint PRIMARY KEY,
  "repo_id" text NOT NULL,
  "commit_oid" text,
  "name" text,
  "status" text,
  "conclusion" text,
  "started_at" timestamptz,
  "completed_at" timestamptz
);
