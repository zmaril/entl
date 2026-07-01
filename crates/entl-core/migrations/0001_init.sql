CREATE TABLE "blobs" (
	"oid" blob PRIMARY KEY NOT NULL,
	"repo_id" text NOT NULL,
	"size" bigint NOT NULL,
	"is_binary" boolean DEFAULT false NOT NULL,
	"content_text" text,
	"content_sha" text
);
--> statement-breakpoint
CREATE TABLE "commit_parents" (
	"commit_oid" blob NOT NULL,
	"parent_oid" blob NOT NULL,
	"idx" integer NOT NULL,
	CONSTRAINT "commit_parents_commit_oid_idx_pk" PRIMARY KEY("commit_oid","idx")
);
--> statement-breakpoint
-- One row per commit, walked from every ref. Author and committer time/identity are
-- kept separate; `summary` is the first line of `message`.
CREATE TABLE "commits" (
	"oid" blob PRIMARY KEY NOT NULL, -- commit SHA (binary; hex via `lower(hex(oid))`)
	"repo_id" text NOT NULL,
	"tree_oid" blob NOT NULL, -- root tree this commit points at
	"message" text NOT NULL, -- full commit message
	"summary" text NOT NULL, -- first line of the message
	"author_name" text,
	"author_email" text,
	"author_when" TIMESTAMP,
	"author_tz" text,
	"committer_name" text,
	"committer_email" text,
	"committer_when" TIMESTAMP,
	"committer_tz" text,
	"parent_count" integer DEFAULT 0 NOT NULL,
	"is_merge" boolean DEFAULT false NOT NULL,
	"gpg_signed" boolean DEFAULT false NOT NULL
);
--> statement-breakpoint
CREATE TABLE "file_changes" (
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
--> statement-breakpoint
-- Branches, tags, remote-tracking refs, and HEAD — one row each.
CREATE TABLE "refs" (
	"repo_id" text NOT NULL,
	"name" text NOT NULL, -- short ref name, e.g. `main` or `origin/main`
	"kind" text NOT NULL, -- branch | tag | remote | head
	"target_oid" blob NOT NULL, -- commit the ref points at
	"is_symbolic" boolean DEFAULT false NOT NULL,
	"upstream" text,
	CONSTRAINT "refs_repo_id_name_pk" PRIMARY KEY("repo_id","name")
);
--> statement-breakpoint
CREATE TABLE "repos" (
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
--> statement-breakpoint
CREATE TABLE "tree_entries" (
	"tree_oid" blob NOT NULL,
	"name" text NOT NULL,
	"path" text NOT NULL,
	"mode" text NOT NULL,
	"entry_type" text NOT NULL,
	"child_oid" blob NOT NULL,
	CONSTRAINT "tree_entries_tree_oid_name_pk" PRIMARY KEY("tree_oid","name")
);
--> statement-breakpoint
CREATE TABLE "trees" (
	"oid" blob PRIMARY KEY NOT NULL,
	"repo_id" text NOT NULL
);
--> statement-breakpoint
CREATE TABLE "gh_assignees" (
	"repo_id" text NOT NULL,
	"subject_type" text NOT NULL,
	"subject_number" integer NOT NULL,
	"user_id" bigint NOT NULL,
	CONSTRAINT "assignees_repo_id_subject_type_subject_number_user_id_pk" PRIMARY KEY("repo_id","subject_type","subject_number","user_id")
);
--> statement-breakpoint
CREATE TABLE "gh_check_runs" (
	"id" bigint PRIMARY KEY NOT NULL,
	"repo_id" text NOT NULL,
	"commit_oid" blob,
	"name" text,
	"status" text,
	"conclusion" text,
	"started_at" TIMESTAMP,
	"completed_at" TIMESTAMP
);
--> statement-breakpoint
CREATE TABLE "gh_comments" (
	"id" bigint PRIMARY KEY NOT NULL,
	"repo_id" text NOT NULL,
	"subject_type" text NOT NULL,
	"subject_number" integer NOT NULL,
	"author_id" bigint,
	"body" text,
	"created_at" TIMESTAMP
);
--> statement-breakpoint
CREATE TABLE "gh_commit_statuses" (
	"id" bigint PRIMARY KEY NOT NULL,
	"repo_id" text NOT NULL,
	"commit_oid" blob NOT NULL,
	"context" text,
	"state" text,
	"description" text,
	"target_url" text,
	"created_at" TIMESTAMP
);
--> statement-breakpoint
CREATE TABLE "gh_users" (
	"id" bigint PRIMARY KEY NOT NULL,
	"login" text NOT NULL,
	"type" text,
	"name" text
);
--> statement-breakpoint
CREATE TABLE "gh_issues" (
	"repo_id" text NOT NULL,
	"number" integer NOT NULL,
	"title" text,
	"body" text,
	"state" text NOT NULL,
	"author_id" bigint,
	"created_at" TIMESTAMP,
	"updated_at" TIMESTAMP,
	"closed_at" TIMESTAMP,
	CONSTRAINT "issues_repo_id_number_pk" PRIMARY KEY("repo_id","number")
);
--> statement-breakpoint
CREATE TABLE "gh_jobs" (
	"id" bigint PRIMARY KEY NOT NULL,
	"run_id" bigint NOT NULL,
	"name" text,
	"status" text,
	"conclusion" text,
	"started_at" TIMESTAMP,
	"completed_at" TIMESTAMP,
	"runner_name" text
);
--> statement-breakpoint
CREATE TABLE "gh_labeled" (
	"repo_id" text NOT NULL,
	"subject_type" text NOT NULL,
	"subject_number" integer NOT NULL,
	"label_name" text NOT NULL,
	CONSTRAINT "labeled_repo_id_subject_type_subject_number_label_name_pk" PRIMARY KEY("repo_id","subject_type","subject_number","label_name")
);
--> statement-breakpoint
CREATE TABLE "gh_labels" (
	"repo_id" text NOT NULL,
	"name" text NOT NULL,
	"color" text,
	"description" text,
	CONSTRAINT "labels_repo_id_name_pk" PRIMARY KEY("repo_id","name")
);
--> statement-breakpoint
CREATE TABLE "gh_pr_commits" (
	"repo_id" text NOT NULL,
	"pr_number" integer NOT NULL,
	"commit_oid" blob NOT NULL,
	CONSTRAINT "pr_commits_repo_id_pr_number_commit_oid_pk" PRIMARY KEY("repo_id","pr_number","commit_oid")
);
--> statement-breakpoint
CREATE TABLE "gh_pr_reviews" (
	"id" bigint PRIMARY KEY NOT NULL,
	"repo_id" text NOT NULL,
	"pr_number" integer NOT NULL,
	"reviewer_id" bigint,
	"state" text,
	"submitted_at" TIMESTAMP,
	"body" text
);
--> statement-breakpoint
-- Pull requests and their lifecycle. `mergeable` + `checks` are the live conflict and
-- CI signals; `head_oid`/`base_oid` drive on-demand `base...head` PR diffs.
CREATE TABLE "gh_pull_requests" (
	"repo_id" text NOT NULL,
	"number" integer NOT NULL, -- PR number (unique per repo)
	"title" text,
	"body" text,
	"state" text NOT NULL, -- OPEN | CLOSED | MERGED
	"author_id" bigint,
	"created_at" TIMESTAMP,
	"updated_at" TIMESTAMP,
	"closed_at" TIMESTAMP,
	"merged_at" TIMESTAMP,
	"merge_commit_oid" blob,
	"head_ref" text,
	"base_ref" text,
	"additions" integer,
	"deletions" integer,
	"changed_files" integer,
	"is_draft" boolean DEFAULT false NOT NULL,
	"mergeable" text, -- MERGEABLE | CONFLICTING | UNKNOWN (GitHub computes it lazily)
	"checks" text, -- head commit CI rollup: SUCCESS | FAILURE | PENDING | …
	"head_oid" blob, -- PR head commit (for base...head diffs)
	"base_oid" blob, -- base branch tip at fetch time
	CONSTRAINT "pull_requests_repo_id_number_pk" PRIMARY KEY("repo_id","number")
);
--> statement-breakpoint
CREATE TABLE "gh_requested_reviewers" (
	"repo_id" text NOT NULL,
	"pr_number" integer NOT NULL,
	"user_id" bigint NOT NULL,
	CONSTRAINT "requested_reviewers_repo_id_pr_number_user_id_pk" PRIMARY KEY("repo_id","pr_number","user_id")
);
--> statement-breakpoint
CREATE TABLE "gh_review_comments" (
	"id" bigint PRIMARY KEY NOT NULL,
	"repo_id" text NOT NULL,
	"pr_number" integer NOT NULL,
	"path" text,
	"line" integer,
	"side" text,
	"commit_oid" blob,
	"author_id" bigint,
	"body" text,
	"created_at" TIMESTAMP,
	"in_reply_to" bigint
);
--> statement-breakpoint
CREATE TABLE "gh_steps" (
	"job_id" bigint NOT NULL,
	"number" integer NOT NULL,
	"name" text,
	"status" text,
	"conclusion" text,
	"started_at" TIMESTAMP,
	"completed_at" TIMESTAMP,
	CONSTRAINT "steps_job_id_number_pk" PRIMARY KEY("job_id","number")
);
--> statement-breakpoint
CREATE TABLE "gh_workflow_runs" (
	"id" bigint PRIMARY KEY NOT NULL,
	"repo_id" text NOT NULL,
	"workflow_id" bigint,
	"head_oid" blob,
	"head_branch" text,
	"event" text,
	"status" text,
	"conclusion" text,
	"run_number" integer,
	"run_attempt" integer,
	"created_at" TIMESTAMP,
	"updated_at" TIMESTAMP,
	"run_started_at" TIMESTAMP
);
--> statement-breakpoint
CREATE TABLE "gh_workflows" (
	"id" bigint PRIMARY KEY NOT NULL,
	"repo_id" text NOT NULL,
	"name" text,
	"path" text,
	"state" text
);
--> statement-breakpoint
CREATE TABLE "sync_state" (
	"resource" text PRIMARY KEY NOT NULL,
	"cursor" text,
	"etag" text,
	"watermark" TIMESTAMP,
	"last_synced_at" TIMESTAMP,
	"last_error" text
);
