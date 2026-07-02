-- entl postgres template for `gh_events` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "repo_id" text NOT NULL,
  "id" text NOT NULL,
  "type" text,
  "actor_id" bigint,
  "actor_login" text,
  "created_at" timestamptz,
  "payload" text,
  PRIMARY KEY ("repo_id", "id")
);
