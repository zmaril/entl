-- entl postgres template for `gh_pull_requests` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "repo_id" text NOT NULL,
  "number" integer NOT NULL,
  "title" text,
  "body" text,
  "state" text NOT NULL,
  "author_id" bigint,
  "created_at" timestamptz,
  "updated_at" timestamptz,
  "closed_at" timestamptz,
  "merged_at" timestamptz,
  "merge_commit_oid" text,
  "head_ref" text,
  "base_ref" text,
  "additions" integer,
  "deletions" integer,
  "changed_files" integer,
  "is_draft" boolean NOT NULL,
  "mergeable" text,
  "checks" text,
  "head_oid" text,
  "base_oid" text,
  PRIMARY KEY ("repo_id", "number")
);
