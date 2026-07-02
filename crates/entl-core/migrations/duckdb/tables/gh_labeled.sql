CREATE TABLE IF NOT EXISTS "__table__" (
	"repo_id" text NOT NULL,
	"subject_type" text NOT NULL,
	"subject_number" integer NOT NULL,
	"label_name" text NOT NULL,
	CONSTRAINT "labeled_repo_id_subject_type_subject_number_label_name_pk" PRIMARY KEY("repo_id","subject_type","subject_number","label_name")
);
