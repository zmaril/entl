CREATE TABLE IF NOT EXISTS "__table__" (
	"commit_oid" blob NOT NULL,
	"path" text NOT NULL,
	"old_path" text,
	"status" text NOT NULL,
	"additions" integer,
	"deletions" integer,
	"blob_oid" blob,
	"old_blob_oid" blob,
	CONSTRAINT "file_changes_commit_oid_path_pk" PRIMARY KEY("commit_oid","path")
);
