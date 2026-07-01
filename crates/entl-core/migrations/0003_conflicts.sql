-- Merge-conflict hot zones (the north star). Populated by the `entl conflicts`
-- pass: replay every historical 2-parent merge with gix's 3-way tree merge and
-- record the paths that conflicted. `unresolved` = git would have required a
-- human to resolve it (text markers or structural failure), vs auto-resolved.
CREATE TABLE IF NOT EXISTS conflicts (
  repo_id TEXT NOT NULL,
  merge_oid TEXT NOT NULL,
  path TEXT NOT NULL,
  unresolved BOOLEAN NOT NULL,
  CONSTRAINT conflicts_pk PRIMARY KEY (repo_id, merge_oid, path)
);
