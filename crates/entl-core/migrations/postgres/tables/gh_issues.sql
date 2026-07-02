-- entl postgres template for `gh_issues` — `__table__` is substituted with the
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
  PRIMARY KEY ("repo_id", "number")
);
