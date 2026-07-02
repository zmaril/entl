-- entl postgres template for `gh_pr_reviews` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "id" bigint PRIMARY KEY,
  "repo_id" text NOT NULL,
  "pr_number" integer NOT NULL,
  "reviewer_id" bigint,
  "state" text,
  "submitted_at" timestamptz,
  "body" text
);
