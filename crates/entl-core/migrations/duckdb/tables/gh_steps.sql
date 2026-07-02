CREATE TABLE IF NOT EXISTS "__table__" (
	"job_id" bigint NOT NULL,
	"number" integer NOT NULL,
	"name" text,
	"status" text,
	"conclusion" text,
	"started_at" TIMESTAMP,
	"completed_at" TIMESTAMP,
	CONSTRAINT "steps_job_id_number_pk" PRIMARY KEY("job_id","number")
);
