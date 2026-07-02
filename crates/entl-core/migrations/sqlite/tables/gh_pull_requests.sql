-- entl sqlite template for `gh_pull_requests` — `__table__` is substituted with the
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
  "merged_at" TEXT,
  "merge_commit_oid" TEXT,
  "head_ref" TEXT,
  "base_ref" TEXT,
  "additions" INTEGER,
  "deletions" INTEGER,
  "changed_files" INTEGER,
  "is_draft" INTEGER NOT NULL,
  "mergeable" TEXT,
  "checks" TEXT,
  "head_oid" TEXT,
  "base_oid" TEXT,
  PRIMARY KEY ("repo_id", "number")
);
