-- entl postgres template for `commits` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "oid" text PRIMARY KEY,
  "repo_id" text NOT NULL,
  "tree_oid" text NOT NULL,
  "message" text NOT NULL,
  "summary" text NOT NULL,
  "author_name" text,
  "author_email" text,
  "author_when" timestamptz,
  "author_tz" text,
  "committer_name" text,
  "committer_email" text,
  "committer_when" timestamptz,
  "committer_tz" text,
  "parent_count" integer NOT NULL,
  "is_merge" boolean NOT NULL,
  "gpg_signed" boolean NOT NULL
);
