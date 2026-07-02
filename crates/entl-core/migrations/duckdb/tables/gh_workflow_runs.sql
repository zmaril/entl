CREATE TABLE IF NOT EXISTS "__table__" (
	"id" bigint PRIMARY KEY NOT NULL,
	"repo_id" text NOT NULL,
	"workflow_id" bigint,
	"head_oid" blob,
	"head_branch" text,
	"event" text,
	"status" text,
	"conclusion" text,
	"run_number" integer,
	"run_attempt" integer,
	"created_at" TIMESTAMP,
	"updated_at" TIMESTAMP,
	"run_started_at" TIMESTAMP
);
