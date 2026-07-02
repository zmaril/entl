-- entl sqlite template for `gh_pr_reviews` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "id" INTEGER PRIMARY KEY NOT NULL,
  "repo_id" TEXT NOT NULL,
  "pr_number" INTEGER NOT NULL,
  "reviewer_id" INTEGER,
  "state" TEXT,
  "submitted_at" TEXT,
  "body" TEXT
);
