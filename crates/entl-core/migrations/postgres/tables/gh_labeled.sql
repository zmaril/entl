-- entl postgres template for `gh_labeled` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "repo_id" text NOT NULL,
  "subject_type" text NOT NULL,
  "subject_number" integer NOT NULL,
  "label_name" text NOT NULL,
  PRIMARY KEY ("repo_id", "subject_type", "subject_number", "label_name")
);
