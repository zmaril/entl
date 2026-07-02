CREATE TABLE IF NOT EXISTS "__table__" (
	"repo_id" text NOT NULL,
	"number" integer NOT NULL,
	"title" text,
	"body" text,
	"state" text NOT NULL,
	"author_id" bigint,
	"created_at" TIMESTAMP,
	"updated_at" TIMESTAMP,
	"closed_at" TIMESTAMP,
	CONSTRAINT "issues_repo_id_number_pk" PRIMARY KEY("repo_id","number")
);
