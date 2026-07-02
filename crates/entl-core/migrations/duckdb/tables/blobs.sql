CREATE TABLE IF NOT EXISTS "__table__" (
	"oid" blob PRIMARY KEY NOT NULL,
	"repo_id" text NOT NULL,
	"size" bigint NOT NULL,
	"is_binary" boolean DEFAULT false NOT NULL,
	"content_text" text,
	"content_sha" text,
	"content" blob -- raw file bytes (object ingest / --objects); enables `entl rebuild`
);
