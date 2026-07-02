CREATE TABLE IF NOT EXISTS "__table__" (
	"repo_id" text NOT NULL,
	"subject_type" text NOT NULL,
	"subject_number" integer NOT NULL,
	"user_id" bigint NOT NULL,
	CONSTRAINT "assignees_repo_id_subject_type_subject_number_user_id_pk" PRIMARY KEY("repo_id","subject_type","subject_number","user_id")
);
