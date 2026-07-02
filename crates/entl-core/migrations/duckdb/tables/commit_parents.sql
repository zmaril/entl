CREATE TABLE IF NOT EXISTS "__table__" (
	"commit_oid" blob NOT NULL,
	"parent_oid" blob NOT NULL,
	"idx" integer NOT NULL,
	CONSTRAINT "commit_parents_commit_oid_idx_pk" PRIMARY KEY("commit_oid","idx")
);
