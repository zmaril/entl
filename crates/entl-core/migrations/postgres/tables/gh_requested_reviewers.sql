-- entl postgres template for `gh_requested_reviewers` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "repo_id" text NOT NULL,
  "pr_number" integer NOT NULL,
  "user_id" bigint NOT NULL,
  PRIMARY KEY ("repo_id", "pr_number", "user_id")
);
