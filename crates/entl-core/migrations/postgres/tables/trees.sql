-- entl postgres template for `trees` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "oid" text PRIMARY KEY,
  "repo_id" text NOT NULL
);
