CREATE TABLE IF NOT EXISTS "__table__" (
	"repo_id" text NOT NULL,
	"name" text NOT NULL,
	"color" text,
	"description" text,
	CONSTRAINT "labels_repo_id_name_pk" PRIMARY KEY("repo_id","name")
);
