-- entl sqlite template for `commits` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "oid" TEXT PRIMARY KEY NOT NULL,
  "repo_id" TEXT NOT NULL,
  "tree_oid" TEXT NOT NULL,
  "message" TEXT NOT NULL,
  "summary" TEXT NOT NULL,
  "author_name" TEXT,
  "author_email" TEXT,
  "author_when" TEXT,
  "author_tz" TEXT,
  "committer_name" TEXT,
  "committer_email" TEXT,
  "committer_when" TEXT,
  "committer_tz" TEXT,
  "parent_count" INTEGER NOT NULL,
  "is_merge" INTEGER NOT NULL,
  "gpg_signed" INTEGER NOT NULL
);
