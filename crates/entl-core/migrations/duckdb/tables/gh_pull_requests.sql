-- Pull requests and their lifecycle. `mergeable` + `checks` are the live conflict and
-- CI signals; `head_oid`/`base_oid` drive on-demand `base...head` PR diffs.
CREATE TABLE IF NOT EXISTS "__table__" (
	"repo_id" text NOT NULL,
	"number" integer NOT NULL, -- PR number (unique per repo)
	"title" text,
	"body" text,
	"state" text NOT NULL, -- OPEN | CLOSED | MERGED
	"author_id" bigint,
	"created_at" TIMESTAMP,
	"updated_at" TIMESTAMP,
	"closed_at" TIMESTAMP,
	"merged_at" TIMESTAMP,
	"merge_commit_oid" blob,
	"head_ref" text,
	"base_ref" text,
	"additions" integer,
	"deletions" integer,
	"changed_files" integer,
	"is_draft" boolean DEFAULT false NOT NULL,
	"mergeable" text, -- MERGEABLE | CONFLICTING | UNKNOWN (GitHub computes it lazily)
	"checks" text, -- head commit CI rollup: SUCCESS | FAILURE | PENDING | …
	"head_oid" blob, -- PR head commit (for base...head diffs)
	"base_oid" blob, -- base branch tip at fetch time
	CONSTRAINT "pull_requests_repo_id_number_pk" PRIMARY KEY("repo_id","number")
);
