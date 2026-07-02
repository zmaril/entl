-- entl postgres template for `blobs` — `__table__` is substituted with the
-- (possibly renamed) target table name by the sink.
CREATE TABLE IF NOT EXISTS "__table__" (
  "oid" text PRIMARY KEY,
  "repo_id" text NOT NULL,
  "size" bigint NOT NULL,
  "is_binary" boolean NOT NULL,
  "content_text" text,
  "content_sha" text,
  "content" text
);
