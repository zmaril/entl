CREATE TABLE IF NOT EXISTS "__table__" (
	"resource" text PRIMARY KEY NOT NULL,
	"cursor" text,
	"etag" text,
	"watermark" TIMESTAMP,
	"last_synced_at" TIMESTAMP,
	"last_error" text
);
