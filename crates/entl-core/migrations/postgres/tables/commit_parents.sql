-- entl postgres template for `commit_parents` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "commit_oid" text NOT NULL,
  "parent_oid" text NOT NULL,
  "idx" integer NOT NULL,
  PRIMARY KEY ("commit_oid", "idx")
);
