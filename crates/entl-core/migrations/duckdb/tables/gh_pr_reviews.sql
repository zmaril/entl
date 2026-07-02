CREATE TABLE IF NOT EXISTS "__table__" (
	"id" bigint PRIMARY KEY NOT NULL,
	"repo_id" text NOT NULL,
	"pr_number" integer NOT NULL,
	"reviewer_id" bigint,
	"state" text,
	"submitted_at" TIMESTAMP,
	"body" text
);
