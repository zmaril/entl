CREATE TABLE IF NOT EXISTS "__table__" (
	"id" bigint PRIMARY KEY NOT NULL,
	"repo_id" text NOT NULL,
	"commit_oid" blob,
	"name" text,
	"status" text,
	"conclusion" text,
	"started_at" TIMESTAMP,
	"completed_at" TIMESTAMP
);
