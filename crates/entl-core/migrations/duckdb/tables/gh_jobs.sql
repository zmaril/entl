CREATE TABLE IF NOT EXISTS "__table__" (
	"id" bigint PRIMARY KEY NOT NULL,
	"run_id" bigint NOT NULL,
	"name" text,
	"status" text,
	"conclusion" text,
	"started_at" TIMESTAMP,
	"completed_at" TIMESTAMP,
	"runner_name" text
);
