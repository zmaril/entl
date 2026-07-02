-- entl postgres template for `gh_labels` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "repo_id" text NOT NULL,
  "name" text NOT NULL,
  "color" text,
  "description" text,
  PRIMARY KEY ("repo_id", "name")
);
