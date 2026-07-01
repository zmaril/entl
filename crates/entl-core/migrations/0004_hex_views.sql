-- Oids are stored as raw BLOB (fast ingest, fast memcmp joins, smaller indexes).
-- This migration hides that behind a readability layer:
--   * oid('a1b2…')  -> bytes, for index-friendly lookups: WHERE oid = oid('…')
--   * *_hex views    -> lowercase-hex projections for browsing.
-- Joins and the graph macros operate on the raw BLOB columns (faster than text);
-- hex is computed lazily, only on the rows a human selects.

CREATE MACRO oid(h) AS unhex(h);

CREATE VIEW commits_hex AS
SELECT lower(hex(oid)) AS oid, repo_id, lower(hex(tree_oid)) AS tree_oid,
       message, summary, author_name, author_email, author_when, author_tz,
       committer_name, committer_email, committer_when, committer_tz,
       parent_count, is_merge, gpg_signed
FROM commits;

CREATE VIEW commit_parents_hex AS
SELECT lower(hex(commit_oid)) AS commit_oid, lower(hex(parent_oid)) AS parent_oid, idx
FROM commit_parents;

CREATE VIEW file_changes_hex AS
SELECT lower(hex(commit_oid)) AS commit_oid, path, old_path, status,
       additions, deletions,
       lower(hex(blob_oid)) AS blob_oid, lower(hex(old_blob_oid)) AS old_blob_oid
FROM file_changes;

CREATE VIEW refs_hex AS
SELECT repo_id, name, kind, lower(hex(target_oid)) AS target_oid, is_symbolic, upstream
FROM refs;

CREATE VIEW conflicts_hex AS
SELECT repo_id, lower(hex(merge_oid)) AS merge_oid, path, unresolved
FROM conflicts;
