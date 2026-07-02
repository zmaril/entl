-- entl sqlite template for `gh_review_comments` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "id" INTEGER PRIMARY KEY NOT NULL,
  "repo_id" TEXT NOT NULL,
  "pr_number" INTEGER NOT NULL,
  "path" TEXT,
  "line" INTEGER,
  "side" TEXT,
  "commit_oid" TEXT,
  "author_id" INTEGER,
  "body" TEXT,
  "created_at" TEXT,
  "in_reply_to" INTEGER
);
