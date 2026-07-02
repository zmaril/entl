-- entl sqlite template for `gh_labels` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "repo_id" TEXT NOT NULL,
  "name" TEXT NOT NULL,
  "color" TEXT,
  "description" TEXT,
  PRIMARY KEY ("repo_id", "name")
);
