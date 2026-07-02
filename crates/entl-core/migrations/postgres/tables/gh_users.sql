-- entl postgres template for `gh_users` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "id" bigint PRIMARY KEY,
  "login" text NOT NULL,
  "type" text,
  "name" text
);
