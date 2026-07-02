CREATE TABLE IF NOT EXISTS "__table__" (
	"repo_id" text NOT NULL,
	"pr_number" integer NOT NULL,
	"commit_oid" blob NOT NULL,
	CONSTRAINT "pr_commits_repo_id_pr_number_commit_oid_pk" PRIMARY KEY("repo_id","pr_number","commit_oid")
);
