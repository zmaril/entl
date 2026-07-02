-- entl postgres template for `gh_workflow_runs` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "id" bigint PRIMARY KEY,
  "repo_id" text NOT NULL,
  "workflow_id" bigint,
  "head_oid" text,
  "head_branch" text,
  "event" text,
  "status" text,
  "conclusion" text,
  "run_number" integer,
  "run_attempt" integer,
  "created_at" timestamptz,
  "updated_at" timestamptz,
  "run_started_at" timestamptz
);
