CREATE TABLE IF NOT EXISTS "__table__" (
	"repo_id" text NOT NULL,
	"pr_number" integer NOT NULL,
	"user_id" bigint NOT NULL,
	CONSTRAINT "requested_reviewers_repo_id_pr_number_user_id_pk" PRIMARY KEY("repo_id","pr_number","user_id")
);
