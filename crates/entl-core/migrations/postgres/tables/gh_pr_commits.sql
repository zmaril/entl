-- entl postgres template for `gh_pr_commits` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "repo_id" text NOT NULL,
  "pr_number" integer NOT NULL,
  "commit_oid" text NOT NULL,
  PRIMARY KEY ("repo_id", "pr_number", "commit_oid")
);
