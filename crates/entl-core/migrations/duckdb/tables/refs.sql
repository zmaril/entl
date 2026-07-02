-- Branches, tags, remote-tracking refs, and HEAD — one row each.
CREATE TABLE IF NOT EXISTS "__table__" (
	"repo_id" text NOT NULL,
	"name" text NOT NULL, -- short ref name, e.g. `main` or `origin/main`
	"kind" text NOT NULL, -- branch | tag | remote | head
	"target_oid" blob NOT NULL, -- commit the ref points at
	"is_symbolic" boolean DEFAULT false NOT NULL,
	"upstream" text,
	CONSTRAINT "refs_repo_id_name_pk" PRIMARY KEY("repo_id","name")
);
