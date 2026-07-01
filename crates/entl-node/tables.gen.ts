// AUTO-GENERATED from entl's schema by `bun run gen`. Do not edit by hand.
// Regenerate after changing migrations; the coverage test fails if this drifts.

/** The entl tables, as a typed enum. Pass `EntlTables.ghPullRequests` to syncInto. */
export const EntlTables = {
  blobs: "blobs",
  commitParents: "commit_parents",
  commits: "commits",
  conflicts: "conflicts",
  fileChanges: "file_changes",
  ghAssignees: "gh_assignees",
  ghCheckRuns: "gh_check_runs",
  ghComments: "gh_comments",
  ghCommitStatuses: "gh_commit_statuses",
  ghEvents: "gh_events",
  ghIssues: "gh_issues",
  ghJobs: "gh_jobs",
  ghLabeled: "gh_labeled",
  ghLabels: "gh_labels",
  ghPrCommits: "gh_pr_commits",
  ghPrReviews: "gh_pr_reviews",
  ghPullRequests: "gh_pull_requests",
  ghRequestedReviewers: "gh_requested_reviewers",
  ghReviewComments: "gh_review_comments",
  ghSteps: "gh_steps",
  ghUsers: "gh_users",
  ghWorkflowRuns: "gh_workflow_runs",
  ghWorkflows: "gh_workflows",
  refs: "refs",
  repos: "repos",
  treeEntries: "tree_entries",
  trees: "trees",
} as const;

export type EntlTable = (typeof EntlTables)[keyof typeof EntlTables];

/** Every entl table name (the values of EntlTables). */
export const ENTL_TABLES = Object.values(EntlTables) as EntlTable[];
