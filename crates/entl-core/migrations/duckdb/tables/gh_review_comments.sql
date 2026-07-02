CREATE TABLE IF NOT EXISTS "__table__" (
	"id" bigint PRIMARY KEY NOT NULL,
	"repo_id" text NOT NULL,
	"pr_number" integer NOT NULL,
	"path" text,
	"line" integer,
	"side" text,
	"commit_oid" blob,
	"author_id" bigint,
	"body" text,
	"created_at" TIMESTAMP,
	"in_reply_to" bigint
);
