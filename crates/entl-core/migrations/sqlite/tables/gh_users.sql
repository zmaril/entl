-- entl sqlite template for `gh_users` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "id" INTEGER PRIMARY KEY NOT NULL,
  "login" TEXT NOT NULL,
  "type" TEXT,
  "name" TEXT
);
