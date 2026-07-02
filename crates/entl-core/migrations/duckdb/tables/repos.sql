CREATE TABLE IF NOT EXISTS "__table__" (
	"id" text PRIMARY KEY NOT NULL,
	"path" text NOT NULL,
	"remote_url" text,
	"host" text,
	"owner" text,
	"name" text,
	"default_branch" text,
	"first_synced_at" TIMESTAMP,
	"last_synced_at" TIMESTAMP
);
