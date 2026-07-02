-- entl sqlite template for `gh_labeled` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "repo_id" TEXT NOT NULL,
  "subject_type" TEXT NOT NULL,
  "subject_number" INTEGER NOT NULL,
  "label_name" TEXT NOT NULL,
  PRIMARY KEY ("repo_id", "subject_type", "subject_number", "label_name")
);
