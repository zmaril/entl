-- entl postgres template for `gh_review_comments` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "id" bigint PRIMARY KEY,
  "repo_id" text NOT NULL,
  "pr_number" integer NOT NULL,
  "path" text,
  "line" integer,
  "side" text,
  "commit_oid" text,
  "author_id" bigint,
  "body" text,
  "created_at" timestamptz,
  "in_reply_to" bigint
);
