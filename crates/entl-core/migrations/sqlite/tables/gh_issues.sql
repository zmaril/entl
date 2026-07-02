-- entl sqlite template for `gh_issues` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "repo_id" TEXT NOT NULL,
  "number" INTEGER NOT NULL,
  "title" TEXT,
  "body" TEXT,
  "state" TEXT NOT NULL,
  "author_id" INTEGER,
  "created_at" TEXT,
  "updated_at" TEXT,
  "closed_at" TEXT,
  PRIMARY KEY ("repo_id", "number")
);
