-- Custom migration (DESIGN.md §9.2): DuckDB-native secondary indexes +
-- recursive graph MACROs that the drizzle schema can't express.
-- Statements are separated by the drizzle `statement-breakpoint` marker.

-- ── secondary indexes (DuckDB syntax: no USING btree) ─────────────────────
CREATE INDEX IF NOT EXISTS commits_repo_idx ON "commits" ("repo_id");--> statement-breakpoint
CREATE INDEX IF NOT EXISTS commits_author_when_idx ON "commits" ("author_when");--> statement-breakpoint
CREATE INDEX IF NOT EXISTS commits_author_email_idx ON "commits" ("author_email");--> statement-breakpoint
CREATE INDEX IF NOT EXISTS commit_parents_parent_idx ON "commit_parents" ("parent_oid");--> statement-breakpoint
CREATE INDEX IF NOT EXISTS refs_target_idx ON "refs" ("target_oid");--> statement-breakpoint
CREATE INDEX IF NOT EXISTS file_changes_path_idx ON "file_changes" ("path");--> statement-breakpoint
CREATE INDEX IF NOT EXISTS tree_entries_child_idx ON "tree_entries" ("child_oid");--> statement-breakpoint
CREATE INDEX IF NOT EXISTS pr_updated_idx ON "gh_pull_requests" ("updated_at");--> statement-breakpoint
CREATE INDEX IF NOT EXISTS pr_author_idx ON "gh_pull_requests" ("author_id");--> statement-breakpoint
CREATE INDEX IF NOT EXISTS issues_updated_idx ON "gh_issues" ("updated_at");--> statement-breakpoint
CREATE INDEX IF NOT EXISTS pr_reviews_pr_idx ON "gh_pr_reviews" ("repo_id","pr_number");--> statement-breakpoint
CREATE INDEX IF NOT EXISTS review_comments_pr_idx ON "gh_review_comments" ("repo_id","pr_number");--> statement-breakpoint
CREATE INDEX IF NOT EXISTS comments_subject_idx ON "gh_comments" ("repo_id","subject_type","subject_number");--> statement-breakpoint
CREATE INDEX IF NOT EXISTS pr_commits_commit_idx ON "gh_pr_commits" ("commit_oid");--> statement-breakpoint
CREATE INDEX IF NOT EXISTS workflow_runs_head_idx ON "gh_workflow_runs" ("head_oid");--> statement-breakpoint
CREATE INDEX IF NOT EXISTS workflow_runs_updated_idx ON "gh_workflow_runs" ("updated_at");--> statement-breakpoint
CREATE INDEX IF NOT EXISTS jobs_run_idx ON "gh_jobs" ("run_id");--> statement-breakpoint
CREATE INDEX IF NOT EXISTS check_runs_commit_idx ON "gh_check_runs" ("commit_oid");--> statement-breakpoint
CREATE INDEX IF NOT EXISTS commit_statuses_commit_idx ON "gh_commit_statuses" ("commit_oid");--> statement-breakpoint

-- ── graph macros ──────────────────────────────────────────────────────────
-- Last-N along a branch's first-parent line (git log --first-parent).
CREATE OR REPLACE MACRO first_parent_chain(ref_name, n) AS TABLE
  WITH RECURSIVE chain(oid, depth) AS (
      SELECT target_oid, 0
      FROM refs WHERE kind = 'branch' AND name = ref_name
    UNION ALL
      SELECT cp.parent_oid, chain.depth + 1
      FROM chain
      JOIN commit_parents cp
        ON cp.commit_oid = chain.oid AND cp.idx = 0
      WHERE chain.depth < n - 1
  )
  SELECT oid, depth FROM chain;--> statement-breakpoint

-- Full ancestry (all parents) reachable from a commit.
CREATE OR REPLACE MACRO ancestors(start_oid) AS TABLE
  WITH RECURSIVE anc(oid) AS (
      SELECT start_oid
    UNION
      SELECT cp.parent_oid
      FROM anc
      JOIN commit_parents cp ON cp.commit_oid = anc.oid
  )
  SELECT oid FROM anc;