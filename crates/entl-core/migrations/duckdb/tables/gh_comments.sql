CREATE TABLE IF NOT EXISTS "__table__" (
	"id" bigint PRIMARY KEY NOT NULL,
	"repo_id" text NOT NULL,
	"subject_type" text NOT NULL,
	"subject_number" integer NOT NULL,
	"author_id" bigint,
	"body" text,
	"created_at" TIMESTAMP
);
