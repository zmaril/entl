-- entl postgres template for `gh_comments` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "id" bigint PRIMARY KEY,
  "repo_id" text NOT NULL,
  "subject_type" text NOT NULL,
  "subject_number" integer NOT NULL,
  "author_id" bigint,
  "body" text,
  "created_at" timestamptz
);
