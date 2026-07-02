CREATE TABLE IF NOT EXISTS "__table__" (
	"id" bigint PRIMARY KEY NOT NULL,
	"repo_id" text NOT NULL,
	"commit_oid" blob NOT NULL,
	"context" text,
	"state" text,
	"description" text,
	"target_url" text,
	"created_at" TIMESTAMP
);
