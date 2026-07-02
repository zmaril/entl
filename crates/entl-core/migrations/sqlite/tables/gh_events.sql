-- entl sqlite template for `gh_events` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "repo_id" TEXT NOT NULL,
  "id" TEXT NOT NULL,
  "type" TEXT,
  "actor_id" INTEGER,
  "actor_login" TEXT,
  "created_at" TEXT,
  "payload" TEXT,
  PRIMARY KEY ("repo_id", "id")
);
