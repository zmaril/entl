-- One row per commit, walked from every ref. Author and committer time/identity are
-- kept separate; `summary` is the first line of `message`.
CREATE TABLE IF NOT EXISTS "__table__" (
	"oid" blob PRIMARY KEY NOT NULL, -- commit SHA (binary; hex via `lower(hex(oid))`)
	"repo_id" text NOT NULL,
	"tree_oid" blob NOT NULL, -- root tree this commit points at
	"message" text NOT NULL, -- full commit message
	"summary" text NOT NULL, -- first line of the message
	"author_name" text,
	"author_email" text,
	"author_when" TIMESTAMP,
	"author_tz" text,
	"committer_name" text,
	"committer_email" text,
	"committer_when" TIMESTAMP,
	"committer_tz" text,
	"parent_count" integer DEFAULT 0 NOT NULL,
	"is_merge" boolean DEFAULT false NOT NULL,
	"gpg_signed" boolean DEFAULT false NOT NULL
);
