CREATE TABLE IF NOT EXISTS "__table__" (
	"tree_oid" blob NOT NULL,
	"name" text NOT NULL,
	"path" text NOT NULL,
	"mode" text NOT NULL,
	"entry_type" text NOT NULL,
	"child_oid" blob NOT NULL,
	CONSTRAINT "tree_entries_tree_oid_name_pk" PRIMARY KEY("tree_oid","name")
);
