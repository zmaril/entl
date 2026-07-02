-- entl sqlite template for `gh_comments` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "id" INTEGER PRIMARY KEY NOT NULL,
  "repo_id" TEXT NOT NULL,
  "subject_type" TEXT NOT NULL,
  "subject_number" INTEGER NOT NULL,
  "author_id" INTEGER,
  "body" TEXT,
  "created_at" TEXT
);
